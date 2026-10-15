use crate::entry::{Cause, KeyKind, Service};
use crate::routes::{
    ROUTE_TEMPLATES, is_static_template, static_templates, template_head, template_path_pattern,
};
use crate::status::{
    FIRST_CLIENT_ERROR, FIRST_SERVER_ERROR, FORBIDDEN, NOT_FOUND, TOO_MANY_REQUESTS, UNAUTHORIZED,
};
use crate::{KEY_PREFIX_CHARS, TIME_ORDERED_VERSION};

use super::LATENCY_BUCKET_BOUNDS_MS;

const LEGACY_ANONYMOUS: &str = "none";
const PERSONAL_KEY: &str = "personal";
const DEVELOPER_KEY: &str = "developer";
const UPSTREAM_LIMIT_ERROR: &str = r#"{"error":"Rate limited"}"#;
const STARFISH_ROUTES: &str = "/api/v1/starfish/";
const BIN_ORIGIN: &str = "1970-01-01 00:00:00+00";

pub fn rolled_rows() -> String {
    format!(
        "{known},
classified AS (
  SELECT l.ts, l.method, l.path, l.route, l.status, l.outcome_status, l.cause, l.error,
         l.user_agent, l.key_kind, l.member_id, l.actor_id, l.key_prefix, l.latency_ms, l.subjects,
         l.request_id IS NULL OR uuid_extract_version(l.request_id) <> {TIME_ORDERED_VERSION} AS legacy,
         k.prefix IS NOT NULL AS known_prefix, k.member_id AS known_member, k.kind AS known_kind
  FROM api_request_log l
  LEFT JOIN known k ON k.prefix = l.key_prefix
  WHERE l.ts >= $1 AND l.ts < $2
),
attributed AS (
  SELECT c.ts, c.method, c.path, c.error, c.user_agent, c.actor_id, c.key_prefix, c.known_prefix,
         c.latency_ms, c.cause,
         CASE WHEN c.legacy THEN {legacy_route} ELSE c.route END AS route,
         coalesce(c.member_id, CASE WHEN c.legacy THEN c.known_member END) AS member_id,
         coalesce(c.outcome_status, c.status) AS status,
         CASE WHEN c.legacy THEN {legacy_kind} ELSE {recorded_kind} END AS key_kind,
         coalesce(c.subjects, 0) AS subjects
  FROM classified c
),
rolled AS (
  SELECT a.ts, a.route, a.method, a.status, a.key_kind, a.member_id, a.actor_id,
         a.latency_ms, a.subjects,
         width_bucket(a.latency_ms, {latency_bounds}) AS latency_bucket,
         coalesce(a.cause, {legacy_cause}) AS cause,
         CASE WHEN a.member_id IS NULL AND a.known_prefix THEN a.key_prefix END AS key_prefix,
         CASE WHEN a.key_kind = {internal} THEN {service} ELSE {unknown_service} END AS service
  FROM attributed a
)",
        known = known_prefixes(),
        legacy_route = legacy_route("c.path"),
        legacy_kind = legacy_key_kind(),
        recorded_kind = recorded_key_kind("c.key_kind"),
        latency_bounds = latency_bounds(),
        legacy_cause = legacy_cause(),
        internal = smallint(KeyKind::Internal.code()),
        service = service_from_agent("a.user_agent"),
        unknown_service = smallint(Service::Unknown.code()),
    )
}

pub(crate) fn known_prefixes() -> String {
    format!(
        "known AS (
  SELECT left(api_key, {KEY_PREFIX_CHARS}) AS prefix, min(member_id) AS member_id,
         CASE WHEN count(DISTINCT kind) = 1 THEN min(kind) END AS kind
  FROM member_api_keys
  GROUP BY left(api_key, {KEY_PREFIX_CHARS})
  HAVING count(DISTINCT member_id) = 1
)"
    )
}

pub fn time_bin(stride: &str, column: &str) -> String {
    format!(
        "date_bin(interval {stride}, {column}, timestamptz {origin})",
        stride = quoted(stride),
        origin = quoted(BIN_ORIGIN),
    )
}

pub fn latency_bucket_count() -> usize {
    LATENCY_BUCKET_BOUNDS_MS.len() + 1
}

pub fn legacy_route(path: &str) -> String {
    let statics: Vec<String> = static_templates().map(quoted).collect();
    let dynamic: Vec<String> = ROUTE_TEMPLATES
        .iter()
        .copied()
        .filter(|template| !is_static_template(template))
        .map(|template| {
            format!(
                "WHEN starts_with({path}, {head}) AND {path} ~ {pattern} THEN {template}",
                head = quoted(template_head(template)),
                pattern = quoted(&template_path_pattern(template)),
                template = quoted(template),
            )
        })
        .collect();
    format!(
        "CASE WHEN {path} = ANY (ARRAY[{statics}]) THEN {path} {dynamic} END",
        statics = statics.join(", "),
        dynamic = dynamic.join(" "),
    )
}

pub fn status_cause(status: &str, route: &str) -> String {
    format!(
        "CASE WHEN {status} < {FIRST_CLIENT_ERROR} THEN {ok} \
         WHEN {status} IN ({UNAUTHORIZED}, {FORBIDDEN}) THEN {auth} \
         WHEN {status} = {NOT_FOUND} AND {route} IS NULL THEN {unmatched} \
         WHEN {status} = {TOO_MANY_REQUESTS} THEN {caller_limit} \
         WHEN {status} < {FIRST_SERVER_ERROR} THEN {validation} \
         ELSE {internal} END",
        ok = smallint(Cause::Ok.code()),
        auth = smallint(Cause::Auth.code()),
        unmatched = smallint(Cause::Unmatched.code()),
        caller_limit = smallint(Cause::CallerLimit.code()),
        validation = smallint(Cause::Validation.code()),
        internal = smallint(Cause::Internal.code()),
    )
}

pub fn service_from_agent(agent: &str) -> String {
    let products: Vec<String> = Service::ALL
        .into_iter()
        .filter_map(|service| {
            let product = service.agent()?;
            Some(format!(
                "WHEN {} THEN {}",
                quoted(product),
                smallint(service.code())
            ))
        })
        .collect();
    format!(
        "CASE btrim(split_part({agent}, '/', 1), E' \\t') {products} ELSE {unknown} END",
        products = products.join(" "),
        unknown = smallint(Service::Unknown.code()),
    )
}

pub(crate) fn legacy_key_kind() -> String {
    format!(
        "CASE WHEN c.key_kind = {anonymous} AND c.member_id IS NOT NULL THEN {session} \
         WHEN c.key_kind = {anonymous} THEN {none} \
         WHEN c.known_kind = {personal_key} THEN {personal} \
         WHEN c.known_kind = {developer_key} THEN {developer} \
         WHEN c.key_prefix = $3 THEN {internal} \
         WHEN c.member_id IS NULL AND NOT c.known_prefix THEN {invalid} \
         ELSE {legacy} END",
        anonymous = quoted(LEGACY_ANONYMOUS),
        personal_key = quoted(PERSONAL_KEY),
        developer_key = quoted(DEVELOPER_KEY),
        session = smallint(KeyKind::Session.code()),
        none = smallint(KeyKind::None.code()),
        personal = smallint(KeyKind::Personal.code()),
        developer = smallint(KeyKind::Developer.code()),
        internal = smallint(KeyKind::Internal.code()),
        invalid = smallint(KeyKind::Invalid.code()),
        legacy = smallint(KeyKind::Legacy.code()),
    )
}

pub(crate) fn recorded_key_kind(label: &str) -> String {
    let kinds: Vec<String> = KeyKind::ALL
        .into_iter()
        .map(|kind| {
            format!(
                "WHEN {} THEN {}",
                quoted(kind.label()),
                smallint(kind.code())
            )
        })
        .collect();
    format!(
        "CASE {label} {kinds} ELSE {legacy} END",
        kinds = kinds.join(" "),
        legacy = smallint(KeyKind::Legacy.code()),
    )
}

fn legacy_cause() -> String {
    format!(
        "CASE WHEN a.status = {TOO_MANY_REQUESTS} AND a.error = {upstream_error} \
         AND NOT starts_with(a.path, {starfish}) THEN {upstream_limit} \
         ELSE {by_status} END",
        upstream_error = quoted(UPSTREAM_LIMIT_ERROR),
        starfish = quoted(STARFISH_ROUTES),
        upstream_limit = smallint(Cause::UpstreamLimit.code()),
        by_status = status_cause("a.status", "a.route"),
    )
}

fn latency_bounds() -> String {
    let bounds: Vec<String> = LATENCY_BUCKET_BOUNDS_MS
        .iter()
        .map(ToString::to_string)
        .collect();
    format!("ARRAY[{}]", bounds.join(", "))
}

pub(crate) fn smallint(code: i16) -> String {
    format!("{code}::int2")
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_quoted_for_sql() {
        assert_eq!(quoted("/v3/players"), "'/v3/players'");
        assert_eq!(quoted("it's"), "'it''s'");
    }

    #[test]
    fn time_bins_start_at_the_unix_epoch_in_utc() {
        assert_eq!(
            time_bin("1 hour", "r.ts"),
            "date_bin(interval '1 hour', r.ts, timestamptz '1970-01-01 00:00:00+00')"
        );
    }

    #[test]
    fn every_latency_bound_is_listed_once() {
        assert_eq!(
            latency_bounds(),
            "ARRAY[10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000, 30000]"
        );
        assert_eq!(latency_bucket_count(), 12);
    }

    #[test]
    fn the_legacy_route_case_lists_statics_then_each_dynamic_template_in_order() {
        let case = legacy_route("p");
        let statics = case.find("= ANY").unwrap();
        let markers = case.find("'/v3/player/sessions/markers/{name}'").unwrap();
        let resolve = case.find("'/v3/resolve/{identifier}'").unwrap();
        let yank = case
            .find("'/api/v1/starfish/plugins/{slug}/releases/{version}/yank'")
            .unwrap();
        assert!(statics < markers && markers < resolve && resolve < yank);
        assert!(case.contains(
            "WHEN starts_with(p, '/v3/resolve/') AND p ~ '^/v3/resolve/[^/]+$' THEN '/v3/resolve/{identifier}'"
        ));
    }
}
