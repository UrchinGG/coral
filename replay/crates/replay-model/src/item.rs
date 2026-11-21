use std::collections::HashMap;

use replay_protocol::{Nbt, Slot, decode_textures};

const SKULL: i16 = 397;
const PLAYER_SKULL: i16 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Item {
    pub id: i16,
    pub damage: i16,
    pub material: Option<Material>,
    pub color: Option<u32>,
    pub dyed: bool,
    pub enchanted: bool,
    pub skin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    Leather,
    Chainmail,
    Iron,
    Diamond,
    Gold,
}

impl Material {
    fn of(id: i16) -> Option<Material> {
        Some(match id {
            298..=301 => Material::Leather,
            302..=305 => Material::Chainmail,
            306..=309 => Material::Iron,
            310..=313 => Material::Diamond,
            314..=317 => Material::Gold,
            _ => return None,
        })
    }

    fn default_color(self) -> u32 {
        match self {
            Material::Leather => 0xA06540,
            Material::Chainmail => 0x70777B,
            Material::Iron => 0xD8D8D8,
            Material::Diamond => 0x4AEEE2,
            Material::Gold => 0xF4C638,
        }
    }
}

impl Item {
    fn new(slot: &Slot) -> Option<Item> {
        let (id, damage, nbt) = (slot.id, slot.damage, slot.nbt.as_ref());
        let material = Material::of(id);
        let dye = nbt
            .filter(|_| material == Some(Material::Leather))
            .and_then(|nbt| match nbt.get("display")?.get("color")? {
                Nbt::Int(color) => Some(*color as u32 & 0xFFFFFF),
                _ => None,
            });
        (id > 0).then(|| Item {
            id,
            damage,
            material,
            color: dye.or(material.map(Material::default_color)),
            dyed: dye.is_some(),
            enchanted: nbt.is_some_and(|nbt| {
                nbt.get("ench").is_some() || nbt.get("StoredEnchantments").is_some()
            }),
            skin: nbt
                .filter(|_| id == SKULL && damage == PLAYER_SKULL)
                .and_then(skull_skin),
        })
    }
}

fn skull_skin(nbt: &Nbt) -> Option<String> {
    let Nbt::List(textures) = nbt.get("SkullOwner")?.get("Properties")?.get("textures")? else {
        return None;
    };
    let Nbt::String(value) = textures.first()?.get("Value")? else {
        return None;
    };
    skin_hash(&decode_textures(value)?.skin_url?)
}

pub(crate) fn known_block(id: i32) -> bool {
    (0..=197).contains(&id)
}

pub(crate) fn known_item(id: i32) -> bool {
    matches!(id, 256..=425 | 427..=431 | 2256..=2267)
}

pub(crate) fn skin_hash(url: &str) -> Option<String> {
    let hash = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?
        .strip_prefix("textures.minecraft.net/texture/")?;
    let valid = (16..=80).contains(&hash.len())
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    valid.then(|| hash.to_owned())
}

#[derive(Default)]
pub(crate) struct Items {
    pub list: Vec<Item>,
    indices: HashMap<Item, u32>,
}

impl Items {
    pub fn intern(&mut self, slot: Option<&Slot>) -> Option<u32> {
        let item = Item::new(slot?)?;
        Some(*self.indices.entry(item).or_insert_with_key(|item| {
            self.list.push(item.clone());
            self.list.len() as u32 - 1
        }))
    }
}
