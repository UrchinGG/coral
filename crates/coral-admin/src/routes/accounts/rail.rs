use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use coral_redis::{
    KEY_DISTINCT_UUID_BUDGET, PERSONAL_RATE_LIMIT, RateLimiter, SESSION_RATE_LIMIT,
    SESSION_UUID_BUDGET, tag_limit_for_access,
};
use database::{AccountRepository, MemberRepository, ReviewRulesRepository, standing};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::AccountTarget;
use crate::error::{AdminError, Context};
use crate::state::AppState;
use crate::{serde_id, serde_ts};

pub const LIMIT_WINDOW_SECS: i64 = 300;

#[derive(Serialize, FromRow)]
pub struct KeyRecord {
    key_id: i64,
    kind: String,
    key: String,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    created_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    retired_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, FromRow)]
pub struct LinkRecord {
    uuid: String,
    relation: String,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    linked_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    unlinked_at: Option<DateTime<Utc>>,
}

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

#[derive(Debug, Serialize)]
pub struct Budget {
    limiter: Limiter,
    #[serde(serialize_with = "serde_id::uuid_undashed_opt")]
    player: Option<Uuid>,
    used: i64,
    limit: Option<i64>,
    window_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownLimiter {
    pub limiter: Limiter,
    pub name: String,
    pub limit: Option<i64>,
    pub player: Option<Uuid>,
}

#[derive(FromRow)]
struct DeveloperLimit {
    key: String,
    rate_limit: i32,
}

#[derive(Default)]
struct LimiterSource {
    personal_key: Option<String>,
    developer: Option<DeveloperLimit>,
    tag_level: Option<i16>,
    minecraft: Vec<Uuid>,
}

pub async fn keys(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Vec<KeyRecord>>, AdminError> {
    let target = AccountTarget::resolve(&state, &raw).await?;
    let Some(member_id) = target.member_id else {
        return Ok(Json(Vec::new()));
    };
    let keys = sqlx::query_as(
        "SELECT id AS key_id, kind, api_key AS key, created_at, retired_at FROM member_api_keys
         WHERE member_id = $1
         ORDER BY retired_at IS NULL DESC, coalesce(retired_at, created_at) DESC NULLS LAST, id DESC",
    )
    .bind(member_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's key history")?;
    Ok(Json(keys))
}

pub async fn links(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Vec<LinkRecord>>, AdminError> {
    let target = AccountTarget::resolve(&state, &raw).await?;
    let Some(member_id) = target.member_id else {
        return Ok(Json(Vec::new()));
    };
    let links = sqlx::query_as(
        "SELECT uuid, relation, linked_at, unlinked_at FROM member_mc_links
         WHERE member_id = $1
         ORDER BY unlinked_at IS NULL DESC, coalesce(unlinked_at, linked_at) DESC NULLS LAST, id DESC",
    )
    .bind(member_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's Minecraft link history")?;
    Ok(Json(links))
}

pub async fn limits(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Vec<Budget>>, AdminError> {
    let target = AccountTarget::resolve(&state, &raw).await?;
    let known = known_limiters(&state, target).await?;
    let names: Vec<String> = known.iter().map(|limiter| limiter.name.clone()).collect();
    let used = RateLimiter::new(state.redis.clone())
        .usage_many(&names)
        .await
        .context("read the account's limiter usage")?;
    let budgets = known
        .into_iter()
        .zip(used)
        .map(|(known, used)| Budget {
            limiter: known.limiter,
            player: known.player,
            used,
            limit: known.limit,
            window_s: LIMIT_WINDOW_SECS,
        })
        .collect();
    Ok(Json(budgets))
}

pub async fn known_limiters(
    state: &AppState,
    target: AccountTarget,
) -> Result<Vec<KnownLimiter>, AdminError> {
    let pool = state.db.pool();
    let (member, developer, rules, minecraft) = tokio::try_join!(
        async {
            MemberRepository::new(pool)
                .get_by_discord_id(target.discord_id)
                .await
                .context("read the account's member row")
        },
        developer_limit(state, target.member_id),
        async {
            ReviewRulesRepository::new(pool)
                .current()
                .await
                .context("read the review rules")
        },
        async {
            AccountRepository::new(pool)
                .list_uuids(target.discord_id)
                .await
                .context("read the account's Minecraft accounts")
        },
    )?;
    let source = LimiterSource {
        tag_level: member
            .as_ref()
            .map(|member| standing::effective_level(member, &rules)),
        personal_key: member.and_then(|member| member.api_key),
        developer,
        minecraft: minecraft
            .iter()
            .filter_map(|uuid| Uuid::try_parse(uuid).ok())
            .collect(),
    };
    Ok(limiters_for(target.discord_id, source))
}

async fn developer_limit(
    state: &AppState,
    member_id: Option<i64>,
) -> Result<Option<DeveloperLimit>, AdminError> {
    let Some(member_id) = member_id else {
        return Ok(None);
    };
    sqlx::query_as("SELECT api_key AS key, rate_limit FROM developer_keys WHERE member_id = $1")
        .bind(member_id)
        .fetch_optional(state.db.pool())
        .await
        .context("read the developer key's rate limit")
}

fn limiters_for(discord_id: i64, source: LimiterSource) -> Vec<KnownLimiter> {
    let mut known = Vec::new();
    if let Some(key) = source.personal_key {
        known.push(KnownLimiter {
            limiter: Limiter::Personal,
            name: key,
            limit: Some(PERSONAL_RATE_LIMIT),
            player: None,
        });
    }
    if let Some(developer) = source.developer {
        known.push(KnownLimiter {
            limiter: Limiter::Developer,
            name: developer.key,
            limit: Some(i64::from(developer.rate_limit)),
            player: None,
        });
    }
    known.extend([
        KnownLimiter {
            limiter: Limiter::Session,
            name: format!("sf:{discord_id}"),
            limit: Some(SESSION_RATE_LIMIT),
            player: None,
        },
        KnownLimiter {
            limiter: Limiter::SessionUuids,
            name: format!("sfuuids:{discord_id}"),
            limit: Some(SESSION_UUID_BUDGET),
            player: None,
        },
    ]);
    known.extend(source.minecraft.into_iter().flat_map(minecraft_limiters));
    let Some(tag_level) = source.tag_level else {
        return known;
    };
    let tag_limit = tag_limit_for_access(tag_level);
    known.extend([
        KnownLimiter {
            limiter: Limiter::KeyUuids,
            name: format!("keyuuids:{discord_id}"),
            limit: Some(KEY_DISTINCT_UUID_BUDGET),
            player: None,
        },
        KnownLimiter {
            limiter: Limiter::Tagging,
            name: format!("tag:{discord_id}"),
            limit: (tag_limit > 0).then_some(tag_limit),
            player: None,
        },
    ]);
    known
}

fn minecraft_limiters(player: Uuid) -> [KnownLimiter; 2] {
    let uuid = player.simple();
    [
        KnownLimiter {
            limiter: Limiter::Minecraft,
            name: format!("mc:{uuid}"),
            limit: Some(SESSION_RATE_LIMIT),
            player: Some(player),
        },
        KnownLimiter {
            limiter: Limiter::MinecraftUuids,
            name: format!("mcuuids:{uuid}"),
            limit: Some(SESSION_UUID_BUDGET),
            player: Some(player),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: Uuid = Uuid::from_u128(0x2c1f_6a0e_7d4b_4f3a_9e1d_5b8c_0a6f_3e21);

    #[test]
    fn every_key_and_discord_limiter_of_a_member_is_known() {
        let known = limiters_for(
            42,
            LimiterSource {
                personal_key: Some("personal-key".into()),
                developer: Some(DeveloperLimit {
                    key: "developer-key".into(),
                    rate_limit: 900,
                }),
                tag_level: Some(5),
                minecraft: vec![PLAYER],
            },
        );
        let names: Vec<&str> = known.iter().map(|limiter| limiter.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "personal-key",
                "developer-key",
                "sf:42",
                "sfuuids:42",
                "mc:2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21",
                "mcuuids:2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21",
                "keyuuids:42",
                "tag:42"
            ]
        );
        assert_eq!(known[1].limit, Some(900));
        assert_eq!(known[4].player, Some(PLAYER));
        assert_eq!(known[7].limit, None);
    }

    #[test]
    fn the_tagging_limit_follows_the_effective_level() {
        let suspended = limiters_for(
            7,
            LimiterSource {
                tag_level: Some(-1),
                ..LimiterSource::default()
            },
        );
        let tagging = suspended
            .iter()
            .find(|known| known.limiter == Limiter::Tagging)
            .unwrap();
        assert_eq!(tagging.limit, Some(tag_limit_for_access(-1)));
        let granted = limiters_for(
            7,
            LimiterSource {
                tag_level: Some(1),
                ..LimiterSource::default()
            },
        );
        assert_eq!(granted.last().unwrap().limit, Some(tag_limit_for_access(1)));
    }

    #[test]
    fn a_discord_user_without_an_account_has_only_session_limiters() {
        let known = limiters_for(7, LimiterSource::default());
        let limiters: Vec<Limiter> = known.iter().map(|known| known.limiter).collect();
        assert_eq!(limiters, [Limiter::Session, Limiter::SessionUuids]);
    }
}
