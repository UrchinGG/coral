mod blocks;
mod entities;
mod glyphs;
mod items;
mod model;
mod texture;

use std::{
    array,
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fs,
    io::{self, Write},
    ops::Range,
    path::Path,
    slice,
};

use flate2::{Compression, write::GzEncoder};
use replay_world::{Block, BlockModels, Form};
use serde::{Deserialize, de::DeserializeOwned};

use blocks::Variant;
use model::{Models, Pick};
use texture::Textures;

#[derive(Deserialize)]
struct BlockInfo {
    id: u16,
    name: String,
    #[serde(rename = "boundingBox")]
    bounding_box: String,
    transparent: bool,
}

#[derive(Deserialize)]
struct StateFile {
    variants: BTreeMap<String, Options>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Options {
    One(Pick),
    Many(Vec<Pick>),
}

#[derive(Default)]
struct Baker {
    table: BlockModels,
    baked: HashMap<String, Option<Range<usize>>>,
    options: HashMap<Vec<Range<usize>>, Range<usize>>,
    sequences: HashMap<Vec<Form>, usize>,
    missing: BTreeSet<String>,
}

pub fn bake_assets(out: &Path) -> Result<(), Box<dyn Error>> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets-src");
    let assets = source.join("minecraft-assets-1.8.8");
    let mut infos: Vec<BlockInfo> = read_json(&source.join("minecraft-data/blocks.json"))?;
    infos.sort_by_key(|info| info.id);
    let states: HashMap<String, Option<StateFile>> = read_json(&assets.join("blocks_states.json"))?;
    let models = Models::load(&assets.join("blocks_models.json"))?;
    let grass = texture::plains_color(assets.join("colormap/grass.png"))?;
    let foliage = texture::plains_color(assets.join("colormap/foliage.png"))?;
    let mut textures = Textures::new(assets.join("blocks"));

    let mut baker = Baker::default();
    let ids = infos.last().map_or(0, |info| usize::from(info.id) + 1);
    baker.table.blocks = vec![Block::default(); ids << 4];
    for info in &infos {
        let state = states.get(&info.name).and_then(Option::as_ref);
        for meta in 0..16 {
            let rule = blocks::rule(&info.name, meta);
            let forms = (0..rule.forms())
                .map(|form| {
                    let options = picks(blocks::variant(&info.name, meta, form), state)
                        .iter()
                        .filter_map(|pick| baker.bake(&models, pick, &mut textures))
                        .collect();
                    (
                        options,
                        blocks::tint(&info.name, meta, form, grass, foliage),
                    )
                })
                .collect();
            let particle = match info.name.as_str() {
                "chest" | "trapped_chest" => Some("planks_oak"),
                "ender_chest" => Some("obsidian"),
                _ => picks(blocks::variant(&info.name, meta, 0), state)
                    .first()
                    .and_then(|pick| models.particle(pick)),
            };
            baker.table.blocks[usize::from(info.id) << 4 | usize::from(meta)] = Block {
                flags: blocks::flags(info, meta),
                rule,
                first_form: baker.forms(forms),
                particle: particle.map_or(u16::MAX, |texture| textures.layer(texture)),
            };
        }
    }
    baker.table.water = textures.layer("water_still");
    baker.table.lava = textures.layer("lava_still");
    baker.table.item_frame = textures.layer("itemframe_background");
    baker.table.destroy = array::from_fn(|stage| textures.layer(&format!("destroy_stage_{stage}")));

    let items = items::bake(&source, grass, foliage)?;
    let (atlas, entities) = entities::bake(&assets)?;
    let (glyphs, font) = glyphs::bake(&assets.join("font"))?;
    let blocks = textures.bake(grass)?;
    let models = &baker.table;
    let header =
        |bytes: &[u8]| [0, 4].map(|at| bytes[at..at + 4].try_into().map_or(0, u32::from_le_bytes));
    let layered = |bytes: &[u8]| {
        let [layers, mips] = header(bytes);
        format!("{layers} layers, {mips} mips")
    };
    let sheet = |bytes: &[u8]| {
        let [width, height] = header(bytes);
        format!("{width}x{height}")
    };

    fs::create_dir_all(out)?;
    let save = |name: &str, bytes: &[u8], detail: String| -> io::Result<()> {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::best());
        gzip.write_all(bytes)?;
        let gzipped = gzip.finish()?;
        println!(
            "{name:<14}{:>9} bytes{:>9} gzipped  {detail}",
            bytes.len(),
            gzipped.len()
        );
        fs::write(out.join(name), bytes)?;
        fs::write(out.join(format!("{name}.gz")), gzipped)
    };
    save(
        "blocks.bin",
        &models.encode(),
        format!(
            "{} states, {} forms, {} models, {} quads",
            models.blocks.len(),
            models.forms.len(),
            models.models.len(),
            models.quads.len(),
        ),
    )?;
    save("blocks.rgba", &blocks, layered(&blocks))?;
    save(
        "items.bin",
        &items.items.encode(),
        format!("{} looks", items.items.looks.len()),
    )?;
    save("items.rgba", &items.rgba, layered(&items.rgba))?;
    save(
        "entities.bin",
        &atlas.encode(),
        format!("{} textures", atlas.rects.len()),
    )?;
    save("entities.rgba", &entities, sheet(&entities))?;
    save(
        "glyphs.bin",
        &glyphs.encode(),
        format!("{} glyphs", glyphs.glyphs.len()),
    )?;
    save("glyphs.rgba", &font, sheet(&font))?;
    if !baker.missing.is_empty() {
        println!("missing models: {:?}", baker.missing);
    }
    if !items.missing.is_empty() {
        println!("missing item textures: {:?}", items.missing);
    }
    Ok(())
}

impl Baker {
    fn bake(
        &mut self,
        models: &Models,
        pick: &Pick,
        textures: &mut Textures,
    ) -> Option<Range<usize>> {
        let key = format!(
            "{}|{}|{}|{}|{:?}",
            pick.model, pick.x, pick.y, pick.uvlock, pick.retexture
        );
        self.baked
            .entry(key)
            .or_insert_with(|| {
                let Some(quads) = models.bake(pick, |name| textures.layer(name)) else {
                    self.missing.insert(pick.model.clone());
                    return None;
                };
                let start = self.table.quads.len();
                self.table.quads.extend(quads);
                Some(start..self.table.quads.len())
            })
            .clone()
    }

    fn forms(&mut self, forms: Vec<(Vec<Range<usize>>, u32)>) -> usize {
        let forms: Vec<Form> = forms
            .into_iter()
            .map(|(options, tint)| {
                let models = self.options.entry(options.clone()).or_insert_with(|| {
                    let start = self.table.models.len();
                    self.table.models.extend(options);
                    start..self.table.models.len()
                });
                Form {
                    models: models.clone(),
                    tint,
                }
            })
            .collect();
        *self.sequences.entry(forms.clone()).or_insert_with(|| {
            let first = self.table.forms.len();
            self.table.forms.extend(forms);
            first
        })
    }
}

impl Options {
    fn list(&self) -> &[Pick] {
        match self {
            Options::One(pick) => slice::from_ref(pick),
            Options::Many(picks) => picks,
        }
    }
}

fn picks(variant: Variant, state: Option<&StateFile>) -> Vec<Pick> {
    match variant {
        Variant::Nothing => Vec::new(),
        Variant::Model(pick) => vec![pick],
        Variant::State { key, rename } => {
            let variants = state.map(|state| &state.variants);
            let options = variants.and_then(|variants| {
                variants
                    .get(&key)
                    .or_else(|| variants.get("normal"))
                    .or_else(|| variants.values().next())
            });
            options
                .map_or(&[][..], Options::list)
                .iter()
                .cloned()
                .map(|mut pick| {
                    if let Some((from, to)) = &rename
                        && let Some(rest) = pick.model.strip_prefix(from)
                    {
                        pick.model = format!("{to}{rest}");
                    }
                    pick
                })
                .collect()
        }
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
