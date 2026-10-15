use chrono::{DateTime, Utc};
use reqlog::{Cause, KeyKind, Service};
use sqlx::FromRow;
use uuid::Uuid;

use super::attribution::Attribution;
use super::cursor::Cursor;
use crate::refs::{EntityRef, Match};

const LEGACY_KEYED: &str = "key";
const LEGACY_KEYLESS: &str = "none";
const TOO_MANY_REQUESTS: i16 = 429;
const UPSTREAM_LIMIT_BODY: &str = r#"{"error":"Rate limited"}"#;
const STARFISH_PREFIX: &str = "/api/v1/starfish/";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LogRow {
    pub ts: DateTime<Utc>,
    pub request_key: Uuid,
    pub legacy: bool,
    pub method: String,
    pub route: Option<String>,
    pub path: String,
    pub query: Option<String>,
    pub status: i16,
    pub outcome_status: Option<i16>,
    pub cause: Option<i16>,
    pub error: Option<String>,
    pub latency_ms: i32,
    pub key_kind: Option<String>,
    pub key_prefix: Option<String>,
    pub member_id: Option<i64>,
    pub member_discord_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub actor_member_id: Option<i64>,
    pub ip: Option<String>,
    pub country: Option<String>,
    pub user_agent: Option<String>,
    pub subjects: Option<i16>,
    pub matched: Option<String>,
}

impl LogRow {
    pub fn cursor(&self) -> Cursor {
        Cursor::new(self.ts, self.request_key)
    }

    pub fn semantic_status(&self) -> i16 {
        self.outcome_status.unwrap_or(self.status)
    }

    pub fn route_template(&self) -> Option<String> {
        match self.legacy {
            true => reqlog::template_for_path(&self.path).map(str::to_string),
            false => self.route.clone(),
        }
    }

    pub fn key_kind(&self) -> KeyKind {
        let label = self.key_kind.as_deref();
        match (self.legacy, label) {
            (true, Some(LEGACY_KEYLESS)) if self.member_id.is_some() => KeyKind::Session,
            (true, Some(LEGACY_KEYLESS)) | (_, None) => KeyKind::None,
            (true, Some(LEGACY_KEYED)) => KeyKind::Legacy,
            (_, Some(label)) => KeyKind::from_label(label).unwrap_or_else(|| {
                tracing::warn!(ts = %self.ts, label, "request log row has an unknown key kind");
                KeyKind::Legacy
            }),
        }
    }

    pub fn cause(&self) -> Cause {
        let Some(code) = self.cause else {
            return self.legacy_cause();
        };
        Cause::from_code(code).unwrap_or_else(|| {
            tracing::warn!(ts = %self.ts, code, "request log row has an unknown cause");
            self.legacy_cause()
        })
    }

    pub fn matched(&self) -> Option<Match> {
        self.matched.as_deref().and_then(Match::from_label)
    }

    pub fn attribution(&self) -> Attribution {
        Attribution {
            key_kind: self.key_kind(),
            service: Service::from_user_agent(self.user_agent.as_deref()),
            member_id: self.member_id,
            member_discord_id: self.member_discord_id,
            actor_id: self.actor_id,
            actor_member_id: self.actor_member_id,
            key_prefix: self.key_prefix.clone(),
        }
    }

    pub fn requester(&self) -> EntityRef {
        self.attribution().requester()
    }

    pub fn actor(&self) -> Option<EntityRef> {
        self.attribution().actor()
    }

    fn legacy_cause(&self) -> Cause {
        let status = self.semantic_status();
        if status != TOO_MANY_REQUESTS {
            let matched = self.route_template().is_some();
            return Cause::from_status(status as u16, matched);
        }
        let upstream = self.error.as_deref() == Some(UPSTREAM_LIMIT_BODY)
            && !self.path.starts_with(STARFISH_PREFIX);
        match upstream {
            true => Cause::UpstreamLimit,
            false => Cause::CallerLimit,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use chrono::TimeZone;

    use super::*;

    pub fn row() -> LogRow {
        LogRow {
            ts: Utc.with_ymd_and_hms(2026, 10, 14, 22, 0, 0).unwrap(),
            request_key: Uuid::nil(),
            legacy: false,
            method: "GET".into(),
            route: Some("/v3/player/tags".into()),
            path: "/v3/player/tags".into(),
            query: None,
            status: 200,
            outcome_status: None,
            cause: Some(Cause::Ok.code()),
            error: None,
            latency_ms: 5,
            key_kind: Some("personal".into()),
            key_prefix: Some("1a2b3c4d".into()),
            member_id: Some(23025),
            member_discord_id: Some(354_598_337_325_826_048),
            actor_id: None,
            actor_member_id: None,
            ip: None,
            country: None,
            user_agent: None,
            subjects: Some(1),
            matched: None,
        }
    }

    fn legacy(key_kind: &str, member_id: Option<i64>) -> LogRow {
        LogRow {
            legacy: true,
            route: None,
            cause: None,
            key_kind: Some(key_kind.into()),
            member_id,
            member_discord_id: member_id.map(|_| 354_598_337_325_826_048),
            ..row()
        }
    }

    #[test]
    fn members_are_the_requester_whatever_the_key_kind() {
        assert_eq!(
            row().requester(),
            EntityRef::Account {
                discord_id: 354_598_337_325_826_048,
                member_id: Some(23025)
            }
        );
        assert_eq!(row().actor(), None);
    }

    #[test]
    fn internal_calls_name_the_service_and_the_acting_user() {
        let internal = LogRow {
            key_kind: Some("internal".into()),
            member_id: None,
            member_discord_id: None,
            key_prefix: Some("intern01".into()),
            actor_id: Some(111),
            actor_member_id: Some(5),
            user_agent: Some("coral-bot/1.0".into()),
            ..row()
        };
        assert_eq!(
            internal.requester(),
            EntityRef::Internal {
                service: Service::Bot
            }
        );
        assert_eq!(
            internal.actor(),
            Some(EntityRef::Account {
                discord_id: 111,
                member_id: Some(5)
            })
        );
    }

    #[test]
    fn sessions_without_a_member_belong_to_their_discord_user() {
        let session = LogRow {
            key_kind: Some("session".into()),
            member_id: None,
            member_discord_id: None,
            key_prefix: None,
            actor_id: Some(222),
            ..row()
        };
        assert_eq!(
            session.requester(),
            EntityRef::Account {
                discord_id: 222,
                member_id: None
            }
        );
        assert_eq!(session.actor(), None);
    }

    #[test]
    fn unattributed_keys_and_keyless_calls() {
        let invalid = LogRow {
            key_kind: Some("invalid".into()),
            member_id: None,
            member_discord_id: None,
            ..row()
        };
        assert_eq!(
            invalid.requester(),
            EntityRef::Key {
                key_id: None,
                prefix: "1a2b3c4d".into(),
                key_kind: KeyKind::Invalid
            }
        );
        let anonymous = LogRow {
            key_kind: Some("none".into()),
            member_id: None,
            member_discord_id: None,
            key_prefix: None,
            ..row()
        };
        assert_eq!(anonymous.requester(), EntityRef::Anonymous);
    }

    #[test]
    fn legacy_vocabulary_maps_onto_the_new_key_kinds() {
        assert_eq!(legacy("none", Some(1)).key_kind(), KeyKind::Session);
        assert_eq!(legacy("none", None).key_kind(), KeyKind::None);
        assert_eq!(legacy("key", Some(1)).key_kind(), KeyKind::Legacy);
        let prefix_only = legacy("key", None);
        assert_eq!(
            prefix_only.requester(),
            EntityRef::Key {
                key_id: None,
                prefix: "1a2b3c4d".into(),
                key_kind: KeyKind::Legacy
            }
        );
    }

    #[test]
    fn legacy_rows_get_their_route_and_cause_from_the_path_and_status() {
        let resolve = LogRow {
            path: "/v3/resolve/oJugg".into(),
            status: 404,
            ..legacy("key", Some(1))
        };
        assert_eq!(
            resolve.route_template().as_deref(),
            Some("/v3/resolve/{identifier}")
        );
        assert_eq!(resolve.cause(), Cause::Validation);
        let probe = LogRow {
            path: "/wp-login.php".into(),
            status: 404,
            ..legacy("none", None)
        };
        assert_eq!(probe.route_template(), None);
        assert_eq!(probe.cause(), Cause::Unmatched);
    }

    #[test]
    fn legacy_429s_split_between_caller_and_upstream_limits() {
        let caller = LogRow {
            status: 429,
            error: None,
            ..legacy("key", Some(1))
        };
        assert_eq!(caller.cause(), Cause::CallerLimit);
        let upstream = LogRow {
            error: Some(UPSTREAM_LIMIT_BODY.into()),
            ..caller.clone()
        };
        assert_eq!(upstream.cause(), Cause::UpstreamLimit);
        let starfish = LogRow {
            path: "/api/v1/starfish/heartbeat".into(),
            ..upstream.clone()
        };
        assert_eq!(starfish.cause(), Cause::CallerLimit);
    }

    #[test]
    fn recorded_causes_and_outcomes_win() {
        let cubelify = LogRow {
            status: 200,
            outcome_status: Some(429),
            cause: Some(Cause::CallerLimit.code()),
            ..row()
        };
        assert_eq!(cubelify.semantic_status(), 429);
        assert_eq!(cubelify.cause(), Cause::CallerLimit);
    }

    #[test]
    fn stored_match_labels_become_matches() {
        let matched = LogRow {
            matched: Some(Match::NameOnly.label().into()),
            ..row()
        };
        assert_eq!(matched.matched(), Some(Match::NameOnly));
        assert_eq!(row().matched(), None);
    }
}
