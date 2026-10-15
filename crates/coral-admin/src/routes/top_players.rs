use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::error::AdminError;
use crate::names::LabelRequest;
use crate::refs::Labels;
use crate::rollups::{TopList, TopWindow};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopQuery {
    window: TopWindow,
}

#[derive(Debug, Serialize)]
pub struct RankedPlayers {
    #[serde(flatten)]
    ranking: TopList,
    labels: Labels,
}

pub async fn top_players(
    State(state): State<AppState>,
    query: Result<Query<TopQuery>, QueryRejection>,
) -> Result<Json<RankedPlayers>, AdminError> {
    let Query(query) = query?;
    let ranking = state
        .rollups
        .top_players()
        .get(query.window)
        .await
        .ok_or_else(|| {
            AdminError::NotFound(format!(
                "players looked up in the last {} have no current ranking",
                query.window.label()
            ))
        })?;
    let request = LabelRequest {
        discord: Vec::new(),
        minecraft: ranking
            .players
            .iter()
            .map(|ranked| ranked.player.simple().to_string())
            .collect(),
    };
    let labels = state.names.cached(&request).await?;
    Ok(Json(RankedPlayers {
        ranking: TopList::clone(&ranking),
        labels,
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    fn parsed(uri: &'static str) -> Result<TopWindow, AdminError> {
        let Query(query) = Query::<TopQuery>::try_from_uri(&Uri::from_static(uri))?;
        Ok(query.window)
    }

    #[test]
    fn the_window_is_one_hour_or_one_day_and_anything_else_is_a_bad_request() {
        assert_eq!(
            parsed("/players/top?window=1h").unwrap(),
            TopWindow::LastHour
        );
        assert_eq!(
            parsed("/players/top?window=24h").unwrap(),
            TopWindow::LastDay
        );
        for uri in [
            "/players/top?window=7d",
            "/players/top",
            "/players/top?window=1h&x=1",
        ] {
            assert_eq!(parsed(uri).unwrap_err().code(), "bad_request", "{uri}");
        }
    }
}
