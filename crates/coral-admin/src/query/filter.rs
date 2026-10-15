use std::net::IpAddr;

use chrono::{DateTime, Utc};
use reqlog::{Cause, KEY_PREFIX_CHARS, KeyKind, ROUTE_TEMPLATES};
use serde::Deserialize;

use super::cursor::Cursor;
use super::players::PlayerInput;
use crate::error::AdminError;

pub const DEFAULT_PAGE_LIMIT: usize = 100;
pub const MAX_PAGE_LIMIT: usize = 500;

const LOWEST_STATUS: u16 = 100;
const HIGHEST_STATUS: u16 = 599;
const SUCCESS_STATUSES: (i16, i16) = (200, 299);
const CLIENT_ERROR_STATUSES: (i16, i16) = (400, 499);
const SERVER_ERROR_STATUSES: (i16, i16) = (500, 599);
const ERROR_STATUSES: (i16, i16) = (CLIENT_ERROR_STATUSES.0, SERVER_ERROR_STATUSES.1);
const RATE_LIMITED_STATUSES: (i16, i16) = (429, 429);
const LIKE_ESCAPED: [char; 3] = ['\\', '%', '_'];
const COUNTRY_CODE_CHARS: usize = 2;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestFilter {
    pub cursor: Option<Cursor>,
    pub after: Option<Cursor>,
    pub limit: Option<usize>,
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
    pub ip: Option<String>,
    pub key_prefix: Option<String>,
    pub country: Option<String>,
    pub player: Option<String>,
    #[serde(default)]
    pub include_display: bool,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum StatusClass {
    #[serde(rename = "2xx")]
    Success,
    #[serde(rename = "4xx")]
    ClientError,
    #[serde(rename = "5xx")]
    ServerError,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "rate_limited")]
    RateLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyset {
    Newest,
    Before(Cursor),
    After(Cursor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub lo: Option<DateTime<Utc>>,
    pub hi: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Condition {
    Method(String),
    Route(&'static str),
    Status(i16),
    Class(StatusClass),
    Cause(Cause),
    KeyKind(KeyKind),
    Actor(i64),
    Ip(String),
    KeyPrefix(String),
    Country(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub keyset: Keyset,
    pub limit: usize,
    pub window: Bounds,
    pub conditions: Vec<Condition>,
    pub member: Option<i64>,
    pub player: Option<PlayerInput>,
    pub include_display: bool,
}

impl StatusClass {
    pub fn range(self) -> (i16, i16) {
        match self {
            Self::Success => SUCCESS_STATUSES,
            Self::ClientError => CLIENT_ERROR_STATUSES,
            Self::ServerError => SERVER_ERROR_STATUSES,
            Self::Error => ERROR_STATUSES,
            Self::RateLimited => RATE_LIMITED_STATUSES,
        }
    }
}

impl Keyset {
    pub fn reads_forward(self) -> bool {
        matches!(self, Self::After(_))
    }
}

impl Bounds {
    pub const OPEN: Self = Self { lo: None, hi: None };

    pub fn intersect(self, other: Self) -> Self {
        Self {
            lo: later(self.lo, other.lo),
            hi: earlier(self.hi, other.hi),
        }
    }

    pub fn is_empty(self) -> bool {
        matches!((self.lo, self.hi), (Some(lo), Some(hi)) if lo >= hi)
    }

    pub fn starts_before(self, at: DateTime<Utc>) -> bool {
        self.lo.is_none_or(|lo| lo < at)
    }
}

impl Condition {
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }
}

impl Selection {
    pub fn parse(filter: RequestFilter) -> Result<Self, AdminError> {
        let keyset = keyset(filter.cursor, filter.after)?;
        let limit = limit(filter.limit)?;
        let window = window(filter.from, filter.to)?;
        let player = filter
            .player
            .as_deref()
            .map(PlayerInput::parse)
            .transpose()?;
        let member = filter.member.map(member).transpose()?;
        let mut conditions = Vec::new();
        conditions.extend(filter.method.as_deref().map(method).transpose()?);
        conditions.extend(filter.route.as_deref().map(route).transpose()?);
        conditions.extend(filter.status.map(status).transpose()?);
        conditions.extend(filter.class.map(Condition::Class));
        conditions.extend(filter.cause.map(Condition::Cause));
        conditions.extend(filter.key_kind.map(Condition::KeyKind));
        conditions.extend(filter.actor.as_deref().map(actor).transpose()?);
        conditions.extend(filter.ip.as_deref().map(ip).transpose()?);
        conditions.extend(filter.key_prefix.as_deref().map(key_prefix).transpose()?);
        conditions.extend(filter.country.as_deref().map(country).transpose()?);
        conditions.extend(filter.q.as_deref().and_then(text));
        Ok(Self {
            keyset,
            limit,
            window,
            conditions,
            member,
            player,
            include_display: filter.include_display,
        })
    }
}

fn keyset(cursor: Option<Cursor>, after: Option<Cursor>) -> Result<Keyset, AdminError> {
    match (cursor, after) {
        (Some(_), Some(_)) => Err(bad("cursor and after cannot be combined")),
        (Some(cursor), None) => Ok(Keyset::Before(cursor)),
        (None, Some(after)) => Ok(Keyset::After(after)),
        (None, None) => Ok(Keyset::Newest),
    }
}

fn limit(requested: Option<usize>) -> Result<usize, AdminError> {
    let limit = requested.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(bad(format!("limit must be between 1 and {MAX_PAGE_LIMIT}")));
    }
    Ok(limit)
}

fn window(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> Result<Bounds, AdminError> {
    if let (Some(from), Some(to)) = (from, to)
        && from >= to
    {
        return Err(bad("from must be earlier than to"));
    }
    Ok(Bounds { lo: from, hi: to })
}

fn method(raw: &str) -> Result<Condition, AdminError> {
    let method = raw.trim().to_ascii_uppercase();
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(bad(format!("{raw:?} is not an HTTP method")));
    }
    Ok(Condition::Method(method))
}

fn route(raw: &str) -> Result<Condition, AdminError> {
    ROUTE_TEMPLATES
        .iter()
        .find(|template| **template == raw)
        .map(|template| Condition::Route(template))
        .ok_or_else(|| bad(format!("{raw:?} is not a known route template")))
}

fn status(status: u16) -> Result<Condition, AdminError> {
    if !(LOWEST_STATUS..=HIGHEST_STATUS).contains(&status) {
        return Err(bad(format!("{status} is not an HTTP status")));
    }
    Ok(Condition::Status(status as i16))
}

fn member(id: i64) -> Result<i64, AdminError> {
    if id <= 0 {
        return Err(bad(format!("{id} is not a member id")));
    }
    Ok(id)
}

fn actor(raw: &str) -> Result<Condition, AdminError> {
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .map(Condition::Actor)
        .ok_or_else(|| bad(format!("{raw:?} is not a Discord id")))
}

fn ip(raw: &str) -> Result<Condition, AdminError> {
    raw.trim()
        .parse::<IpAddr>()
        .map(|ip| Condition::Ip(ip.to_string()))
        .map_err(|_| bad(format!("{raw:?} is not an IP address")))
}

fn key_prefix(raw: &str) -> Result<Condition, AdminError> {
    let raw = raw.trim();
    let prefix: String = raw.chars().take(KEY_PREFIX_CHARS).collect();
    if prefix.chars().count() < KEY_PREFIX_CHARS {
        return Err(bad(format!(
            "a key prefix needs at least {KEY_PREFIX_CHARS} characters"
        )));
    }
    Ok(Condition::KeyPrefix(prefix))
}

fn country(raw: &str) -> Result<Condition, AdminError> {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() != COUNTRY_CODE_CHARS || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(bad(format!("{raw:?} is not a two-character country code")));
    }
    Ok(Condition::Country(code))
}

fn text(raw: &str) -> Option<Condition> {
    let needle = raw.trim();
    if needle.is_empty() {
        return None;
    }
    let mut pattern = String::with_capacity(needle.len() + 2);
    pattern.push('%');
    for c in needle.chars() {
        if LIKE_ESCAPED.contains(&c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    Some(Condition::Text(pattern))
}

fn later(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

fn earlier(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn bad(message: impl Into<String>) -> AdminError {
    AdminError::BadRequest(message.into())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn parse(body: serde_json::Value) -> Result<Selection, AdminError> {
        Selection::parse(serde_json::from_value(body).unwrap())
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, hour, 0, 0).unwrap()
    }

    #[test]
    fn an_empty_body_selects_the_newest_default_page() {
        let selection = parse(json!({})).unwrap();
        assert_eq!(selection.keyset, Keyset::Newest);
        assert_eq!(selection.limit, DEFAULT_PAGE_LIMIT);
        assert_eq!(selection.window, Bounds::OPEN);
        assert!(selection.conditions.is_empty());
    }

    #[test]
    fn filters_become_normalized_conditions() {
        let selection = parse(json!({
            "method": "get",
            "route": "/v3/resolve/{identifier}",
            "status": 429,
            "class": "rate_limited",
            "cause": "upstream_limit",
            "key_kind": "invalid",
            "member": 23025,
            "actor": "354598337325826048",
            "ip": "2001:0db8::0001",
            "key_prefix": "1a2b3c4d5e6f7a8b",
            "q": "50%_off\\",
        }))
        .unwrap();
        assert_eq!(
            selection.conditions,
            vec![
                Condition::Method("GET".into()),
                Condition::Route("/v3/resolve/{identifier}"),
                Condition::Status(429),
                Condition::Class(StatusClass::RateLimited),
                Condition::Cause(Cause::UpstreamLimit),
                Condition::KeyKind(KeyKind::Invalid),
                Condition::Actor(354_598_337_325_826_048),
                Condition::Ip("2001:db8::1".into()),
                Condition::KeyPrefix("1a2b3c4d".into()),
                Condition::Text(r"%50\%\_off\\%".into()),
            ]
        );
        assert_eq!(selection.member, Some(23025));
    }

    #[test]
    fn invalid_filters_are_bad_requests() {
        for body in [
            json!({ "limit": 0 }),
            json!({ "limit": MAX_PAGE_LIMIT + 1 }),
            json!({ "from": at(2), "to": at(1) }),
            json!({ "route": "/v3/unknown" }),
            json!({ "status": 42 }),
            json!({ "member": 0 }),
            json!({ "actor": "bob" }),
            json!({ "ip": "999.1.1.1" }),
            json!({ "key_prefix": "short" }),
            json!({ "method": "GE T" }),
            json!({ "player": "not a player!" }),
        ] {
            let error = parse(body.clone()).unwrap_err();
            assert_eq!(error.code(), "bad_request", "{body}");
        }
    }

    #[test]
    fn countries_are_two_uppercase_characters() {
        let selection = parse(json!({ "country": " nl " })).unwrap();
        assert_eq!(selection.conditions, vec![Condition::Country("NL".into())]);
        for raw in ["N", "NLD", "N-"] {
            let error = parse(json!({ "country": raw })).unwrap_err();
            assert_eq!(error.code(), "bad_request", "{raw:?}");
        }
    }

    #[test]
    fn cursor_and_after_are_exclusive() {
        let cursor = Cursor::before(at(3)).encode();
        let error = parse(json!({ "cursor": cursor, "after": cursor })).unwrap_err();
        assert_eq!(error.code(), "bad_request");
        let after = parse(json!({ "after": cursor })).unwrap();
        assert_eq!(after.keyset, Keyset::After(Cursor::before(at(3))));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let parsed = serde_json::from_value::<RequestFilter>(json!({ "caller": "x" }));
        assert!(parsed.is_err());
    }

    #[test]
    fn blank_text_adds_no_condition() {
        assert!(parse(json!({ "q": "   " })).unwrap().conditions.is_empty());
    }

    #[test]
    fn bounds_intersect_and_detect_emptiness() {
        let window = Bounds {
            lo: Some(at(1)),
            hi: None,
        };
        let era = Bounds {
            lo: None,
            hi: Some(at(3)),
        };
        assert_eq!(
            window.intersect(era),
            Bounds {
                lo: Some(at(1)),
                hi: Some(at(3))
            }
        );
        assert!(
            Bounds {
                lo: Some(at(3)),
                hi: Some(at(3))
            }
            .is_empty()
        );
        assert!(!Bounds::OPEN.is_empty());
        assert!(Bounds::OPEN.starts_before(at(1)));
        assert!(!window.starts_before(at(1)));
    }
}
