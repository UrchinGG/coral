use std::collections::HashMap;

use chrono::{DateTime, Utc};
use database::{Database, MinecraftBlockRepository};
use serde::Serialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use super::lookups::RequestTotals;
use super::tags::{Blacklist, LockView, PlayerEvents};
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
    pub minecraft_block: Option<LockView>,
    pub snapshots: SnapshotSummary,
    pub guild: Option<PlayerGuild>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotSummary {
    pub count: i64,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub first: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub last: Option<DateTime<Utc>>,
    pub months: Vec<SnapshotMonth>,
}

/// Snapshots taken in one UTC calendar month, keyed `YYYY-MM`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotMonth {
    pub month: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerGuild {
    pub guild: EntityRef,
    pub tag: Option<String>,
    pub rank: Option<String>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub joined: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct MonthRow {
    month: String,
    count: i64,
    first: DateTime<Utc>,
    last: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct GuildRow {
    guild_id: String,
    name: String,
    tag: Option<String>,
    rank: Option<String>,
    joined: Option<i64>,
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

pub async fn read_core(state: &AppState, player: Uuid) -> Result<PlayerCore, AdminError> {
    let coverage = state.coverage.current();
    let uuid = player.simple().to_string();
    let input = PlayerInput::Uuid(player);
    let (keys, snapshots, guild, owners, events, session_markers, tally, block) = tokio::try_join!(
        state.players.keys(&input),
        snapshot_summary(&state.db, &uuid),
        player_guild(&state.db, &uuid),
        owners(&state.db, &uuid),
        PlayerEvents::read(&state.db, player),
        session_markers(&state.db, &uuid),
        state.player_lookups.tally(player, &coverage),
        async {
            MinecraftBlockRepository::new(state.db.pool())
                .get(player)
                .await
                .context("read the Minecraft block")
        },
    )?;
    let labels_request = LabelRequest {
        discord: owners
            .iter()
            .filter_map(|owner| account_id(&owner.account))
            .chain(events.mentioned.iter().copied())
            .chain(block.as_ref().map(|block| block.blocked_by))
            .collect(),
        minecraft: vec![uuid.clone()],
    };
    let (labels, spellings) = tokio::try_join!(
        state.names.cached(&labels_request),
        name_spellings(&state.db, &uuid, &keys),
    )?;
    Ok(PlayerCore {
        uuid: player,
        dashed: player.hyphenated().to_string(),
        name: current_name(&labels, player, &keys, &spellings),
        names: player_names(&keys, &spellings),
        owners,
        blacklist: events.blacklist(),
        minecraft_block: block.map(LockView::from),
        snapshots,
        guild,
        session_markers,
        requests: tally.totals(),
        labels,
    })
}

async fn snapshot_summary(db: &Database, uuid: &str) -> Result<SnapshotSummary, AdminError> {
    let rows: Vec<MonthRow> = sqlx::query_as(
        "SELECT to_char(timestamp AT TIME ZONE 'UTC', 'YYYY-MM') AS month,
                count(*) AS count, min(timestamp) AS first, max(timestamp) AS last
         FROM player_snapshots WHERE uuid = $1
         GROUP BY 1 ORDER BY 1",
    )
    .bind(uuid)
    .fetch_all(db.pool())
    .await
    .context("count the player's snapshots per month")?;
    Ok(SnapshotSummary {
        count: rows.iter().map(|row| row.count).sum(),
        first: rows.first().map(|row| row.first),
        last: rows.last().map(|row| row.last),
        months: rows
            .into_iter()
            .map(|row| SnapshotMonth {
                month: row.month,
                count: row.count,
            })
            .collect(),
    })
}

async fn name_spellings(
    db: &Database,
    uuid: &str,
    keys: &PlayerKeys,
) -> Result<HashMap<String, String>, AdminError> {
    let names: Vec<&str> = keys.names.iter().map(|held| held.name.as_str()).collect();
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT held.name, spelled.username
         FROM unnest($2::text[]) AS held(name)
         CROSS JOIN LATERAL (SELECT username FROM player_snapshots
                             WHERE uuid = $1 AND lower(username) = held.name
                             ORDER BY timestamp DESC LIMIT 1) spelled",
    )
    .bind(uuid)
    .bind(&names)
    .fetch_all(db.pool())
    .await
    .context("read how the player spelled their names")?;
    Ok(rows.into_iter().collect())
}

async fn player_guild(db: &Database, uuid: &str) -> Result<Option<PlayerGuild>, AdminError> {
    let row: Option<GuildRow> = sqlx::query_as(
        "SELECT g.guild_id, g.name, g.tag, member->>'rank' AS rank,
                (member->>'joined')::bigint AS joined
         FROM guild_current g
         CROSS JOIN LATERAL jsonb_array_elements(g.raw->'members') member
         WHERE g.raw @> $1 AND member->>'uuid' = $2
         ORDER BY g.updated_at DESC LIMIT 1",
    )
    .bind(json!({ "members": [{ "uuid": uuid }] }))
    .bind(uuid)
    .fetch_optional(db.pool())
    .await
    .context("read the player's guild")?;
    Ok(row.map(GuildRow::into_guild))
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

impl GuildRow {
    fn into_guild(self) -> PlayerGuild {
        PlayerGuild {
            guild: EntityRef::Guild {
                guild_id: self.guild_id,
                name: Some(self.name),
            },
            tag: self.tag,
            rank: self.rank,
            joined: self.joined.and_then(DateTime::from_timestamp_millis),
        }
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
        let ids: Vec<i64> = sqlx::query_scalar(
            "INSERT INTO members (discord_id)
             SELECT 730000120000000 + (random() * 1e9)::bigint FROM generate_series(1, 4)
             RETURNING id",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO member_mc_links (member_id, uuid, relation, linked_at, unlinked_at) VALUES
               ($2, $1, 'primary', '2025-03-01', NULL),
               ($3, $1, 'primary', '2025-04-01', NULL),
               ($4, $1, 'alt', '2025-05-01', NULL),
               ($5, $1, 'primary', '2024-01-01', '2024-06-01')",
        )
        .bind(uuid)
        .bind(ids[0])
        .bind(ids[1])
        .bind(ids[2])
        .bind(ids[3])
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
                (Some(ids[2]), Relation::Alt, false, 0),
                (Some(ids[1]), Relation::Primary, false, 1),
                (Some(ids[0]), Relation::Primary, false, 1),
                (Some(ids[3]), Relation::Primary, true, 0),
            ]
        );
        forget_links(&db, uuid).await;
    }

    #[tokio::test]
    async fn snapshots_count_per_month_names_keep_their_spelling_and_the_guild_is_found() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let uuid = "73000018000040008000000000000001";
        let guild_id = "730000180000400080000001";
        for (statement, key) in [
            ("DELETE FROM player_snapshots WHERE uuid = $1", uuid),
            ("DELETE FROM guild_current WHERE guild_id = $1", guild_id),
        ] {
            sqlx::query(statement)
                .bind(key)
                .execute(db.pool())
                .await
                .unwrap();
        }
        sqlx::query(
            "INSERT INTO player_snapshots (uuid, timestamp, username, is_baseline, data) VALUES
               ($1, '2020-01-31 23:00Z', 'kes_tr', true, '{}'),
               ($1, '2020-03-02 00:00Z', 'Kestrel_', false, '{}'),
               ($1, '2020-03-05 00:00Z', 'KESTREL_', false, '{}')",
        )
        .bind(uuid)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO guild_current (guild_id, name, tag, level, experience, member_count, raw)
             VALUES ($1, 'Tidal Order', 'TIDE', 1, 0, 1,
                     jsonb_build_object('members', jsonb_build_array(
                       jsonb_build_object('uuid', $2::text, 'rank', 'Officer', 'joined', 1580515200000))))",
        )
        .bind(guild_id)
        .bind(uuid)
        .execute(db.pool())
        .await
        .unwrap();

        let summary = snapshot_summary(&db, uuid).await.unwrap();
        let spellings = name_spellings(&db, uuid, &keys()).await.unwrap();
        let guild = player_guild(&db, uuid).await.unwrap().unwrap();

        let months: Vec<(&str, i64)> = summary
            .months
            .iter()
            .map(|month| (month.month.as_str(), month.count))
            .collect();
        assert_eq!(months, vec![("2020-01", 1), ("2020-03", 2)]);
        assert_eq!(summary.count, 3);
        assert_eq!(
            summary.last,
            Some(Utc.with_ymd_and_hms(2020, 3, 5, 0, 0, 0).unwrap())
        );
        assert_eq!(
            spellings,
            HashMap::from([
                ("kes_tr".to_string(), "kes_tr".to_string()),
                ("kestrel_".to_string(), "KESTREL_".to_string()),
            ])
        );
        assert_eq!(guild.rank.as_deref(), Some("Officer"));
        assert_eq!(
            guild.joined,
            Some(Utc.with_ymd_and_hms(2020, 2, 1, 0, 0, 0).unwrap())
        );
        sqlx::query("DELETE FROM guild_current WHERE guild_id = $1")
            .bind(guild_id)
            .execute(db.pool())
            .await
            .unwrap();
    }
}
