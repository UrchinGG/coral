use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use serde_json::json;

use crate::audit;
use crate::auth::AdminActor;
use crate::error::AdminError;
use crate::flags::{self, Dismissal};
use crate::serde_ts;
use crate::state::AppState;
use crate::views::{CachedView, View};

const FLAGS_VIEW: &str = "attention";
const DISMISS_ACTION: &str = "dismiss_flag";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(current_flags))
        .route("/{key}/dismiss", post(dismiss_flag))
}

async fn current_flags(State(state): State<AppState>) -> Result<CachedView, AdminError> {
    state
        .views
        .get_or_load(View::Flags, FLAGS_VIEW, || {
            flags::collect(&state, Utc::now())
        })
        .await
}

async fn dismiss_flag(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    key: Result<Path<String>, PathRejection>,
) -> Result<Json<Dismissal>, AdminError> {
    let Path(key) = key.map_err(|rejection| AdminError::BadRequest(rejection.body_text()))?;
    let dismissal = flags::dismiss(state.db.pool(), &key, Utc::now()).await?;
    audit::log(
        &state,
        actor.discord_id,
        DISMISS_ACTION,
        &dismissal.key,
        json!({ "until": serde_ts::format(&dismissal.dismissed_until) }),
    )
    .await;
    state.views.forget(View::Flags, FLAGS_VIEW).await;
    Ok(Json(dismissal))
}
