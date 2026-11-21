use std::{
    collections::HashMap,
    error::Error,
    iter::successors,
    path::{Path, PathBuf},
};

use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use replay_assets::halve;

const SIZE: usize = 16;

pub type Tile = Vec<[u8; 4]>;

pub struct Textures {
    dir: PathBuf,
    names: Vec<String>,
    layers: HashMap<String, u16>,
}

impl Textures {
    pub fn new(dir: PathBuf) -> Textures {
        Textures {
            dir,
            names: vec!["missing".into()],
            layers: HashMap::new(),
        }
    }

    pub fn layer(&mut self, name: &str) -> u16 {
        if let Some(&layer) = self.layers.get(name) {
            return layer;
        }
        let layer = if self.dir.join(format!("{name}.png")).exists() {
            self.names.push(name.into());
            (self.names.len() - 1) as u16
        } else {
            0
        };
        self.layers.insert(name.into(), layer);
        layer
    }

    pub fn bake(&self, grass: u32) -> Result<Vec<u8>, Box<dyn Error>> {
        let checker = (0..SIZE * SIZE)
            .map(|index| match (index % SIZE / 8 + index / SIZE / 8) % 2 {
                0 => [248, 0, 248, 255],
                _ => [0, 0, 0, 255],
            })
            .collect();
        let mut tiles = vec![checker];
        for name in &self.names[1..] {
            let mut tile = self.load(name)?;
            if name == "grass_side" {
                let overlay = self.load("grass_side_overlay")?;
                for (texel, over) in tile.iter_mut().zip(overlay) {
                    let alpha = f32::from(over[3]) / 255.0;
                    for channel in 0..3 {
                        let tinted = f32::from(over[channel])
                            * (grass >> (16 - 8 * channel) & 0xff) as f32
                            / 255.0;
                        texel[channel] = (f32::from(texel[channel]) * (1.0 - alpha)
                            + tinted * alpha)
                            .round() as u8;
                    }
                }
            }
            tiles.push(tile);
        }
        Ok(array(tiles))
    }

    fn load(&self, name: &str) -> Result<Tile, Box<dyn Error>> {
        tile(&self.dir.join(format!("{name}.png")))
    }
}

pub fn tile(path: &Path) -> Result<Tile, Box<dyn Error>> {
    let image = image::open(path)?.to_rgba8();
    let frame = image.width().min(image.height());
    let frame = imageops::crop_imm(&image, 0, 0, frame, frame).to_image();
    let frame = imageops::resize(&frame, SIZE as u32, SIZE as u32, FilterType::Nearest);
    Ok(frame.pixels().map(|pixel| pixel.0).collect())
}

pub fn array(mut tiles: Vec<Tile>) -> Vec<u8> {
    tiles.iter_mut().for_each(bleed);
    let layers = tiles.len() as u32;
    let levels: Vec<Vec<Tile>> = successors(Some((tiles, SIZE)), |(tiles, size)| {
        (*size > 1).then(|| {
            (
                tiles.iter().map(|tile| downsample(tile, *size)).collect(),
                size / 2,
            )
        })
    })
    .map(|(tiles, _)| tiles)
    .collect();
    let mut out = Vec::new();
    out.extend(layers.to_le_bytes());
    out.extend((levels.len() as u32).to_le_bytes());
    out.extend(levels.iter().flatten().flatten().flatten());
    out
}

pub fn plains_color(path: PathBuf) -> Result<u32, Box<dyn Error>> {
    let [red, green, blue, _] = image::open(path)?.to_rgba8().get_pixel(50, 173).0;
    Ok(u32::from_be_bytes([0, red, green, blue]))
}

fn bleed(tile: &mut Tile) {
    let mut known: Vec<bool> = tile.iter().map(|texel| texel[3] > 0).collect();
    loop {
        let fills: Vec<(usize, [u8; 4])> = (0..SIZE * SIZE)
            .filter(|&index| !known[index])
            .filter_map(|index| {
                let (x, y) = (index % SIZE, index / SIZE);
                let neighbors: Vec<[u8; 4]> = (y.saturating_sub(1)..=(y + 1).min(SIZE - 1))
                    .flat_map(|y| {
                        (x.saturating_sub(1)..=(x + 1).min(SIZE - 1)).map(move |x| y * SIZE + x)
                    })
                    .filter(|&neighbor| known[neighbor])
                    .map(|neighbor| tile[neighbor])
                    .collect();
                let mean = |channel: usize| {
                    let sum: u32 = neighbors
                        .iter()
                        .map(|texel| u32::from(texel[channel]))
                        .sum();
                    (sum / neighbors.len() as u32) as u8
                };
                (!neighbors.is_empty()).then(|| (index, [mean(0), mean(1), mean(2), 0]))
            })
            .collect();
        if fills.is_empty() {
            return;
        }
        for (index, texel) in fills {
            tile[index] = texel;
            known[index] = true;
        }
    }
}

fn downsample(tile: &Tile, size: usize) -> Tile {
    let (halved, ..) = halve(tile.as_flattened(), size, size);
    halved.as_chunks::<4>().0.to_vec()
}

pub fn sheet(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(image.width().to_le_bytes());
    out.extend(image.height().to_le_bytes());
    out.extend(image.as_raw());
    out
}
