use chrono::{DateTime, Utc};
use database::Database;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::accounts::MemberIds;
use super::tags::TagLabel;
use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;
use crate::state::AppState;

pub const RECENTLY_TAGGED: i64 = 15;
pub const RECENT_TAG_CANDIDATES: i64 = 200;

#[derive(Debug, Clone, Serialize)]
pub struct RecentlyTagged {
    pub items: Vec<TaggedPlayer>,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaggedPlayer {
    pub id: i64,
    pub player: EntityRef,
    #[serde(flatten)]
    pub tag: TagLabel,
    pub author: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct TagSet {
    id: i64,
    player: Uuid,
    tag_type: String,
    author: Option<i64>,
    ts: DateTime<Utc>,
}

pub async fn read_recently_tagged(state: &AppState) -> Result<RecentlyTagged, AdminError> {
    let tag_sets = newest_active_tags(&state.db).await?;
    let authors = authors_of(&tag_sets);
    let labels_request = LabelRequest {
        discord: authors.clone(),
        minecraft: tag_sets
            .iter()
            .map(|tag_set| tag_set.player.simple().to_string())
            .collect(),
    };
    let (members, labels) = tokio::try_join!(
        MemberIds::load(&state.db, &authors),
        state.names.cached(&labels_request),
    )?;
    Ok(RecentlyTagged {
        items: tag_sets
            .into_iter()
            .map(|tag_set| tag_set.into_tagged(&members))
            .collect(),
        labels,
    })
}

async fn newest_active_tags(db: &Database) -> Result<Vec<TagSet>, AdminError> {
    sqlx::query_as(
        "SELECT c.id, c.uuid::uuid AS player, c.tag_type, c.author, c.ts
         FROM (SELECT id, uuid, tag_type, author, ts, expires_at FROM player_events
               WHERE kind = 'tag_set' ORDER BY ts DESC, id DESC LIMIT $1) c
         WHERE (c.expires_at IS NULL OR c.expires_at > now())
           AND NOT EXISTS (SELECT 1 FROM player_events later
                           WHERE later.uuid = c.uuid AND later.tag_type = c.tag_type
                             AND (later.ts, later.id) > (c.ts, c.id))
         ORDER BY c.ts DESC, c.id DESC
         LIMIT $2",
    )
    .bind(RECENT_TAG_CANDIDATES)
    .bind(RECENTLY_TAGGED)
    .fetch_all(db.pool())
    .await
    .context("read the most recently tagged players")
}

fn authors_of(tag_sets: &[TagSet]) -> Vec<i64> {
    let mut authors: Vec<i64> = tag_sets
        .iter()
        .filter_map(|tag_set| tag_set.author)
        .collect();
    authors.sort_unstable();
    authors.dedup();
    authors
}

impl TagSet {
    fn into_tagged(self, members: &MemberIds) -> TaggedPlayer {
        TaggedPlayer {
            id: self.id,
            player: EntityRef::Player { uuid: self.player },
            tag: TagLabel::of(self.tag_type),
            author: self.author.map(|id| members.account(id)),
            at: self.ts,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    const STAFF: i64 = 300_000_000_000_000_007;

    async fn forget(db: &Database, players: &[&str]) {
        sqlx::query("DELETE FROM player_events WHERE uuid = ANY($1)")
            .bind(players)
            .execute(db.pool())
            .await
            .unwrap();
    }

    async fn newest_event(db: &Database) -> DateTime<Utc> {
        sqlx::query_scalar("SELECT coalesce(max(ts), now()) FROM player_events")
            .fetch_one(db.pool())
            .await
            .unwrap()
    }

    async fn event(
        db: &Database,
        anchor: DateTime<Utc>,
        player: &str,
        kind: &str,
        tag_type: &str,
        minutes_ago: i64,
    ) {
        sqlx::query(
            "INSERT INTO player_events (uuid, kind, tag_type, reason, author, ts)
             VALUES ($1, $2, $3, 'fixture', $4, $5)",
        )
        .bind(player)
        .bind(kind)
        .bind(tag_type)
        .bind(STAFF)
        .bind(anchor + TimeDelta::days(1) - TimeDelta::minutes(minutes_ago))
        .execute(db.pool())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn only_tags_still_in_force_are_listed_newest_first() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let kept = "73000013000040008000000000000001";
        let cleared = "73000014000040008000000000000001";
        let replaced = "73000015000040008000000000000001";
        forget(&db, &[kept, cleared, replaced]).await;
        let anchor = newest_event(&db).await;
        event(&db, anchor, kept, "tag_set", "sniper", 5).await;
        event(&db, anchor, cleared, "tag_set", "caution", 4).await;
        event(&db, anchor, cleared, "tag_clear", "caution", 3).await;
        event(&db, anchor, replaced, "tag_set", "sniper", 2).await;
        event(&db, anchor, replaced, "tag_set", "sniper", 1).await;

        let tag_sets = newest_active_tags(&db).await.unwrap();
        let ours: Vec<(String, String)> = tag_sets
            .iter()
            .map(|tag_set| {
                (
                    tag_set.player.simple().to_string(),
                    tag_set.tag_type.clone(),
                )
            })
            .filter(|(player, _)| [kept, cleared, replaced].contains(&player.as_str()))
            .collect();
        assert_eq!(
            ours,
            vec![
                (replaced.to_string(), "sniper".to_string()),
                (kept.to_string(), "sniper".to_string()),
            ]
        );
        let tagged = tag_sets[0].clone().into_tagged(&MemberIds::default());
        assert_eq!(tagged.tag.color.as_deref(), Some("#ff0000"));
        assert_eq!(
            tagged.author,
            Some(EntityRef::Account {
                discord_id: STAFF,
                member_id: None
            })
        );
        forget(&db, &[kept, cleared, replaced]).await;
    }
}
