use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    path::Path,
};

use replay_assets::{Hold, ItemLook, Items};
use serde::Deserialize;

use super::{
    blocks::{self, COLORS},
    read_json, texture,
};

#[derive(Deserialize)]
struct ItemInfo {
    id: i16,
    name: String,
    #[serde(default)]
    variations: Vec<Variation>,
}

#[derive(Deserialize)]
struct Variation {
    metadata: i16,
}

#[derive(Deserialize)]
struct ItemTexture {
    name: String,
    texture: Option<String>,
}

pub struct Baked {
    pub items: Items,
    pub rgba: Vec<u8>,
    pub missing: BTreeSet<String>,
}

pub fn bake(source: &Path, grass: u32, foliage: u32) -> Result<Baked, Box<dyn Error>> {
    let assets = source.join("minecraft-assets-1.8.8");
    let infos: Vec<ItemInfo> = read_json(&source.join("minecraft-data/items.json"))?;
    let textures: HashMap<String, String> =
        read_json::<Vec<ItemTexture>>(&assets.join("items_textures.json"))?
            .into_iter()
            .filter_map(|entry| Some((entry.name, entry.texture?)))
            .collect();

    let mut tiles = Vec::new();
    let mut layers = HashMap::new();
    let mut missing = BTreeSet::new();
    let mut sprite = |path: String, tint: u32| -> Result<Option<u16>, Box<dyn Error>> {
        if let Some(&layer) = layers.get(&(path.clone(), tint)) {
            return Ok(Some(layer));
        }
        let file = assets.join(format!("{path}.png"));
        if !file.exists() {
            missing.insert(path);
            return Ok(None);
        }
        let mut tile = texture::tile(&file)?;
        for texel in &mut tile {
            for (channel, value) in texel[..3].iter_mut().enumerate() {
                *value = (u32::from(*value) * (tint >> (24 - 8 * channel) & 0xff) / 255) as u8;
            }
        }
        tiles.push(tile);
        let layer = (tiles.len() - 1) as u16;
        layers.insert((path, tint), layer);
        Ok(Some(layer))
    };

    let mut looks = BTreeMap::new();
    for info in &infos {
        let name = info.name.as_str();
        if info.id < 256 {
            let damages: Vec<i16> = match info.variations.as_slice() {
                [] => vec![0],
                variations => variations
                    .iter()
                    .map(|variation| variation.metadata)
                    .collect(),
            };
            for damage in damages {
                let meta = damage as u16 & 15;
                let look = match flat_block(name, usize::from(meta)) {
                    Some(path) => sprite(path, blocks::tint(name, meta, 0, grass, foliage))?
                        .map(|layer| ItemLook::Sprite(layer, Hold::Generated)),
                    None => {
                        let meta = if name == "anvil" { meta << 2 } else { meta };
                        Some(ItemLook::Block((info.id as u16) << 4 | meta))
                    }
                };
                looks.extend(look.map(|look| ((info.id, damage), look)));
            }
        } else {
            for (damage, path) in item_textures(name, textures.get(name)) {
                if let Some(layer) = sprite(path, u32::MAX)? {
                    looks.insert((info.id, damage), ItemLook::Sprite(layer, hold(name)));
                }
            }
        }
    }

    Ok(Baked {
        items: Items { looks },
        rgba: texture::array(tiles),
        missing,
    })
}

fn item_textures(name: &str, texture: Option<&String>) -> Vec<(i16, String)> {
    let numbered = |names: &[&str]| -> Vec<(i16, String)> {
        names
            .iter()
            .zip(0..)
            .map(|(name, damage)| (damage, format!("items/{name}")))
            .collect()
    };
    match name {
        "coal" => numbered(&["coal", "charcoal"]),
        "fish" => numbered(&[
            "fish_cod_raw",
            "fish_salmon_raw",
            "fish_clownfish_raw",
            "fish_pufferfish_raw",
        ]),
        "cooked_fish" => numbered(&["fish_cod_cooked", "fish_salmon_cooked"]),
        "dye" => COLORS
            .iter()
            .rev()
            .zip(0..)
            .map(|(color, damage)| (damage, format!("items/dye_powder_{color}")))
            .collect(),
        "potion" => vec![
            (0, "items/potion_bottle_drinkable".into()),
            (0x4000, "items/potion_bottle_splash".into()),
        ],
        _ => vec![(
            0,
            texture.cloned().unwrap_or_else(|| format!("items/{name}")),
        )],
    }
}

fn flat_block(name: &str, meta: usize) -> Option<String> {
    let texture = match name {
        "sapling" => format!(
            "sapling_{}",
            ["oak", "spruce", "birch", "jungle", "acacia", "roofed_oak"].get(meta)?
        ),
        "tallgrass" => ["deadbush", "tallgrass", "fern"].get(meta)?.to_string(),
        "red_flower" => format!(
            "flower_{}",
            [
                "rose",
                "blue_orchid",
                "allium",
                "houstonia",
                "tulip_red",
                "tulip_orange",
                "tulip_white",
                "tulip_pink",
                "oxeye_daisy",
            ]
            .get(meta)?
        ),
        "double_plant" => format!(
            "double_plant_{}",
            [
                "sunflower_front",
                "syringa_top",
                "grass_top",
                "fern_top",
                "rose_top",
                "paeonia_top",
            ]
            .get(meta)?
        ),
        "stained_glass_pane" => format!("glass_{}", COLORS.get(meta)?),
        "yellow_flower" => "flower_dandelion".into(),
        "brown_mushroom" => "mushroom_brown".into(),
        "red_mushroom" => "mushroom_red".into(),
        "torch" => "torch_on".into(),
        "redstone_torch" => "redstone_torch_on".into(),
        "tripwire_hook" => "trip_wire_source".into(),
        "rail" => "rail_normal".into(),
        "golden_rail" => "rail_golden".into(),
        "detector_rail" => "rail_detector".into(),
        "activator_rail" => "rail_activator".into(),
        "glass_pane" => "glass".into(),
        "deadbush" | "web" | "vine" | "waterlily" | "ladder" | "lever" | "iron_bars" => name.into(),
        "hopper" | "barrier" => return Some(format!("items/{name}")),
        _ => return None,
    };
    Some(format!("blocks/{texture}"))
}

fn hold(name: &str) -> Hold {
    let tool = ["_pickaxe", "_axe", "_shovel", "_hoe"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    match name {
        "bow" => Hold::Bow,
        _ if name.ends_with("_sword") => Hold::Sword,
        "stick" | "bone" | "blaze_rod" | "fishing_rod" | "carrot_on_a_stick" => Hold::Handheld,
        _ if tool => Hold::Handheld,
        _ => Hold::Generated,
    }
}
