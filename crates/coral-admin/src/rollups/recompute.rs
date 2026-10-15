use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqlog::rollup::{CATCH_UP_CHUNK, Rewritten, Span};
use serde::Serialize;

use super::Rollups;
use crate::error::AdminError;
use crate::jobs::{Job, JobContext};
use crate::serde_ts;

pub const RECOMPUTE_JOB: &str = "recompute";

pub struct RecomputeJob {
    rollups: Rollups,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct RecomputeProgress {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    to: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    done_until: DateTime<Utc>,
    written: i64,
    removed: i64,
}

impl RecomputeJob {
    pub fn new(rollups: Rollups, span: Span) -> Self {
        Self { rollups, span }
    }
}

#[async_trait]
impl Job for RecomputeJob {
    fn name(&self) -> &'static str {
        RECOMPUTE_JOB
    }

    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
        let attribution = self.rollups.attribution().await?;
        let mut progress = RecomputeProgress::starting(self.span);
        for chunk in self.span.chunks(CATCH_UP_CHUNK) {
            let started = Instant::now();
            let rewritten = self.rollups.recompute(chunk, &attribution).await?;
            progress.record(chunk, rewritten);
            ctx.report(progress.encoded()?).await?;
            if chunk.to < self.span.to {
                tokio::time::sleep(started.elapsed()).await;
            }
        }
        Ok(())
    }
}

impl RecomputeProgress {
    fn starting(span: Span) -> Self {
        Self {
            from: span.from,
            to: span.to,
            done_until: span.from,
            written: 0,
            removed: 0,
        }
    }

    fn record(&mut self, chunk: Span, rewritten: Rewritten) {
        self.done_until = chunk.to;
        self.written += rewritten.written;
        self.removed += rewritten.removed;
    }

    fn encoded(&self) -> Result<serde_json::Value, AdminError> {
        serde_json::to_value(self).map_err(|error| {
            AdminError::Internal(format!("encode the recompute progress: {error}"))
        })
    }
}
