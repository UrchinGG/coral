use crate::{Reader, write_list};

const MAGIC: &[u8; 4] = b"UEA1";

pub const ENTITY_TEXTURES: [&str; 39] = [
    "entity/steve",
    "entity/villager/villager",
    "entity/villager/farmer",
    "entity/villager/librarian",
    "entity/villager/priest",
    "entity/villager/smith",
    "entity/villager/butcher",
    "entity/iron_golem",
    "entity/silverfish",
    "entity/bat",
    "entity/enderdragon/dragon",
    "entity/explosion",
    "entity/arrow",
    "entity/skeleton/skeleton",
    "entity/skeleton/wither_skeleton",
    "entity/zombie/zombie",
    "entity/creeper/creeper",
    "entity/chest/normal",
    "entity/chest/trapped",
    "entity/chest/ender",
    "entity/chest/normal_double",
    "entity/chest/trapped_double",
    "models/armor/leather_layer_1",
    "models/armor/leather_layer_1_overlay",
    "models/armor/leather_layer_2",
    "models/armor/leather_layer_2_overlay",
    "models/armor/chainmail_layer_1",
    "models/armor/chainmail_layer_2",
    "models/armor/iron_layer_1",
    "models/armor/iron_layer_2",
    "models/armor/diamond_layer_1",
    "models/armor/diamond_layer_2",
    "models/armor/gold_layer_1",
    "models/armor/gold_layer_2",
    "blocks/fire_layer_0",
    "blocks/fire_layer_1",
    "particle/particles",
    "particle/footprint",
    "misc/enchanted_item_glint",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityAtlas {
    pub rects: Vec<Rect>,
}

impl EntityAtlas {
    pub fn rect(&self, path: &str) -> Option<Rect> {
        let index = ENTITY_TEXTURES.iter().position(|&known| known == path)?;
        self.rects.get(index).copied()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        write_list(&mut out, &self.rects, |out, rect| {
            for value in [rect.x, rect.y, rect.width, rect.height] {
                out.extend(value.to_le_bytes());
            }
        });
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<EntityAtlas> {
        let mut reader = Reader::new(bytes, MAGIC)?;
        let rects = reader.list(|reader| {
            Some(Rect {
                x: reader.u16()?,
                y: reader.u16()?,
                width: reader.u16()?,
                height: reader.u16()?,
            })
        })?;
        (reader.is_empty() && rects.len() == ENTITY_TEXTURES.len()).then_some(EntityAtlas { rects })
    }
}
