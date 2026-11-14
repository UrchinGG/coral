use chrono::{DateTime, TimeDelta, Utc};
use coral_redis::{
    KEY_DISTINCT_UUID_BUDGET, LIMIT_WINDOW_SECS, PERSONAL_RATE_LIMIT, RateLimiter, RedisPool,
    SESSION_RATE_LIMIT, SESSION_UUID_BUDGET, tag_limit_for_access,
};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::{serde_id, serde_ts};

pub const PRESSURE_THRESHOLD: f64 = 0.8;
pub const CANDIDATE_WINDOW: TimeDelta = TimeDelta::seconds(LIMIT_WINDOW_SECS);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Limiter {
    Personal,
    Developer,
    Session,
    SessionUuids,
    Minecraft,
    MinecraftUuids,
    KeyUuids,
    Tagging,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pressure {
    #[serde(serialize_with = "serde_ts::micros")]
    pub generated_at: DateTime<Utc>,
    pub threshold: f64,
    pub window_s: i64,
    pub candidates: usize,
    pub accounts: Vec<PressuredAccount>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PressuredAccount {
    #[serde(skip)]
    pub discord_id: i64,
    pub account: EntityRef,
    pub limits: Vec<LimitUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LimitUsage {
    pub limiter: Limiter,
    #[serde(serialize_with = "serde_id::uuid_undashed_opt")]
    pub player: Option<Uuid>,
    pub used: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
struct Candidate {
    discord_id: i64,
    member_id: Option<i64>,
    api_key: Option<String>,
    access_level: Option<i16>,
    developer_key: Option<String>,
    developer_limit: Option<i32>,
    minecraft: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownLimit {
    limiter: Limiter,
    name: String,
    limit: i64,
    player: Option<Uuid>,
}

impl LimitUsage {
    pub fn ratio(self) -> f64 {
        self.used as f64 / self.limit as f64
    }

    pub fn pressing(self) -> bool {
        self.ratio() >= PRESSURE_THRESHOLD
    }
}

impl PressuredAccount {
    pub fn worst(&self) -> Option<LimitUsage> {
        self.limits.first().copied()
    }
}

impl Candidate {
    fn account(&self) -> EntityRef {
        EntityRef::Account {
            discord_id: self.discord_id,
            member_id: self.member_id,
        }
    }

    fn limits(&self) -> Vec<KnownLimit> {
        let discord_id = self.discord_id;
        let keyed = [
            self.api_key
                .clone()
                .map(|name| known(Limiter::Personal, name, PERSONAL_RATE_LIMIT)),
            self.developer_key
                .clone()
                .zip(self.developer_limit)
                .map(|(name, limit)| known(Limiter::Developer, name, i64::from(limit))),
            self.access_level
                .map(tag_limit_for_access)
                .filter(|limit| *limit > 0)
                .map(|limit| known(Limiter::Tagging, format!("tag:{discord_id}"), limit)),
        ];
        let by_account = [
            known(
                Limiter::Session,
                format!("sf:{discord_id}"),
                SESSION_RATE_LIMIT,
            ),
            known(
                Limiter::SessionUuids,
                format!("sfuuids:{discord_id}"),
                SESSION_UUID_BUDGET,
            ),
            known(
                Limiter::KeyUuids,
                format!("keyuuids:{discord_id}"),
                KEY_DISTINCT_UUID_BUDGET,
            ),
        ];
        let minecraft = self
            .minecraft
            .iter()
            .filter_map(|uuid| Uuid::try_parse(uuid).ok())
            .flat_map(minecraft_limits);
        keyed
            .into_iter()
            .flatten()
            .chain(by_account)
            .chain(minecraft)
            .collect()
    }
}

pub async fn pressure(
    pool: &PgPool,
    redis: &RedisPool,
    now: DateTime<Utc>,
) -> Result<Pressure, AdminError> {
    let candidates = candidates(pool, now - CANDIDATE_WINDOW).await?;
    let limits: Vec<Vec<KnownLimit>> = candidates.iter().map(Candidate::limits).collect();
    let names: Vec<String> = limits
        .iter()
        .flatten()
        .map(|known| known.name.clone())
        .collect();
    let used = RateLimiter::new(redis.clone())
        .usage_many(&names)
        .await
        .context("count limiter usage")?;
    Ok(Pressure {
        generated_at: now,
        threshold: PRESSURE_THRESHOLD,
        window_s: LIMIT_WINDOW_SECS,
        candidates: candidates.len(),
        accounts: pressed(&candidates, &limits, &used),
    })
}

async fn candidates(pool: &PgPool, since: DateTime<Utc>) -> Result<Vec<Candidate>, AdminError> {
    sqlx::query_as(
        "WITH recent AS (
             SELECT DISTINCT member_id, actor_id FROM api_request_log
             WHERE ts >= $1 AND (member_id IS NOT NULL OR actor_id IS NOT NULL)
         ), callers AS (
             SELECT m.discord_id FROM recent JOIN members m ON m.id = recent.member_id
             UNION
             SELECT actor_id FROM recent WHERE actor_id IS NOT NULL
         )
         SELECT callers.discord_id, m.id AS member_id, m.api_key, m.access_level,
                d.api_key AS developer_key, d.rate_limit AS developer_limit,
                ARRAY(SELECT m.uuid WHERE m.uuid IS NOT NULL
                      UNION SELECT a.uuid FROM minecraft_accounts a WHERE a.member_id = m.id) AS minecraft
         FROM callers
         LEFT JOIN members m ON m.discord_id = callers.discord_id
         LEFT JOIN developer_keys d ON d.member_id = m.id",
    )
    .bind(since)
    .fetch_all(pool)
    .await
    .context("read the callers of the last five minutes")
}

fn pressed(
    candidates: &[Candidate],
    limits: &[Vec<KnownLimit>],
    used: &[i64],
) -> Vec<PressuredAccount> {
    let mut counts = used.iter().copied();
    let mut accounts: Vec<PressuredAccount> = candidates
        .iter()
        .zip(limits)
        .filter_map(|(candidate, known)| {
            let mut usage: Vec<LimitUsage> = known
                .iter()
                .zip(counts.by_ref())
                .map(|(known, used)| LimitUsage {
                    limiter: known.limiter,
                    player: known.player,
                    used,
                    limit: known.limit,
                })
                .collect();
            usage.sort_by(|a, b| b.ratio().total_cmp(&a.ratio()));
            let account = PressuredAccount {
                discord_id: candidate.discord_id,
                account: candidate.account(),
                limits: usage,
            };
            account
                .worst()
                .is_some_and(LimitUsage::pressing)
                .then_some(account)
        })
        .collect();
    accounts.sort_by(|a, b| worst_ratio(b).total_cmp(&worst_ratio(a)));
    accounts
}

fn worst_ratio(account: &PressuredAccount) -> f64 {
    account.worst().map_or(0.0, LimitUsage::ratio)
}

fn minecraft_limits(player: Uuid) -> [KnownLimit; 2] {
    let uuid = player.simple();
    [
        KnownLimit {
            player: Some(player),
            ..known(Limiter::Minecraft, format!("mc:{uuid}"), SESSION_RATE_LIMIT)
        },
        KnownLimit {
            player: Some(player),
            ..known(
                Limiter::MinecraftUuids,
                format!("mcuuids:{uuid}"),
                SESSION_UUID_BUDGET,
            )
        },
    ]
}

fn known(limiter: Limiter, name: String, limit: i64) -> KnownLimit {
    KnownLimit {
        limiter,
        name,
        limit,
        player: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCORD: i64 = 1_218_840_276_405_911_602;

    fn member() -> Candidate {
        Candidate {
            discord_id: DISCORD,
            member_id: Some(21111),
            api_key: Some("personal-key".into()),
            access_level: Some(0),
            developer_key: Some("developer-key".into()),
            developer_limit: Some(1200),
            minecraft: vec!["2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21".into()],
        }
    }

    fn visitor() -> Candidate {
        Candidate {
            discord_id: 7,
            member_id: None,
            api_key: None,
            access_level: None,
            developer_key: None,
            developer_limit: None,
            minecraft: Vec::new(),
        }
    }

    #[test]
    fn members_carry_their_keys_and_every_account_limit() {
        let names: Vec<(Limiter, String, i64)> = member()
            .limits()
            .into_iter()
            .map(|known| (known.limiter, known.name, known.limit))
            .collect();
        assert_eq!(
            names,
            vec![
                (
                    Limiter::Personal,
                    "personal-key".into(),
                    PERSONAL_RATE_LIMIT
                ),
                (Limiter::Developer, "developer-key".into(), 1200),
                (
                    Limiter::Tagging,
                    format!("tag:{DISCORD}"),
                    tag_limit_for_access(0)
                ),
                (
                    Limiter::Session,
                    format!("sf:{DISCORD}"),
                    SESSION_RATE_LIMIT
                ),
                (
                    Limiter::SessionUuids,
                    format!("sfuuids:{DISCORD}"),
                    SESSION_UUID_BUDGET
                ),
                (
                    Limiter::KeyUuids,
                    format!("keyuuids:{DISCORD}"),
                    KEY_DISTINCT_UUID_BUDGET
                ),
                (
                    Limiter::Minecraft,
                    "mc:2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21".into(),
                    SESSION_RATE_LIMIT
                ),
                (
                    Limiter::MinecraftUuids,
                    "mcuuids:2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21".into(),
                    SESSION_UUID_BUDGET
                ),
            ]
        );
    }

    #[test]
    fn accounts_without_a_member_row_only_have_account_limits() {
        let limiters: Vec<Limiter> = visitor()
            .limits()
            .into_iter()
            .map(|known| known.limiter)
            .collect();
        assert_eq!(
            limiters,
            vec![Limiter::Session, Limiter::SessionUuids, Limiter::KeyUuids]
        );
    }

    #[test]
    fn only_accounts_at_eighty_percent_of_a_limit_are_pressed_and_worst_comes_first() {
        let candidates = vec![visitor(), member()];
        let limits: Vec<Vec<KnownLimit>> = candidates.iter().map(Candidate::limits).collect();
        let quiet = [239, 0, 0];
        let busy = [598, 0, 0, 0, 0, 900, 0, 0];
        let used: Vec<i64> = quiet.into_iter().chain(busy).collect();
        let accounts = pressed(&candidates, &limits, &used);
        assert_eq!(accounts.len(), 1);
        let worst = accounts[0].worst().unwrap();
        assert_eq!(worst.limiter, Limiter::Personal);
        assert_eq!(worst.used, 598);
        assert_eq!(accounts[0].limits[1].limiter, Limiter::KeyUuids);
        assert!(!pressed(&candidates, &limits, &[240, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_empty());
    }
}
