use std::collections::HashMap;

use chrono::{DateTime, Utc};
use database::Database;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::lookups::RequestTotals;
use super::tags::{Blacklist, PlayerEvents};
use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::query::{HeldName, PlayerInput, PlayerKeys};
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;
use crate::{serde_id, serde_ts};

const PRIMARY: &str = "primary";
const ALT: &str = "alt";

#[derive(Debug, Clone, Serialize)]
pub struct PlayerCore {
    #[serde(serialize_with = "serde_id::uuid_undashed")]
    pub uuid: Uuid,
    pub dashed: String,
    pub name: Option<String>,
    pub names: Vec<PlayerName>,
    pub owners: Vec<Owner>,
    pub blacklist: Blacklist,
    pub snapshots: SnapshotSummary,
    pub session_markers: i64,
    pub requests: RequestTotals,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerName {
    pub name: String,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub first_seen: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub last_seen: Option<DateTime<Utc>>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Owner {
    pub account: EntityRef,
    pub relation: Relation,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub linked_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub unlinked_at: Option<DateTime<Utc>>,
    pub primary_shared_with: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Primary,
    Alt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotSummary {
    pub count: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub first: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub last: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct SpelledName {
    name: Option<String>,
    spelled: Option<String>,
    count: i64,
    first: DateTime<Utc>,
    last: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct OwnerRow {
    discord_id: i64,
    member_id: i64,
    relation: String,
    linked_at: Option<DateTime<Utc>>,
    unlinked_at: Option<DateTime<Utc>>,
    primary_shared_with: i64,
}

struct SnapshotNames {
    summary: SnapshotSummary,
    spellings: HashMap<String, String>,
}

pub async fn read_core(state: &AppState, player: Uuid) -> Result<PlayerCore, AdminError> {
    let coverage = state.coverage.current();
    let uuid = player.simple().to_string();
    let input = PlayerInput::Uuid(player);
    let (keys, snapshots, owners, events, session_markers, tally) = tokio::try_join!(
        state.players.keys(&input),
        snapshot_names(&state.db, &uuid),
        owners(&state.db, &uuid),
        PlayerEvents::read(&state.db, player),
        session_markers(&state.db, &uuid),
        state.player_lookups.tally(player, &coverage),
    )?;
    let labels = state
        .names
        .cached(&LabelRequest {
            discord: owners
                .iter()
                .filter_map(|owner| account_id(&owner.account))
                .chain(events.mentioned.iter().copied())
                .collect(),
            minecraft: vec![uuid],
        })
        .await?;
    Ok(PlayerCore {
        uuid: player,
        dashed: player.hyphenated().to_string(),
        name: current_name(&labels, player, &keys, &snapshots.spellings),
        names: player_names(&keys, &snapshots.spellings),
        owners,
        blacklist: events.blacklist(),
        snapshots: snapshots.summary,
        session_markers,
        requests: tally.totals(),
        labels,
    })
}

async fn snapshot_names(db: &Database, uuid: &str) -> Result<SnapshotNames, AdminError> {
    let rows: Vec<SpelledName> = sqlx::query_as(
        "SELECT lower(username) AS name, (array_agg(username ORDER BY timestamp DESC))[1] AS spelled,
                count(*) AS count, min(timestamp) AS first, max(timestamp) AS last
         FROM player_snapshots WHERE uuid = $1
         GROUP BY lower(username)",
    )
    .bind(uuid)
    .fetch_all(db.pool())
    .await
    .context("read the player's snapshot names")?;
    Ok(SnapshotNames {
        summary: SnapshotSummary {
            count: rows.iter().map(|row| row.count).sum(),
            first: rows.iter().map(|row| row.first).min(),
            last: rows.iter().map(|row| row.last).max(),
        },
        spellings: rows
            .into_iter()
            .filter_map(|row| Some((row.name?, row.spelled?)))
            .collect(),
    })
}

async fn owners(db: &Database, uuid: &str) -> Result<Vec<Owner>, AdminError> {
    let rows: Vec<OwnerRow> = sqlx::query_as(
        "SELECT m.discord_id, l.member_id, l.relation, l.linked_at, l.unlinked_at,
                CASE WHEN l.relation = $2 AND l.unlinked_at IS NULL THEN
                  (SELECT count(*) FROM member_mc_links o
                   WHERE o.uuid = l.uuid AND o.relation = $2 AND o.unlinked_at IS NULL
                     AND o.member_id <> l.member_id)
                ELSE 0 END AS primary_shared_with
         FROM member_mc_links l JOIN members m ON m.id = l.member_id
         WHERE l.uuid = $1
         ORDER BY l.unlinked_at IS NOT NULL, coalesce(l.unlinked_at, l.linked_at) DESC NULLS LAST, l.id DESC",
    )
    .bind(uuid)
    .bind(PRIMARY)
    .fetch_all(db.pool())
    .await
    .context("read the player's owners")?;
    rows.into_iter().map(OwnerRow::into_owner).collect()
}

async fn session_markers(db: &Database, uuid: &str) -> Result<i64, AdminError> {
    sqlx::query_scalar("SELECT count(*) FROM session_markers WHERE uuid = $1")
        .bind(uuid)
        .fetch_one(db.pool())
        .await
        .context("count the player's session markers")
}

impl OwnerRow {
    fn into_owner(self) -> Result<Owner, AdminError> {
        let relation = match self.relation.as_str() {
            PRIMARY => Relation::Primary,
            ALT => Relation::Alt,
            other => {
                return Err(AdminError::Internal(format!(
                    "member {} links the player with the unknown relation {other:?}",
                    self.member_id
                )));
            }
        };
        Ok(Owner {
            account: EntityRef::Account {
                discord_id: self.discord_id,
                member_id: Some(self.member_id),
            },
            relation,
            linked_at: self.linked_at,
            unlinked_at: self.unlinked_at,
            primary_shared_with: self.primary_shared_with,
        })
    }
}

fn player_names(keys: &PlayerKeys, spellings: &HashMap<String, String>) -> Vec<PlayerName> {
    let mut names: Vec<PlayerName> = keys
        .names
        .iter()
        .map(|held| player_name(held, spellings))
        .collect();
    names.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then(b.first_seen.cmp(&a.first_seen))
    });
    names
}

fn player_name(held: &HeldName, spellings: &HashMap<String, String>) -> PlayerName {
    PlayerName {
        name: spelled(&held.name, spellings),
        first_seen: held.from,
        last_seen: held.to,
        current: held.to.is_none(),
    }
}

fn spelled(name: &str, spellings: &HashMap<String, String>) -> String {
    spellings
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

fn current_name(
    labels: &Labels,
    player: Uuid,
    keys: &PlayerKeys,
    spellings: &HashMap<String, String>,
) -> Option<String> {
    labels
        .minecraft
        .get(&player.simple().to_string())
        .cloned()
        .or_else(|| {
            let held = keys.names.iter().find(|held| held.to.is_none())?;
            Some(spelled(&held.name, spellings))
        })
}

fn account_id(entity: &EntityRef) -> Option<i64> {
    match entity {
        EntityRef::Account { discord_id, .. } => Some(*discord_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn day(d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, d, 0, 0, 0).unwrap()
    }

    fn keys() -> PlayerKeys {
        PlayerKeys {
            uuids: Vec::new(),
            names: vec![
                HeldName {
                    name: "kes_tr".into(),
                    from: Some(day(2)),
                    to: Some(day(9)),
                },
                HeldName {
                    name: "kestrel_".into(),
                    from: Some(day(9)),
                    to: None,
                },
            ],
        }
    }

    #[test]
    fn names_keep_their_snapshot_spelling_and_the_current_name_comes_first() {
        let spellings = HashMap::from([("kestrel_".to_string(), "Kestrel_".to_string())]);
        let names = player_names(&keys(), &spellings);
        assert_eq!(
            names,
            vec![
                PlayerName {
                    name: "Kestrel_".into(),
                    first_seen: Some(day(9)),
                    last_seen: None,
                    current: true
                },
                PlayerName {
                    name: "kes_tr".into(),
                    first_seen: Some(day(2)),
                    last_seen: Some(day(9)),
                    current: false
                },
            ]
        );
    }

    #[test]
    fn the_current_name_prefers_the_label_and_falls_back_to_the_open_held_name() {
        let player = Uuid::nil();
        let spellings = HashMap::from([("kestrel_".to_string(), "Kestrel_".to_string())]);
        let mut labels = Labels::default();
        assert_eq!(
            current_name(&labels, player, &keys(), &spellings).as_deref(),
            Some("Kestrel_")
        );
        labels
            .minecraft
            .insert(player.simple().to_string(), "KESTREL_".into());
        assert_eq!(
            current_name(&labels, player, &keys(), &spellings).as_deref(),
            Some("KESTREL_")
        );
        assert_eq!(
            current_name(
                &Labels::default(),
                player,
                &PlayerKeys::default(),
                &spellings
            ),
            None
        );
    }

    async fn forget_links(db: &Database, uuid: &str) {
        sqlx::query("DELETE FROM member_mc_links WHERE uuid = $1")
            .bind(uuid)
            .execute(db.pool())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn owners_list_current_links_first_and_count_shared_primaries() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let uuid = "73000012000040008000000000000001";
        forget_links(&db, uuid).await;
        sqlx::query(
            "INSERT INTO member_mc_links (member_id, uuid, relation, linked_at, unlinked_at) VALUES
               (1, $1, 'primary', '2025-03-01', NULL),
               (2, $1, 'primary', '2025-04-01', NULL),
               (3, $1, 'alt', '2025-05-01', NULL),
               (4, $1, 'primary', '2024-01-01', '2024-06-01')",
        )
        .bind(uuid)
        .execute(db.pool())
        .await
        .unwrap();

        let owners = owners(&db, uuid).await.unwrap();

        let shape: Vec<(Option<i64>, Relation, bool, i64)> = owners
            .iter()
            .map(|owner| {
                let member = match owner.account {
                    EntityRef::Account { member_id, .. } => member_id,
                    _ => None,
                };
                (
                    member,
                    owner.relation,
                    owner.unlinked_at.is_some(),
                    owner.primary_shared_with,
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some(3), Relation::Alt, false, 0),
                (Some(2), Relation::Primary, false, 1),
                (Some(1), Relation::Primary, false, 1),
                (Some(4), Relation::Primary, true, 0),
            ]
        );
        forget_links(&db, uuid).await;
    }
}
