use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use clients::FlagLevel;

use super::{Flag, FlagKind, day_key, grouped};
use crate::coverage::WriterHealth;

pub const WRITER_LAG_LIMIT_S: f64 = 120.0;
pub const LISTED_COUNTERS: usize = 3;

pub fn flags(writer: &WriterHealth, now: DateTime<Utc>) -> Vec<Flag> {
    let day = day_key(now);
    [
        heartbeat_flag(writer),
        counted_flag(
            &writer.dropped_today,
            format!("writer:dropped:{day}"),
            FlagLevel::Danger,
            "requests were dropped before reaching the request log today",
        ),
        counted_flag(
            &writer.unmapped_today,
            format!("writer:unmapped:{day}"),
            FlagLevel::Warning,
            "requests today matched no route template",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn heartbeat_flag(writer: &WriterHealth) -> Option<Flag> {
    let Some(lag) = writer.lag_s else {
        return Some(Flag::new(
            FlagKind::Writer,
            FlagLevel::Warning,
            "writer:unseen".into(),
            "The request-log writer has not reported a heartbeat".into(),
        ));
    };
    (lag > WRITER_LAG_LIMIT_S).then(|| {
        Flag::new(
            FlagKind::Writer,
            FlagLevel::Danger,
            "writer:lag".into(),
            format!(
                "The request-log writer last reported {} s ago",
                grouped(lag.round() as i64)
            ),
        )
    })
}

fn counted_flag(
    counts: &BTreeMap<String, u64>,
    key: String,
    level: FlagLevel,
    what: &str,
) -> Option<Flag> {
    let total: u64 = counts.values().sum();
    if total == 0 {
        return None;
    }
    let mut largest: Vec<(&String, &u64)> = counts.iter().collect();
    largest.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let listed: Vec<String> = largest
        .iter()
        .take(LISTED_COUNTERS)
        .map(|(name, count)| format!("{name} × {}", grouped(**count as i64)))
        .collect();
    let more = largest.len().saturating_sub(LISTED_COUNTERS);
    let tail = match more {
        0 => String::new(),
        more => format!(" and {more} more"),
    };
    Some(Flag::new(
        FlagKind::Writer,
        level,
        key,
        format!(
            "{} {what}: {}{tail}",
            grouped(total as i64),
            listed.join(", ")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap()
    }

    fn healthy() -> WriterHealth {
        WriterHealth {
            last_seen: Some(now()),
            lag_s: Some(3.0),
            ..WriterHealth::default()
        }
    }

    #[test]
    fn a_healthy_writer_raises_nothing() {
        assert!(flags(&healthy(), now()).is_empty());
    }

    #[test]
    fn a_silent_or_late_writer_is_flagged() {
        let unseen = flags(&WriterHealth::default(), now());
        assert_eq!(unseen[0].key, "writer:unseen");
        let late = WriterHealth {
            lag_s: Some(121.4),
            ..healthy()
        };
        let flags = flags(&late, now());
        assert_eq!(flags[0].key, "writer:lag");
        assert_eq!(
            flags[0].summary,
            "The request-log writer last reported 121 s ago"
        );
    }

    #[test]
    fn counters_list_the_largest_reasons_first() {
        let writer = WriterHealth {
            unmapped_today: BTreeMap::from([
                ("GET /a".to_string(), 1),
                ("GET /b".to_string(), 9),
                ("GET /c".to_string(), 4),
                ("GET /d".to_string(), 2),
            ]),
            ..healthy()
        };
        let flags = flags(&writer, now());
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].key, "writer:unmapped:2026-10-15");
        assert_eq!(
            flags[0].summary,
            "16 requests today matched no route template: GET /b × 9, GET /c × 4, GET /d × 2 and 1 more"
        );
    }
}
