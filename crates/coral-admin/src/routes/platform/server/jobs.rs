use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Extension, Json};
use coral_redis::{RedisPool, SYNC_CHANNEL, SyncEvent};
use futures_util::{Stream, StreamExt};
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{change, home_guild, sync_settings};
use crate::auth::AdminActor;
use crate::error::{AdminError, Context};
use crate::state::AppState;
use crate::sync_settings::SyncSettings;

const EVENT_BUFFER: usize = 32;
const FINISHED_STATES: [&str; 3] = ["done", "cancelled", "failed"];

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JobEvent {
    Snapshot { job: Value },
    Progress { processed: i32, total: i32 },
    Finished { status: String },
    Failed { message: String },
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, AdminError> {
    let guild_id = home_guild(&state)?;
    let path = format!("/guilds/{guild_id}/jobs");
    Ok(Json(sync_settings(&state)?.read(&path).await?))
}

pub async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(job_id): Path<i64>,
) -> Result<StatusCode, AdminError> {
    let path = format!("/jobs/{job_id}");
    change(
        &state,
        &actor,
        Method::DELETE,
        &path,
        None,
        "cancel_sync_job",
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn events(
    State(state): State<AppState>,
    Path(job_id): Path<i64>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AdminError> {
    let guild_id = home_guild(&state)?;
    let path = format!("/guilds/{guild_id}/jobs/{job_id}");
    let settings = sync_settings(&state)?;
    let job = settings.read(&path).await?;
    let (sender, receiver) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(stream_job(state.redis.clone(), settings, path, job, sender));
    let stream = ReceiverStream::new(receiver).map(Ok::<_, Infallible>);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn stream_job(
    redis: RedisPool,
    settings: Arc<SyncSettings>,
    path: String,
    job: Value,
    sender: mpsc::Sender<Event>,
) {
    if let Err(error) = forward_job_events(&redis, &settings, &path, job, &sender).await {
        let failed = JobEvent::Failed {
            message: error.to_string(),
        };
        let _ = sender.send(event(&failed)).await;
    }
}

async fn forward_job_events(
    redis: &RedisPool,
    settings: &SyncSettings,
    path: &str,
    job: Value,
    sender: &mpsc::Sender<Event>,
) -> Result<(), AdminError> {
    let job_id = job["id"].as_i64().unwrap_or_default();
    let mut pubsub = redis
        .pubsub()
        .await
        .context("open the sync event subscription")?;
    pubsub
        .subscribe(SYNC_CHANNEL)
        .await
        .context("subscribe to sync events")?;
    let current = settings.read(path).await.unwrap_or(job);
    let finished = current["state"].as_str().is_some_and(is_finished);
    if sender
        .send(event(&JobEvent::Snapshot { job: current }))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

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
    fn finished_states_end_the_stream() {
        assert!(is_finished("failed"));
        assert!(!is_finished("running"));
    }
}
