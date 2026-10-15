use std::net::IpAddr;

use chrono::{DateTime, SubsecRound, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::{NoContext, Timestamp, Uuid};

use crate::STORED_BODY_BYTES;
use crate::status::{
    FIRST_CLIENT_ERROR, FIRST_SERVER_ERROR, FORBIDDEN, NOT_FOUND, TOO_MANY_REQUESTS, UNAUTHORIZED,
};
use crate::subjects::RequestParts;
use crate::text::{StoredText, without_nul};

const STORED_TIMESTAMP_DIGITS: u16 = 6;
const BOT_AGENT: &str = "coral-bot";
const SYNC_AGENT: &str = "coral-sync";
const VERIFY_AGENT: &str = "mc-verify";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i16)]
pub enum KeyKind {
    None = 0,
    Personal = 1,
    Developer = 2,
    Internal = 3,
    Session = 4,
    Invalid = 5,
    Unverified = 6,
    Legacy = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i16)]
pub enum Service {
    Unknown = 0,
    Bot = 1,
    Sync = 2,
    Verify = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i16)]
pub enum Cause {
    Ok = 0,
    CallerLimit = 1,
    UpstreamLimit = 2,
    UpstreamError = 3,
    Auth = 4,
    Validation = 5,
    NotFound = 6,
    Internal = 7,
    Unmatched = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub key_kind: KeyKind,
    pub member_id: Option<i64>,
    pub actor_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub status: u16,
    pub cause: Cause,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct Client {
    pub ip: Option<IpAddr>,
    pub country: Option<String>,
    pub user_agent: Option<String>,
    pub key_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Detail {
    pub headers: Value,
    pub body: Option<String>,
    pub body_bytes: i32,
    pub body_complete: bool,
    pub server_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub request_id: Uuid,
    pub ts: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub route: Option<String>,
    pub query: Option<String>,
    pub client: Client,
    pub caller: Caller,
    pub status: u16,
    pub outcome_status: Option<u16>,
    pub cause: Cause,
    pub latency_ms: i32,
    pub error: Option<String>,
    pub detail: Detail,
}

impl KeyKind {
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Personal,
        Self::Developer,
        Self::Internal,
        Self::Session,
        Self::Invalid,
        Self::Unverified,
        Self::Legacy,
    ];

    pub fn code(self) -> i16 {
        self as i16
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.label() == label)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Personal => "personal",
            Self::Developer => "developer",
            Self::Internal => "internal",
            Self::Session => "session",
            Self::Invalid => "invalid",
            Self::Unverified => "unverified",
            Self::Legacy => "legacy",
        }
    }
}

impl Service {
    pub const ALL: [Self; 4] = [Self::Unknown, Self::Bot, Self::Sync, Self::Verify];

    pub fn code(self) -> i16 {
        self as i16
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|service| service.code() == code)
    }

    pub fn from_user_agent(user_agent: Option<&str>) -> Self {
        let product = user_agent
            .and_then(|agent| agent.split('/').next())
            .map(str::trim);
        Self::ALL
            .into_iter()
            .find(|service| product.is_some_and(|product| service.agent() == Some(product)))
            .unwrap_or(Self::Unknown)
    }

    pub fn agent(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Bot => Some(BOT_AGENT),
            Self::Sync => Some(SYNC_AGENT),
            Self::Verify => Some(VERIFY_AGENT),
        }
    }
}

impl Cause {
    pub const ALL: [Self; 9] = [
        Self::Ok,
        Self::CallerLimit,
        Self::UpstreamLimit,
        Self::UpstreamError,
        Self::Auth,
        Self::Validation,
        Self::NotFound,
        Self::Internal,
        Self::Unmatched,
    ];

    pub fn code(self) -> i16 {
        self as i16
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|cause| cause.code() == code)
    }

    pub fn from_status(status: u16, route_matched: bool) -> Self {
        match status {
            ..FIRST_CLIENT_ERROR => Self::Ok,
            UNAUTHORIZED | FORBIDDEN => Self::Auth,
            NOT_FOUND if !route_matched => Self::Unmatched,
            TOO_MANY_REQUESTS => Self::CallerLimit,
            FIRST_CLIENT_ERROR..FIRST_SERVER_ERROR => Self::Validation,
            _ => Self::Internal,
        }
    }
}

impl Caller {
    pub fn anonymous(key_kind: KeyKind) -> Self {
        Self {
            key_kind,
            member_id: None,
            actor_id: None,
        }
    }

    pub fn member(key_kind: KeyKind, member_id: i64) -> Self {
        Self {
            key_kind,
            member_id: Some(member_id),
            actor_id: None,
        }
    }

    pub fn session(member_id: Option<i64>, actor_id: i64) -> Self {
        Self {
            key_kind: KeyKind::Session,
            member_id,
            actor_id: Some(actor_id),
        }
    }

    pub fn internal(actor_id: Option<i64>) -> Self {
        Self {
            key_kind: KeyKind::Internal,
            member_id: None,
            actor_id,
        }
    }
}

impl Detail {
    pub fn capture(
        headers: Value,
        body: &[u8],
        received_bytes: usize,
        accepted: bool,
        server_error: Option<String>,
    ) -> Self {
        let kept = &body[..body.len().min(STORED_BODY_BYTES)];
        let stored = StoredText::decode(kept).truncated(STORED_BODY_BYTES);
        Self {
            headers,
            body_complete: accepted && stored.exact && kept.len() == received_bytes,
            body: (!stored.text.is_empty()).then_some(stored.text),
            body_bytes: received_bytes.min(i32::MAX as usize) as i32,
            server_error: server_error.map(without_nul),
        }
    }
}

impl LogEntry {
    pub fn semantic_status(&self) -> u16 {
        self.outcome_status.unwrap_or(self.status)
    }

    pub fn service(&self) -> Service {
        match self.caller.key_kind {
            KeyKind::Internal => Service::from_user_agent(self.client.user_agent.as_deref()),
            _ => Service::Unknown,
        }
    }

    pub fn subject_key_prefix(&self) -> Option<&str> {
        match self.caller.member_id {
            Some(_) => None,
            None => self.client.key_prefix.as_deref(),
        }
    }

    pub fn request_parts(&self) -> RequestParts<'_> {
        RequestParts {
            method: &self.method,
            status: self.status,
            route: self.route.as_deref(),
            path: &self.path,
            query: self.query.as_deref(),
            body: self.detail.body.as_deref(),
        }
    }

    pub fn text_bytes(&self) -> usize {
        let texts = [
            Some(self.method.as_str()),
            Some(self.path.as_str()),
            self.route.as_deref(),
            self.query.as_deref(),
            self.client.country.as_deref(),
            self.client.user_agent.as_deref(),
            self.client.key_prefix.as_deref(),
            self.error.as_deref(),
            self.detail.body.as_deref(),
            self.detail.server_error.as_deref(),
        ];
        let text: usize = texts.into_iter().flatten().map(str::len).sum();
        text + json_text_bytes(&self.detail.headers)
    }
}

pub fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(STORED_TIMESTAMP_DIGITS)
}

pub fn request_id_for(ts: DateTime<Utc>) -> Uuid {
    Uuid::new_v7(Timestamp::from_unix(
        NoContext,
        ts.timestamp() as u64,
        ts.timestamp_subsec_nanos(),
    ))
}

fn json_text_bytes(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Array(items) => items.iter().map(json_text_bytes).sum(),
        Value::Object(fields) => fields
            .iter()
            .map(|(name, value)| name.len() + json_text_bytes(value))
            .sum(),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn request_id_embeds_the_millisecond_of_its_timestamp() {
        let ts = Utc
            .with_ymd_and_hms(2026, 10, 14, 3, 14, 32)
            .unwrap()
            .checked_add_signed(chrono::Duration::microseconds(123_456))
            .unwrap();
        let id = request_id_for(ts);
        assert_eq!(id.get_version_num(), 7);
        let (seconds, nanos) = id.get_timestamp().unwrap().to_unix();
        let embedded = Utc.timestamp_opt(seconds as i64, nanos).unwrap();
        assert_eq!(embedded, ts.trunc_subsecs(3));
    }

    #[test]
    fn request_ids_for_the_same_instant_are_distinct() {
        let ts = now();
        assert_ne!(request_id_for(ts), request_id_for(ts));
    }

    #[test]
    fn stored_timestamps_have_microsecond_precision() {
        assert_eq!(now().timestamp_subsec_nanos() % 1_000, 0);
    }

    #[test]
    fn default_cause_follows_the_semantic_status() {
        assert_eq!(Cause::from_status(200, true), Cause::Ok);
        assert_eq!(Cause::from_status(304, true), Cause::Ok);
        assert_eq!(Cause::from_status(401, true), Cause::Auth);
        assert_eq!(Cause::from_status(403, false), Cause::Auth);
        assert_eq!(Cause::from_status(404, false), Cause::Unmatched);
        assert_eq!(Cause::from_status(404, true), Cause::Validation);
        assert_eq!(Cause::from_status(429, true), Cause::CallerLimit);
        assert_eq!(Cause::from_status(422, true), Cause::Validation);
        assert_eq!(Cause::from_status(502, true), Cause::Internal);
    }

    #[test]
    fn codes_and_labels_round_trip() {
        for kind in KeyKind::ALL {
            assert_eq!(KeyKind::from_code(kind.code()), Some(kind));
            assert_eq!(KeyKind::from_label(kind.label()), Some(kind));
        }
        for service in Service::ALL {
            assert_eq!(Service::from_code(service.code()), Some(service));
        }
        for cause in Cause::ALL {
            assert_eq!(Cause::from_code(cause.code()), Some(cause));
        }
        assert_eq!(KeyKind::from_label("key"), None);
        assert_eq!(Cause::from_code(99), None);
    }

    #[test]
    fn vocabulary_serializes_in_snake_case() {
        assert_eq!(
            serde_json::to_value(Cause::CallerLimit).unwrap(),
            "caller_limit"
        );
        assert_eq!(
            serde_json::to_value(KeyKind::Unverified).unwrap(),
            "unverified"
        );
        assert_eq!(serde_json::to_value(Service::Bot).unwrap(), "bot");
    }

    #[test]
    fn service_comes_from_the_user_agent_product() {
        assert_eq!(Service::from_user_agent(Some("coral-bot")), Service::Bot);
        assert_eq!(
            Service::from_user_agent(Some("coral-sync/0.1")),
            Service::Sync
        );
        assert_eq!(Service::from_user_agent(Some("mc-verify")), Service::Verify);
        assert_eq!(
            Service::from_user_agent(Some("Mozilla/5.0")),
            Service::Unknown
        );
        assert_eq!(Service::from_user_agent(None), Service::Unknown);
    }

    #[test]
    fn only_internal_callers_carry_a_service() {
        let mut entry = sample_entry();
        entry.client.user_agent = Some("coral-bot".into());
        assert_eq!(entry.service(), Service::Unknown);
        entry.caller = Caller::internal(Some(7));
        assert_eq!(entry.service(), Service::Bot);
    }

    #[test]
    fn subject_key_prefix_is_kept_only_without_a_member() {
        let mut entry = sample_entry();
        entry.client.key_prefix = Some("abcdefgh".into());
        assert_eq!(entry.subject_key_prefix(), Some("abcdefgh"));
        entry.caller = Caller::member(KeyKind::Personal, 3);
        assert_eq!(entry.subject_key_prefix(), None);
    }

    #[test]
    fn accepted_bodies_are_stored_whole_and_complete() {
        let detail = Detail::capture(Value::Null, br#"{"uuids":[]}"#, 12, true, None);
        assert_eq!(detail.body.as_deref(), Some(r#"{"uuids":[]}"#));
        assert_eq!(detail.body_bytes, 12);
        assert!(detail.body_complete);
    }

    #[test]
    fn rejected_bodies_keep_a_partial_prefix() {
        let body = vec![b'a'; STORED_BODY_BYTES + 1];
        let detail = Detail::capture(Value::Null, &body, body.len(), false, None);
        assert_eq!(
            detail.body.as_deref().map(str::len),
            Some(STORED_BODY_BYTES)
        );
        assert_eq!(detail.body_bytes as usize, body.len());
        assert!(!detail.body_complete);

        let detail = Detail::capture(Value::Null, b"{}", 2, false, None);
        assert_eq!(detail.body.as_deref(), Some("{}"));
        assert!(!detail.body_complete);
    }

    #[test]
    fn stored_text_never_holds_nul_or_outgrows_the_store_limit() {
        let detail = Detail::capture(Value::Null, b"a\0b", 3, true, Some("x\0".into()));
        assert_eq!(detail.body.as_deref(), Some("a\u{FFFD}b"));
        assert!(!detail.body_complete);
        assert_eq!(detail.server_error.as_deref(), Some("x\u{FFFD}"));

        let invalid = vec![0xFF; crate::MAX_REQUEST_BODY];
        let detail = Detail::capture(Value::Null, &invalid, invalid.len(), true, None);
        assert!(detail.body.as_deref().unwrap().len() <= STORED_BODY_BYTES);
        assert!(!detail.body_complete);
    }

    #[test]
    fn accepted_bodies_past_the_store_limit_keep_their_true_size() {
        let body = vec![b'a'; STORED_BODY_BYTES + 1];
        let detail = Detail::capture(Value::Null, &body, body.len(), true, None);
        assert_eq!(
            detail.body.as_deref().map(str::len),
            Some(STORED_BODY_BYTES)
        );
        assert_eq!(detail.body_bytes as usize, STORED_BODY_BYTES + 1);
        assert!(!detail.body_complete);
    }

    #[test]
    fn empty_bodies_are_not_stored() {
        let detail = Detail::capture(Value::Null, b"", 0, true, None);
        assert_eq!(detail.body, None);
        assert!(detail.body_complete);
    }

    #[test]
    fn text_bytes_count_every_stored_text_and_header() {
        let mut entry = sample_entry();
        let bare = entry.text_bytes();
        assert_eq!(bare, "GET".len() + 2 * "/v3/player/tags".len());
        entry.detail.body = Some("x".repeat(1_000));
        entry.error = Some("boom".into());
        entry.detail.headers = serde_json::json!({"user-agent": "agent"});
        assert_eq!(
            entry.text_bytes(),
            bare + 1_000 + "boom".len() + "user-agent".len() + "agent".len()
        );
    }

    fn sample_entry() -> LogEntry {
        let ts = now();
        LogEntry {
            request_id: request_id_for(ts),
            ts,
            method: "GET".into(),
            path: "/v3/player/tags".into(),
            route: Some("/v3/player/tags".into()),
            query: None,
            client: Client::default(),
            caller: Caller::anonymous(KeyKind::Invalid),
            status: 200,
            outcome_status: None,
            cause: Cause::Ok,
            latency_ms: 1,
            error: None,
            detail: Detail {
                headers: Value::Null,
                body: None,
                body_bytes: 0,
                body_complete: true,
                server_error: None,
            },
        }
    }
}
