use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};
use clients::FlagLevel;
use database::FlagDismissalRepository;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::state::AppState;

mod extension;
mod pressure;
mod probe;
mod spike;
mod upstream;
mod writer;

pub use spike::SpikeBaselines;

pub const DISMISS_FOR: TimeDelta = TimeDelta::hours(24);
pub const MAX_FLAG_KEY_CHARS: usize = 200;
pub const DAY_KEY_FORMAT: &str = "%Y-%m-%d";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    Upstream,
    Writer,
    Pressure,
    Probe,
    Spike,
    Extension,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flag {
    pub key: String,
    pub kind: FlagKind,
    pub level: FlagLevel,
    pub summary: String,
    pub refs: Vec<EntityRef>,
    pub extension: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlagFailure {
    pub kind: FlagKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flags {
    #[serde(serialize_with = "serde_ts::micros")]
    pub generated_at: DateTime<Utc>,
    pub flags: Vec<Flag>,
    pub failures: Vec<FlagFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dismissal {
    pub key: String,
    #[serde(serialize_with = "serde_ts::micros")]
    pub dismissed_until: DateTime<Utc>,
}

impl Flag {
    pub fn new(kind: FlagKind, level: FlagLevel, key: String, summary: String) -> Self {
        Self {
            key,
            kind,
            level,
            summary,
            refs: Vec::new(),
            extension: None,
        }
    }

    pub fn with_refs(self, refs: Vec<EntityRef>) -> Self {
        Self { refs, ..self }
    }
}

pub async fn collect(state: &AppState, now: DateTime<Utc>) -> Result<Flags, AdminError> {
    let pool = state.db.pool();
    let coverage = state.coverage.current();
    let (spike, probe, pressure, upstream, extension, dismissed) = tokio::join!(
        spike::flags(pool, &state.spikes, &coverage, now),
        probe::flags(pool, &coverage, now),
        pressure::flags(pool, &state.redis, now),
        upstream::flags(pool, &state.redis, &state.upstream_history, now),
        extension::flags(&state.extensions),
        dismissed(pool, now),
    );
    let dismissed = dismissed?;
    let mut flags = Vec::new();
    let mut failures = Vec::new();
    let checks = [
        (FlagKind::Spike, spike),
        (FlagKind::Probe, probe),
        (FlagKind::Pressure, pressure),
        (FlagKind::Upstream, upstream),
        (FlagKind::Writer, Ok(writer::flags(&coverage.writer, now))),
        (FlagKind::Extension, Ok(extension)),
    ];
    for (kind, outcome) in checks {
        match outcome {
            Ok(found) => flags.extend(found),
            Err(error) => failures.push(FlagFailure {
                kind,
                message: error.message().to_string(),
            }),
        }
    }
    flags.retain(|flag| !dismissed.contains(&flag.key));
    flags.sort_by(|a, b| {
        severity(b.level)
            .cmp(&severity(a.level))
            .then(a.kind.cmp(&b.kind))
            .then(a.key.cmp(&b.key))
    });
    Ok(Flags {
        generated_at: now,
        flags,
        failures,
    })
}

pub async fn dismiss(
    pool: &PgPool,
    key: &str,
    now: DateTime<Utc>,
) -> Result<Dismissal, AdminError> {
    validate_key(key)?;
    let until = now + DISMISS_FOR;
    FlagDismissalRepository::new(pool)
        .dismiss(key, until)
        .await
        .context("record the flag dismissal")?;
    Ok(Dismissal {
        key: key.to_string(),
        dismissed_until: until,
    })
}

pub fn day_key(now: DateTime<Utc>) -> String {
    now.format(DAY_KEY_FORMAT).to_string()
}

pub fn grouped(count: i64) -> String {
    let digits = count.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    match count < 0 {
        true => format!("-{out}"),
        false => out,
    }
}

pub fn percent(ratio: f64) -> String {
    format!("{:.0}%", ratio * 100.0)
}

async fn dismissed(pool: &PgPool, now: DateTime<Utc>) -> Result<HashSet<String>, AdminError> {
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT flag_key FROM flag_dismissals WHERE dismissed_until > $1")
            .bind(now)
            .fetch_all(pool)
            .await
            .context("read dismissed flags")?;
    Ok(keys.into_iter().collect())
}

fn validate_key(key: &str) -> Result<(), AdminError> {
    let length = key.chars().count();
    if length == 0 || length > MAX_FLAG_KEY_CHARS {
        return Err(AdminError::BadRequest(format!(
            "a flag key has 1 to {MAX_FLAG_KEY_CHARS} characters"
        )));
    }
    if key.chars().any(char::is_control) {
        return Err(AdminError::BadRequest(
            "a flag key cannot hold control characters".into(),
        ));
    }
    Ok(())
}

fn severity(level: FlagLevel) -> u8 {
    match level {
        FlagLevel::Info => 0,
        FlagLevel::Warning => 1,
        FlagLevel::Danger => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_with_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(5_912), "5,912");
        assert_eq!(grouped(1_730_031), "1,730,031");
        assert_eq!(grouped(-12_000), "-12,000");
    }

    #[test]
    fn keys_are_bounded_printable_text() {
        assert!(validate_key("probe:ip:2001:db8::3").is_ok());
        assert_eq!(validate_key("").unwrap_err().code(), "bad_request");
        let long = "k".repeat(MAX_FLAG_KEY_CHARS + 1);
        assert_eq!(validate_key(&long).unwrap_err().code(), "bad_request");
        assert_eq!(validate_key("a\nb").unwrap_err().code(), "bad_request");
    }

    #[test]
    fn danger_outranks_warning_and_info() {
        assert!(severity(FlagLevel::Danger) > severity(FlagLevel::Warning));
        assert!(severity(FlagLevel::Warning) > severity(FlagLevel::Info));
    }

    #[tokio::test]
    async fn a_dismissal_hides_its_key_for_a_day() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        let key = "test:dismissal:2019-03-01";
        let now = Utc::now();
        let dismissal = dismiss(&pool, key, now).await.unwrap();
        let during = dismissed(&pool, now + DISMISS_FOR - TimeDelta::minutes(1)).await;
        let after = dismissed(&pool, now + DISMISS_FOR).await;
        sqlx::query("DELETE FROM flag_dismissals WHERE flag_key = $1")
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(dismissal.dismissed_until, now + DISMISS_FOR);
        assert!(during.unwrap().contains(key));
        assert!(!after.unwrap().contains(key));
    }

    #[test]
    fn flags_serialize_kind_and_level_by_name() {
        let flag = Flag::new(
            FlagKind::Spike,
            FlagLevel::Warning,
            "spike:1:2026-10-15".into(),
            "summary".into(),
        );
        let value = serde_json::to_value(flag).unwrap();
        assert_eq!(value["kind"], "spike");
        assert_eq!(value["level"], "warning");
        assert_eq!(value["extension"], serde_json::Value::Null);
    }
}
