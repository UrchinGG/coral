use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;

use clients::normalize_uuid;
use database::{BlacklistRepository, Member, MemberRepository, PlayerTagRow};

use coral_redis::RateLimitResult;

use crate::cache::refresh_player_cache;
use crate::responses::{CubelifyResponse, CubelifyScore, CubelifyTag};
use crate::state::AppState;

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub(crate) struct CubelifyQuery {
    pub id: String,
    pub key: String,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub sources: String,
}

pub fn router(_state: AppState) -> Router<AppState> {
    Router::new().route("/cubelify/{uuid}", get(get_cubelify))
}

#[utoipa::path(
    get,
    path = "/v1/cubelify/{uuid}",
    params(
        ("uuid" = String, Path, description = "Player UUID"),
        CubelifyQuery
    ),
    responses(
        (status = 200, description = "Cubelify data", body = CubelifyResponse),
    ),
    tag = "Cubelify",
)]
pub async fn get_cubelify(
    State(state): State<AppState>,
    Query(query): Query<CubelifyQuery>,
) -> Json<CubelifyResponse> {
    let result = process_cubelify(&state, &query).await;
    Json(result.unwrap_or_else(|e| e))
}

async fn process_cubelify(
    state: &AppState,
    query: &CubelifyQuery,
) -> Result<CubelifyResponse, CubelifyResponse> {
    let member = validate_api_key(state, &query.key).await?;
    check_rate_limit(state, &query.key, &member).await?;

    let uuid = normalize_uuid(&query.id);

    refresh_player_cache(state, &uuid, None).await;

    let tags = fetch_player_tags(state, &uuid).await?;
    Ok(build_cubelify_response(&tags))
}

async fn validate_api_key(state: &AppState, api_key: &str) -> Result<Member, CubelifyResponse> {
    let repo = MemberRepository::new(state.db.pool());

    let member = repo
        .get_by_api_key(api_key)
        .await
        .map_err(|_| CubelifyResponse::error("Internal Error", "mdi-alert-circle"))?
        .ok_or_else(|| CubelifyResponse::error("Invalid Key", "mdi-key-remove"))?;

    if member.key_locked {
        return Err(CubelifyResponse::error(
            "Your key has been locked",
            "mdi-account-lock-outline",
        ));
    }

    Ok(member)
}

async fn check_rate_limit(
    state: &AppState,
    api_key: &str,
    member: &Member,
) -> Result<(), CubelifyResponse> {
    let limit = match member.access_level {
        4.. => 3000,
        2..=3 => 1200,
        _ => 600,
    };

    match state.rate_limiter.check_and_record(api_key, limit).await {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(CubelifyResponse::error(
            "Rate limit exceeded",
            "mdi-speedometer",
        )),
        Err(_) => Err(CubelifyResponse::error(
            "Internal Error",
            "mdi-alert-circle",
        )),
    }
}

async fn fetch_player_tags(
    state: &AppState,
    uuid: &str,
) -> Result<Vec<PlayerTagRow>, CubelifyResponse> {
    BlacklistRepository::new(state.db.pool())
        .get_tags(uuid)
        .await
        .map_err(|_| CubelifyResponse::error("Internal Error", "mdi-alert-circle"))
}

fn build_cubelify_response(tags: &[PlayerTagRow]) -> CubelifyResponse {
    let mut cubelify_tags = Vec::new();
    let mut total_score = 0.0;

    for tag in tags {
        if let Some(def) = blacklist::lookup(&tag.tag_type) {
            cubelify_tags.push(CubelifyTag {
                icon: def.icon.to_string(),
                color: def.color,
                tooltip: build_tooltip(def.name, &tag.reason),
                text: None,
            });
            total_score += def.score;
        }
    }

    CubelifyResponse {
        score: CubelifyScore {
            value: total_score,
            mode: "add",
        },
        tags: cubelify_tags,
    }
}

fn build_tooltip(tag_name: &str, reason: &str) -> String {
    if reason.is_empty() {
        capitalize(tag_name)
    } else {
        format!("{}: {}", capitalize(tag_name), reason)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}
