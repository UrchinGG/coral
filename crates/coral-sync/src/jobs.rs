use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serenity::all::*;

use coral_redis::{RedisPool, SyncEvent, SyncEventPublisher};
use database::{GuildSyncJob, GuildSyncJobRepository};

use crate::framework::Data;
use crate::sync::{CancelToken, Progress, RoleConfigField, cancel_token};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

pub fn spawn_job(ctx: Context, data: Data, job_id: i64) {
    tokio::spawn(async move {
        run_guild_jobs(&ctx, &data, job_id).await;
    });
}

pub fn request_cancel(data: &Data, job_id: i64) {
    if let Some(token) = data.job_cancel_tokens.lock().unwrap().get(&job_id) {
        token.store(true, Ordering::Relaxed);
    }
}

pub fn spawn_startup_sweep(ctx: Context, data: Data) {
    tokio::spawn(async move {
        let repo = GuildSyncJobRepository::new(data.db.pool());
        if let Err(e) = repo.reset_running_to_queued().await {
            tracing::error!("Job sweep failed to reset running jobs: {e}");
            return;
        }
        let queued = match repo.list_queued().await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!("Job sweep failed to list queued jobs: {e}");
                return;
            }
        };

        let mut seen_guilds = std::collections::HashSet::new();
        for job in queued {
            if seen_guilds.insert(job.guild_id) {
                spawn_job(ctx.clone(), data.clone(), job.id);
            }
        }
    });
}

async fn run_guild_jobs(ctx: &Context, data: &Data, first_job_id: i64) {
    let mut job_id = first_job_id;
    loop {
        let Some(guild_id) = run_one(ctx, data, job_id).await else {
            return;
        };

        let next = GuildSyncJobRepository::new(data.db.pool())
            .next_queued_for_guild(guild_id)
            .await;
        match next {
            Ok(Some(next_id)) => job_id = next_id,
            Ok(None) => return,
            Err(e) => {
                tracing::error!("Failed to fetch next queued job for guild {guild_id}: {e}");
                return;
            }
        }
    }
}

async fn run_one(ctx: &Context, data: &Data, job_id: i64) -> Option<i64> {
    let repo = GuildSyncJobRepository::new(data.db.pool());
    let job = match repo.claim(job_id).await {
        Ok(Some(job)) => job,
        Ok(None) => return None,
        Err(e) => {
            tracing::error!("Failed to claim job {job_id}: {e}");
            return None;
        }
    };
    let guild_id = job.guild_id;

    let publisher = data
        .redis
        .as_ref()
        .map(|pool| SyncEventPublisher::new(pool.clone()));

    if job.cancel_requested {
        finish(data, &publisher, &job, "cancelled", None).await;
        return Some(guild_id);
    }

    let cancel = cancel_token();
    data.job_cancel_tokens
        .lock()
        .unwrap()
        .insert(job.id, cancel.clone());

    let progress = job_progress(data, &publisher, &job, &cancel);
    let result = execute(ctx, data, &job, &cancel, &progress).await;

    data.job_cancel_tokens.lock().unwrap().remove(&job.id);

    let status = if result.is_err() {
        "failed"
    } else if cancel.load(Ordering::Relaxed) {
        "cancelled"
    } else {
        "done"
    };
    let error = result.as_ref().err().map(|e| format!("{e:#}"));
    if let Some(e) = &error {
        tracing::warn!("Guild job {} ({}) failed: {e}", job.id, job.kind);
    }
    finish(data, &publisher, &job, status, error.as_deref()).await;
    Some(guild_id)
}

async fn execute(
    ctx: &Context,
    data: &Data,
    job: &GuildSyncJob,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    let guild_id = GuildId::new(job.guild_id as u64);
    match job.kind.as_str() {
        "resync" => crate::sync::sync_guild_cached(ctx, data, guild_id, cancel, progress).await,
        "clear_nicknames" => {
            crate::sync::clear_nicknames(ctx, data, guild_id, cancel, progress).await
        }
        "strip_role" => {
            let role_id = payload_role(&job.payload, "role_id")?
                .ok_or_else(|| anyhow!("strip_role job missing role_id"))?;
            crate::sync::clear_role(ctx, data, guild_id, role_id, cancel, progress).await
        }
        "swap_role" => {
            let old_role = payload_role(&job.payload, "old_role_id")?;
            let new_role = payload_role(&job.payload, "new_role_id")?;
            let field = job
                .payload
                .get("field")
                .and_then(|v| v.as_str())
                .and_then(RoleConfigField::parse)
                .ok_or_else(|| anyhow!("swap_role job missing field"))?;
            crate::sync::swap_role(
                ctx, data, guild_id, old_role, new_role, field, cancel, progress,
            )
            .await?;
            crate::sync::sync_guild_cached(ctx, data, guild_id, cancel, progress).await
        }
        other => Err(anyhow!("unknown job kind {other}")),
    }
}

fn payload_role(payload: &serde_json::Value, key: &str) -> Result<Option<RoleId>> {
    let Some(value) = payload.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let id = value
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| value.as_u64())
        .ok_or_else(|| anyhow!("invalid {key} in job payload"))?;
    Ok(Some(RoleId::new(id)))
}

fn job_progress(
    data: &Data,
    publisher: &Option<SyncEventPublisher>,
    job: &GuildSyncJob,
    cancel: &CancelToken,
) -> Progress {
    let db = data.db.clone();
    let publisher = publisher.clone();
    let cancel = cancel.clone();
    let job_id = job.id;
    let guild_id = job.guild_id as u64;
    let last_report = std::sync::Mutex::new(std::time::Instant::now() - PROGRESS_INTERVAL);

    Progress::new(move |processed, total| {
        let due = {
            let mut last = last_report.lock().unwrap();
            let due = last.elapsed() >= PROGRESS_INTERVAL || processed == total;
            if due {
                *last = std::time::Instant::now();
            }
            due
        };
        if !due {
            return;
        }

        let db = db.clone();
        let publisher = publisher.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let repo = GuildSyncJobRepository::new(db.pool());
            match repo
                .update_progress(job_id, processed as i32, total as i32)
                .await
            {
                Ok(true) => cancel.store(true, Ordering::Relaxed),
                Ok(false) => {}
                Err(e) => tracing::warn!("Failed to update progress for job {job_id}: {e}"),
            }
            if let Some(publisher) = &publisher {
                publisher
                    .publish(&SyncEvent::GuildJobProgress {
                        job_id,
                        guild_id,
                        processed: processed as i32,
                        total: total as i32,
                    })
                    .await;
            }
        });
    })
}

async fn finish(
    data: &Data,
    publisher: &Option<SyncEventPublisher>,
    job: &GuildSyncJob,
    status: &str,
    error: Option<&str>,
) {
    if let Err(e) = GuildSyncJobRepository::new(data.db.pool())
        .finish(job.id, status, error)
        .await
    {
        tracing::error!("Failed to mark job {} as {status}: {e}", job.id);
    }
    if let Some(publisher) = publisher {
        publisher
            .publish(&SyncEvent::GuildJobFinished {
                job_id: job.id,
                guild_id: job.guild_id as u64,
                status: status.to_string(),
            })
            .await;
    }
}

pub async fn connect_redis(redis_url: &str) -> Option<RedisPool> {
    match RedisPool::connect(redis_url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            tracing::error!("Redis unavailable for job events: {e}");
            None
        }
    }
}
