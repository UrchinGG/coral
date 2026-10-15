use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqlog::{KeyKind, Service};
use serde::{Serialize, Serializer};

use crate::error::AdminError;
use crate::refs::EntityRef;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequesterKey {
    pub key_kind: i16,
    pub service: i16,
    pub member_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub key_prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accounts {
    pub member_discord_id: Option<i64>,
    pub actor_member_id: Option<i64>,
}

type EncodedKey = (i16, i16, Option<i64>, Option<i64>, Option<String>);

impl RequesterKey {
    pub fn encode(&self) -> String {
        let fields: EncodedKey = (
            self.key_kind,
            self.service,
            self.member_id,
            self.actor_id,
            self.key_prefix.clone(),
        );
        let json = serde_json::to_vec(&fields).expect("a requester key always serializes");
        URL_SAFE_NO_PAD.encode(json)
    }

    pub fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid = || AdminError::BadRequest(format!("{raw:?} is not a requester key"));
        let json = URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())?;
        let (key_kind, service, member_id, actor_id, key_prefix): EncodedKey =
            serde_json::from_slice(&json).map_err(|_| invalid())?;
        if KeyKind::from_code(key_kind).is_none() || Service::from_code(service).is_none() {
            return Err(invalid());
        }
        Ok(Self {
            key_kind,
            service,
            member_id,
            actor_id,
            key_prefix,
        })
    }

    pub fn key_kind(&self) -> KeyKind {
        KeyKind::from_code(self.key_kind).unwrap_or_else(|| {
            tracing::warn!(code = self.key_kind, "subject row has an unknown key kind");
            KeyKind::Legacy
        })
    }

    pub fn service(&self) -> Service {
        Service::from_code(self.service).unwrap_or(Service::Unknown)
    }

    pub fn requester(&self, accounts: Accounts) -> EntityRef {
        if let (Some(member_id), Some(discord_id)) = (self.member_id, accounts.member_discord_id) {
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
                member_id: accounts.actor_member_id,
            },
            (KeyKind::None | KeyKind::Session, _, _) | (_, _, None) => EntityRef::Anonymous,
            (_, _, Some(prefix)) => EntityRef::Key {
                key_id: None,
                prefix: prefix.clone(),
                key_kind,
            },
        }
    }

    pub fn actor(&self, accounts: Accounts) -> Option<EntityRef> {
        let actor_id = self.actor_id?;
        let acting_as_self = matches!(
            self.requester(accounts),
            EntityRef::Account { discord_id, .. } if discord_id == actor_id
        );
        (!acting_as_self).then_some(EntityRef::Account {
            discord_id: actor_id,
            member_id: accounts.actor_member_id,
        })
    }
}

impl Serialize for RequesterKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key_kind: KeyKind, member_id: Option<i64>, actor_id: Option<i64>) -> RequesterKey {
        RequesterKey {
            key_kind: key_kind.code(),
            service: Service::Unknown.code(),
            member_id,
            actor_id,
            key_prefix: Some("1a2b3c4d".into()),
        }
    }

    const NO_ACCOUNTS: Accounts = Accounts {
        member_discord_id: None,
        actor_member_id: None,
    };

    #[test]
    fn keys_round_trip_through_their_token() {
        let original = RequesterKey {
            key_prefix: Some("a.b/c=d".into()),
            ..key(KeyKind::Invalid, None, Some(354_598_337_325_826_048))
        };
        assert_eq!(RequesterKey::decode(&original.encode()).unwrap(), original);
        for raw in ["", "%%%", "WzEsMl0"] {
            assert_eq!(
                RequesterKey::decode(raw).unwrap_err().code(),
                "bad_request",
                "{raw}"
            );
        }
    }

    #[test]
    fn unknown_codes_are_rejected() {
        let bogus = RequesterKey {
            key_kind: 99,
            ..key(KeyKind::Personal, Some(1), None)
        };
        assert_eq!(
            RequesterKey::decode(&bogus.encode()).unwrap_err().code(),
            "bad_request"
        );
    }

    #[test]
    fn members_are_the_requester_and_bots_carry_their_actor() {
        let member = key(KeyKind::Personal, Some(25), None);
        let accounts = Accounts {
            member_discord_id: Some(300_000_000_000_000_025),
            actor_member_id: None,
        };
        assert_eq!(
            member.requester(accounts),
            EntityRef::Account {
                discord_id: 300_000_000_000_000_025,
                member_id: Some(25)
            }
        );
        assert_eq!(member.actor(accounts), None);

        let bot = RequesterKey {
            service: Service::Bot.code(),
            ..key(KeyKind::Internal, None, Some(111))
        };
        assert_eq!(
            bot.requester(NO_ACCOUNTS),
            EntityRef::Internal {
                service: Service::Bot
            }
        );
        assert_eq!(
            bot.actor(NO_ACCOUNTS),
            Some(EntityRef::Account {
                discord_id: 111,
                member_id: None
            })
        );
    }

    #[test]
    fn keys_without_an_account_stay_keys_and_keyless_calls_are_anonymous() {
        assert_eq!(
            key(KeyKind::Invalid, None, None).requester(NO_ACCOUNTS),
            EntityRef::Key {
                key_id: None,
                prefix: "1a2b3c4d".into(),
                key_kind: KeyKind::Invalid
            }
        );
        let keyless = RequesterKey {
            key_prefix: None,
            ..key(KeyKind::None, None, None)
        };
        assert_eq!(keyless.requester(NO_ACCOUNTS), EntityRef::Anonymous);
        let session = key(KeyKind::Session, None, Some(222));
        assert_eq!(
            session.requester(NO_ACCOUNTS),
            EntityRef::Account {
                discord_id: 222,
                member_id: None
            }
        );
        assert_eq!(session.actor(NO_ACCOUNTS), None);
    }
}
