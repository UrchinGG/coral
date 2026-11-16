use std::collections::HashMap;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};

use super::platform::page::ContainsText;
use super::platform::server::list::server_names;
use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;
use crate::state::AppState;

pub const DEFAULT_ACTIONS: i64 = 20;
pub const MAX_ACTIONS: i64 = 100;

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
const DISCORD_SERVER_ACTIONS: [&str; 12] = [
    "set_sync_enabled",
    "set_sync_sources",
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
];
const SNOWFLAKE_DIGITS: std::ops::RangeInclusive<usize> = 17..=20;
const CURSOR_SEPARATOR: char = '.';
const ACTION_COLUMNS: &str =
    "SELECT id, actor, action, target, details, ts FROM admin_actions WHERE TRUE";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionParams {
    limit: Option<i64>,
    before: Option<String>,
    staff: Option<String>,
    action: Option<String>,
    from: Option<DateTime<Utc>>,
    q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionPage {
    items: Vec<ActionRow>,
    next_before: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionFacets {
    staff: Vec<String>,
    actions: Vec<String>,
    labels: Labels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActionCursor {
    at: DateTime<Utc>,
    id: i64,
}

#[derive(Debug)]
struct ActionFilter {
    before: Option<ActionCursor>,
    staff: Option<i64>,
    action: Option<String>,
    from: Option<DateTime<Utc>>,
    text: Option<ContainsText>,
}

#[derive(Debug, Serialize)]
pub struct ActionRow {
    id: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    at: DateTime<Utc>,
    actor: EntityRef,
    action: String,
    target: Option<EntityRef>,
    target_text: String,
    server_id: Option<String>,
    details: Value,
}

#[derive(Debug, FromRow)]
struct ActionRecord {
    id: i64,
    actor: i64,
    action: String,
    target: String,
    details: Value,
    ts: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct MemberId {
    discord_id: i64,
    id: i64,
}

struct TargetNames {
    plugins: HashMap<String, String>,
    servers: HashMap<String, String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(recent_actions))
        .route("/facets", get(facets))
}

async fn recent_actions(
    State(state): State<AppState>,
    params: Result<Query<ActionParams>, QueryRejection>,
) -> Result<Json<ActionPage>, AdminError> {
    let Query(params) = params?;
    let limit = action_limit(params.limit)?;
    let filter = ActionFilter::parse(params)?;
    let pool = state.db.pool();
    let mut sql = QueryBuilder::new(ACTION_COLUMNS);
    filter.push(&mut sql);
    sql.push(" ORDER BY ts DESC, id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut records: Vec<ActionRecord> = sql
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("read recent admin actions")?;
    let next_before = match records.len() > limit as usize {
        true => {
            records.truncate(limit as usize);
            records
                .last()
                .map(ActionCursor::after)
                .map(ActionCursor::encode)
        }
        false => None,
    };
    let targets: Vec<Option<EntityRef>> = records
        .iter()
        .map(|record| target_ref(&record.action, &record.target))
        .collect();
    let (members, names) = tokio::try_join!(
        member_ids(pool, &records, &targets),
        TargetNames::load(&state, &records),
    )?;
    let items = records
        .into_iter()
        .zip(targets)
        .map(|(record, target)| names.row(record, target, &members))
        .collect();
    Ok(Json(ActionPage { items, next_before }))
}

/// The staff and actions the audit log can be filtered by.
async fn facets(State(state): State<AppState>) -> Result<Json<ActionFacets>, AdminError> {
    let pool = state.db.pool();
    let (staff, actions): (Vec<i64>, Vec<String>) = tokio::try_join!(
        async {
            sqlx::query_scalar("SELECT DISTINCT actor FROM admin_actions")
                .fetch_all(pool)
                .await
                .context("list the staff in the audit log")
        },
        async {
            sqlx::query_scalar("SELECT DISTINCT action FROM admin_actions ORDER BY action")
                .fetch_all(pool)
                .await
                .context("list the actions in the audit log")
        },
    )?;
    let labels = state
        .names
        .cached(&LabelRequest {
            discord: staff.clone(),
            minecraft: Vec::new(),
        })
        .await?;
    Ok(Json(ActionFacets {
        staff: staff.iter().map(i64::to_string).collect(),
        actions,
        labels,
    }))
}

impl ActionFilter {
    fn parse(params: ActionParams) -> Result<Self, AdminError> {
        let staff = params
            .staff
            .as_deref()
            .map(|raw| {
                raw.parse::<i64>()
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a Discord id")))
            })
            .transpose()?;
        Ok(Self {
            before: params
                .before
                .as_deref()
                .map(ActionCursor::parse)
                .transpose()?,
            staff,
            action: params.action,
            from: params.from,
            text: params
                .q
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ContainsText::new),
        })
    }

    fn push(&self, sql: &mut QueryBuilder<'_, Postgres>) {
        if let Some(cursor) = self.before {
            sql.push(" AND (ts, id) < (")
                .push_bind(cursor.at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        if let Some(staff) = self.staff {
            sql.push(" AND actor = ").push_bind(staff);
        }
        if let Some(action) = &self.action {
            sql.push(" AND action = ").push_bind(action.clone());
        }
        if let Some(from) = self.from {
            sql.push(" AND ts >= ").push_bind(from);
        }
        if let Some(text) = &self.text {
            sql.push(" AND (");
            text.push_match(sql, "target");
            sql.push(" OR ");
            text.push_match(sql, "action");
            sql.push(" OR ");
            text.push_match(sql, "details::text");
            sql.push(")");
        }
    }
}

impl TargetNames {
    async fn load(state: &AppState, records: &[ActionRecord]) -> Result<Self, AdminError> {
        let plugin_slugs = targets_of(records, &PLUGIN_ACTIONS);
        let has_servers = !targets_of(records, &DISCORD_SERVER_ACTIONS).is_empty();
        let plugins = async {
            sqlx::query_as::<_, (String, String)>(
                "SELECT slug, display_name FROM plugins WHERE slug = ANY($1)",
            )
            .bind(&plugin_slugs)
            .fetch_all(state.db.pool())
            .await
            .context("read the names of audited plugins")
        };
        let servers = async {
            match has_servers {
                true => known_server_names(state).await,
                false => HashMap::new(),
            }
        };
        let (plugins, servers) = tokio::join!(plugins, servers);
        Ok(Self {
            plugins: plugins?.into_iter().collect(),
            servers,
        })
    }

    fn row(
        &self,
        record: ActionRecord,
        target: Option<EntityRef>,
        members: &HashMap<i64, i64>,
    ) -> ActionRow {
        let server = DISCORD_SERVER_ACTIONS.contains(&record.action.as_str());
        let target = target.map(|target| match target {
            EntityRef::Plugin { slug, .. } => EntityRef::Plugin {
                name: self.plugins.get(&slug).cloned(),
                slug,
            },
            other => with_member(other, members),
        });
        ActionRow {
            id: record.id,
            at: record.ts,
            actor: account(record.actor, members),
            target,
            target_text: match server {
                true => self.servers.get(&record.target).cloned(),
                false => None,
            }
            .unwrap_or_else(|| record.target.clone()),
            server_id: server.then_some(record.target),
            action: record.action,
            details: record.details,
        }
    }
}

async fn known_server_names(state: &AppState) -> HashMap<String, String> {
    server_names(state).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "audit log: server names unavailable");
        HashMap::new()
    })
}

fn targets_of(records: &[ActionRecord], actions: &[&str]) -> Vec<String> {
    records
        .iter()
        .filter(|record| actions.contains(&record.action.as_str()))
        .map(|record| record.target.clone())
        .collect()
}

impl ActionCursor {
    fn after(record: &ActionRecord) -> Self {
        Self {
            at: record.ts,
            id: record.id,
        }
    }

    fn parse(raw: &str) -> Result<Self, AdminError> {
        let invalid = || AdminError::BadRequest(format!("{raw:?} is not an admin action cursor"));
        let (micros, id) = raw.split_once(CURSOR_SEPARATOR).ok_or_else(invalid)?;
        let micros: i64 = micros.parse().map_err(|_| invalid())?;
        Ok(Self {
            at: DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?,
            id: id.parse().map_err(|_| invalid())?,
        })
    }

    fn encode(self) -> String {
        format!(
            "{}{CURSOR_SEPARATOR}{}",
            self.at.timestamp_micros(),
            self.id
        )
    }
}

fn action_limit(requested: Option<i64>) -> Result<i64, AdminError> {
    let limit = requested.unwrap_or(DEFAULT_ACTIONS);
    match (1..=MAX_ACTIONS).contains(&limit) {
        true => Ok(limit),
        false => Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_ACTIONS}"
        ))),
    }
}

fn target_ref(action: &str, target: &str) -> Option<EntityRef> {
    if PLUGIN_ACTIONS.contains(&action) {
        return Some(EntityRef::Plugin {
            slug: target.to_string(),
            name: None,
        });
    }
    if DISCORD_SERVER_ACTIONS.contains(&action) {
        return None;
    }
    if let Some(uuid) = reqlog::parse_uuid(target).filter(|_| target.len() == 32) {
        return Some(EntityRef::Player { uuid });
    }
    let snowflake = SNOWFLAKE_DIGITS.contains(&target.len())
        && target.chars().all(|digit| digit.is_ascii_digit());
    snowflake
        .then(|| target.parse().ok())
        .flatten()
        .map(|discord_id| EntityRef::Account {
            discord_id,
            member_id: None,
        })
}

async fn member_ids(
    pool: &PgPool,
    records: &[ActionRecord],
    targets: &[Option<EntityRef>],
) -> Result<HashMap<i64, i64>, AdminError> {
    let target_accounts = targets.iter().flatten().filter_map(|target| match target {
        EntityRef::Account { discord_id, .. } => Some(*discord_id),
        _ => None,
    });
    let discord_ids: Vec<i64> = records
        .iter()
        .map(|record| record.actor)
        .chain(target_accounts)
        .collect();
    let rows: Vec<MemberId> =
        sqlx::query_as("SELECT discord_id, id FROM members WHERE discord_id = ANY($1)")
            .bind(&discord_ids)
            .fetch_all(pool)
            .await
            .context("read the member ids of admin action accounts")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.discord_id, row.id))
        .collect())
}

fn account(discord_id: i64, members: &HashMap<i64, i64>) -> EntityRef {
    EntityRef::Account {
        discord_id,
        member_id: members.get(&discord_id).copied(),
    }
}

fn with_member(target: EntityRef, members: &HashMap<i64, i64>) -> EntityRef {
    match target {
        EntityRef::Account { discord_id, .. } => account(discord_id, members),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: &str = "94589c87999b4a7ab3add2ae50711e64";

    #[test]
    fn targets_follow_the_action_and_the_shape_of_the_target() {
        assert_eq!(
            target_ref("yank_release", "bedwars-stats"),
            Some(EntityRef::Plugin {
                slug: "bedwars-stats".into(),
                name: None
            })
        );
        assert_eq!(target_ref("strip_role", "1200000000000000001"), None);
        assert!(matches!(
            target_ref("add_tag", PLAYER),
            Some(EntityRef::Player { .. })
        ));
        assert_eq!(
            target_ref("lock_member", "1218840276405911602"),
            Some(EntityRef::Account {
                discord_id: 1_218_840_276_405_911_602,
                member_id: None
            })
        );
        for text in ["review_guide", "rollups", "always_on:personal:42", "12345"] {
            assert_eq!(target_ref("other", text), None, "{text}");
        }
    }

    #[test]
    fn server_targets_are_never_accounts() {
        assert_eq!(target_ref("set_sync_enabled", "1200000000000000001"), None);
    }

    #[test]
    fn filters_narrow_the_audit_query() {
        let filter = ActionFilter::parse(ActionParams {
            limit: None,
            before: Some("1760500000123456.42".into()),
            staff: Some("1218840276405911602".into()),
            action: Some("add_tag".into()),
            from: DateTime::from_timestamp_micros(1_760_000_000_000_000),
            q: Some(" spam ".into()),
        })
        .unwrap();
        let mut sql = QueryBuilder::new(ACTION_COLUMNS);
        filter.push(&mut sql);
        let text = sql.into_sql();
        for part in [
            "(ts, id) < ($1, $2)",
            "actor = $3",
            "action = $4",
            "ts >= $5",
            "details::text",
        ] {
            assert!(text.contains(part), "{part} in {text}");
        }
        let bad_staff = ActionParams {
            limit: None,
            before: None,
            staff: Some("someone".into()),
            action: None,
            from: None,
            q: None,
        };
        assert_eq!(
            ActionFilter::parse(bad_staff).unwrap_err().code(),
            "bad_request"
        );
    }

    #[test]
    fn dashed_uuids_are_never_players() {
        assert_eq!(
            target_ref("set_api_key", "6f3b9d2e-8a41-4c7f-b0e5-2d9c1a8f4e73"),
            None
        );
    }

    #[test]
    fn cursors_round_trip_and_reject_anything_else() {
        let cursor = ActionCursor {
            at: DateTime::from_timestamp_micros(1_760_500_000_123_456).unwrap(),
            id: 42,
        };
        assert_eq!(cursor.encode(), "1760500000123456.42");
        assert_eq!(ActionCursor::parse(&cursor.encode()).unwrap(), cursor);
        for raw in ["", "42", "x.1", "1.y", "1.2.3"] {
            assert_eq!(
                ActionCursor::parse(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn the_page_size_is_bounded() {
        assert_eq!(action_limit(None).unwrap(), DEFAULT_ACTIONS);
        assert_eq!(
            action_limit(Some(MAX_ACTIONS + 1)).unwrap_err().code(),
            "bad_request"
        );
        assert_eq!(action_limit(Some(0)).unwrap_err().code(), "bad_request");
    }
}
