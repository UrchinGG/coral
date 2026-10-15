use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::jobs::{Job, JobStatus, backfill_window_open};
use crate::maintenance::{
    BackfillPlan, DISCORD_NAMES_JOB, DiscordNamesJob, INDEX_CATCHUP_JOB, IndexCatchupJob,
    SUBJECTS_JOB, SubjectsJob, missing_discord_names, missing_indexes,
};
use crate::rollups::{RECOMPUTE_JOB, ROLLUP_JOB, RecomputeJob, RollupState, Rollups};
use crate::serde_ts;
use crate::state::AppState;

pub const MAINTENANCE_JOBS: [&str; 5] = [
    ROLLUP_JOB,
    RECOMPUTE_JOB,
    SUBJECTS_JOB,
    DISCORD_NAMES_JOB,
    INDEX_CATCHUP_JOB,
];

const RECOMPUTE_ACTION: &str = "recompute_rollups";
const START_ACTION: &str = "start_maintenance_job";

#[derive(Debug, Serialize)]
pub struct Maintenance {
    jobs: Vec<MaintenanceJob>,
    rollups: RollupState,
    pending: PendingWork,
    window_open: bool,
}

#[derive(Debug, Serialize)]
struct MaintenanceJob {
    name: &'static str,
    status: Option<JobStatus>,
}

#[derive(Debug, Serialize)]
struct PendingWork {
    subjects: BackfillPlan,
    discord_names: i64,
    indexes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecomputeBody {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

pub async fn overview(State(state): State<AppState>) -> Result<Json<Maintenance>, AdminError> {
    let now = Utc::now();
    let (jobs, rollups, subjects, discord_names, indexes, window_open) = tokio::try_join!(
        job_statuses(&state),
        Rollups::state(&state.db),
        BackfillPlan::load(&state.db, now),
        missing_discord_names(&state.db),
        missing_indexes(&state.db),
        backfill_window_open(&state.db, now),
    )?;
    Ok(Json(Maintenance {
        jobs,
        rollups,
        pending: PendingWork {
            subjects,
            discord_names,
            indexes,
        },
        window_open,
    }))
}

pub async fn recompute(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    body: Result<Json<RecomputeBody>, JsonRejection>,
) -> Result<(StatusCode, Json<JobStatus>), AdminError> {
    let Json(body) = body?;
    let span = state
        .rollups
        .recompute_span(&state.db, body.from, body.to, Utc::now())
        .await?;
    let job = RecomputeJob::new(state.rollups.clone(), span);
    let status = state.jobs.start(Arc::new(job)).await?;
    crate::audit::log(
        &state,
        actor.discord_id,
        RECOMPUTE_ACTION,
        RECOMPUTE_JOB,
        json!({ "from": serde_ts::format(&span.from), "to": serde_ts::format(&span.to) }),
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(status)))
}

pub async fn start(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<JobStatus>), AdminError> {
    let job = startable_job(&state, &name).await?;
    let status = state.jobs.start(job).await?;
    crate::audit::log(&state, actor.discord_id, START_ACTION, &name, json!({})).await;
    Ok((StatusCode::ACCEPTED, Json(status)))
}

async fn startable_job(state: &AppState, name: &str) -> Result<Arc<dyn Job>, AdminError> {
    match name {
        SUBJECTS_JOB => Ok(Arc::new(SubjectsJob::planned(&state.db, Utc::now()).await?)),
        DISCORD_NAMES_JOB => Ok(Arc::new(
            DiscordNamesJob::planned(&state.db, state.names.clone()).await?,
        )),
        INDEX_CATCHUP_JOB => Ok(Arc::new(IndexCatchupJob)),
        other => Err(AdminError::NotFound(format!(
            "no maintenance job is called {other:?}; start {SUBJECTS_JOB}, {DISCORD_NAMES_JOB} or {INDEX_CATCHUP_JOB}"
        ))),
    }
}

async fn job_statuses(state: &AppState) -> Result<Vec<MaintenanceJob>, AdminError> {
    try_join_all(MAINTENANCE_JOBS.map(|name| async move {
        let status = state.jobs.status(name).await?;
        Ok::<_, AdminError>(MaintenanceJob { name, status })
    }))
    .await
}
