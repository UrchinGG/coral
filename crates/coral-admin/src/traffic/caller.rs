use reqlog::{KeyKind, Service};
use sqlx::FromRow;

use crate::refs::EntityRef;

pub const CALLER_IDENTITY_JOINS: &str =
    "LEFT JOIN members requester ON requester.id = grouped.member_id
LEFT JOIN members acting ON acting.discord_id = grouped.actor_id";
pub const CALLER_IDENTITY_COLUMNS: &str = "grouped.key_kind, grouped.member_id, \
requester.discord_id AS member_discord_id, grouped.actor_id, acting.id AS actor_member_id, \
grouped.key_prefix, grouped.service";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CallerKey {
    pub key_kind: i16,
    pub member_id: Option<i64>,
    pub member_discord_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub actor_member_id: Option<i64>,
    pub key_prefix: Option<String>,
    pub service: i16,
}

impl CallerKey {
    pub fn key_kind(&self) -> KeyKind {
        KeyKind::from_code(self.key_kind).unwrap_or_else(|| {
            tracing::warn!(code = self.key_kind, "a rollup row has an unknown key kind");
            KeyKind::Legacy
        })
    }

    pub fn service(&self) -> Service {
        Service::from_code(self.service).unwrap_or_else(|| {
            tracing::warn!(code = self.service, "a rollup row has an unknown service");
            Service::Unknown
        })
    }

    pub fn requester(&self) -> EntityRef {
        if let (Some(member_id), Some(discord_id)) = (self.member_id, self.member_discord_id) {
            return EntityRef::Account {
                discord_id,
                member_id: Some(member_id),
            };
        }
        let key_kind = self.key_kind();
        match (key_kind, self.actor_id, &self.key_prefix) {
            (KeyKind::Internal, _, _) => EntityRef::Internal {
                service: self.service(),
            },
            (KeyKind::Session, Some(actor_id), _) => EntityRef::Account {
                discord_id: actor_id,
                member_id: self.actor_member_id,
            },
            (KeyKind::None | KeyKind::Session, _, _) | (_, _, None) => EntityRef::Anonymous,
            (_, _, Some(prefix)) => EntityRef::Key {
                key_id: None,
                prefix: prefix.clone(),
                key_kind,
            },
        }
    }

    pub fn actor(&self) -> Option<EntityRef> {
        let actor_id = self.actor_id?;
        let acting_as_self = matches!(
            self.requester(),
            EntityRef::Account { discord_id, .. } if discord_id == actor_id
        );
        (!acting_as_self).then_some(EntityRef::Account {
            discord_id: actor_id,
            member_id: self.actor_member_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMBER_DISCORD: i64 = 1_218_840_276_405_911_602;
    const ACTOR: i64 = 702_913_845_516_288_031;

    fn key(kind: KeyKind) -> CallerKey {
        CallerKey {
            key_kind: kind.code(),
            member_id: None,
            member_discord_id: None,
            actor_id: None,
            actor_member_id: None,
            key_prefix: None,
            service: Service::Unknown.code(),
        }
    }

    #[test]
    fn members_are_accounts_and_keep_a_distinct_actor() {
        let caller = CallerKey {
            member_id: Some(21111),
            member_discord_id: Some(MEMBER_DISCORD),
            actor_id: Some(ACTOR),
            ..key(KeyKind::Personal)
        };
        assert_eq!(
            caller.requester(),
            EntityRef::Account {
                discord_id: MEMBER_DISCORD,
                member_id: Some(21111)
            }
        );
        assert_eq!(
            caller.actor(),
            Some(EntityRef::Account {
                discord_id: ACTOR,
                member_id: None
            })
        );
    }

    #[test]
    fn sessions_are_their_actor_and_have_no_separate_actor() {
        let caller = CallerKey {
            actor_id: Some(ACTOR),
            actor_member_id: Some(23025),
            ..key(KeyKind::Session)
        };
        assert_eq!(
            caller.requester(),
            EntityRef::Account {
                discord_id: ACTOR,
                member_id: Some(23025)
            }
        );
        assert_eq!(caller.actor(), None);
    }

    #[test]
    fn internal_callers_name_their_service_and_unknown_keys_keep_their_prefix() {
        let internal = CallerKey {
            service: Service::Sync.code(),
            ..key(KeyKind::Internal)
        };
        assert_eq!(
            internal.requester(),
            EntityRef::Internal {
                service: Service::Sync
            }
        );
        let invalid = CallerKey {
            key_prefix: Some("1a2b3c4d".into()),
            ..key(KeyKind::Invalid)
        };
        assert_eq!(
            invalid.requester(),
            EntityRef::Key {
                key_id: None,
                prefix: "1a2b3c4d".into(),
                key_kind: KeyKind::Invalid
            }
        );
        assert_eq!(key(KeyKind::None).requester(), EntityRef::Anonymous);
    }
}
