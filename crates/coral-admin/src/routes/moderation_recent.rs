use axum::Json;
use axum::extract::State;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::state::AppState;

pub const MODERATION_WINDOW: TimeDelta = TimeDelta::hours(24);
pub const PLUGIN_WINDOW: TimeDelta = TimeDelta::days(7);
pub const LISTED: i64 = 10;

const TAG_SET: &str = "tag_set";
const TAG_CLEAR: &str = "tag_clear";
const LOCK: &str = "lock";
const UNLOCK: &str = "unlock";
const OPEN_REVIEW: &str = "open";

#[derive(Debug, Serialize)]
pub struct RecentModeration {
    #[serde(serialize_with = "serde_ts::micros")]
    from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    to: DateTime<Utc>,
    tags: TagActivity,
    locks: LockActivity,
    strikes: StrikeActivity,
    reviews: ReviewQueue,
    plugins: PluginActivity,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
struct TagActivity {
    added: i64,
    removed: i64,
    by_type: Vec<TagTypeCount>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct TagTypeCount {
    tag_type: Option<String>,
    count: i64,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
struct LockActivity {
    locked: i64,
    unlocked: i64,
    players: Vec<EntityRef>,
}

#[derive(Debug, Serialize)]
struct StrikeActivity {
    issued: i64,
    accounts: Vec<EntityRef>,
}

#[derive(Debug, Serialize, FromRow)]
struct ReviewQueue {
    open: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    oldest_open: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct PluginActivity {
    #[serde(serialize_with = "serde_ts::micros")]
    since: DateTime<Utc>,
    changes: Vec<PluginChange>,
}

#[derive(Debug, Serialize)]
struct PluginChange {
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    plugin: EntityRef,
    change: PluginChangeKind,
    version: Option<String>,
    by: Option<EntityRef>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PluginChangeKind {
    Published,
    Yanked,
    Disabled,
    Unlisted,
}

#[derive(Debug, FromRow)]
struct TagEventCount {
    kind: String,
    tag_type: Option<String>,
    events: i64,
}

#[derive(Debug, FromRow)]
struct StrikeRecord {
    issued: i64,
    discord_id: i64,
    member_id: i64,
}

#[derive(Debug, FromRow)]
struct PluginChangeRecord {
    at: DateTime<Utc>,
    slug: String,
    display_name: String,
    change: String,
    version: Option<String>,
    by_discord_id: Option<i64>,
    reason: Option<String>,
}

pub async fn recent_moderation(
    State(state): State<AppState>,
) -> Result<Json<RecentModeration>, AdminError> {
    let pool = state.db.pool();
    let to = Utc::now();
    let from = to - MODERATION_WINDOW;
    let plugins_since = to - PLUGIN_WINDOW;
    let (tag_events, locked, strikes, reviews, plugin_changes) = tokio::try_join!(
        tag_events(pool, from),
        locked_players(pool, from),
        strikes(pool, from),
        reviews(pool),
        plugin_changes(pool, plugins_since),
    )?;
    let (tags, mut locks) = tag_activity(tag_events);
    locks.players = locked;
    Ok(Json(RecentModeration {
        from,
        to,
        tags,
        locks,
        strikes,
        reviews,
        plugins: PluginActivity {
            since: plugins_since,
            changes: plugin_changes,
        },
    }))
}

async fn tag_events(pool: &PgPool, from: DateTime<Utc>) -> Result<Vec<TagEventCount>, AdminError> {
    sqlx::query_as(
        "SELECT kind, tag_type, count(*)::bigint AS events FROM player_events
         WHERE ts >= $1 GROUP BY kind, tag_type",
    )
    .bind(from)
    .fetch_all(pool)
    .await
    .context("count recent tag and lock events")
}

async fn locked_players(pool: &PgPool, from: DateTime<Utc>) -> Result<Vec<EntityRef>, AdminError> {
    let uuids: Vec<String> = sqlx::query_scalar(
        "SELECT uuid FROM player_events WHERE ts >= $1 AND kind = $2
         GROUP BY uuid ORDER BY max(ts) DESC LIMIT $3",
    )
    .bind(from)
    .bind(LOCK)
    .bind(LISTED)
    .fetch_all(pool)
    .await
    .context("read recently locked players")?;
    uuids.iter().map(|uuid| player_ref(uuid)).collect()
}

async fn strikes(pool: &PgPool, from: DateTime<Utc>) -> Result<StrikeActivity, AdminError> {
    let records: Vec<StrikeRecord> = sqlx::query_as(
        "SELECT count(*) OVER ()::bigint AS issued, m.discord_id, s.member_id
         FROM member_strike s JOIN members m ON m.id = s.member_id
         WHERE s.issued_at >= $1 ORDER BY s.issued_at DESC LIMIT $2",
    )
    .bind(from)
    .bind(LISTED)
    .fetch_all(pool)
    .await
    .context("read recent strikes")?;
    Ok(StrikeActivity {
        issued: records.first().map_or(0, |record| record.issued),
        accounts: records
            .into_iter()
            .map(|record| EntityRef::Account {
                discord_id: record.discord_id,
                member_id: Some(record.member_id),
            })
            .collect(),
    })
}

async fn reviews(pool: &PgPool) -> Result<ReviewQueue, AdminError> {
    sqlx::query_as(
        "SELECT count(*)::bigint AS open, min(created_at) AS oldest_open
         FROM review WHERE status = $1",
    )
    .bind(OPEN_REVIEW)
    .fetch_one(pool)
    .await
    .context("count open reviews")
}

async fn plugin_changes(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<PluginChange>, AdminError> {
    let records: Vec<PluginChangeRecord> = sqlx::query_as(
        "SELECT * FROM (
             SELECT r.created_at AS at, p.slug, p.display_name, 'published' AS change,
                    r.version, u.discord_id AS by_discord_id, NULL::text AS reason
             FROM plugin_releases r
             JOIN plugins p ON p.id = r.plugin_id
             JOIN starfish_users u ON u.id = p.owner_user_id
             WHERE r.created_at >= $1
             UNION ALL
             SELECT r.yanked_at, p.slug, p.display_name, 'yanked', r.version, NULL, r.yanked_reason
             FROM plugin_releases r JOIN plugins p ON p.id = r.plugin_id
             WHERE r.yanked AND r.yanked_at >= $1
             UNION ALL
             SELECT p.disabled_at, p.slug, p.display_name, 'disabled', NULL, NULL, p.disabled_reason
             FROM plugins p WHERE p.disabled AND p.disabled_at >= $1
             UNION ALL
             SELECT p.unlisted_at, p.slug, p.display_name, 'unlisted', NULL, NULL, NULL
             FROM plugins p WHERE p.unlisted AND p.unlisted_at >= $1
         ) changes ORDER BY at DESC LIMIT $2",
    )
    .bind(since)
    .bind(LISTED)
    .fetch_all(pool)
    .await
    .context("read recent plugin changes")?;
    records.into_iter().map(plugin_change).collect()
}

fn tag_activity(events: Vec<TagEventCount>) -> (TagActivity, LockActivity) {
    let mut tags = TagActivity::default();
    let mut locks = LockActivity::default();
    for event in events {
        match event.kind.as_str() {
            TAG_SET => {
                tags.added += event.events;
                tags.by_type.push(TagTypeCount {
                    tag_type: event.tag_type,
                    count: event.events,
                });
            }
            TAG_CLEAR => tags.removed += event.events,
            LOCK => locks.locked += event.events,
            UNLOCK => locks.unlocked += event.events,
            other => tracing::warn!(kind = other, "a player event has an unknown kind"),
        }
    }
    tags.by_type
        .sort_by(|a, b| b.count.cmp(&a.count).then(a.tag_type.cmp(&b.tag_type)));
    (tags, locks)
}

fn plugin_change(record: PluginChangeRecord) -> Result<PluginChange, AdminError> {
    let change = match record.change.as_str() {
        "published" => PluginChangeKind::Published,
        "yanked" => PluginChangeKind::Yanked,
        "disabled" => PluginChangeKind::Disabled,
        "unlisted" => PluginChangeKind::Unlisted,
        other => {
            return Err(AdminError::Internal(format!(
                "the plugin change query produced an unknown change {other}"
            )));
        }
    };
    Ok(PluginChange {
        at: record.at,
        plugin: EntityRef::Plugin {
            slug: record.slug,
            name: Some(record.display_name),
        },
        change,
        version: record.version,
        by: record.by_discord_id.map(|discord_id| EntityRef::Account {
            discord_id,
            member_id: None,
        }),
        reason: record.reason,
    })
}

fn player_ref(uuid: &str) -> Result<EntityRef, AdminError> {
    Uuid::try_parse(uuid)
        .map(|uuid| EntityRef::Player { uuid })
        .map_err(|_| {
            AdminError::Internal(format!("player_events holds {uuid:?}, which is not a UUID"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, tag_type: Option<&str>, events: i64) -> TagEventCount {
        TagEventCount {
            kind: kind.into(),
            tag_type: tag_type.map(str::to_string),
            events,
        }
    }

    #[test]
    fn events_split_into_tag_and_lock_activity() {
        let (tags, locks) = tag_activity(vec![
            event(TAG_SET, Some("sniper"), 6),
            event(TAG_SET, Some("closet_cheater"), 5),
            event(TAG_SET, Some("caution"), 6),
            event(TAG_CLEAR, Some("sniper"), 3),
            event(LOCK, None, 1),
            event(UNLOCK, None, 2),
        ]);
        assert_eq!(tags.added, 17);
        assert_eq!(tags.removed, 3);
        assert_eq!(
            tags.by_type
                .iter()
                .map(|count| count.tag_type.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["caution", "sniper", "closet_cheater"]
        );
        assert_eq!((locks.locked, locks.unlocked), (1, 2));
    }

    #[test]
    fn plugin_changes_name_the_plugin_and_reject_unknown_kinds() {
        let record = |change: &str| PluginChangeRecord {
            at: Utc::now(),
            slug: "party-scan".into(),
            display_name: "Party Scan".into(),
            change: change.into(),
            version: Some("1.4.0".into()),
            by_discord_id: Some(42),
            reason: None,
        };
        let published = plugin_change(record("published")).unwrap();
        assert_eq!(published.change, PluginChangeKind::Published);
        assert_eq!(
            published.plugin,
            EntityRef::Plugin {
                slug: "party-scan".into(),
                name: Some("Party Scan".into())
            }
        );
        assert_eq!(
            plugin_change(record("renamed")).unwrap_err().code(),
            "internal"
        );
    }

    #[test]
    fn stored_player_ids_must_be_uuids() {
        assert!(player_ref("94589c87999b4a7ab3add2ae50711e64").is_ok());
        assert_eq!(player_ref("nope").unwrap_err().code(), "internal");
    }
}
