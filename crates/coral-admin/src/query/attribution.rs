use reqlog::{KeyKind, Service};
use uuid::Uuid;

use crate::refs::EntityRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub key_kind: KeyKind,
    pub service: Service,
    pub member_id: Option<i64>,
    pub member_discord_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub actor_member_id: Option<i64>,
    pub actor_uuid: Option<Uuid>,
    pub key_prefix: Option<String>,
}

impl Attribution {
    pub fn requester(&self) -> EntityRef {
        if let (Some(member_id), Some(discord_id)) = (self.member_id, self.member_discord_id) {
            return EntityRef::Account {
                discord_id,
                member_id: Some(member_id),
            };
        }
        match (
            self.key_kind,
            self.actor_id,
            self.actor_uuid,
            &self.key_prefix,
        ) {
            (KeyKind::Internal, ..) => EntityRef::Internal {
                service: self.service,
            },
            (KeyKind::Session, Some(actor_id), ..) => EntityRef::Account {
                discord_id: actor_id,
                member_id: self.actor_member_id,
            },
            (KeyKind::Minecraft, _, Some(uuid), _) => EntityRef::Player { uuid },
            (KeyKind::None | KeyKind::Session, ..) | (.., None) => EntityRef::Anonymous,
            (.., Some(prefix)) => EntityRef::Key {
                key_id: None,
                prefix: prefix.clone(),
                key_kind: self.key_kind,
            },
        }
    }

    pub fn actor(&self) -> Option<EntityRef> {
        let actor = match (self.actor_id, self.actor_uuid) {
            (Some(discord_id), _) => EntityRef::Account {
                discord_id,
                member_id: self.actor_member_id,
            },
            (None, Some(uuid)) => EntityRef::Player { uuid },
            (None, None) => return None,
        };
        (actor != self.requester()).then_some(actor)
    }
}
