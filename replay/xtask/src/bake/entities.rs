use std::{cmp::Reverse, error::Error, path::Path};

use image::{RgbaImage, imageops};
use replay_assets::{ENTITY_TEXTURES, EntityAtlas, Rect};

use super::texture;

const WIDTH: usize = 512;
const STEP: usize = 8;

pub fn bake(assets: &Path) -> Result<(EntityAtlas, Vec<u8>), Box<dyn Error>> {
    let images = ENTITY_TEXTURES
        .iter()
        .map(|path| Ok(image::open(assets.join(format!("{path}.png")))?.to_rgba8()))
        .collect::<Result<Vec<RgbaImage>, Box<dyn Error>>>()?;

    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&index| Reverse((images[index].height(), images[index].width())));
    let mut skyline = [0; WIDTH];
    let mut rects = vec![Rect::default(); images.len()];
    for index in order {
        let (width, height) = images[index].dimensions();
        let span = width as usize;
        let last = WIDTH
            .checked_sub(span)
            .ok_or_else(|| format!("{} is wider than the atlas", ENTITY_TEXTURES[index]))?;
        let (x, y) = (0..=last)
            .step_by(STEP)
            .map(|x| (x, skyline[x..x + span].iter().copied().max().unwrap_or(0)))
            .min_by_key(|&(x, y)| (y, x))
            .unwrap_or_default();
        skyline[x..x + span].fill(y + height);
        rects[index] = Rect {
            x: x as u16,
            y: y as u16,
            width: width as u16,
            height: height as u16,
        };
    }

    let height = skyline.iter().copied().max().unwrap_or(0);
    let mut atlas = RgbaImage::new(WIDTH as u32, height);
    for (image, rect) in images.iter().zip(&rects) {
        imageops::replace(&mut atlas, image, rect.x.into(), rect.y.into());
    }
    Ok((EntityAtlas { rects }, texture::sheet(&atlas)))
}
