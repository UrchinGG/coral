use std::collections::HashMap;

use replay_protocol::PlayerListAction;

use crate::{Player, item::skin_hash, strip};

#[derive(Default)]
pub(crate) struct Roster {
    pub players: Vec<Player>,
    indices: HashMap<Key, usize>,
    tab: HashMap<u128, TabEntry>,
}

#[derive(PartialEq, Eq, Hash)]
enum Key {
    Uuid(u128),
    Name(String),
}

#[derive(Default, Clone)]
struct TabEntry {
    name: String,
    skin_hash: Option<String>,
    skin_owner: Option<String>,
    profile_id: Option<u128>,
    slim: bool,
}

impl Roster {
    pub fn ensure(&mut self, uuid: u128) -> usize {
        let entry = self.tab.get(&uuid).cloned().unwrap_or_default();
        let keys = keys(uuid, &entry.name);
        let index = keys
            .iter()
            .find_map(|key| self.indices.get(key).copied())
            .unwrap_or_else(|| {
                let mut player = Player {
                    username: format!("Player {}", self.players.len() + 1),
                    ..Player::default()
                };
                player.update(uuid, &entry);
                self.players.push(player);
                self.players.len() - 1
            });
        self.indices
            .extend(keys.into_iter().map(|key| (key, index)));
        index
    }

    pub fn player(&mut self, uuid: u128) -> &mut Player {
        let index = self.ensure(uuid);
        &mut self.players[index]
    }

    pub fn tab(&mut self, uuid: u128, action: PlayerListAction) {
        let entry = self.tab.entry(uuid).or_default();
        match action {
            PlayerListAction::Add { name, textures, .. } => {
                let profile_name = textures
                    .as_ref()
                    .and_then(|textures| textures.profile_name.clone());
                if let Some(name) = Some(strip(&name))
                    .filter(|name| !name.is_empty())
                    .or(profile_name)
                {
                    entry.name = name;
                }
                if let Some(textures) = textures {
                    entry.skin_hash = textures.skin_url.as_deref().and_then(skin_hash);
                    entry.skin_owner = textures.profile_name;
                    entry.profile_id = textures.profile_id;
                    entry.slim = textures.slim;
                }
            }
            PlayerListAction::UpdateLatency(_) => {}
            _ => return,
        }
        let entry = entry.clone();
        let keys = keys(uuid, &entry.name);
        if let Some(index) = keys.iter().find_map(|key| self.indices.get(key).copied()) {
            self.players[index].update(uuid, &entry);
            self.indices
                .extend(keys.into_iter().map(|key| (key, index)));
        }
    }
}

impl Player {
    fn update(&mut self, uuid: u128, entry: &TabEntry) {
        if !entry.name.is_empty() {
            self.username.clone_from(&entry.name);
        }
        self.uuid = entry.profile_id.unwrap_or(uuid);
        self.skin_hash = entry.skin_hash.clone().or(self.skin_hash.take());
        self.skin_owner = entry.skin_owner.clone().or(self.skin_owner.take());
        self.slim = entry.slim;
    }
}

fn keys(uuid: u128, name: &str) -> Vec<Key> {
    let name = (!name.is_empty()).then(|| Key::Name(name.to_lowercase()));
    [Some(Key::Uuid(uuid)), name]
        .into_iter()
        .flatten()
        .collect()
}
