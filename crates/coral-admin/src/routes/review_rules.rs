use axum::http::StatusCode;
use axum::{Extension, Json, Router, extract::State, routing::get};
use coral_redis::{SyncEvent, SyncEventPublisher};
use serde::Serialize;
use serde_json::json;

use database::{MemberRepository, ReviewRules, ReviewRulesRepository, standing};

use crate::auth::AdminActor;
use crate::state::AppState;

type ApiError = (StatusCode, String);

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_rules).put(update_rules))
}

#[derive(Serialize)]
struct RulesResponse {
    rules: ReviewRules,
    defaults: ReviewRules,
}

#[derive(Serialize)]
struct UpdateResponse {
    rules: ReviewRules,
    vote_changes: usize,
    tag_changes: usize,
}

async fn get_rules(State(state): State<AppState>) -> Result<Json<RulesResponse>, ApiError> {
    let rules = ReviewRulesRepository::new(state.db.pool())
        .fetch()
        .await
        .map_err(internal)?;
    Ok(Json(RulesResponse {
        rules,
        defaults: ReviewRules::DEFAULT,
    }))
}

async fn update_rules(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
    Json(rules): Json<ReviewRules>,
) -> Result<Json<UpdateResponse>, ApiError> {
    rules.validate().map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let repo = ReviewRulesRepository::new(state.db.pool());
    let before = repo.fetch().await.map_err(internal)?;
    repo.update(&rules, actor.discord_id)
        .await
        .map_err(internal)?;
    let (vote_changes, tag_changes) = reevaluate_members(&state, &rules).await?;
    crate::audit::log(
        &state,
        actor.discord_id,
        "update_review_rules",
        "review_rules",
        json!({
            "before": before,
            "after": rules,
            "vote_changes": vote_changes,
            "tag_changes": tag_changes,
        }),
    )
    .await;
    Ok(Json(UpdateResponse {
        rules,
        vote_changes,
        tag_changes,
    }))
}

async fn reevaluate_members(
    state: &AppState,
    rules: &ReviewRules,
) -> Result<(usize, usize), ApiError> {
    let members = MemberRepository::new(state.db.pool());
    let mut changes = (0, 0);
    for discord_id in members.with_review_standing().await.map_err(internal)? {
        let change = standing::refresh(&members, rules, discord_id)
            .await
            .map_err(internal)?;
        changes.0 += usize::from(change.vote.is_some());
        if change.tag.is_some() {
            changes.1 += 1;
            publish_sync_user(state, discord_id as u64).await;
        }
    }
    Ok(changes)
}

async fn publish_sync_user(state: &AppState, discord_id: u64) {
    let event = SyncEvent::SyncUser { discord_id };
    match &state.redis {
        Some(pool) => SyncEventPublisher::new(pool.clone()).publish(&event).await,
        None => tracing::warn!("Redis unavailable, sync event not published: {event:?}"),
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("review rules route error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
}
