use coral_redis::RedisPool;
use database::Database;
use futures_util::future::join_all;
use serde::Serialize;

use crate::coverage::Coverage;
use crate::error::AdminError;
use crate::names::{LabelRequest, NameService};
use crate::query::SearchLimits;
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;

mod input;
mod lookup;
mod probe;
mod rank;
mod remote;
#[cfg(test)]
mod tests;

pub use input::MAX_QUERY_CHARS;
pub use rank::SearchHit;

use input::Input;
use lookup::Lookup;
use probe::{Found, ProbeContext};

pub const DEFAULT_HIT_LIMIT: u8 = 8;
pub const MAX_HIT_LIMIT: u8 = 50;

pub struct Searcher<'a> {
    db: &'a Database,
    redis: &'a RedisPool,
    names: &'a NameService,
    limits: &'a SearchLimits,
    coverage: Coverage,
}

#[derive(Debug, Serialize)]
pub struct SearchResults {
    pub interpretations: Vec<String>,
    pub hits: Vec<SearchHit>,
    pub remote_pending: bool,
    pub failures: Vec<LookupFailure>,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LookupFailure {
    pub lookup: &'static str,
    pub code: &'static str,
    pub message: String,
}

struct Outcomes {
    found: Vec<Found>,
    failures: Vec<LookupFailure>,
}

impl<'a> Searcher<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            db: &state.db,
            redis: &state.redis,
            names: &state.names,
            limits: &state.search,
            coverage: state.coverage.current(),
        }
    }

    pub async fn search(&self, q: &str, limit: u8) -> Result<SearchResults, AdminError> {
        let input = Input::parse(q);
        let lookups = input.lookups();
        let context = self.probe_context(limit);
        let results = join_all(
            lookups
                .iter()
                .map(|lookup| self.limited_probe(lookup, &context)),
        )
        .await;
        let Outcomes { found, failures } = collect(&lookups, results)?;
        let remote_pending = remote_pending(&input, &found);
        let hits = rank::rank(found, limit);
        let labels = self.names.cached(&label_request(&hits)).await?;
        Ok(SearchResults {
            interpretations: input.interpretations(),
            hits,
            remote_pending,
            failures,
            labels,
        })
    }

    async fn limited_probe(
        &self,
        lookup: &Lookup,
        context: &ProbeContext<'_>,
    ) -> Result<Vec<Found>, AdminError> {
        let _permit = self.limits.probe().await;
        probe::probe(lookup, context).await
    }

    fn probe_context(&self, limit: u8) -> ProbeContext<'_> {
        ProbeContext {
            db: self.db,
            redis: self.redis,
            coverage: &self.coverage,
            match_limit: i64::from(limit),
        }
    }
}

fn collect(
    lookups: &[Lookup],
    results: Vec<Result<Vec<Found>, AdminError>>,
) -> Result<Outcomes, AdminError> {
    let mut outcomes = Outcomes {
        found: Vec::new(),
        failures: Vec::new(),
    };
    let mut first_error: Option<AdminError> = None;
    for (lookup, result) in lookups.iter().zip(results) {
        match result {
            Ok(found) => outcomes.found.extend(found),
            Err(error) => {
                outcomes.failures.push(LookupFailure {
                    lookup: lookup.label(),
                    code: error.code(),
                    message: error.message().to_string(),
                });
                first_error.get_or_insert_with(move || error.with_context(lookup.label()));
            }
        }
    }
    match first_error {
        Some(error) if outcomes.failures.len() == lookups.len() => {
            Err(error.with_context("every search lookup failed"))
        }
        _ => Ok(outcomes),
    }
}

fn remote_pending(input: &Input, found: &[Found]) -> bool {
    input.remote_name().is_some_and(|name| {
        let lower = name.to_ascii_lowercase();
        !found.iter().any(|found| found.is_current_holder_of(&lower))
    })
}

fn label_request(hits: &[SearchHit]) -> LabelRequest {
    let mut request = LabelRequest::default();
    for hit in hits {
        request.discord.extend(hit.account_id);
        if let EntityRef::Player { uuid } = &hit.entity {
            request.minecraft.push(uuid.simple().to_string());
        }
    }
    request
}
