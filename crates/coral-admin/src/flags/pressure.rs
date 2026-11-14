use chrono::{DateTime, Utc};
use clients::FlagLevel;
use coral_redis::RedisPool;
use sqlx::PgPool;

use super::{Flag, FlagKind, grouped, percent};
use crate::error::AdminError;
use crate::traffic::pressure::{LimitUsage, Limiter, PressuredAccount, pressure};

const FULL: f64 = 1.0;

pub async fn flags(
    pool: &PgPool,
    redis: &RedisPool,
    now: DateTime<Utc>,
) -> Result<Vec<Flag>, AdminError> {
    let pressure = pressure(pool, redis, now).await?;
    Ok(pressure
        .accounts
        .into_iter()
        .filter_map(pressure_flag)
        .collect())
}

fn pressure_flag(account: PressuredAccount) -> Option<Flag> {
    let worst = account.worst()?;
    let level = match worst.ratio() >= FULL {
        true => FlagLevel::Danger,
        false => FlagLevel::Warning,
    };
    let flag = Flag::new(
        FlagKind::Pressure,
        level,
        format!("pressure:{}", account.discord_id),
        summary(worst),
    );
    Some(flag.with_refs(vec![account.account]))
}

fn summary(usage: LimitUsage) -> String {
    format!(
        "At {} of the {} ({} of {} per 5 min)",
        percent(usage.ratio()),
        limiter_label(usage.limiter),
        grouped(usage.used),
        grouped(usage.limit),
    )
}

fn limiter_label(limiter: Limiter) -> &'static str {
    match limiter {
        Limiter::Personal => "personal key limit",
        Limiter::Developer => "developer key limit",
        Limiter::Session => "Starfish session limit",
        Limiter::SessionUuids => "Starfish batch player budget",
        Limiter::Minecraft => "Minecraft session limit",
        Limiter::MinecraftUuids => "Minecraft batch player budget",
        Limiter::KeyUuids => "distinct player budget",
        Limiter::Tagging => "tagging limit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::EntityRef;

    fn account(used: i64, limit: i64) -> PressuredAccount {
        PressuredAccount {
            discord_id: 42,
            account: EntityRef::Account {
                discord_id: 42,
                member_id: Some(7),
            },
            limits: vec![LimitUsage {
                limiter: Limiter::KeyUuids,
                player: None,
                used,
                limit,
            }],
        }
    }

    #[test]
    fn pressed_accounts_warn_and_full_ones_are_dangerous() {
        let near = pressure_flag(account(900, 1_000)).unwrap();
        assert_eq!(near.key, "pressure:42");
        assert_eq!(near.level, FlagLevel::Warning);
        assert_eq!(
            near.summary,
            "At 90% of the distinct player budget (900 of 1,000 per 5 min)"
        );
        let full = pressure_flag(account(1_000, 1_000)).unwrap();
        assert_eq!(full.level, FlagLevel::Danger);
    }
}
