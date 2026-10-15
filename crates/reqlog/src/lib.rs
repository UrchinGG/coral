use std::time::Duration;

pub mod backfill;
mod context;
mod entry;
pub mod partitions;
pub mod rollup;
mod routes;
mod status;
mod subjects;
#[cfg(test)]
mod test_support;
mod text;
mod writer;

pub use context::{
    Recorded, RequestContext, record_caller, record_cause, record_outcome, record_server_error,
    scope,
};
pub use entry::{
    Caller, Cause, Client, Detail, KeyKind, LogEntry, Outcome, Service, now, request_id_for,
};
pub use routes::{
    MAX_BATCH_UUIDS, Methods, ROUTE_SUBJECTS, ROUTE_TEMPLATES, RouteSubjects, SubjectSource,
    is_static_template, static_templates, template_for_path, template_path_pattern,
};
pub use subjects::{
    Extraction, Form, RequestParts, Subject, SubjectSources, extract, is_player_name, parse_uuid,
};
pub use text::StoredText;
pub use writer::{LogReceiver, RequestLog, WriterConfig, channel, spawn_writer};

pub const CHANNEL_CAPACITY: usize = 10_000;
pub const CHANNEL_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_BATCH: usize = 500;
pub const MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(250);
pub const RETRY_BACKOFF_MIN: Duration = Duration::from_millis(250);
pub const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(8);
pub const WRITER_STATEMENT_TIMEOUT_MS: i64 = 30_000;
pub const LATE_BATCH_AGE: Duration = Duration::from_secs(9 * 60);
pub const WRITER_SEEN_INTERVAL: Duration = Duration::from_secs(60);
pub const WRITER_GAP_TOLERANCE: chrono::TimeDelta = chrono::TimeDelta::minutes(2);
pub const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(30);
pub const SHUTDOWN_DRAIN: Duration = Duration::from_secs(8);
pub const MAX_REQUEST_BODY: usize = 2 * 1024 * 1024;
pub const STORED_BODY_BYTES: usize = 64 * 1024;
pub const KEY_PREFIX_CHARS: usize = 8;
pub const TIME_ORDERED_VERSION: usize = 7;
pub const LOG_RETENTION_DAYS: u64 = 14;
pub const DETAIL_RETENTION_DAYS: u64 = 14;
pub const SUBJECT_RETENTION_DAYS: u64 = 14;
pub const PARTITIONS_AHEAD_DAYS: u64 = 3;
pub const DDL_LOCK_TIMEOUT: &str = "3s";
pub const PARTITION_LOCK_KEY: i64 = 7_210_002;
pub const DROP_LOCK_TIMEOUT: &str = "1s";
pub const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
pub const CREATION_RETRY: Duration = Duration::from_secs(5);
pub const DROPPED_COUNTER_KEY: &str = "reqlog:dropped";
pub const UNMAPPED_COUNTER_KEY: &str = "reqlog:unmapped";
pub const COUNTER_TTL_SECS: i64 = 15 * 86_400;
pub const COUNTER_DAY_FORMAT: &str = "%Y%m%d";
pub const SUBJECT_SOURCES_ENV: &str = "REQLOG_SUBJECT_SOURCES";
