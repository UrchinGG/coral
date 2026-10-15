use reqlog::{KeyKind, Service};

use crate::refs::EntityRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub key_kind: KeyKind,
    pub service: Service,
    pub member_id: Option<i64>,
    pub member_discord_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub actor_member_id: Option<i64>,
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
        match (self.key_kind, self.actor_id, &self.key_prefix) {
            (KeyKind::Internal, _, _) => EntityRef::Internal {
                service: self.service,
            },
            (KeyKind::Session, Some(actor_id), _) => EntityRef::Account {
                discord_id: actor_id,
                member_id: self.actor_member_id,
            },
            (KeyKind::None | KeyKind::Session, _, _) | (_, _, None) => EntityRef::Anonymous,
            (_, _, Some(prefix)) => EntityRef::Key {
                key_id: None,
                prefix: prefix.clone(),
                key_kind: self.key_kind,
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
