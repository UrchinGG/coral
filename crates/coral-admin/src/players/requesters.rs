use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::batches::uncaptured_batches;
use super::forms::PlayerForm;
use super::lookups::PlayerRequester;
use super::params::RequestersParams;
use crate::error::AdminError;
use crate::names::LabelRequest;
use crate::refs::Labels;
use crate::serde_ts;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Requesters {
    #[serde(serialize_with = "serde_ts::micros")]
    pub from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub to: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub clamped_from: Option<DateTime<Utc>>,
    pub groups: Vec<PlayerRequester>,
    pub forms: BTreeMap<PlayerForm, i64>,
    pub uncaptured_batches: Option<i64>,
    pub labels: Labels,
}

pub async fn read_requesters(
    state: &AppState,
    player: Uuid,
    params: RequestersParams,
) -> Result<Requesters, AdminError> {
    let forms = params.forms()?;
    let coverage = state.coverage.current();
    let tally = state.player_lookups.tally(player, &coverage).await?;
    let from = tally.window_start(params.window);
    let labels_request = LabelRequest {
        discord: tally.account_ids(),
        minecraft: Vec::new(),
    };
    let (uncaptured, labels) = tokio::try_join!(
        uncaptured_batches(&state.db, &coverage, from, tally.computed_at),
        state.names.cached(&labels_request),
    )?;
    Ok(Requesters {
        from,
        to: tally.computed_at,
        clamped_from: tally.clamped_for(params.window),
        groups: tally.requesters(params.window, &forms),
        forms: tally.form_totals(params.window),
        uncaptured_batches: uncaptured,
        labels,
    })
}
