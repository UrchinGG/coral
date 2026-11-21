use std::array;

use js_sys::{Reflect, Uint8Array};
use replay_assets::{EntityAtlas, Glyphs, Items, Rect};
use replay_render::Renderer;
use replay_world::BlockModels;
use wasm_bindgen::{JsCast, JsError, JsValue};

use crate::skin;

const TILE: usize = 16;
const LAYER_HEADER: usize = 8;

pub struct Assets {
    pub models: BlockModels,
    pub items: Items,
    pub atlas: EntityAtlas,
    pub glyphs: Glyphs,
    pub first_item_layer: i32,
    pub masks: Vec<[u16; TILE]>,
    pub steve: Vec<u8>,
}

struct Sheet<'a> {
    width: usize,
    height: usize,
    rgba: &'a [u8],
}

impl Assets {
    pub fn load(files: &JsValue, renderer: &mut Renderer) -> Result<Assets, JsError> {
        let file = |name: &str| {
            Reflect::get(files, &name.into())
                .ok()
                .and_then(|value| value.dyn_into::<Uint8Array>().ok())
                .map(|bytes| bytes.to_vec())
                .ok_or_else(|| JsError::new(&format!("missing {name}")))
        };
        let unreadable = |name: &str| JsError::new(&format!("unreadable {name}"));
        let models =
            BlockModels::decode(&file("blocks.bin")?).ok_or_else(|| unreadable("blocks.bin"))?;
        let items = Items::decode(&file("items.bin")?).ok_or_else(|| unreadable("items.bin"))?;
        let atlas = EntityAtlas::decode(&file("entities.bin")?)
            .ok_or_else(|| unreadable("entities.bin"))?;
        let mut glyphs =
            Glyphs::decode(&file("glyphs.bin")?).ok_or_else(|| unreadable("glyphs.bin"))?;

        let item_rgba = file("items.rgba")?;
        let first_item_layer = renderer.set_block_textures(&file("blocks.rgba")?, &item_rgba)?;
        let sprites = item_rgba
            .first_chunk()
            .map_or(0, |&count| u32::from_le_bytes(count));
        let masks = item_rgba
            .get(LAYER_HEADER..)
            .unwrap_or_default()
            .chunks_exact(TILE * TILE * 4)
            .take(sprites as usize)
            .map(mask)
            .collect();

        let entities = file("entities.rgba")?;
        let font = file("glyphs.rgba")?;
        let [top, bottom] = [&entities, &font].map(|bytes| sheet(bytes));
        let (top, bottom) = top
            .zip(bottom)
            .ok_or_else(|| unreadable("entities.rgba or glyphs.rgba"))?;
        let width = top.width.max(bottom.width);
        let mut stacked = vec![0; width * (top.height + bottom.height) * 4];
        for (offset, sheet) in [(0, &top), (top.height, &bottom)] {
            for (row, pixels) in sheet.rgba.chunks_exact(sheet.width * 4).enumerate() {
                let start = (offset + row) * width * 4;
                stacked[start..start + pixels.len()].copy_from_slice(pixels);
            }
        }
        renderer.set_sheet(width as u32, (stacked.len() / width / 4) as u32, &stacked)?;
        let glint = atlas.rect("misc/enchanted_item_glint").unwrap_or_default();
        renderer.set_glint([glint.x, glint.y, glint.width, glint.height].map(f32::from));
        for glyph in &mut glyphs.glyphs {
            glyph.y = glyph.y.saturating_add(top.height as u16);
        }
        let steve = atlas
            .rect("entity/steve")
            .and_then(|rect| skin::normalize(&crop(&stacked, width, rect)))
            .ok_or_else(|| unreadable("entity/steve"))?;

        Ok(Assets {
            models,
            items,
            atlas,
            glyphs,
            first_item_layer,
            masks,
            steve,
        })
    }
}

fn sheet(bytes: &[u8]) -> Option<Sheet<'_>> {
    let (width, rest) = bytes.split_first_chunk()?;
    let (height, rgba) = rest.split_first_chunk()?;
    let [width, height] = [width, height].map(|value| u32::from_le_bytes(*value) as usize);
    (width > 0 && rgba.len() == width * height * 4).then_some(Sheet {
        width,
        height,
        rgba,
    })
}

fn crop(rgba: &[u8], width: usize, rect: Rect) -> Vec<u8> {
    let [x, y, columns, rows] = [rect.x, rect.y, rect.width, rect.height].map(usize::from);
    (y..y + rows)
        .flat_map(|row| {
            let start = (row * width + x) * 4;
            rgba.get(start..start + columns * 4).unwrap_or_default()
        })
        .copied()
        .collect()
}

fn mask(tile: &[u8]) -> [u16; TILE] {
    array::from_fn(|y| {
        (0..TILE)
            .filter(|x| tile[(y * TILE + x) * 4 + 3] > 0)
            .fold(0, |row, x| row | 1 << x)
    })
}
