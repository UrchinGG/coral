use std::error::Error;

use image::{Rgba, RgbaImage, imageops};
use mctext::{FORMAT_SIGN, Font, GlyphImage};
use replay_assets::{GLYPH_HEIGHT, Glyph, Glyphs, TEXELS_PER_PIXEL};

use super::texture;

const COLUMNS: u32 = 16;
const CELL_WIDTH: u32 = 9 * TEXELS_PER_PIXEL;
const BOLD_SHIFT: u32 = TEXELS_PER_PIXEL;

pub fn bake() -> Result<(Glyphs, Vec<u8>), Box<dyn Error>> {
    let font = Font::legacy()?;
    let characters: Vec<char> = Glyphs::characters().collect();
    let rows = (2 * characters.len() as u32).div_ceil(COLUMNS);
    let mut atlas = RgbaImage::new(COLUMNS * CELL_WIDTH, rows * GLYPH_HEIGHT);
    let mut glyphs = Vec::new();
    for bold in [false, true] {
        for &c in &characters {
            let (bitmap, advance) = glyph(&font, c);
            let index = glyphs.len() as u32;
            let (x, y) = (index % COLUMNS * CELL_WIDTH, index / COLUMNS * GLYPH_HEIGHT);
            imageops::overlay(&mut atlas, &bitmap, x.into(), y.into());
            let (width, advance) = match (bold, advance, bitmap.width()) {
                (true, 1.., 0) => (0, advance + TEXELS_PER_PIXEL),
                (true, 1.., width) => {
                    imageops::overlay(&mut atlas, &bitmap, (x + BOLD_SHIFT).into(), y.into());
                    (width + BOLD_SHIFT, advance + TEXELS_PER_PIXEL)
                }
                (_, _, width) => (width, advance),
            };
            glyphs.push(Glyph {
                x: x as u16,
                y: y as u16,
                width: width as u8,
                advance: advance as u8,
            });
        }
    }
    Ok((Glyphs { glyphs }, texture::sheet(&atlas)))
}

fn glyph(font: &Font, c: char) -> (RgbaImage, u32) {
    let blank = || RgbaImage::new(0, GLYPH_HEIGHT);
    if c == FORMAT_SIGN || c.is_control() {
        return (blank(), 0);
    }
    let glyph = font.glyph(c);
    let advance = texels(glyph.advance);
    let Some(image) = glyph.image else {
        return (blank(), advance);
    };
    let width = texels(glyph.advance - 1.0);
    (bitmap(&image, width), advance)
}

fn bitmap(image: &GlyphImage, width: u32) -> RgbaImage {
    let texels_per_image_pixel = image.scale * TEXELS_PER_PIXEL as f32;
    let source = |texel: u32| (texel as f32 / texels_per_image_pixel) as u32;
    RgbaImage::from_fn(width, GLYPH_HEIGHT, |x, y| {
        Rgba(image.pixel(source(x), source(y)))
    })
}

fn texels(units: f32) -> u32 {
    (units * TEXELS_PER_PIXEL as f32).round() as u32
}
