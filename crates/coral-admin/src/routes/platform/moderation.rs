use std::collections::HashMap;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use database::Database;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use super::page::{Accounts, ContainsText, Mentions, RowCursor, RowPage, page_size, split_page};
use crate::error::{AdminError, Context};
use crate::query::PlayerInput;
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::state::AppState;

const PLUGIN_ACTIONS: [&str; 8] = [
    "set_plugin_official",
    "set_plugin_unlisted",
    "set_plugin_disabled",
    "delete_plugin",
    "yank_release",
    "unyank_release",
    "delete_release",
    "delete_plugin_review",
];

const UNLINKED_TARGET_ACTIONS: [&str; 14] = [
    "set_link_role",
    "set_unlinked_role",
    "set_link_channel",
    "set_nickname_template",
    "reset_nicknames",
    "add_autorole_rule",
    "update_autorole_rule",
    "remove_autorole_rule",
    "strip_role",
    "cancel_sync_job",
    "update_review_guide_content",
    "set_review_ping_roles",
    "publish_review_guide",
    "update_review_rules",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchField {
    #[default]
    Player,
    Tagger,
    Reason,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Desc,
    Asc,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrikeFilter {
    #[default]
    Active,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum StrikeState {
    Active,
    Expired,
    Revoked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlacklistQuery {
    q: Option<String>,
    #[serde(default)]
    field: SearchField,
    tag: Option<String>,
    #[serde(default)]
    dir: Direction,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikesQuery {
    #[serde(default)]
    state: StrikeFilter,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    action: Option<String>,
    cursor: Option<RowCursor>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TagType {
    name: &'static str,
    display_name: &'static str,
    color: String,
}

#[derive(Debug, Serialize)]
pub struct BlacklistRow {
    id: i64,
    player: EntityRef,
    tag_type: String,
    reason: Option<String>,
    author: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    expires_at: Option<DateTime<Utc>>,
    hide_username: bool,
    lock: Option<LockView>,
}

#[derive(Debug, Serialize)]
struct LockView {
    by: Option<EntityRef>,
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StrikeRow {
    id: i64,
    account: EntityRef,
    reason: String,
    struck_by: EntityRef,
    #[serde(serialize_with = "serde_ts::micros")]
    issued_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    expires_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<EntityRef>,
    source_ref: Option<String>,
    state: StrikeState,
}

#[derive(Debug, Serialize)]
pub struct AuditRow {
    id: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    ts: DateTime<Utc>,
    actor: EntityRef,
    action: String,
    target: String,
    target_ref: Option<EntityRef>,
    details: Value,
}

#[derive(FromRow)]
struct TagEvent {
    id: i64,
    uuid: String,
    tag_type: Option<String>,
    reason: Option<String>,
    author: Option<i64>,
    ts: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    hide_username: Option<bool>,
}

#[derive(FromRow)]
struct LockEvent {
    uuid: String,
    author: Option<i64>,
    ts: DateTime<Utc>,
    reason: Option<String>,
    locked: bool,
}

#[derive(FromRow)]
struct StrikeRecord {
    id: i64,
    discord_id: i64,
    reason: String,
    struck_by: i64,
    issued_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<i64>,
    source_ref: Option<String>,
}

#[derive(FromRow)]
struct ActionRecord {
    id: i64,
    actor: i64,
    action: String,
    target: String,
    details: Value,
    ts: DateTime<Utc>,
}

enum TagScope {
    Everyone,
    Players(Vec<String>),
    Tagger(i64),
    Reason(ContainsText),
}

impl Direction {
    fn order(self) -> &'static str {
        match self {
            Self::Desc => "DESC",
            Self::Asc => "ASC",
        }
    }

    fn after(self) -> &'static str {
        match self {
            Self::Desc => "<",
            Self::Asc => ">",
        }
    }
}

pub async fn tag_types() -> Json<Vec<TagType>> {
    Json(
        blacklist::all()
            .iter()
            .map(|tag| TagType {
                name: tag.name,
                display_name: tag.display_name,
                color: format!("#{:06x}", tag.color),
            })
            .collect(),
    )
}

pub async fn blacklist(
    State(state): State<AppState>,
    query: Result<Query<BlacklistQuery>, QueryRejection>,
) -> Result<Json<RowPage<BlacklistRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let scope = tag_scope(&state, &query).await?;
    let events = active_tags(&state.db, &query, &scope, limit).await?;
    let (events, next_cursor) =
        split_page(events, limit, |event| RowCursor::at(event.ts, event.id));
    let uuids: Vec<String> = events.iter().map(|event| event.uuid.clone()).collect();
    let locks = current_locks(&state, &uuids).await?;
    let mut mentions = Mentions::default();
    for event in &events {
        event.author.into_iter().for_each(|id| mentions.account(id));
        mentions.player(parse_player(&event.uuid)?);
    }
    locks
        .values()
        .filter_map(|lock| lock.author)
        .for_each(|id| mentions.account(id));
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let items = events
        .into_iter()
        .map(|event| blacklist_row(event, &locks, &accounts))
        .collect::<Result<_, _>>()?;
    let labels = mentions.labels(&state).await?;
    Ok(Json(RowPage {
        items,
        next_cursor,
        labels,
    }))
}

pub async fn strikes(
    State(state): State<AppState>,
    query: Result<Query<StrikesQuery>, QueryRejection>,
) -> Result<Json<RowPage<StrikeRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let (before, before_id) = cursor_bounds(query.cursor.as_ref())?;
    let records: Vec<StrikeRecord> = sqlx::query_as(
        "SELECT s.id, m.discord_id, s.reason, s.struck_by, s.issued_at, s.expires_at,
                s.revoked_at, s.revoked_by, s.source_ref
         FROM member_strike s
         JOIN members m ON m.id = s.member_id
         WHERE ($1 OR (s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > now())))
           AND ($2::timestamptz IS NULL OR (s.issued_at, s.id) < ($2, $3))
         ORDER BY s.issued_at DESC, s.id DESC
         LIMIT $4",
    )
    .bind(query.state == StrikeFilter::All)
    .bind(before)
    .bind(before_id)
    .bind(limit + 1)
    .fetch_all(state.db.pool())
    .await
    .context("read strikes")?;
    let (records, next_cursor) = split_page(records, limit, |record| {
        RowCursor::at(record.issued_at, record.id)
    });
    let mut mentions = Mentions::default();
    for record in &records {
        mentions.account(record.discord_id);
        mentions.account(record.struck_by);
        record
            .revoked_by
            .into_iter()
            .for_each(|id| mentions.account(id));
    }
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let now = Utc::now();
    let items = records
        .into_iter()
        .map(|record| strike_row(record, &accounts, now))
        .collect();
    let labels = mentions.labels(&state).await?;
    Ok(Json(RowPage {
        items,
        next_cursor,
        labels,
    }))
}

pub async fn audit(
    State(state): State<AppState>,
    query: Result<Query<AuditQuery>, QueryRejection>,
) -> Result<Json<RowPage<AuditRow>>, AdminError> {
    let Query(query) = query?;
    let limit = page_size(query.limit)?;
    let (before, before_id) = cursor_bounds(query.cursor.as_ref())?;
    let action = query.action.filter(|action| !action.trim().is_empty());
    let records: Vec<ActionRecord> = sqlx::query_as(
        "SELECT id, actor, action, target, details, ts FROM admin_actions
         WHERE ($1::text IS NULL OR action = $1)
           AND ($2::timestamptz IS NULL OR (ts, id) < ($2, $3))
         ORDER BY ts DESC, id DESC
         LIMIT $4",
    )
    .bind(action)
    .bind(before)
    .bind(before_id)
    .bind(limit + 1)
    .fetch_all(state.db.pool())
    .await
    .context("read the audit log")?;
    let (records, next_cursor) =
        split_page(records, limit, |record| RowCursor::at(record.ts, record.id));
    let mut mentions = Mentions::default();
    let targets: Vec<Option<EntityRef>> = records
        .iter()
        .map(|record| target_ref(&record.action, &record.target))
        .collect();
    for (record, target) in records.iter().zip(&targets) {
        mentions.account(record.actor);
        match target {
            Some(EntityRef::Account { discord_id, .. }) => mentions.account(*discord_id),
            Some(EntityRef::Player { uuid }) => mentions.player(*uuid),
            _ => {}
        }
    }
    let accounts = Accounts::load(&state, &mentions.discord_ids()).await?;
    let items = records
        .into_iter()
        .zip(targets)
        .map(|(record, target)| AuditRow {
            id: record.id,
            ts: record.ts,
            actor: accounts.of(record.actor),
            target_ref: target.map(|target| with_member(target, &accounts)),
            action: record.action,
            target: record.target,
            details: record.details,
        })
        .collect();
    let labels = mentions.labels(&state).await?;
    Ok(Json(RowPage {
        items,
        next_cursor,
        labels,
    }))
}

async fn tag_scope(state: &AppState, query: &BlacklistQuery) -> Result<TagScope, AdminError> {
    let Some(text) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) else {
        return Ok(TagScope::Everyone);
    };
    match query.field {
        SearchField::Player => {
            let keys = state.players.keys(&PlayerInput::parse(text)?).await?;
            Ok(TagScope::Players(
                keys.uuids
                    .iter()
                    .map(|uuid| uuid.simple().to_string())
                    .collect(),
            ))
        }
        SearchField::Tagger => text
            .parse()
            .map(TagScope::Tagger)
            .map_err(|_| AdminError::BadRequest(format!("{text:?} is not a Discord id"))),
        SearchField::Reason => Ok(TagScope::Reason(ContainsText::new(text))),
    }
}

async fn active_tags(
    db: &Database,
    query: &BlacklistQuery,
    scope: &TagScope,
    limit: i64,
) -> Result<Vec<TagEvent>, AdminError> {
    let order = query.dir.order();
    let mut sql = QueryBuilder::<Postgres>::new(
        "WITH candidates AS MATERIALIZED (
           SELECT e.id, e.ts FROM player_events e
           WHERE e.kind = 'tag_set'
             AND (e.expires_at IS NULL OR e.expires_at > now())",
    );
    push_tag_filters(&mut sql, query, scope)?;
    sql.push(format!(
        " ORDER BY e.ts {order}, e.id {order})
         SELECT e.id, e.uuid, e.tag_type, e.reason, e.author, e.ts, e.expires_at, e.hide_username
         FROM candidates c JOIN player_events e ON e.id = c.id
         WHERE NOT EXISTS (
           SELECT 1 FROM player_events later
           WHERE later.uuid = e.uuid AND later.tag_type = e.tag_type
             AND later.kind IN ('tag_set', 'tag_clear')
             AND (later.ts, later.id) > (c.ts, c.id))
         ORDER BY c.ts {order}, c.id {order}
         LIMIT "
    ))
    .push_bind(limit + 1);
    sql.build_query_as()
        .fetch_all(db.pool())
        .await
        .context("read active tags")
}

fn push_tag_filters(
    sql: &mut QueryBuilder<'_, Postgres>,
    query: &BlacklistQuery,
    scope: &TagScope,
) -> Result<(), AdminError> {
    match scope {
        TagScope::Everyone => {}
        TagScope::Players(uuids) => {
            sql.push(" AND e.uuid = ANY(")
                .push_bind(uuids.clone())
                .push(")");
        }
        TagScope::Tagger(author) => {
            sql.push(" AND e.author = ").push_bind(*author);
        }
        TagScope::Reason(text) => {
            sql.push(" AND ");
            text.push_match(sql, "e.reason");
        }
    }
    if let Some(tag) = query.tag.as_deref().filter(|tag| !tag.is_empty()) {
        sql.push(" AND e.tag_type = ").push_bind(tag.to_string());
    }
    if let Some(cursor) = &query.cursor {
        sql.push(format!(" AND (e.ts, e.id) {} (", query.dir.after()))
            .push_bind(cursor.ts()?)
            .push(", ")
            .push_bind(cursor.id()?)
            .push(")");
    }
    Ok(())
}

async fn current_locks(
    state: &AppState,
    uuids: &[String],
) -> Result<HashMap<String, LockEvent>, AdminError> {
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }
    let latest: Vec<LockEvent> = sqlx::query_as(
        "SELECT DISTINCT ON (uuid) uuid, author, ts, reason,
                kind = 'lock' AND (expires_at IS NULL OR expires_at > now()) AS locked
         FROM player_events
         WHERE uuid = ANY($1) AND kind IN ('lock', 'unlock')
         ORDER BY uuid, ts DESC, id DESC",
    )
    .bind(uuids)
    .fetch_all(state.db.pool())
    .await
    .context("read player locks")?;
    Ok(latest
        .into_iter()
        .filter(|lock| lock.locked)
        .map(|lock| (lock.uuid.clone(), lock))
        .collect())
}

fn blacklist_row(
    event: TagEvent,
    locks: &HashMap<String, LockEvent>,
    accounts: &Accounts,
) -> Result<BlacklistRow, AdminError> {
    let lock = locks.get(&event.uuid).map(|lock| LockView {
        by: lock.author.map(|id| accounts.of(id)),
        at: lock.ts,
        reason: lock.reason.clone(),
    });
    Ok(BlacklistRow {
        id: event.id,
        player: EntityRef::Player {
            uuid: parse_player(&event.uuid)?,
        },
        tag_type: event.tag_type.unwrap_or_default(),
        reason: event.reason,
        author: event.author.map(|id| accounts.of(id)),
        at: event.ts,
        expires_at: event.expires_at,
        hide_username: event.hide_username.unwrap_or(false),
        lock,
    })
}

fn strike_row(record: StrikeRecord, accounts: &Accounts, now: DateTime<Utc>) -> StrikeRow {
    let state = match (record.revoked_at, record.expires_at) {
        (Some(_), _) => StrikeState::Revoked,
        (None, Some(expires)) if expires <= now => StrikeState::Expired,
        _ => StrikeState::Active,
    };
    StrikeRow {
        id: record.id,
        account: accounts.of(record.discord_id),
        reason: record.reason,
        struck_by: accounts.of(record.struck_by),
        issued_at: record.issued_at,
        expires_at: record.expires_at,
        revoked_at: record.revoked_at,
        revoked_by: record.revoked_by.map(|id| accounts.of(id)),
        source_ref: record.source_ref,
        state,
    }
}

fn target_ref(action: &str, target: &str) -> Option<EntityRef> {
    if PLUGIN_ACTIONS.contains(&action) {
        return Some(EntityRef::Plugin {
            slug: target.to_string(),
            name: None,
        });
    }
    if UNLINKED_TARGET_ACTIONS.contains(&action) {
        return None;
    }
    if let Some(uuid) = reqlog::parse_uuid(target) {
        return Some(EntityRef::Player { uuid });
    }
    target
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .map(|discord_id| EntityRef::Account {
            discord_id,
            member_id: None,
        })
}

fn with_member(target: EntityRef, accounts: &Accounts) -> EntityRef {
    match target {
        EntityRef::Account { discord_id, .. } => accounts.of(discord_id),
        other => other,
    }
}

fn cursor_bounds(
    cursor: Option<&RowCursor>,
) -> Result<(Option<DateTime<Utc>>, Option<i64>), AdminError> {
    match cursor {
        Some(cursor) => Ok((Some(cursor.ts()?), Some(cursor.id()?))),
        None => Ok((None, None)),
    }
}

fn parse_player(uuid: &str) -> Result<Uuid, AdminError> {
    reqlog::parse_uuid(uuid).ok_or_else(|| {
        AdminError::Database(format!("player_events holds {uuid:?}, which is not a UUID"))
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    struct TagFixture {
        player: usize,
        tag: &'static str,
        kind: &'static str,
        minutes: i64,
        expired: bool,
    }

    const FIXTURE_EVENTS: [TagFixture; 7] = [
        tag_event(0, "sniper", "tag_set", 0, false),
        tag_event(1, "sniper", "tag_set", 1, false),
        tag_event(1, "sniper", "tag_clear", 2, false),
        tag_event(2, "caution", "tag_set", 3, true),
        tag_event(3, "caution", "tag_set", 4, false),
        tag_event(3, "caution", "tag_set", 5, false),
        tag_event(0, "caution", "tag_set", 6, false),
    ];
    const ACTIVE_NEWEST_FIRST: [usize; 3] = [6, 5, 0];

    const fn tag_event(
        player: usize,
        tag: &'static str,
        kind: &'static str,
        minutes: i64,
        expired: bool,
    ) -> TagFixture {
        TagFixture {
            player,
            tag,
            kind,
            minutes,
            expired,
        }
    }

    fn listing(dir: Direction, cursor: Option<RowCursor>) -> BlacklistQuery {
        BlacklistQuery {
            q: None,
            field: SearchField::Player,
            tag: None,
            dir,
            cursor,
            limit: None,
        }
    }

    async fn test_db() -> Option<Database> {
        Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()
    }

    async fn insert_fixture(db: &Database, players: &[String]) -> Vec<i64> {
        let base = Utc::now() - TimeDelta::days(1);
        let mut ids = Vec::new();
        for event in &FIXTURE_EVENTS {
            let expires_at = event.expired.then(|| base - TimeDelta::hours(1));
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO player_events (uuid, tag_type, reason, ts, kind, expires_at)
                 VALUES ($1, $2, 'fixture', $3, $4, $5) RETURNING id",
            )
            .bind(&players[event.player])
            .bind(event.tag)
            .bind(base + TimeDelta::minutes(event.minutes))
            .bind(event.kind)
            .bind(expires_at)
            .fetch_one(db.pool())
            .await
            .unwrap();
            ids.push(id);
        }
        ids
    }

    #[tokio::test]
    async fn active_tags_are_each_player_and_tag_newest_unexpired_set() {
        let Some(db) = test_db().await else {
            return;
        };
        let players: Vec<String> = (0..4)
            .map(|_| Uuid::new_v4().simple().to_string())
            .collect();
        let ids = insert_fixture(&db, &players).await;
        let expected: Vec<i64> = ACTIVE_NEWEST_FIRST.iter().map(|at| ids[*at]).collect();
        let scope = TagScope::Players(players);
        let ids_of =
            |events: Vec<TagEvent>| events.into_iter().map(|event| event.id).collect::<Vec<_>>();

        let all = active_tags(&db, &listing(Direction::Desc, None), &scope, 10)
            .await
            .unwrap();
        assert_eq!(ids_of(all), expected);

        let first = active_tags(&db, &listing(Direction::Desc, None), &scope, 1)
            .await
            .unwrap();
        let (first, cursor) = split_page(first, 1, |event| RowCursor::at(event.ts, event.id));
        assert_eq!(ids_of(first), expected[..1]);
        let rest = active_tags(&db, &listing(Direction::Desc, cursor), &scope, 10)
            .await
            .unwrap();
        assert_eq!(ids_of(rest), expected[1..]);

        let oldest_first = active_tags(&db, &listing(Direction::Asc, None), &scope, 10)
            .await
            .unwrap();
        let reversed: Vec<i64> = expected.iter().rev().copied().collect();
        assert_eq!(ids_of(oldest_first), reversed);
    }

    #[test]
    fn audit_targets_link_to_the_entity_their_action_names() {
        assert_eq!(
            target_ref("delete_plugin", "bedwars-stats"),
            Some(EntityRef::Plugin {
                slug: "bedwars-stats".into(),
                name: None
            })
        );
        assert_eq!(target_ref("set_link_role", "1200000000000000001"), None);
        assert_eq!(
            target_ref("lock_member", "1218840276405911602"),
            Some(EntityRef::Account {
                discord_id: 1_218_840_276_405_911_602,
                member_id: None
            })
        );
        assert_eq!(
            target_ref("add_tag", "94589c87999b4a7ab3add2ae50711e64"),
            Some(EntityRef::Player {
                uuid: Uuid::parse_str("94589c87999b4a7ab3add2ae50711e64").unwrap()
            })
        );
        assert_eq!(target_ref("recompute_rollups", "recompute"), None);
    }

    #[test]
    fn strikes_are_revoked_before_they_are_expired() {
        let now = Utc::now();
        let record = |revoked: bool, expired: bool| StrikeRecord {
            id: 1,
            discord_id: 2,
            reason: String::new(),
            struck_by: 3,
            issued_at: now,
            expires_at: expired.then(|| now - chrono::Duration::days(1)),
            revoked_at: revoked.then_some(now),
            revoked_by: None,
            source_ref: None,
        };
        let accounts = Accounts::empty();
        assert_eq!(
            strike_row(record(true, true), &accounts, now).state,
            StrikeState::Revoked
        );
        assert_eq!(
            strike_row(record(false, true), &accounts, now).state,
            StrikeState::Expired
        );
        assert_eq!(
            strike_row(record(false, false), &accounts, now).state,
            StrikeState::Active
        );
    }
}
