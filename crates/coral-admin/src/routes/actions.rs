use axum::{Json, Router, extract::*, routing::get};
use database::{AdminAction, AdminActionRepository};
use serde::Deserialize;

use crate::identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<i64>,
}

#[derive(serde::Serialize)]
struct ActionRow {
    #[serde(flatten)]
    action: AdminAction,
    actor_username: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<Vec<ActionRow>> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    let actions = AdminActionRepository::new(state.db.pool())
        .list_recent(limit)
        .await
        .unwrap_or_default();

    let actor_ids: Vec<i64> = actions.iter().map(|a| a.actor).collect();
    let names = identity::resolve_discord_usernames(&state, &actor_ids).await;

    Json(
        actions
            .into_iter()
            .map(|action| {
                let actor_username = names.get(&action.actor).cloned();
                ActionRow {
                    action,
                    actor_username,
                }
            })
            .collect(),
    )
}
