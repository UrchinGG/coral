use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use coral_redis::{RedisPool, SYNC_CHANNEL, SyncEvent, SyncEventPublisher};
use database::{Database, GuildSyncJob, GuildSyncJobRepository};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use serenity::all::GuildId;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::home_guild;
use crate::auth::AdminActor;
use crate::error::{AdminError, Context};
use crate::serde_ts;
use crate::state::AppState;

pub const FINISHED_JOBS_SHOWN: i64 = 5;

const EVENT_BUFFER: usize = 32;
const FINISHED_STATES: [&str; 3] = ["done", "cancelled", "failed"];

#[derive(Debug, Serialize)]
pub struct SyncJob {
    id: i64,
    kind: String,
    label: String,
    state: String,
    processed: i32,
    total: i32,
    error: Option<String>,
    #[serde(serialize_with = "serde_ts::micros")]
    started_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct SyncJobs {
    jobs: Vec<SyncJob>,
}

#[derive(Debug, Serialize)]
pub struct StartedJob {
    job: Option<SyncJob>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JobEvent {
    Snapshot { job: SyncJob },
    Progress { processed: i32, total: i32 },
    Finished { status: String },
    Failed { message: String },
}

impl StartedJob {
    pub fn none() -> Self {
        Self { job: None }
    }
}

pub async fn list(State(state): State<AppState>) -> Result<Json<SyncJobs>, AdminError> {
    let guild_id = home_guild(&state)?;
    let jobs = GuildSyncJobRepository::new(state.db.pool())
        .list_recent(guild_id.get() as i64, FINISHED_JOBS_SHOWN)
        .await
        .context("read the sync jobs")?;
    Ok(Json(SyncJobs {
        jobs: jobs.iter().map(job_view).collect(),
    }))
}

pub async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(job_id): Path<i64>,
) -> Result<StatusCode, AdminError> {
    let guild_id = home_guild(&state)?;
    let repo = GuildSyncJobRepository::new(state.db.pool());
    let job = guild_job(&repo, guild_id, job_id).await?;
    repo.request_cancel(job_id)
        .await
        .context("request the sync job's cancellation")?;
    SyncEventPublisher::new(state.redis.clone())
        .publish(&SyncEvent::GuildJobCancelRequested { job_id })
        .await;
    crate::audit::log(
        &state,
        actor.discord_id,
        "cancel_sync_job",
        &guild_id.to_string(),
        json!({ "job_id": job_id, "kind": job.kind }),
    )
    .await;
    Ok(StatusCode::ACCEPTED)
}

pub async fn events(
    State(state): State<AppState>,
    Path(job_id): Path<i64>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AdminError> {
    let guild_id = home_guild(&state)?;
    let job = guild_job(
        &GuildSyncJobRepository::new(state.db.pool()),
        guild_id,
        job_id,
    )
    .await?;
    let (sender, receiver) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(stream_job(
        state.redis.clone(),
        state.db.clone(),
        job,
        sender,
    ));
    let stream = ReceiverStream::new(receiver).map(Ok::<_, Infallible>);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn enqueue(
    state: &AppState,
    guild_id: GuildId,
    kind: &str,
    payload: Value,
    created_by: i64,
) -> Result<StartedJob, AdminError> {
    let job = GuildSyncJobRepository::new(state.db.pool())
        .enqueue(guild_id.get() as i64, kind, &payload, Some(created_by))
        .await
        .context(format!("queue the {kind} sync job"))?;
    SyncEventPublisher::new(state.redis.clone())
        .publish(&SyncEvent::GuildJobQueued { job_id: job.id })
        .await;
    Ok(StartedJob {
        job: Some(job_view(&job)),
    })
}

async fn guild_job(
    repo: &GuildSyncJobRepository<'_>,
    guild_id: GuildId,
    job_id: i64,
) -> Result<GuildSyncJob, AdminError> {
    repo.get(job_id)
        .await
        .context("read the sync job")?
        .filter(|job| job.guild_id == guild_id.get() as i64)
        .ok_or_else(|| AdminError::NotFound(format!("no sync job {job_id} on the home server")))
}

async fn stream_job(
    redis: RedisPool,
    db: std::sync::Arc<Database>,
    job: GuildSyncJob,
    sender: mpsc::Sender<Event>,
) {
    if let Err(error) = forward_job_events(&redis, &db, job, &sender).await {
        let failed = JobEvent::Failed {
            message: error.to_string(),
        };
        let _ = sender.send(event(&failed)).await;
    }
}

async fn forward_job_events(
    redis: &RedisPool,
    db: &Database,
    job: GuildSyncJob,
    sender: &mpsc::Sender<Event>,
) -> Result<(), AdminError> {
    let job_id = job.id;
    let mut pubsub = redis
        .pubsub()
        .await
        .context("open the sync event subscription")?;
    pubsub
        .subscribe(SYNC_CHANNEL)
        .await
        .context("subscribe to sync events")?;
    let current = GuildSyncJobRepository::new(db.pool())
        .get(job_id)
        .await
        .context("read the sync job")?
        .unwrap_or(job);
    let finished = is_finished(&current.status);
    if sender
        .send(event(&JobEvent::Snapshot {
            job: job_view(&current),
        }))
        .await
        .is_err()
        || finished
    {
        return Ok(());
    }
    let mut messages = pubsub.into_on_message();
    while let Some(message) = messages.next().await {
        let Some(update) = job_update(job_id, &message) else {
            continue;
        };
        let done = matches!(update, JobEvent::Finished { .. });
        if sender.send(event(&update)).await.is_err() || done {
            break;
        }
    }
    Ok(())
}

fn job_update(job_id: i64, message: &redis::Msg) -> Option<JobEvent> {
    let payload: String = message.get_payload().ok()?;
    let event = match serde_json::from_str::<SyncEvent>(&payload) {
        Ok(event) => event,
        Err(error) => {
            tracing::warn!(%error, "a sync event could not be parsed");
            return None;
        }
    };
    match event {
        SyncEvent::GuildJobProgress {
            job_id: id,
            processed,
            total,
            ..
        } if id == job_id => Some(JobEvent::Progress { processed, total }),
        SyncEvent::GuildJobFinished {
            job_id: id, status, ..
        } if id == job_id => Some(JobEvent::Finished { status }),
        _ => None,
    }
}

fn event(update: &JobEvent) -> Event {
    let data = serde_json::to_string(update).expect("job events always serialize");
    Event::default().data(data)
}

fn is_finished(status: &str) -> bool {
    FINISHED_STATES.contains(&status)
}

fn job_view(job: &GuildSyncJob) -> SyncJob {
    let label = job
        .payload
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_else(|| default_label(&job.kind))
        .to_string();
    SyncJob {
        id: job.id,
        kind: job.kind.clone(),
        label,
        state: job.status.clone(),
        processed: job.processed,
        total: job.total.unwrap_or(0),
        error: job.error.clone(),
        started_at: job.started_at.unwrap_or(job.created_at),
        finished_at: job.finished_at,
    }
}

fn default_label(kind: &str) -> &'static str {
    match kind {
        "resync" => "Syncing all members",
        "clear_nicknames" => "Clearing all nicknames",
        "strip_role" => "Stripping role",
        "swap_role" => "Swapping role",
        _ => "Bulk update",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_events_are_tagged_by_type() {
        let progress = serde_json::to_value(JobEvent::Progress {
            processed: 3,
            total: 9,
        })
        .unwrap();
        assert_eq!(
            progress,
            json!({ "type": "progress", "processed": 3, "total": 9 })
        );
        let finished = serde_json::to_value(JobEvent::Finished {
            status: "done".into(),
        })
        .unwrap();
        assert_eq!(finished, json!({ "type": "finished", "status": "done" }));
    }

    #[test]
    fn jobs_without_a_label_are_named_after_their_kind() {
        assert_eq!(default_label("resync"), "Syncing all members");
        assert_eq!(default_label("unknown"), "Bulk update");
        assert!(is_finished("failed"));
        assert!(!is_finished("running"));
    }
}
