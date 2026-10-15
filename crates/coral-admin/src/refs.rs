use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use reqlog::{Cause, Form, KeyKind, Service};
use serde::Serialize;
use uuid::Uuid;

use crate::query::Cursor;
use crate::{serde_id, serde_ts};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityRef {
    Account {
        #[serde(serialize_with = "serde_id::discord_id")]
        discord_id: i64,
        member_id: Option<i64>,
    },
    Player {
        #[serde(serialize_with = "serde_id::uuid_undashed")]
        uuid: Uuid,
    },
    Name {
        name: String,
    },
    Request {
        #[serde(serialize_with = "serde_id::uuid_undashed")]
        request_id: Uuid,
        #[serde(serialize_with = "serde_ts::micros")]
        ts: DateTime<Utc>,
    },
    Ip {
        ip: String,
        country: Option<String>,
        proxied: bool,
    },
    Key {
        key_id: Option<i64>,
        prefix: String,
        key_kind: KeyKind,
    },
    Guild {
        guild_id: String,
        name: Option<String>,
    },
    Plugin {
        slug: String,
        name: Option<String>,
    },
    Internal {
        service: Service,
    },
    Anonymous,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Labels {
    pub discord: BTreeMap<i64, String>,
    pub minecraft: BTreeMap<String, String>,
    pub pending: PendingLabels,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PendingLabels {
    #[serde(serialize_with = "serde_id::discord_ids")]
    pub discord: Vec<i64>,
    pub minecraft: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub scanned_to: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub clamped_from: Option<DateTime<Utc>>,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Subject {
    #[serde(serialize_with = "serde_id::uuid_undashed_opt")]
    pub player: Option<Uuid>,
    pub name: Option<String>,
    pub raw: String,
    pub form: Form,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(i32)]
pub enum Match {
    Uuid = 0,
    ResolvedName = 1,
    NameOnly = 2,
    DisplayName = 3,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestRow {
    #[serde(serialize_with = "serde_id::uuid_undashed")]
    pub request_id: Uuid,
    #[serde(serialize_with = "serde_ts::micros")]
    pub ts: DateTime<Utc>,
    pub method: String,
    pub route: Option<String>,
    pub path: String,
    pub query: Option<String>,
    pub status: i16,
    pub outcome_status: Option<i16>,
    pub cause: Cause,
    pub error: Option<String>,
    pub latency_ms: i32,
    pub key_kind: KeyKind,
    pub key_prefix: Option<String>,
    pub requester: EntityRef,
    pub actor: Option<EntityRef>,
    pub ip: Option<EntityRef>,
    pub country: Option<String>,
    pub user_agent: Option<String>,
    pub subjects: Vec<Subject>,
    pub body_subjects: i16,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub matched: Option<Match>,
}

impl Match {
    pub const ALL: [Self; 4] = [
        Self::Uuid,
        Self::ResolvedName,
        Self::NameOnly,
        Self::DisplayName,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Uuid => "uuid",
            Self::ResolvedName => "resolved_name",
            Self::NameOnly => "name_only",
            Self::DisplayName => "display_name",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|matched| matched.label() == label)
    }

    pub fn preference(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    fn ojugg() -> Uuid {
        Uuid::parse_str(OJUGG).unwrap()
    }

    fn json(value: impl Serialize) -> serde_json::Value {
        serde_json::to_value(value).unwrap()
    }

    #[test]
    fn entity_refs_are_tagged_by_kind() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 21, 41, 7).unwrap();
        let cases = [
            (
                EntityRef::Account {
                    discord_id: 1_547_000_000_000_000_000,
                    member_id: Some(23025),
                },
                json!({ "kind": "account", "discord_id": "1547000000000000000", "member_id": 23025 }),
            ),
            (
                EntityRef::Player { uuid: ojugg() },
                json!({ "kind": "player", "uuid": OJUGG }),
            ),
            (
                EntityRef::Name {
                    name: "ojugg".into(),
                },
                json!({ "kind": "name", "name": "ojugg" }),
            ),
            (
                EntityRef::Request {
                    request_id: ojugg(),
                    ts,
                },
                json!({ "kind": "request", "request_id": OJUGG, "ts": "2026-10-14T21:41:07.000000Z" }),
            ),
            (
                EntityRef::Ip {
                    ip: "203.0.113.9".into(),
                    country: Some("NL".into()),
                    proxied: false,
                },
                json!({ "kind": "ip", "ip": "203.0.113.9", "country": "NL", "proxied": false }),
            ),
            (
                EntityRef::Key {
                    key_id: None,
                    prefix: "1a2b3c4d".into(),
                    key_kind: KeyKind::Invalid,
                },
                json!({ "kind": "key", "key_id": null, "prefix": "1a2b3c4d", "key_kind": "invalid" }),
            ),
            (
                EntityRef::Guild {
                    guild_id: "5b3e8f1a9c2d4e6f7a8b9c0d".into(),
                    name: None,
                },
                json!({ "kind": "guild", "guild_id": "5b3e8f1a9c2d4e6f7a8b9c0d", "name": null }),
            ),
            (
                EntityRef::Plugin {
                    slug: "bedwars".into(),
                    name: Some("Bedwars".into()),
                },
                json!({ "kind": "plugin", "slug": "bedwars", "name": "Bedwars" }),
            ),
            (
                EntityRef::Internal {
                    service: Service::Sync,
                },
                json!({ "kind": "internal", "service": "sync" }),
            ),
            (EntityRef::Anonymous, json!({ "kind": "anonymous" })),
        ];
        for (entity, expected) in cases {
            assert_eq!(json(&entity), expected);
        }
    }

    #[test]
    fn labels_key_discord_names_by_string_snowflake() {
        let labels = Labels {
            discord: BTreeMap::from([(1_547_000_000_000_000_000, "owner".into())]),
            minecraft: BTreeMap::from([(OJUGG.into(), "oJugg".into())]),
            pending: PendingLabels {
                discord: vec![1_547_000_000_000_000_001],
                minecraft: vec![],
            },
        };
        assert_eq!(
            json(&labels),
            json!({
                "discord": { "1547000000000000000": "owner" },
                "minecraft": { OJUGG: "oJugg" },
                "pending": { "discord": ["1547000000000000001"], "minecraft": [] },
            })
        );
    }

    #[test]
    fn page_carries_scan_and_clamp_bounds() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 21, 41, 7).unwrap();
        let page = Page::<u8> {
            items: vec![],
            next_cursor: Some(Cursor::new(ts, Uuid::nil())),
            scanned_to: Some(ts),
            clamped_from: None,
            labels: Labels::default(),
        };
        let value = json(&page);
        assert_eq!(value["scanned_to"], "2026-10-14T21:41:07.000000Z");
        assert_eq!(value["clamped_from"], serde_json::Value::Null);
        assert!(value["next_cursor"].is_string());
    }

    #[test]
    fn request_rows_name_the_match_only_when_there_is_one() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 21, 41, 7).unwrap();
        let mut row = RequestRow {
            request_id: ojugg(),
            ts,
            method: "GET".into(),
            route: Some("/v3/player/tags".into()),
            path: "/v3/player/tags".into(),
            query: Some(format!("player={OJUGG}")),
            status: 200,
            outcome_status: None,
            cause: Cause::Ok,
            error: None,
            latency_ms: 12,
            key_kind: KeyKind::Personal,
            key_prefix: Some("1a2b3c4d".into()),
            requester: EntityRef::Anonymous,
            actor: None,
            ip: None,
            country: None,
            user_agent: None,
            subjects: vec![Subject {
                player: Some(ojugg()),
                name: None,
                raw: OJUGG.into(),
                form: Form::Undashed,
            }],
            body_subjects: 0,
            matched: None,
        };
        let value = json(&row);
        assert!(value.get("match").is_none());
        assert_eq!(
            value["subjects"],
            json!([{ "player": OJUGG, "name": null, "raw": OJUGG, "form": "undashed" }])
        );
        assert_eq!(value["cause"], "ok");
        assert_eq!(value["key_kind"], "personal");

        row.matched = Some(Match::ResolvedName);
        assert_eq!(json(&row)["match"], "resolved_name");
    }

    #[test]
    fn match_labels_are_their_wire_names() {
        for matched in Match::ALL {
            assert_eq!(json(matched), matched.label());
            assert_eq!(Match::from_label(matched.label()), Some(matched));
        }
    }
}
