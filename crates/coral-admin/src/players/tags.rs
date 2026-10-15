use std::collections::HashMap;

use chrono::{DateTime, Utc};
use database::{AddOutcome, BlacklistRepository, Database};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use super::accounts::MemberIds;
use crate::audit;
use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;
use crate::state::AppState;

const TAG_SET: &str = "tag_set";
const TAG_CLEAR: &str = "tag_clear";
const LOCK: &str = "lock";
const UNLOCK: &str = "unlock";
const HEX_COLOR_DIGITS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagView {
    pub id: i64,
    pub tag_type: String,
    pub display_name: String,
    pub color: Option<String>,
    pub reason: Option<String>,
    pub author: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub expires_at: Option<DateTime<Utc>>,
    pub hide_username: bool,
    pub reviewed_by: Vec<EntityRef>,
    pub displaced: Option<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagLabel {
    pub tag_type: String,
    pub display_name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagRecord {
    #[serde(flatten)]
    pub tag: TagView,
    pub ended: Option<TagEnding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagEnding {
    pub how: EndedBy,
    pub by: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndedBy {
    Cleared,
    Replaced,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockEvent {
    pub locked: bool,
    pub by: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
    pub reason: Option<String>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockView {
    pub by: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blacklist {
    pub active: Vec<TagView>,
    pub locked: Option<LockView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagType {
    pub name: &'static str,
    pub display_name: &'static str,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerTags {
    pub active: Vec<TagView>,
    pub history: Vec<TagRecord>,
    pub locks: Vec<LockEvent>,
    pub locked: Option<LockView>,
    pub tag_types: Vec<TagType>,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEvents {
    pub history: Vec<TagRecord>,
    pub locks: Vec<LockEvent>,
    pub mentioned: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTag {
    tag_type: String,
    reason: String,
    #[serde(default)]
    hide_username: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewLock {
    reason: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct EventRow {
    id: i64,
    kind: String,
    tag_type: Option<String>,
    reason: Option<String>,
    hide_username: Option<bool>,
    expires_at: Option<DateTime<Utc>>,
    reviewed_by: Option<Vec<i64>>,
    author: Option<i64>,
    displaced_author: Option<i64>,
    ts: DateTime<Utc>,
}

impl PlayerEvents {
    pub async fn read(db: &Database, player: Uuid) -> Result<Self, AdminError> {
        let rows: Vec<EventRow> = sqlx::query_as(
            "SELECT id, kind, tag_type, reason, hide_username, expires_at, reviewed_by, author,
                    displaced_author, ts
             FROM player_events WHERE uuid = $1 ORDER BY ts, id",
        )
        .bind(player.simple().to_string())
        .fetch_all(db.pool())
        .await
        .context("read the player's tag and lock events")?;
        let mentioned = mentioned_accounts(&rows);
        let members = MemberIds::load(db, &mentioned).await?;
        Ok(Self {
            history: tag_history(&rows, &members, Utc::now()),
            locks: lock_events(&rows, &members),
            mentioned,
        })
    }

    pub fn active(&self) -> Vec<TagView> {
        self.history
            .iter()
            .filter(|record| record.ended.is_none())
            .map(|record| record.tag.clone())
            .collect()
    }

    pub fn locked(&self) -> Option<LockView> {
        let latest = self.locks.first()?;
        let still_locked =
            latest.locked && latest.expires_at.is_none_or(|expires| expires > Utc::now());
        still_locked.then(|| LockView {
            by: latest.by.clone(),
            at: latest.at,
            reason: latest.reason.clone(),
        })
    }

    pub fn blacklist(&self) -> Blacklist {
        Blacklist {
            active: self.active(),
            locked: self.locked(),
        }
    }
}

impl TagLabel {
    pub fn of(tag_type: String) -> Self {
        let definition = blacklist::lookup(&tag_type);
        Self {
            display_name: definition
                .map_or_else(|| tag_type.clone(), |tag| tag.display_name.into()),
            color: definition.map(|tag| hex_color(tag.color)),
            tag_type,
        }
    }
}

pub async fn read_tags(state: &AppState, player: Uuid) -> Result<PlayerTags, AdminError> {
    let events = PlayerEvents::read(&state.db, player).await?;
    let labels = state
        .names
        .cached(&LabelRequest {
            discord: events.mentioned.clone(),
            minecraft: Vec::new(),
        })
        .await?;
    Ok(PlayerTags {
        active: events.active(),
        locked: events.locked(),
        history: events.history,
        locks: events.locks,
        tag_types: tag_types(),
        labels,
    })
}

pub fn tag_types() -> Vec<TagType> {
    blacklist::all()
        .iter()
        .map(|tag| TagType {
            name: tag.name,
            display_name: tag.display_name,
            color: hex_color(tag.color),
        })
        .collect()
}

pub async fn add_tag(
    db: &Database,
    actor: i64,
    player: Uuid,
    tag: NewTag,
) -> Result<(), AdminError> {
    let reason = tag.reason.trim();
    if blacklist::lookup(&tag.tag_type).is_none() {
        return Err(AdminError::BadRequest(format!(
            "{:?} is not a tag type",
            tag.tag_type
        )));
    }
    if reason.is_empty() {
        return Err(AdminError::BadRequest("a tag needs a reason".into()));
    }
    let uuid = player.simple().to_string();
    let outcome = BlacklistRepository::new(db.pool())
        .add_event(
            &uuid,
            &tag.tag_type,
            reason,
            tag.hide_username,
            None,
            None,
            Some(actor),
            &[],
        )
        .await
        .context("add the tag")?;
    if let AddOutcome::Conflict(active) = outcome {
        return Err(AdminError::Rejected(format!(
            "an active {} tag blocks this tag",
            active.tag_type.unwrap_or_default()
        )));
    }
    let details = json!({ "tag_type": tag.tag_type, "reason": reason });
    audit::record(db.pool(), actor, "add_tag", &uuid, &details).await;
    Ok(())
}

pub async fn remove_tag(
    db: &Database,
    actor: i64,
    player: Uuid,
    tag_type: &str,
) -> Result<(), AdminError> {
    let uuid = player.simple().to_string();
    let removed = BlacklistRepository::new(db.pool())
        .remove_event(&uuid, tag_type, Some(actor))
        .await
        .context("remove the tag")?;
    if !removed {
        return Err(AdminError::NotFound(format!(
            "the player has no active {tag_type:?} tag"
        )));
    }
    let details = json!({ "tag_type": tag_type });
    audit::record(db.pool(), actor, "remove_tag", &uuid, &details).await;
    Ok(())
}

pub async fn lock(
    db: &Database,
    actor: i64,
    player: Uuid,
    lock: NewLock,
) -> Result<(), AdminError> {
    let uuid = player.simple().to_string();
    let reason = lock
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty());
    BlacklistRepository::new(db.pool())
        .lock_event(&uuid, reason, actor, None)
        .await
        .context("lock the player")?;
    let details = json!({ "reason": reason });
    audit::record(db.pool(), actor, "lock_player", &uuid, &details).await;
    Ok(())
}

pub async fn unlock(db: &Database, actor: i64, player: Uuid) -> Result<(), AdminError> {
    let uuid = player.simple().to_string();
    let unlocked = BlacklistRepository::new(db.pool())
        .unlock_event(&uuid, actor)
        .await
        .context("unlock the player")?;
    if !unlocked {
        return Err(AdminError::Rejected("the player is not locked".into()));
    }
    audit::record(db.pool(), actor, "unlock_player", &uuid, &json!({})).await;
    Ok(())
}

fn tag_history(rows: &[EventRow], members: &MemberIds, now: DateTime<Utc>) -> Vec<TagRecord> {
    let tag_events: Vec<&EventRow> = rows
        .iter()
        .filter(|row| row.kind == TAG_SET || row.kind == TAG_CLEAR)
        .collect();
    let mut next_of_type: HashMap<&str, &EventRow> = HashMap::new();
    let mut records = Vec::new();
    for row in tag_events.iter().rev() {
        let tag_type = row.tag_type.as_deref().unwrap_or_default();
        let next = next_of_type.insert(tag_type, row);
        if row.kind == TAG_SET {
            records.push(TagRecord {
                tag: tag_view(row, members),
                ended: ending(row, next, members, now),
            });
        }
    }
    records
}

fn ending(
    set: &EventRow,
    next: Option<&EventRow>,
    members: &MemberIds,
    now: DateTime<Utc>,
) -> Option<TagEnding> {
    if let Some(next) = next {
        return Some(TagEnding {
            how: match next.kind == TAG_CLEAR {
                true => EndedBy::Cleared,
                false => EndedBy::Replaced,
            },
            by: next.author.map(|id| members.account(id)),
            at: next.ts,
        });
    }
    let expires = set.expires_at.filter(|expires| *expires <= now)?;
    Some(TagEnding {
        how: EndedBy::Expired,
        by: None,
        at: expires,
    })
}

fn tag_view(row: &EventRow, members: &MemberIds) -> TagView {
    let TagLabel {
        tag_type,
        display_name,
        color,
    } = TagLabel::of(row.tag_type.clone().unwrap_or_default());
    TagView {
        id: row.id,
        tag_type,
        display_name,
        color,
        reason: row.reason.clone(),
        author: row.author.map(|id| members.account(id)),
        at: row.ts,
        expires_at: row.expires_at,
        hide_username: row.hide_username.unwrap_or(false),
        reviewed_by: row
            .reviewed_by
            .iter()
            .flatten()
            .map(|id| members.account(*id))
            .collect(),
        displaced: row.displaced_author.map(|id| members.account(id)),
    }
}

fn lock_events(rows: &[EventRow], members: &MemberIds) -> Vec<LockEvent> {
    rows.iter()
        .rev()
        .filter(|row| row.kind == LOCK || row.kind == UNLOCK)
        .map(|row| LockEvent {
            locked: row.kind == LOCK,
            by: row.author.map(|id| members.account(id)),
            at: row.ts,
            reason: row.reason.clone(),
            expires_at: row.expires_at,
        })
        .collect()
}

fn mentioned_accounts(rows: &[EventRow]) -> Vec<i64> {
    let mut ids: Vec<i64> = rows
        .iter()
        .flat_map(|row| {
            row.author
                .into_iter()
                .chain(row.displaced_author)
                .chain(row.reviewed_by.iter().flatten().copied())
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn hex_color(color: u32) -> String {
    format!("#{color:0width$x}", width = HEX_COLOR_DIGITS)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone};

    use super::*;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 9, 0, 0).unwrap()
    }

    fn event(id: i64, kind: &str, tag_type: Option<&str>, author: i64, day: u32) -> EventRow {
        EventRow {
            id,
            kind: kind.into(),
            tag_type: tag_type.map(str::to_string),
            reason: Some(format!("reason {id}")),
            hide_username: None,
            expires_at: None,
            reviewed_by: None,
            author: Some(author),
            displaced_author: None,
            ts: at(day),
        }
    }

    #[test]
    fn each_tag_ends_at_the_next_event_of_its_type() {
        let rows = [
            event(1, TAG_SET, Some("caution"), 7, 1),
            event(2, TAG_SET, Some("sniper"), 7, 2),
            event(3, TAG_CLEAR, Some("caution"), 9, 3),
            event(4, TAG_SET, Some("sniper"), 8, 4),
        ];
        let history = tag_history(&rows, &MemberIds::default(), at(10));
        let ended: Vec<(i64, Option<EndedBy>)> = history
            .iter()
            .map(|record| (record.tag.id, record.ended.as_ref().map(|end| end.how)))
            .collect();
        assert_eq!(
            ended,
            vec![
                (4, None),
                (2, Some(EndedBy::Replaced)),
                (1, Some(EndedBy::Cleared)),
            ]
        );
        assert_eq!(
            history[2].ended.as_ref().unwrap().by,
            Some(EntityRef::Account {
                discord_id: 9,
                member_id: None
            })
        );
    }

    #[test]
    fn a_tag_past_its_expiry_has_ended() {
        let rows = [EventRow {
            expires_at: Some(at(5)),
            ..event(1, TAG_SET, Some("sniper"), 7, 1)
        }];
        let history = tag_history(&rows, &MemberIds::default(), at(6));
        assert_eq!(history[0].ended.as_ref().unwrap().how, EndedBy::Expired);
        assert_eq!(history[0].ended.as_ref().unwrap().at, at(5));
        let before_expiry = tag_history(&rows, &MemberIds::default(), at(4));
        assert_eq!(before_expiry[0].ended, None);
    }

    #[test]
    fn the_latest_lock_event_decides_whether_the_player_is_locked() {
        let lock = event(1, LOCK, None, 7, 1);
        let unlock = event(2, UNLOCK, None, 7, 2);
        let events = |rows: &[EventRow]| PlayerEvents {
            history: Vec::new(),
            locks: lock_events(rows, &MemberIds::default()),
            mentioned: Vec::new(),
        };
        assert!(events(std::slice::from_ref(&lock)).locked().is_some());
        assert!(events(&[lock.clone(), unlock]).locked().is_none());
        let lapsed = EventRow {
            expires_at: Some(Utc::now() - TimeDelta::minutes(1)),
            ..lock
        };
        assert!(events(&[lapsed]).locked().is_none());
    }

    #[test]
    fn tag_types_carry_their_colour_as_hex() {
        let sniper = tag_types()
            .into_iter()
            .find(|tag| tag.name == "sniper")
            .unwrap();
        assert_eq!(sniper.color, "#ff0000");
        assert_eq!(hex_color(0xC0C0C0), "#c0c0c0");
    }

    async fn tagged_player(db: &Database, player: Uuid) {
        sqlx::query("DELETE FROM player_events WHERE uuid = $1")
            .bind(player.simple().to_string())
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM admin_actions WHERE target = $1")
            .bind(player.simple().to_string())
            .execute(db.pool())
            .await
            .unwrap();
    }

    async fn audited(db: &Database, player: Uuid) -> Vec<String> {
        sqlx::query_scalar("SELECT action FROM admin_actions WHERE target = $1 ORDER BY id")
            .bind(player.simple().to_string())
            .fetch_all(db.pool())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn tag_and_lock_mutations_change_the_events_and_are_audited() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let player = Uuid::parse_str("7300000e000040008000000000000001").unwrap();
        tagged_player(&db, player).await;
        let staff = 300_000_000_000_000_007;
        let sniper = || NewTag {
            tag_type: "sniper".into(),
            reason: "  queue sniped twice  ".into(),
            hide_username: true,
        };

        add_tag(&db, staff, player, sniper()).await.unwrap();
        lock(
            &db,
            staff,
            player,
            NewLock {
                reason: Some(" disputed ".into()),
            },
        )
        .await
        .unwrap();
        let events = PlayerEvents::read(&db, player).await.unwrap();
        let active = events.active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].reason.as_deref(), Some("queue sniped twice"));
        assert!(active[0].hide_username);
        assert_eq!(active[0].color.as_deref(), Some("#ff0000"));
        assert_eq!(events.locked().unwrap().reason.as_deref(), Some("disputed"));
        assert!(events.mentioned.contains(&staff));

        remove_tag(&db, staff, player, "sniper").await.unwrap();
        unlock(&db, staff, player).await.unwrap();
        let events = PlayerEvents::read(&db, player).await.unwrap();
        assert!(events.active().is_empty());
        assert_eq!(
            events.history[0].ended.as_ref().unwrap().how,
            EndedBy::Cleared
        );
        assert!(events.locked().is_none());
        assert_eq!(events.locks.len(), 2);
        assert_eq!(
            audited(&db, player).await,
            ["add_tag", "lock_player", "remove_tag", "unlock_player"]
        );
        tagged_player(&db, player).await;
    }

    #[tokio::test]
    async fn invalid_or_redundant_mutations_are_errors() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let player = Uuid::parse_str("7300000f000040008000000000000001").unwrap();
        tagged_player(&db, player).await;
        let staff = 300_000_000_000_000_007;
        let tag = |tag_type: &str, reason: &str| NewTag {
            tag_type: tag_type.into(),
            reason: reason.into(),
            hide_username: false,
        };
        assert_eq!(
            add_tag(&db, staff, player, tag("not_a_tag", "x"))
                .await
                .unwrap_err()
                .code(),
            "bad_request"
        );
        assert_eq!(
            add_tag(&db, staff, player, tag("sniper", "   "))
                .await
                .unwrap_err()
                .code(),
            "bad_request"
        );
        assert_eq!(
            remove_tag(&db, staff, player, "sniper")
                .await
                .unwrap_err()
                .code(),
            "not_found"
        );
        assert_eq!(
            unlock(&db, staff, player).await.unwrap_err().code(),
            "rejected"
        );
        assert!(audited(&db, player).await.is_empty());
        tagged_player(&db, player).await;
    }
}
