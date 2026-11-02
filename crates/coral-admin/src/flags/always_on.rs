use chrono::{DateTime, TimeDelta, Utc};
use clients::FlagLevel;
use reqlog::KeyKind;
use reqlog::rollup::{HOUR, REROLL_WINDOW, floor_to};
use sqlx::{FromRow, PgPool};

use super::{Flag, FlagKind};
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

pub const UNBROKEN_HOURS: i64 = 48;

const KEY_KINDS: [KeyKind; 2] = [KeyKind::Personal, KeyKind::Developer];

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
struct AlwaysOnKey {
    member_id: i64,
    discord_id: i64,
    key_kind: i16,
}

pub async fn flags(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<Flag>, AdminError> {
    let settled_to = floor_to(now - REROLL_WINDOW, HOUR);
    let kinds: Vec<i16> = KEY_KINDS.iter().map(|kind| *kind as i16).collect();
    let keys: Vec<AlwaysOnKey> = sqlx::query_as(
        "SELECT c.member_id, m.discord_id, c.key_kind
         FROM api_caller_hour c JOIN members m ON m.id = c.member_id
         WHERE c.hour >= $1 AND c.hour < $2 AND c.key_kind = ANY($3)
         GROUP BY c.member_id, m.discord_id, c.key_kind
         HAVING count(DISTINCT c.hour) = $4",
    )
    .bind(settled_to - TimeDelta::hours(UNBROKEN_HOURS))
    .bind(settled_to)
    .bind(&kinds)
    .bind(UNBROKEN_HOURS)
    .fetch_all(pool)
    .await
    .context("find keys that made requests every hour")?;
    Ok(keys.into_iter().map(always_on_flag).collect())
}

fn always_on_flag(key: AlwaysOnKey) -> Flag {
    let kind = match key.key_kind {
        kind if kind == KeyKind::Developer as i16 => "Developer",
        _ => "Personal",
    };
    Flag::new(
        FlagKind::AlwaysOn,
        FlagLevel::Warning,
        format!("always_on:{}:{}", kind.to_lowercase(), key.member_id),
        format!(
            "{kind} key made requests in every one of the last {UNBROKEN_HOURS} hours, which no person keeps up"
        ),
    )
    .with_refs(vec![EntityRef::Account {
        discord_id: key.discord_id,
        member_id: Some(key.member_id),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_always_on_key_names_its_kind_and_account() {
        let flag = always_on_flag(AlwaysOnKey {
            member_id: 7,
            discord_id: 42,
            key_kind: KeyKind::Developer as i16,
        });
        assert_eq!(flag.key, "always_on:developer:7");
        assert_eq!(flag.level, FlagLevel::Warning);
        assert_eq!(
            flag.summary,
            "Developer key made requests in every one of the last 48 hours, which no person keeps up"
        );
    }
}
