use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::AdminError;
use crate::search::{
    DEFAULT_HIT_LIMIT, MAX_HIT_LIMIT, MAX_QUERY_CHARS, SearchHit, SearchResults, Searcher,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchBody {
    q: String,
    #[serde(default = "default_limit")]
    limit: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteBody {
    name: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(local))
        .route("/remote", post(remote))
}

async fn local(
    State(state): State<AppState>,
    body: Result<Json<SearchBody>, JsonRejection>,
) -> Result<Json<SearchResults>, AdminError> {
    let Json(body) = body?;
    body.validate()?;
    let results = Searcher::new(&state).search(&body.q, body.limit).await?;
    Ok(Json(results))
}

async fn remote(
    State(state): State<AppState>,
    body: Result<Json<RemoteBody>, JsonRejection>,
) -> Result<Json<Vec<SearchHit>>, AdminError> {
    let Json(body) = body?;
    let hits = Searcher::new(&state).remote(body.name.trim()).await?;
    Ok(Json(hits))
}

impl SearchBody {
    fn validate(&self) -> Result<(), AdminError> {
        if self.q.chars().count() > MAX_QUERY_CHARS {
            return Err(AdminError::BadRequest(format!(
                "search text is limited to {MAX_QUERY_CHARS} characters"
            )));
        }
        if !(1..=MAX_HIT_LIMIT).contains(&self.limit) {
            return Err(AdminError::BadRequest(format!(
                "limit must be between 1 and {MAX_HIT_LIMIT}"
            )));
        }
        Ok(())
    }
}

fn default_limit() -> u8 {
    DEFAULT_HIT_LIMIT
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(value: serde_json::Value) -> SearchBody {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_limit_defaults_to_eight_and_is_bounded() {
        let parsed = body(json!({ "q": "oJugg" }));
        assert_eq!(parsed.limit, DEFAULT_HIT_LIMIT);
        assert!(parsed.validate().is_ok());
        for limit in [0, MAX_HIT_LIMIT + 1] {
            let error = body(json!({ "q": "oJugg", "limit": limit }))
                .validate()
                .unwrap_err();
            assert_eq!(error.code(), "bad_request", "{limit}");
        }
    }

    #[test]
    fn oversized_text_is_rejected_without_echoing_it() {
        let text = "e".repeat(MAX_QUERY_CHARS + 1);
        let error = body(json!({ "q": text })).validate().unwrap_err();
        assert_eq!(error.code(), "bad_request");
        assert!(!error.message().contains(&text[..32]));
    }

    #[test]
    fn unknown_fields_are_a_bad_request() {
        let rejection = Json::<SearchBody>::from_bytes(br#"{"q":"x","key":"y"}"#).unwrap_err();
        assert_eq!(AdminError::from(rejection).code(), "bad_request");
    }
}
