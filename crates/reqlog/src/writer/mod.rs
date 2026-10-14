mod counters;
mod names;
mod store;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use uuid::Uuid;

use crate::entry::LogEntry;
use crate::partitions;
use crate::subjects::{self, Extraction, Subject, SubjectSources};
use crate::{
    CHANNEL_BYTES, CHANNEL_CAPACITY, DROP_REPORT_INTERVAL, FLUSH_INTERVAL, LATE_BATCH_AGE,
    MAX_BATCH, MAX_BATCH_BYTES, RETRY_BACKOFF_MAX, RETRY_BACKOFF_MIN, UNMAPPED_COUNTER_KEY,
    WRITER_GAP_TOLERANCE, WRITER_SEEN_INTERVAL,
};
use counters::{DropCounters, DropReason};
use store::PreparedEntry;

const MISSING_PARTITION: &str = "23514";
const PROGRAM_LIMIT_EXCEEDED: &str = "54000";
const DATA_EXCEPTION_CLASS: &str = "22";
const INTEGRITY_CONSTRAINT_CLASS: &str = "23";

#[derive(Clone)]
pub struct RequestLog {
    sender: mpsc::Sender<LogEntry>,
    backlog: Arc<Backlog>,
}

pub struct LogReceiver {
    receiver: mpsc::Receiver<LogEntry>,
    backlog: Arc<Backlog>,
}

#[derive(Debug, Default)]
struct Backlog {
    queued_bytes: AtomicUsize,
    drops: DropCounters,
}

#[derive(Default)]
struct Batch {
    entries: Vec<LogEntry>,
    bytes: usize,
}

pub struct WriterConfig {
    pub pool: PgPool,
    pub redis: ConnectionManager,
    pub sources: SubjectSources,
    pub partitions_ready: watch::Receiver<bool>,
}

struct Writer {
    pool: PgPool,
    redis: ConnectionManager,
    sources: SubjectSources,
    backlog: Arc<Backlog>,
    started: DateTime<Utc>,
    attribution_recorded: bool,
    writer_seen_at: Option<Instant>,
}

struct WriteFailure {
    error: sqlx::Error,
    during_commit: bool,
}

enum Failure {
    RowSpecific,
    MissingPartition,
    Transient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartitionRepair {
    Pending,
    Done,
}

struct Backoff {
    delay: Duration,
}

pub fn channel() -> (RequestLog, LogReceiver) {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let backlog = Arc::new(Backlog::default());
    (
        RequestLog {
            sender,
            backlog: backlog.clone(),
        },
        LogReceiver { receiver, backlog },
    )
}

pub fn spawn_writer(config: WriterConfig, log: LogReceiver) -> JoinHandle<()> {
    spawn_drop_reporter(log.backlog.clone(), config.redis.clone());
    let writer = Writer {
        pool: config.pool,
        redis: config.redis,
        sources: config.sources,
        backlog: log.backlog.clone(),
        started: crate::now(),
        attribution_recorded: false,
        writer_seen_at: None,
    };
    tokio::spawn(writer.run(log, config.partitions_ready))
}

impl RequestLog {
    pub fn submit(&self, entry: LogEntry) {
        let bytes = entry.text_bytes();
        if !self.backlog.reserve(bytes) {
            return self.backlog.drops.count(DropReason::Overflow, 1);
        }
        let reason = match self.sender.try_send(entry) {
            Ok(()) => return,
            Err(TrySendError::Full(_)) => DropReason::Overflow,
            Err(TrySendError::Closed(_)) => DropReason::Closed,
        };
        self.backlog.release(bytes);
        self.backlog.drops.count(reason, 1);
    }
}

impl LogReceiver {
    pub async fn recv(&mut self) -> Option<LogEntry> {
        let entry = self.receiver.recv().await?;
        self.backlog.release(entry.text_bytes());
        Some(entry)
    }
}

impl Backlog {
    fn reserve(&self, bytes: usize) -> bool {
        self.queued_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |queued| {
                queued
                    .checked_add(bytes)
                    .filter(|total| *total <= CHANNEL_BYTES)
            })
            .is_ok()
    }

    fn release(&self, bytes: usize) {
        self.queued_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }
}

impl Batch {
    fn push(&mut self, entry: LogEntry) {
        self.bytes += entry.text_bytes();
        self.entries.push(entry);
    }

    fn is_full(&self) -> bool {
        self.entries.len() >= MAX_BATCH || self.bytes >= MAX_BATCH_BYTES
    }
}

impl Writer {
    async fn run(mut self, mut log: LogReceiver, mut partitions_ready: watch::Receiver<bool>) {
        if partitions_ready.wait_for(|ready| *ready).await.is_err() {
            tracing::error!("partition maintenance stopped before the first creation pass");
        }
        self.record_startup_gaps().await;
        while let Some(batch) = next_batch(&mut log).await {
            let prepared = self.prepare(batch.entries).await;
            self.persist(&prepared, &mut PartitionRepair::Pending).await;
        }
        self.backlog.drops.report(&mut self.redis).await;
    }

    async fn record_startup_gaps(&self) {
        let result = async {
            let mut conn = self.pool.acquire().await?;
            store::record_startup_gaps(&mut conn, WRITER_GAP_TOLERANCE, self.sources, self.started)
                .await
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(error = %e, "request log coverage gaps not recorded");
        }
    }

    async fn prepare(&mut self, batch: Vec<LogEntry>) -> Vec<PreparedEntry> {
        let extractions: Vec<Extraction> = batch
            .iter()
            .map(|entry| subjects::extract(&entry.request_parts(), self.sources))
            .collect();
        self.count_unmapped(&batch, &extractions).await;
        let names = self.resolve_names(&extractions).await;
        let enabled = self.sources.enabled();
        batch
            .into_iter()
            .zip(extractions)
            .map(|(entry, extraction)| PreparedEntry {
                entry,
                subjects: enabled.then(|| resolve_subjects(extraction.subjects, &names)),
            })
            .collect()
    }

    async fn count_unmapped(&mut self, batch: &[LogEntry], extractions: &[Extraction]) {
        let mut counts: HashMap<String, u64> = HashMap::new();
        let unmapped = batch
            .iter()
            .zip(extractions)
            .filter(|(_, extraction)| extraction.unmapped)
            .filter_map(|(entry, _)| Some((entry.method.as_str(), entry.route.as_deref()?)));
        for (method, route) in unmapped {
            *counts
                .entry(counters::daily_field(&format!("{method} {route}")))
                .or_default() += 1;
        }
        if counts.is_empty() {
            return;
        }
        let fields: Vec<(String, u64)> = counts.into_iter().collect();
        if let Err(e) =
            counters::increment_daily(&mut self.redis, UNMAPPED_COUNTER_KEY, &fields).await
        {
            tracing::warn!(error = %e, "unmapped route counts not published");
        }
    }

    async fn resolve_names(&mut self, extractions: &[Extraction]) -> HashMap<String, Uuid> {
        let names: Vec<String> = extractions
            .iter()
            .flat_map(|extraction| &extraction.subjects)
            .filter(|subject| needs_resolution(subject))
            .filter_map(|subject| subject.name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if names.is_empty() {
            return HashMap::new();
        }
        match names::resolve(&mut self.redis, &names).await {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::warn!(error = %e, names = names.len(), "subject names left unresolved");
                HashMap::new()
            }
        }
    }

    async fn persist(&mut self, entries: &[PreparedEntry], repair: &mut PartitionRepair) {
        let Some(first) = entries.first() else {
            return;
        };
        let mut backoff = Backoff::new();
        loop {
            let Err(failure) = self.write(entries).await else {
                return;
            };
            match failure.classify() {
                Failure::MissingPartition if *repair == PartitionRepair::Pending => {
                    *repair = self.repair_partitions(&mut backoff).await;
                }
                Failure::RowSpecific | Failure::MissingPartition => {
                    return self.isolate(entries, failure.error, repair).await;
                }
                Failure::Transient => {
                    tracing::warn!(
                        error = %failure.error,
                        entries = entries.len(),
                        "request log batch failed; retrying"
                    );
                    backoff.wait().await;
                    if failure.during_commit && self.was_committed(&first.entry, &mut backoff).await
                    {
                        return;
                    }
                }
            }
        }
    }

    async fn isolate(
        &mut self,
        entries: &[PreparedEntry],
        error: sqlx::Error,
        repair: &mut PartitionRepair,
    ) {
        if let [prepared] = entries {
            tracing::error!(
                request_id = %prepared.entry.request_id,
                sqlstate = ?sqlstate(&error),
                error = %error,
                "request log row dropped"
            );
            self.backlog.drops.count(DropReason::RowError, 1);
            return;
        }
        let (left, right) = entries.split_at(entries.len() / 2);
        self.persist_boxed(left, repair).await;
        self.persist_boxed(right, repair).await;
    }

    fn persist_boxed<'a>(
        &'a mut self,
        entries: &'a [PreparedEntry],
        repair: &'a mut PartitionRepair,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.persist(entries, repair))
    }

    async fn write(&mut self, entries: &[PreparedEntry]) -> Result<(), WriteFailure> {
        let mut tx = self.pool.begin().await?;
        store::limit_statement_time(&mut tx).await?;
        store::insert_log(&mut tx, entries).await?;
        store::insert_detail(&mut tx, entries).await?;
        store::insert_subjects(&mut tx, entries).await?;
        if is_late(entries) {
            store::mark_late_rows(&mut tx, entries).await?;
        }
        let seen_due = self.writer_seen_due();
        if seen_due {
            store::touch_writer_seen(&mut tx, self.sources.gap_features(), self.started).await?;
        }
        let attribution_since = (!self.attribution_recorded)
            .then(|| earliest(entries))
            .flatten();
        if let Some(since) = attribution_since {
            store::record_attribution_start(&mut tx, since, self.sources).await?;
        }
        tx.commit().await.map_err(WriteFailure::during_commit)?;
        if seen_due {
            self.writer_seen_at = Some(Instant::now());
        }
        if attribution_since.is_some() {
            self.attribution_recorded = true;
        }
        Ok(())
    }

    async fn was_committed(&self, entry: &LogEntry, backoff: &mut Backoff) -> bool {
        loop {
            match self.is_committed(entry).await {
                Ok(committed) => return committed,
                Err(e) => {
                    tracing::warn!(error = %e, "request log commit probe failed; retrying");
                    backoff.wait().await;
                }
            }
        }
    }

    async fn is_committed(&self, entry: &LogEntry) -> Result<bool, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        store::is_committed(&mut conn, entry).await
    }

    async fn repair_partitions(&self, backoff: &mut Backoff) -> PartitionRepair {
        let Err(e) = partitions::create_upcoming(&self.pool).await else {
            return PartitionRepair::Done;
        };
        tracing::warn!(error = %e, "request log partition repair failed; retrying");
        backoff.wait().await;
        PartitionRepair::Pending
    }

    fn writer_seen_due(&self) -> bool {
        self.writer_seen_at
            .is_none_or(|at| at.elapsed() >= WRITER_SEEN_INTERVAL)
    }
}

impl Backoff {
    fn new() -> Self {
        Self {
            delay: RETRY_BACKOFF_MIN,
        }
    }

    async fn wait(&mut self) {
        tokio::time::sleep(self.delay).await;
        self.delay = (self.delay * 2).min(RETRY_BACKOFF_MAX);
    }
}

impl WriteFailure {
    fn during_commit(error: sqlx::Error) -> Self {
        Self {
            error,
            during_commit: true,
        }
    }

    fn classify(&self) -> Failure {
        classify_sqlstate(sqlstate(&self.error).as_deref())
    }
}

impl From<sqlx::Error> for WriteFailure {
    fn from(error: sqlx::Error) -> Self {
        Self {
            error,
            during_commit: false,
        }
    }
}

async fn next_batch(log: &mut LogReceiver) -> Option<Batch> {
    let mut batch = Batch::default();
    batch.push(log.recv().await?);
    let deadline = Instant::now() + FLUSH_INTERVAL;
    while !batch.is_full() {
        match tokio::time::timeout_at(deadline, log.recv()).await {
            Ok(Some(entry)) => batch.push(entry),
            Ok(None) | Err(_) => break,
        }
    }
    Some(batch)
}

fn resolve_subjects(subjects: Vec<Subject>, names: &HashMap<String, Uuid>) -> Vec<Subject> {
    subjects
        .into_iter()
        .map(|mut subject| {
            if needs_resolution(&subject) {
                subject.player = subject
                    .name
                    .as_ref()
                    .and_then(|name| names.get(name))
                    .copied();
            }
            subject
        })
        .collect()
}

fn needs_resolution(subject: &Subject) -> bool {
    subject.player.is_none() && subject.form.resolves_by_name()
}

fn earliest(entries: &[PreparedEntry]) -> Option<DateTime<Utc>> {
    entries.iter().map(|prepared| prepared.entry.ts).min()
}

fn is_late(entries: &[PreparedEntry]) -> bool {
    let cutoff: DateTime<Utc> = (SystemTime::now() - LATE_BATCH_AGE).into();
    entries.iter().any(|prepared| prepared.entry.ts < cutoff)
}

fn classify_sqlstate(code: Option<&str>) -> Failure {
    match code {
        Some(MISSING_PARTITION) => Failure::MissingPartition,
        Some(PROGRAM_LIMIT_EXCEEDED) => Failure::RowSpecific,
        Some(code)
            if code.starts_with(DATA_EXCEPTION_CLASS)
                || code.starts_with(INTEGRITY_CONSTRAINT_CLASS) =>
        {
            Failure::RowSpecific
        }
        _ => Failure::Transient,
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|code| code.into_owned())
}

fn spawn_drop_reporter(backlog: Arc<Backlog>, mut redis: ConnectionManager) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(DROP_REPORT_INTERVAL);
        loop {
            ticker.tick().await;
            backlog.drops.report(&mut redis).await;
        }
    });
}

#[cfg(test)]
mod tests;
