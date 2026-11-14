use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use database::Database;
use reqlog::{Cause, KeyKind, Service};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, QueryBuilder};

use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::names::{LabelRequest, NameService};
use crate::query::{Attribution, Condition, RequestFilter, Selection, StatusClass};
use crate::refs::{EntityRef, Labels};
use crate::serde_ts;

pub const DEFAULT_GROUP_SPAN: TimeDelta = TimeDelta::hours(24);
pub const DEFAULT_GROUP_LIMIT: usize = 100;
pub const MAX_GROUP_LIMIT: usize = 500;

const ROLLUP_BUCKET: TimeDelta = TimeDelta::hours(1);
const GROUPING_WORK_MEM: &str = "32MB";
const TIE_BREAKERS: [&str; 5] = ["key_kind", "service", "member_id", "actor_id", "key_prefix"];
const TOO_MANY_REQUESTS: i16 = 429;
const FIRST_ERROR_STATUS: i16 = 400;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub route: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub class: Option<StatusClass>,
    pub cause: Option<Cause>,
    pub key_kind: Option<KeyKind>,
    pub member: Option<i64>,
    pub actor: Option<String>,
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub sort: GroupSort,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupSort {
    #[default]
    Requests,
    RateLimited,
    Errors,
    Subjects,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupPage {
    #[serde(serialize_with = "serde_ts::micros")]
    pub from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub to: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub clamped_from: Option<DateTime<Utc>>,
    pub groups: Vec<RequesterGroup>,
    pub truncated: bool,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequesterGroup {
    pub requester: EntityRef,
    pub actor: Option<EntityRef>,
    pub key_kind: KeyKind,
    pub service: Service,
    pub key_prefix: Option<String>,
    pub requests: i64,
    pub rate_limited: i64,
    pub errors: i64,
    pub subjects: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub first: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub last: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HourWindow {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    clamped_from: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct GroupRow {
    key_kind: i16,
    service: i16,
    member_id: Option<i64>,
    member_discord_id: Option<i64>,
    actor_id: Option<i64>,
    actor_member_id: Option<i64>,
    key_prefix: Option<String>,
    requests: i64,
    rate_limited: i64,
    errors: i64,
    subjects: i64,
    first_hour: DateTime<Utc>,
    last_hour: DateTime<Utc>,
}

type Sql = QueryBuilder<'static, Postgres>;

impl GroupSort {
    fn column(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::RateLimited => "rate_limited",
            Self::Errors => "errors",
            Self::Subjects => "subjects",
        }
    }
}

impl GroupFilter {
    fn selection(&self) -> Result<Selection, AdminError> {
        Selection::parse(RequestFilter {
            from: self.from,
            to: self.to,
            route: self.route.clone(),
            method: self.method.clone(),
            status: self.status,
            class: self.class,
            cause: self.cause,
            key_kind: self.key_kind,
            member: self.member,
            actor: self.actor.clone(),
            key_prefix: self.key_prefix.clone(),
            ..RequestFilter::default()
        })
    }

    fn limit(&self) -> Result<usize, AdminError> {
        let limit = self.limit.unwrap_or(DEFAULT_GROUP_LIMIT);
        if !(1..=MAX_GROUP_LIMIT).contains(&limit) {
            return Err(AdminError::BadRequest(format!(
                "limit must be between 1 and {MAX_GROUP_LIMIT}"
            )));
        }
        Ok(limit)
    }
}

impl HourWindow {
    fn snap(
        selection: &Selection,
        coverage: &Coverage,
        now: DateTime<Utc>,
    ) -> Result<Self, AdminError> {
        let rolled_from = coverage.rollups_from.ok_or_else(|| {
            AdminError::NotFound("traffic rollups have not recorded any hour yet".into())
        })?;
        let to = ceil_hour(selection.window.hi.unwrap_or(now))?;
        let requested = floor_hour(selection.window.lo.unwrap_or(to - DEFAULT_GROUP_SPAN))?;
        let floor = floor_hour(rolled_from)?;
        Ok(Self {
            from: requested.max(floor).min(to),
            to,
            clamped_from: (requested < floor).then_some(floor),
        })
    }
}

impl GroupRow {
    fn into_group(self) -> RequesterGroup {
        let attribution = Attribution {
            key_kind: KeyKind::from_code(self.key_kind).unwrap_or_else(|| {
                tracing::warn!(
                    code = self.key_kind,
                    "caller rollup row has an unknown key kind"
                );
                KeyKind::Legacy
            }),
            service: Service::from_code(self.service).unwrap_or(Service::Unknown),
            member_id: self.member_id,
            member_discord_id: self.member_discord_id,
            actor_id: self.actor_id,
            actor_member_id: self.actor_member_id,
            actor_uuid: None,
            key_prefix: self.key_prefix.clone(),
        };
        RequesterGroup {
            requester: attribution.requester(),
            actor: attribution.actor(),
            key_kind: attribution.key_kind,
            service: attribution.service,
            key_prefix: self.key_prefix,
            requests: self.requests,
            rate_limited: self.rate_limited,
            errors: self.errors,
            subjects: self.subjects,
            first: self.first_hour,
            last: self.last_hour,
        }
    }
}

pub async fn read_groups(
    db: &Database,
    names: &NameService,
    coverage: &Coverage,
    filter: GroupFilter,
) -> Result<GroupPage, AdminError> {
    let selection = filter.selection()?;
    let limit = filter.limit()?;
    let window = HourWindow::snap(&selection, coverage, Utc::now())?;
    let mut sql = group_sql(&selection, window, filter.sort, limit)?;
    let mut tx = db
        .pool()
        .begin()
        .await
        .context("begin reading requester groups")?;
    sqlx::query("SELECT set_config('work_mem', $1, true)")
        .bind(GROUPING_WORK_MEM)
        .execute(&mut *tx)
        .await
        .context("give the requester grouping room to hash")?;
    let mut rows: Vec<GroupRow> = sql
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .context("read requester groups from the caller rollup")?;
    tx.commit()
        .await
        .context("finish reading requester groups")?;
    let truncated = rows.len() > limit;
    rows.truncate(limit);
    let groups: Vec<RequesterGroup> = rows.into_iter().map(GroupRow::into_group).collect();
    let labels = names.cached(&labels_for(&groups)).await?;
    Ok(GroupPage {
        from: window.from,
        to: window.to,
        clamped_from: window.clamped_from,
        groups,
        truncated,
        labels,
    })
}

fn group_sql(
    selection: &Selection,
    window: HourWindow,
    sort: GroupSort,
    limit: usize,
) -> Result<Sql, AdminError> {
    let mut sql = Sql::new(format!(
        "SELECT g.*, m.discord_id AS member_discord_id, am.id AS actor_member_id FROM (\
         SELECT c.key_kind, c.service, c.member_id, c.actor_id, c.key_prefix, \
         sum(c.requests)::int8 AS requests, \
         coalesce(sum(c.requests) FILTER (WHERE c.status = {TOO_MANY_REQUESTS}), 0)::int8 AS rate_limited, \
         coalesce(sum(c.requests) FILTER (WHERE c.status >= {FIRST_ERROR_STATUS} AND c.status <> {TOO_MANY_REQUESTS}), 0)::int8 AS errors, \
         sum(c.subjects)::int8 AS subjects, min(c.hour) AS first_hour, max(c.hour) AS last_hour \
         FROM api_caller_hour c WHERE c.hour >= "
    ));
    sql.push_bind(window.from);
    sql.push(" AND c.hour < ").push_bind(window.to);
    for condition in &selection.conditions {
        sql.push(" AND ");
        push_condition(condition, &mut sql)?;
    }
    if let Some(member) = selection.member {
        sql.push(" AND (c.member_id = ")
            .push_bind(member)
            .push(" OR c.actor_id = (SELECT discord_id FROM members WHERE id = ")
            .push_bind(member)
            .push("))");
    }
    sql.push(format!(
        " GROUP BY c.key_kind, c.service, c.member_id, c.actor_id, c.key_prefix ORDER BY {} LIMIT ",
        order_by(sort, "")
    ))
    .push_bind(limit as i64 + 1);
    sql.push(format!(
        ") g LEFT JOIN members m ON m.id = g.member_id \
         LEFT JOIN members am ON am.discord_id = g.actor_id ORDER BY {}",
        order_by(sort, "g.")
    ));
    Ok(sql)
}

fn order_by(sort: GroupSort, alias: &str) -> String {
    let leading = match sort {
        GroupSort::Requests => vec![GroupSort::Requests],
        other => vec![other, GroupSort::Requests],
    };
    leading
        .into_iter()
        .map(|column| format!("{alias}{} DESC", column.column()))
        .chain(TIE_BREAKERS.iter().map(|column| format!("{alias}{column}")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_condition(condition: &Condition, sql: &mut Sql) -> Result<(), AdminError> {
    match condition {
        Condition::Method(method) => sql.push("c.method = ").push_bind(method.clone()),
        Condition::Route(template) => sql.push("c.route = ").push_bind(*template),
        Condition::Status(status) => sql.push("c.status = ").push_bind(*status),
        Condition::Class(class) => {
            let (low, high) = class.range();
            sql.push("c.status BETWEEN ")
                .push_bind(low)
                .push(" AND ")
                .push_bind(high)
        }
        Condition::Cause(cause) => sql.push("c.cause = ").push_bind(cause.code()),
        Condition::KeyKind(kind) => sql.push("c.key_kind = ").push_bind(kind.code()),
        Condition::Actor(actor) => sql.push("c.actor_id = ").push_bind(*actor),
        Condition::KeyPrefix(prefix) => sql.push("c.key_prefix = ").push_bind(prefix.clone()),
        Condition::ActorPlayer(_)
        | Condition::Ip(_)
        | Condition::Country(_)
        | Condition::Text(_) => {
            return Err(AdminError::BadRequest(
                "requester groups filter only by rollup dimensions".into(),
            ));
        }
    };
    Ok(())
}

fn labels_for(groups: &[RequesterGroup]) -> LabelRequest {
    LabelRequest {
        discord: groups
            .iter()
            .flat_map(|group| [Some(&group.requester), group.actor.as_ref()])
            .flatten()
            .filter_map(|entity| match entity {
                EntityRef::Account { discord_id, .. } => Some(*discord_id),
                _ => None,
            })
            .collect(),
        minecraft: Vec::new(),
    }
}

fn floor_hour(at: DateTime<Utc>) -> Result<DateTime<Utc>, AdminError> {
    at.duration_trunc(ROLLUP_BUCKET).map_err(|error| {
        AdminError::BadRequest(format!("{at} cannot be snapped to an hour: {error}"))
    })
}

fn ceil_hour(at: DateTime<Utc>) -> Result<DateTime<Utc>, AdminError> {
    let floor = floor_hour(at)?;
    Ok(match floor == at {
        true => floor,
        false => floor + ROLLUP_BUCKET,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_order_by_the_chosen_column_then_by_requests() {
        assert_eq!(
            order_by(GroupSort::Requests, ""),
            "requests DESC, key_kind, service, member_id, actor_id, key_prefix"
        );
        assert_eq!(
            order_by(GroupSort::Errors, "g."),
            "g.errors DESC, g.requests DESC, g.key_kind, g.service, g.member_id, g.actor_id, g.key_prefix"
        );
    }
}
