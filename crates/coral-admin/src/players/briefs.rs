use chrono::{DateTime, Utc};
use database::Database;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::parse_player;
use super::tags::TagLabel;
use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;
use crate::{serde_id, serde_ts};

pub const BRIEF_LIMIT: usize = 200;

const PLAYER_SEPARATOR: char = ',';

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefsQuery {
    players: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerBriefs {
    pub players: Vec<PlayerBrief>,
    pub labels: Labels,
}

/// What a player list shows beside each name: the newest tag in force,
/// the account linking the player (primary before alt), and the newest snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerBrief {
    #[serde(serialize_with = "serde_id::uuid_undashed")]
    pub player: Uuid,
    pub tag: Option<TagLabel>,
    pub owner: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct BriefRow {
    player: Uuid,
    tag_type: Option<String>,
    owner_discord: Option<i64>,
    owner_member: Option<i64>,
    last_seen: Option<DateTime<Utc>>,
}

impl BriefsQuery {
    fn players(&self) -> Result<Vec<Uuid>, AdminError> {
        let mut players = self
            .players
            .split(PLAYER_SEPARATOR)
            .map(parse_player)
            .collect::<Result<Vec<_>, _>>()?;
        players.sort_unstable();
        players.dedup();
        if players.len() > BRIEF_LIMIT {
            return Err(AdminError::BadRequest(format!(
                "ask for at most {BRIEF_LIMIT} players at once"
            )));
        }
        Ok(players)
    }
}

pub async fn read_briefs(state: &AppState, query: BriefsQuery) -> Result<PlayerBriefs, AdminError> {
    let briefs = player_briefs(&state.db, &query.players()?).await?;
    let labels = state
        .names
        .cached(&LabelRequest {
            discord: briefs
                .iter()
                .filter_map(|brief| match brief.owner {
                    Some(EntityRef::Account { discord_id, .. }) => Some(discord_id),
                    _ => None,
                })
                .collect(),
            minecraft: Vec::new(),
        })
        .await?;
    Ok(PlayerBriefs {
        players: briefs,
        labels,
    })
}

async fn player_briefs(db: &Database, players: &[Uuid]) -> Result<Vec<PlayerBrief>, AdminError> {
    let uuids: Vec<String> = players
        .iter()
        .map(|player| player.simple().to_string())
        .collect();
    let rows: Vec<BriefRow> = sqlx::query_as(
        "SELECT p.uuid::uuid AS player, tag.tag_type, owner.discord_id AS owner_discord,
                owner.member_id AS owner_member,
                (SELECT max(timestamp) FROM player_snapshots s WHERE s.uuid = p.uuid) AS last_seen
         FROM unnest($1::text[]) AS p(uuid)
         LEFT JOIN LATERAL (
             SELECT tag_type FROM (
                 SELECT DISTINCT ON (tag_type) tag_type, kind, expires_at, ts FROM player_events
                 WHERE uuid = p.uuid AND kind IN ('tag_set', 'tag_clear')
                 ORDER BY tag_type, ts DESC, id DESC) latest
             WHERE kind = 'tag_set' AND (expires_at IS NULL OR expires_at > now())
             ORDER BY ts DESC LIMIT 1) tag ON TRUE
         LEFT JOIN LATERAL (
             SELECT m.discord_id, l.member_id FROM member_mc_links l
             JOIN members m ON m.id = l.member_id
             WHERE l.uuid = p.uuid AND l.unlinked_at IS NULL
             ORDER BY l.relation = 'primary' DESC, l.linked_at DESC NULLS LAST, l.id DESC
             LIMIT 1) owner ON TRUE",
    )
    .bind(&uuids)
    .fetch_all(db.pool())
    .await
    .context("read the players' tags, owners and last snapshots")?;
    Ok(rows.into_iter().map(BriefRow::into_brief).collect())
}

impl BriefRow {
    fn into_brief(self) -> PlayerBrief {
        PlayerBrief {
            player: self.player,
            tag: self.tag_type.map(TagLabel::of),
            owner: self.owner_discord.map(|discord_id| EntityRef::Account {
                discord_id,
                member_id: self.owner_member,
            }),
            last_seen: self.last_seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINKED: &str = "73000016000040008000000000000001";
    const BARE: &str = "73000017000040008000000000000001";

    fn query(players: &str) -> BriefsQuery {
        BriefsQuery {
            players: players.into(),
        }
    }

    #[test]
    fn players_are_deduplicated_and_capped() {
        let parsed = query(&format!("{LINKED},{LINKED},{BARE}"))
            .players()
            .unwrap();
        assert_eq!(parsed.len(), 2);
        let too_many = vec![LINKED; BRIEF_LIMIT + 1].join(",");
        assert_eq!(query(&too_many).players().unwrap().len(), 1);
        let distinct: Vec<String> = (0..=BRIEF_LIMIT).map(|n| format!("{n:032x}")).collect();
        assert_eq!(
            query(&distinct.join(",")).players().unwrap_err().code(),
            "bad_request"
        );
        assert_eq!(query("nope").players().unwrap_err().code(), "bad_request");
    }

    async fn forget(db: &Database) {
        for table in ["player_events", "player_snapshots", "member_mc_links"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE uuid = ANY($1)"))
                .bind([LINKED, BARE])
                .execute(db.pool())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn briefs_carry_the_tag_in_force_the_primary_owner_and_the_last_snapshot() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        forget(&db).await;
        for statement in [
            "INSERT INTO player_events (uuid, kind, tag_type, reason, ts) VALUES
               ($1, 'tag_set', 'sniper', 'old', now() - interval '3 days'),
               ($1, 'tag_clear', 'sniper', NULL, now() - interval '2 days'),
               ($1, 'tag_set', 'caution', 'kept', now() - interval '1 day')",
            "INSERT INTO player_snapshots (uuid, timestamp, is_baseline, data)
               VALUES ($1, '2020-02-01', true, '{}'), ($1, '2020-02-03', false, '{}')",
        ] {
            sqlx::query(statement)
                .bind(LINKED)
                .execute(db.pool())
                .await
                .unwrap();
        }
        let members: Vec<(i64, i64)> = sqlx::query_as(
            "INSERT INTO members (discord_id)
             SELECT 730000160000000 + (random() * 1e9)::bigint FROM generate_series(1, 2)
             RETURNING id, discord_id",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO member_mc_links (member_id, uuid, relation) VALUES ($2, $1, 'alt'), ($3, $1, 'primary')",
        )
        .bind(LINKED)
        .bind(members[0].0)
        .bind(members[1].0)
        .execute(db.pool())
        .await
        .unwrap();

        let players = [
            Uuid::parse_str(LINKED).unwrap(),
            Uuid::parse_str(BARE).unwrap(),
        ];
        let briefs = player_briefs(&db, &players).await.unwrap();

        let linked = briefs
            .iter()
            .find(|brief| brief.player == players[0])
            .unwrap();
        assert_eq!(
            linked.tag.as_ref().map(|tag| tag.tag_type.as_str()),
            Some("caution")
        );
        assert_eq!(
            linked.owner,
            Some(EntityRef::Account {
                discord_id: members[1].1,
                member_id: Some(members[1].0)
            })
        );
        assert_eq!(
            linked
                .last_seen
                .map(|at| at.date_naive().to_string())
                .as_deref(),
            Some("2020-02-03")
        );
        let bare = briefs
            .iter()
            .find(|brief| brief.player == players[1])
            .unwrap();
        assert_eq!(
            (&bare.tag, &bare.owner, bare.last_seen),
            (&None, &None, None)
        );
        forget(&db).await;
    }
}
