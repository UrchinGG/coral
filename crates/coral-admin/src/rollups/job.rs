use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep_until};

use super::{Outcome, Pass, PassReport, Rollups};
use crate::error::AdminError;
use crate::jobs::{Job, JobContext, JobRegistry};

pub const ROLLUP_JOB: &str = "rollups";
pub const SUPERVISION_INTERVAL: Duration = Duration::from_secs(30);

pub struct RollupJob {
    rollups: Rollups,
}

pub struct Schedule {
    due: BTreeMap<Pass, Instant>,
}

impl RollupJob {
    pub fn new(rollups: Rollups) -> Self {
        Self { rollups }
    }
}

#[async_trait]
impl Job for RollupJob {
    fn name(&self) -> &'static str {
        ROLLUP_JOB
    }

    async fn run(&self, ctx: &JobContext) -> Result<(), AdminError> {
        let _here = self.rollups.claim_here();
        let attribution = self.rollups.attribution().await?;
        let mut schedule = Schedule::starting(Instant::now());
        let mut reports: BTreeMap<Pass, PassReport> = BTreeMap::new();
        loop {
            let (pass, due) = schedule.next();
            sleep_until(due).await;
            let report = self.rollups.run_pass(pass, Utc::now(), &attribution).await;
            schedule.record(pass, report.outcome, Instant::now());
            reports.insert(pass, report);
            let progress = serde_json::to_value(&reports).map_err(|error| {
                AdminError::Internal(format!("encode the rollup progress: {error}"))
            })?;
            ctx.report(progress).await?;
        }
    }
}

impl Schedule {
    pub fn starting(now: Instant) -> Self {
        Self {
            due: Pass::ALL.into_iter().map(|pass| (pass, now)).collect(),
        }
    }

    pub fn next(&self) -> (Pass, Instant) {
        self.due
            .iter()
            .min_by_key(|(pass, due)| (**due, **pass))
            .map(|(pass, due)| (*pass, *due))
            .expect("every pass is scheduled")
    }

    pub fn record(&mut self, pass: Pass, outcome: Outcome, now: Instant) {
        let next = match outcome {
            Outcome::Behind => now,
            Outcome::Done
            | Outcome::Idle
            | Outcome::Locked
            | Outcome::Waiting
            | Outcome::Failed => now + pass.interval(),
        };
        self.due.insert(pass, next);
    }
}

impl Rollups {
    pub fn supervise(self, jobs: JobRegistry) {
        tokio::spawn(async move {
            let mut ticks = interval(SUPERVISION_INTERVAL);
            ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticks.tick().await;
                if let Err(error) = self.keep_running(&jobs).await {
                    tracing::warn!(%error, "rollup job not started; retrying next tick");
                }
            }
        });
    }

    async fn keep_running(&self, jobs: &JobRegistry) -> Result<(), AdminError> {
        if self.is_running_here() || jobs.is_running(ROLLUP_JOB).await? {
            return Ok(());
        }
        match jobs.start(Arc::new(RollupJob::new(self.clone()))).await {
            Ok(_) => {
                tracing::info!("rollup job started");
                Ok(())
            }
            Err(AdminError::Rejected(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_overdue_pass_runs_first_and_ties_follow_pass_order() {
        let start = Instant::now();
        let mut schedule = Schedule::starting(start);
        assert_eq!(schedule.next(), (Pass::Traffic, start));
        schedule.record(Pass::Traffic, Outcome::Done, start);
        assert_eq!(schedule.next(), (Pass::Dirty, start));
    }

    #[test]
    fn a_pass_that_is_behind_runs_again_at_once_and_others_wait_their_interval() {
        let start = Instant::now();
        let mut schedule = Schedule::starting(start);
        for pass in Pass::ALL {
            schedule.record(pass, Outcome::Done, start);
        }
        let later = start + Duration::from_secs(1);
        schedule.record(Pass::Callers, Outcome::Behind, later);
        assert_eq!(schedule.next(), (Pass::Callers, later));

        schedule.record(Pass::Callers, Outcome::Failed, later);
        assert_eq!(
            schedule.next(),
            (Pass::Traffic, start + Pass::Traffic.interval())
        );
    }
}
