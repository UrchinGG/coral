use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use database::MemberRepository;

use crate::state::AppState;

pub async fn require_owner(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let key = request
        .uri()
        .query()
        .and_then(query_key)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let authorized = MemberRepository::new(state.db.pool())
        .get_by_api_key(&key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some_and(|m| !m.key_locked && state.owner_ids.contains(&m.discord_id));

    match authorized {
        true => Ok(next.run(request).await),
        false => Err(StatusCode::FORBIDDEN),
    }
}

fn query_key(query: &str) -> Option<String> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("key=").map(String::from))
}
