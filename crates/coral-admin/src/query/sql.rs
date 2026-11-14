use chrono::{DateTime, Utc};
use reqlog::{Form, static_templates, template_path_pattern};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::filter::{Bounds, Condition, Keyset};
use super::players::PlayerKeys;
use super::{MemberIdentity, MemberRole, Plan, Source};
use crate::refs::Match;

pub const REQUEST_KEY: &str = "coalesce(l.request_id, md5('arl:' || l.id)::uuid)";
const LEGACY_ROW: &str = "(l.request_id IS NULL OR uuid_extract_version(l.request_id) <> 7)";
const LOG_COLUMNS: &str = "l.ts, l.method, l.route, l.path, l.query, l.status, l.outcome_status, \
     l.cause, l.error, l.latency_ms, l.key_kind, l.key_prefix, l.member_id, l.actor_id, l.actor_uuid, l.ip, \
     l.country, l.user_agent, l.subjects";
const SEMANTIC_STATUS: &str = "coalesce(l.outcome_status, l.status)";
const RESOLVED_SUBJECT: &str = "x.name IS NOT NULL";
const WIRE_STATUS: &str = "l.status";
const LOG: Columns = Columns {
    ts: "l.ts",
    id: REQUEST_KEY,
};
const SUBJECT: Columns = Columns {
    ts: "s.ts",
    id: "s.request_id",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    Legacy,
    Attributed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraRange {
    pub era: Era,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberArm {
    Own,
    Acted,
    Prefixes,
}

#[derive(Debug, Clone, Copy)]
struct Columns {
    ts: &'static str,
    id: &'static str,
}

type Sql = QueryBuilder<'static, Postgres>;

impl Condition {
    pub fn admits(&self, era: Era) -> bool {
        match self {
            Self::Actor(_)
            | Self::ActorPlayer(_)
            | Self::Cause(_)
            | Self::KeyKind(_)
            | Self::Country(_) => era == Era::Attributed,
            _ => true,
        }
    }

    fn push(&self, era: Era, sql: &mut Sql) {
        match self {
            Self::Method(method) => {
                sql.push("l.method = ").push_bind(method.clone());
            }
            Self::Route(template) => push_route(template, era, sql),
            Self::Status(status) => push_status(*status, era, sql),
            Self::Class(class) => {
                let (low, high) = class.range();
                let column = status_column(era);
                sql.push(column).push(" BETWEEN ").push_bind(low);
                sql.push(" AND ").push_bind(high);
            }
            Self::Cause(cause) => {
                sql.push("l.cause = ").push_bind(cause.code());
            }
            Self::KeyKind(kind) => {
                sql.push("l.key_kind = ").push_bind(kind.label());
            }
            Self::Actor(id) => {
                sql.push("l.actor_id = ").push_bind(*id);
            }
            Self::ActorPlayer(uuid) => {
                sql.push("l.actor_uuid = ").push_bind(*uuid);
            }
            Self::Ip(ip) => {
                sql.push("l.ip = ").push_bind(ip.clone());
            }
            Self::KeyPrefix(prefix) => {
                sql.push("l.key_prefix = ").push_bind(prefix.clone());
            }
            Self::Country(code) => {
                sql.push("l.country = ").push_bind(code.clone());
            }
            Self::Text(pattern) => push_text(pattern, sql),
        }
    }
}

impl MemberIdentity {
    pub fn arms(&self, era: Era) -> Vec<MemberArm> {
        let mut arms = vec![MemberArm::Own];
        if era == Era::Attributed && self.discord_id.is_some() {
            arms.push(MemberArm::Acted);
        }
        if era == Era::Legacy && !self.prefixes.is_empty() {
            arms.push(MemberArm::Prefixes);
        }
        arms
    }

    fn push_arm(&self, arm: MemberArm, sql: &mut Sql) {
        match arm {
            MemberArm::Own => {
                sql.push("l.member_id = ").push_bind(self.id);
            }
            MemberArm::Acted => {
                sql.push("l.actor_id = ").push_bind(self.discord_id);
                sql.push(" AND l.member_id IS DISTINCT FROM ")
                    .push_bind(self.id);
            }
            MemberArm::Prefixes => {
                sql.push("l.member_id IS NULL AND l.key_prefix = ANY(")
                    .push_bind(self.prefixes.clone())
                    .push(")");
            }
        }
    }

    fn push_any_arm(&self, era: Era, sql: &mut Sql) {
        sql.push("(");
        for (index, arm) in self.arms(era).into_iter().enumerate() {
            if index > 0 {
                sql.push(" OR ");
            }
            sql.push("(");
            self.push_arm(arm, sql);
            sql.push(")");
        }
        sql.push(")");
    }
}

pub fn log_row_select() -> String {
    format!(
        "SELECT {LOG_COLUMNS}, {REQUEST_KEY} AS request_key, {LEGACY_ROW} AS legacy, \
         NULL::text AS matched, m.discord_id AS member_discord_id, am.id AS actor_member_id \
         FROM api_request_log l LEFT JOIN members m ON m.id = l.member_id \
         LEFT JOIN members am ON am.discord_id = l.actor_id"
    )
}

pub fn render(plan: &Plan, slice: Bounds, limit: i64) -> Option<Sql> {
    let branches = branches(plan, slice);
    if branches.is_empty() {
        return None;
    }
    let mut sql =
        Sql::new("SELECT b.*, m.discord_id AS member_discord_id, am.id AS actor_member_id FROM (");
    for (index, (era, bounds, arm)) in branches.into_iter().enumerate() {
        if index > 0 {
            sql.push(" UNION ALL ");
        }
        match &plan.source {
            Source::Log => push_log_branch(plan, era, bounds, arm, limit, &mut sql),
            Source::Subjects(keys) => push_subject_branch(plan, keys, era, bounds, limit, &mut sql),
        }
    }
    let order = page_order(plan.keyset);
    sql.push(format!(
        ") b LEFT JOIN members m ON m.id = b.member_id \
         LEFT JOIN members am ON am.discord_id = b.actor_id \
         ORDER BY b.ts {order}, b.request_key {order} LIMIT "
    ))
    .push_bind(limit);
    Some(sql)
}

fn branches(plan: &Plan, slice: Bounds) -> Vec<(Era, Bounds, Option<MemberArm>)> {
    let mut branches = Vec::new();
    for range in &plan.eras {
        let bounds = range.bounds.intersect(slice);
        if bounds.is_empty() {
            continue;
        }
        match (&plan.source, &plan.member) {
            (Source::Log, Some(member)) if member.role == MemberRole::Driver => {
                for arm in member.identity.arms(range.era) {
                    branches.push((range.era, bounds, Some(arm)));
                }
            }
            _ => branches.push((range.era, bounds, None)),
        }
    }
    branches
}

fn push_log_branch(
    plan: &Plan,
    era: Era,
    bounds: Bounds,
    arm: Option<MemberArm>,
    limit: i64,
    sql: &mut Sql,
) {
    sql.push("(SELECT ");
    push_log_columns(sql);
    sql.push(", NULL::text AS matched FROM api_request_log l WHERE TRUE");
    push_bounds(LOG, bounds, sql);
    push_keyset(LOG, plan.keyset, sql);
    push_residuals(plan, era, sql);
    if let (Some(arm), Some(member)) = (arm, &plan.member) {
        sql.push(" AND ");
        member.identity.push_arm(arm, sql);
    }
    push_log_order(plan.keyset, limit, sql);
}

fn push_subject_branch(
    plan: &Plan,
    keys: &PlayerKeys,
    era: Era,
    bounds: Bounds,
    limit: i64,
    sql: &mut Sql,
) {
    let lateral_limit = plan.residuals_are_empty().then_some(limit);
    sql.push("(SELECT ");
    push_log_columns(sql);
    sql.push(
        ", hits.matched FROM (SELECT DISTINCT ON (h.ts, h.request_id) h.ts, h.request_id, h.matched FROM (",
    );
    if !keys.uuids.is_empty() {
        push_uuid_lateral(&keys.uuids, plan.keyset, bounds, lateral_limit, sql);
    }
    if !keys.names.is_empty() {
        if !keys.uuids.is_empty() {
            sql.push(" UNION ALL ");
        }
        push_name_lateral(keys, plan, bounds, lateral_limit, sql);
    }
    let order = page_order(plan.keyset);
    sql.push(format!(
        ") h ORDER BY h.ts {order}, h.request_id {order}, h.rank"
    ));
    push_limit(lateral_limit, sql);
    sql.push(") hits JOIN api_request_log l ON l.ts = hits.ts AND ")
        .push(REQUEST_KEY)
        .push(" = hits.request_id WHERE TRUE");
    push_residuals(plan, era, sql);
    push_log_order(plan.keyset, limit, sql);
}

fn push_uuid_lateral(
    uuids: &[Uuid],
    keyset: Keyset,
    bounds: Bounds,
    limit: Option<i64>,
    sql: &mut Sql,
) {
    let order = page_order(keyset);
    sql.push("SELECT x.ts, x.request_id, ")
        .push(match_columns(
            RESOLVED_SUBJECT,
            Match::ResolvedName,
            Match::Uuid,
        ))
        .push(" FROM unnest(")
        .push_bind(uuids.to_vec())
        .push(
            "::uuid[]) AS v(player) CROSS JOIN LATERAL (\
         SELECT DISTINCT ON (s.ts, s.request_id) s.ts, s.request_id, s.name \
         FROM api_request_subject s WHERE s.player = v.player",
        );
    push_bounds(SUBJECT, bounds, sql);
    push_keyset(SUBJECT, keyset, sql);
    sql.push(format!(
        " ORDER BY s.ts {order}, s.request_id {order}, s.name IS NOT NULL"
    ));
    push_limit(limit, sql);
    sql.push(") x");
}

fn push_name_lateral(
    keys: &PlayerKeys,
    plan: &Plan,
    bounds: Bounds,
    limit: Option<i64>,
    sql: &mut Sql,
) {
    let display = Form::Display.code();
    let order = page_order(plan.keyset);
    let names: Vec<String> = keys.names.iter().map(|held| held.name.clone()).collect();
    let froms: Vec<Option<DateTime<Utc>>> = keys.names.iter().map(|held| held.from).collect();
    let tos: Vec<Option<DateTime<Utc>>> = keys.names.iter().map(|held| held.to).collect();
    sql.push("SELECT x.ts, x.request_id, ")
        .push(match_columns(
            &format!("x.form = {display}"),
            Match::DisplayName,
            Match::NameOnly,
        ))
        .push(" FROM unnest(")
        .push_bind(names)
        .push("::text[], ")
        .push_bind(froms)
        .push("::timestamptz[], ")
        .push_bind(tos)
        .push(
            "::timestamptz[]) AS v(name, held_from, held_to) CROSS JOIN LATERAL (\
         SELECT DISTINCT ON (s.ts, s.request_id) s.ts, s.request_id, s.form \
         FROM api_request_subject s WHERE s.player IS NULL AND s.name = v.name \
         AND s.ts >= coalesce(v.held_from, '-infinity') \
         AND s.ts < coalesce(v.held_to, 'infinity')",
        );
    push_bounds(SUBJECT, bounds, sql);
    push_keyset(SUBJECT, plan.keyset, sql);
    if !plan.include_display {
        sql.push(format!(" AND s.form <> {display}"));
    }
    sql.push(format!(
        " ORDER BY s.ts {order}, s.request_id {order}, s.form = {display}"
    ));
    push_limit(limit, sql);
    sql.push(") x");
}

fn match_columns(matches_when: &str, when: Match, otherwise: Match) -> String {
    format!(
        "CASE WHEN {matches_when} THEN '{}' ELSE '{}' END AS matched, \
         CASE WHEN {matches_when} THEN {} ELSE {} END AS rank",
        when.label(),
        otherwise.label(),
        when.preference(),
        otherwise.preference(),
    )
}

fn push_log_columns(sql: &mut Sql) {
    sql.push(LOG_COLUMNS)
        .push(", ")
        .push(REQUEST_KEY)
        .push(" AS request_key, ")
        .push(LEGACY_ROW)
        .push(" AS legacy");
}

fn push_residuals(plan: &Plan, era: Era, sql: &mut Sql) {
    for condition in &plan.conditions {
        sql.push(" AND ");
        condition.push(era, sql);
    }
    if let Some(member) = &plan.member
        && member.role == MemberRole::Residual
    {
        sql.push(" AND ");
        member.identity.push_any_arm(era, sql);
    }
}

fn push_bounds(columns: Columns, bounds: Bounds, sql: &mut Sql) {
    if let Some(lo) = bounds.lo {
        sql.push(format!(" AND {} >= ", columns.ts)).push_bind(lo);
    }
    if let Some(hi) = bounds.hi {
        sql.push(format!(" AND {} < ", columns.ts)).push_bind(hi);
    }
}

fn push_keyset(columns: Columns, keyset: Keyset, sql: &mut Sql) {
    let (cursor, inclusive, strict) = match keyset {
        Keyset::Newest => return,
        Keyset::Before(cursor) => (cursor, "<=", "<"),
        Keyset::After(cursor) => (cursor, ">=", ">"),
    };
    let Columns { ts, id } = columns;
    sql.push(format!(" AND {ts} {inclusive} "))
        .push_bind(cursor.ts);
    sql.push(format!(" AND ({ts} {strict} "))
        .push_bind(cursor.ts);
    sql.push(format!(" OR {id} {strict} "))
        .push_bind(cursor.request_id)
        .push(")");
}

fn push_log_order(keyset: Keyset, limit: i64, sql: &mut Sql) {
    let order = page_order(keyset);
    sql.push(format!(
        " ORDER BY l.ts {order}, request_key {order} LIMIT "
    ))
    .push_bind(limit)
    .push(")");
}

fn page_order(keyset: Keyset) -> &'static str {
    match keyset.reads_forward() {
        true => "ASC",
        false => "DESC",
    }
}

fn push_limit(limit: Option<i64>, sql: &mut Sql) {
    if let Some(limit) = limit {
        sql.push(" LIMIT ").push_bind(limit);
    }
}

fn push_route(template: &'static str, era: Era, sql: &mut Sql) {
    match era {
        Era::Attributed => {
            sql.push("l.route = ").push_bind(template);
        }
        Era::Legacy if reqlog::is_static_template(template) => {
            sql.push("l.path = ").push_bind(template);
        }
        Era::Legacy => {
            let statics: Vec<&'static str> = static_templates().collect();
            sql.push("l.path ~ ")
                .push_bind(template_path_pattern(template))
                .push(" AND l.path <> ALL(")
                .push_bind(statics)
                .push(")");
        }
    }
}

fn push_status(status: i16, era: Era, sql: &mut Sql) {
    match era {
        Era::Attributed => {
            sql.push("((l.status = ").push_bind(status);
            sql.push(" AND l.outcome_status IS NULL) OR l.outcome_status = ")
                .push_bind(status)
                .push(")");
        }
        Era::Legacy => {
            sql.push("l.status = ").push_bind(status);
        }
    }
}

fn push_text(pattern: &str, sql: &mut Sql) {
    sql.push("(");
    for (index, column) in ["l.path", "l.query", "l.error", "l.user_agent"]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            sql.push(" OR ");
        }
        sql.push(format!("{column} ILIKE "))
            .push_bind(pattern.to_string());
    }
    sql.push(")");
}

fn status_column(era: Era) -> &'static str {
    match era {
        Era::Attributed => SEMANTIC_STATUS,
        Era::Legacy => WIRE_STATUS,
    }
}
