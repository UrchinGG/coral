use std::fmt::Display;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use utoipa::ToSchema;

use clients::normalize_uuid;
use coral_redis::RateLimitResult;
use database::{BlacklistRepository, MemberRepository, PlayerEvent};
use reqlog::{Caller, Cause, KeyKind, Outcome, record_caller, record_outcome};

use crate::cache::spawn_background_refresh;
use crate::error::report_server_error;
use crate::responses::{CubelifyResponse, CubelifyScore, CubelifyTag};
use crate::state::AppState;

struct Rejection {
    outcome: Outcome,
    message: &'static str,
    icon: &'static str,
}

const INVALID_KEY: Rejection = Rejection {
    outcome: Outcome {
        status: StatusCode::UNAUTHORIZED.as_u16(),
        cause: Cause::Auth,
        reason: "invalid_key",
    },
    message: "Invalid Key",
    icon: "mdi-key-remove",
};

const KEY_LOCKED: Rejection = Rejection {
    outcome: Outcome {
        status: StatusCode::FORBIDDEN.as_u16(),
        cause: Cause::Auth,
        reason: "key_locked",
    },
    message: "Your key has been locked",
    icon: "mdi-account-lock-outline",
};

const RATE_LIMITED: Rejection = Rejection {
    outcome: Outcome {
        status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
        cause: Cause::CallerLimit,
        reason: "rate_limited",
    },
    message: "Rate limit exceeded",
    icon: "mdi-speedometer",
};

const INTERNAL_ERROR: Rejection = Rejection {
    outcome: Outcome {
        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        cause: Cause::Internal,
        reason: "internal_error",
    },
    message: "Internal Error",
    icon: "mdi-alert-circle",
};

#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
pub(crate) struct CubelifyQuery {
    pub uuid: String,
    pub key: String,
    pub name: Option<String>,
    pub sources: Option<String>,
}

pub fn router(_state: AppState) -> Router<AppState> {
    Router::new().route("/cubelify", get(get_cubelify))
}

#[utoipa::path(
    get,
    path = "/v3/cubelify",
    description = "Returns a player's blacklist tags in the format expected by the Cubelify overlay: a score and a styled list. Authenticate with a personal key passed in the `key` query parameter. Errors are also returned with a 200 status and an error tag, so that the overlay can display them inline.",
    params(CubelifyQuery),
    responses(
        (status = 200, description = "Cubelify data", body = CubelifyResponse),
    ),
    tag = "Cubelify",
)]
pub async fn get_cubelify(
    State(state): State<AppState>,
    Query(query): Query<CubelifyQuery>,
) -> Json<CubelifyResponse> {
    Json(
        run_cubelify(&state, &query)
            .await
            .unwrap_or_else(|rejection| rejection),
    )
}

async fn run_cubelify(
    state: &AppState,
    query: &CubelifyQuery,
) -> Result<CubelifyResponse, CubelifyResponse> {
    validate_api_key(state, &query.key).await?;
    check_rate_limit(state, &query.key).await?;
    let uuid = normalize_uuid(&query.uuid);
    spawn_background_refresh(state, &uuid);
    let tags = fetch_tags(state, &uuid).await?;
    Ok(build_response(state, &tags).await)
}

async fn validate_api_key(state: &AppState, api_key: &str) -> Result<(), CubelifyResponse> {
    let Some(member) = MemberRepository::new(state.db.pool())
        .get_by_api_key(api_key)
        .await
        .map_err(|e| internal_failure("member key lookup failed", e))?
    else {
        record_caller(Caller::anonymous(KeyKind::Invalid));
        return Err(reject(&INVALID_KEY));
    };
    record_caller(Caller::member(KeyKind::Personal, member.id));

    if member.key_locked {
        return Err(reject(&KEY_LOCKED));
    }
    Ok(())
}

async fn check_rate_limit(state: &AppState, api_key: &str) -> Result<(), CubelifyResponse> {
    match state
        .rate_limiter
        .check_and_record(api_key, crate::auth::PERSONAL_RATE_LIMIT)
        .await
    {
        Ok(RateLimitResult::Allowed { .. }) => Ok(()),
        Ok(RateLimitResult::Exceeded) => Err(reject(&RATE_LIMITED)),
        Err(e) => Err(internal_failure("rate limit check failed", e)),
    }
}

async fn fetch_tags(state: &AppState, uuid: &str) -> Result<Vec<PlayerEvent>, CubelifyResponse> {
    BlacklistRepository::new(state.db.pool())
        .get_active_tags(uuid)
        .await
        .map_err(|e| internal_failure("tag lookup failed", e))
}

fn reject(rejection: &Rejection) -> CubelifyResponse {
    record_outcome(rejection.outcome);
    CubelifyResponse::error(rejection.message, rejection.icon)
}

fn internal_failure(context: &str, error: impl Display) -> CubelifyResponse {
    report_server_error(&format!("{context}: {error}"));
    reject(&INTERNAL_ERROR)
}

async fn build_response(state: &AppState, tags: &[PlayerEvent]) -> CubelifyResponse {
    let mut cubelify_tags = Vec::new();
    let mut total_score = 0.0;

    for tag in tags {
        if let Some(def) = blacklist::lookup(tag.tag_type.as_deref().unwrap_or("")) {
            cubelify_tags.push(CubelifyTag {
                icon: def.icon.to_string(),
                color: def.color,
                tooltip: build_tooltip(state, def.name, tag).await,
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

async fn build_tooltip(state: &AppState, tag_name: &str, tag: &PlayerEvent) -> String {
    let name = capitalize(tag_name);
    let time_ago = relative_time(tag.ts);

    let mut tooltip = match tag.author.filter(|_| !tag.hide_username.unwrap_or(false)) {
        Some(author) => {
            let added_by = state
                .discord
                .resolve_username(author as u64)
                .await
                .unwrap_or_else(|| "Unknown".into());
            format!("{name} (Added by {added_by} {time_ago})")
        }
        None => format!("{name} (Added {time_ago})"),
    };

    if let Some(reason) = tag.reason.as_deref().filter(|r| !r.is_empty()) {
        tooltip.push_str(&format!("\n- {reason}"));
    }
    if let Some(expires_at) = tag.expires_at {
        let remaining = relative_time_future(expires_at);
        tooltip.push_str(&format!("\n- Expires {remaining}"));
    }
    tooltip
}

fn relative_time(timestamp: chrono::DateTime<Utc>) -> String {
    let secs = (Utc::now() - timestamp).num_seconds();
    if secs < 60 {
        return "just now".into();
    }
    let (val, unit) = match secs {
        60..3600 => (secs / 60, "min"),
        3600..86400 => (secs / 3600, "hr"),
        86400..2_592_000 => (secs / 86400, "d"),
        2_592_000..31_536_000 => (secs / 2_592_000, "mon"),
        _ => (secs / 31_536_000, "yr"),
    };
    format!("{val}{unit} ago")
}

fn relative_time_future(timestamp: chrono::DateTime<Utc>) -> String {
    let secs = (timestamp - Utc::now()).num_seconds();
    if secs <= 0 {
        return "soon".into();
    }
    let (val, unit) = match secs {
        0..3600 => (secs / 60, "min"),
        3600..86400 => (secs / 3600, "hr"),
        86400..2_592_000 => (secs / 86400, "d"),
        _ => (secs / 2_592_000, "mon"),
    };
    format!("in {val}{unit}")
}

fn capitalize(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
