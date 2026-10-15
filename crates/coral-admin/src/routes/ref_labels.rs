use std::collections::BTreeSet;

use crate::names::LabelRequest;
use crate::refs::EntityRef;

pub fn label_request<'a>(refs: impl IntoIterator<Item = &'a EntityRef>) -> LabelRequest {
    let mut discord = BTreeSet::new();
    let mut minecraft = BTreeSet::new();
    for entity in refs {
        match entity {
            EntityRef::Account { discord_id, .. } => {
                discord.insert(*discord_id);
            }
            EntityRef::Player { uuid } => {
                minecraft.insert(uuid.simple().to_string());
            }
            _ => {}
        }
    }
    LabelRequest {
        discord: discord.into_iter().collect(),
        minecraft: minecraft.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn only_accounts_and_players_need_labels_and_each_is_asked_once() {
        let player = Uuid::from_u128(7);
        let account = EntityRef::Account {
            discord_id: 354_598_337_325_826_048,
            member_id: Some(1),
        };
        let refs = [
            account.clone(),
            account,
            EntityRef::Player { uuid: player },
            EntityRef::Name {
                name: "oJugg".into(),
            },
            EntityRef::Anonymous,
        ];
        let request = label_request(&refs);
        assert_eq!(request.discord, vec![354_598_337_325_826_048]);
        assert_eq!(request.minecraft, vec![player.simple().to_string()]);
    }
}
