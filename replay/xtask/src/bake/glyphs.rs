use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    path::Path,
};

use image::{RgbaImage, imageops};
use replay_assets::{GLYPH_HEIGHT, Glyph, Glyphs, TEXELS_PER_PIXEL};

use super::texture;

const ASCII: &str = "ÀÁÂÈÊËÍÓÔÕÚßãõğİıŒœŞşŴŵžȇ\0\0\0\0\0\0\0 !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\0ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿®¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αβΓπΣσμτΦΘΩδ∞∅∈∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\0";
const COLUMNS: u32 = 16;
const CELL_WIDTH: u32 = 9 * TEXELS_PER_PIXEL;

pub fn bake(fonts: &Path) -> Result<(Glyphs, Vec<u8>), Box<dyn Error>> {
    let ascii = image::open(fonts.join("ascii.png"))?.to_rgba8();
    let cells: Vec<char> = ASCII.chars().collect();
    assert_eq!(cells.len(), 256);
    let characters: Vec<char> = Glyphs::characters().collect();
    let pages = characters
        .iter()
        .map(|&c| u32::from(c) >> 8)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|page| {
            let path = fonts.join(format!("unicode_page_{page:02x}.png"));
            Ok((page, image::open(path)?.to_rgba8()))
        })
        .collect::<Result<HashMap<_, _>, Box<dyn Error>>>()?;

    let rows = (2 * characters.len() as u32).div_ceil(COLUMNS);
    let mut atlas = RgbaImage::new(COLUMNS * CELL_WIDTH, rows * GLYPH_HEIGHT);
    let mut glyphs = Vec::new();
    for bold in [false, true] {
        for &c in &characters {
            let unicode = &pages[&(u32::from(c) >> 8)];
            let (bitmap, advance, shift) = glyph(c, &cells, &ascii, unicode);
            let index = glyphs.len() as u32;
            let (x, y) = (index % COLUMNS * CELL_WIDTH, index / COLUMNS * GLYPH_HEIGHT);
            imageops::overlay(&mut atlas, &bitmap, x.into(), y.into());
            let (width, advance) = if bold && advance > 0 {
                imageops::overlay(&mut atlas, &bitmap, (x + shift).into(), y.into());
                (bitmap.width() + shift, advance + TEXELS_PER_PIXEL)
            } else {
                (bitmap.width(), advance)
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

fn glyph(c: char, cells: &[char], ascii: &RgbaImage, unicode: &RgbaImage) -> (RgbaImage, u32, u32) {
    let blank = || RgbaImage::new(0, GLYPH_HEIGHT);
    let cell = cells.iter().position(|&cell| cell == c);
    match (c, cell) {
        ('§', _) => (blank(), 0, 0),
        _ if c.is_control() => (blank(), 0, 0),
        (' ', _) => (blank(), 4 * TEXELS_PER_PIXEL, 0),
        (_, Some(cell)) => {
            let (x, y) = (cell as u32 % 16 * 8, cell as u32 / 16 * 8);
            let columns = opaque_columns(ascii, x, y, 8).map_or(0, |(_, end)| end + 1);
            let bitmap = RgbaImage::from_fn(columns * TEXELS_PER_PIXEL, GLYPH_HEIGHT, |px, py| {
                *ascii.get_pixel(x + px / TEXELS_PER_PIXEL, y + py / TEXELS_PER_PIXEL)
            });
            (bitmap, (columns + 1) * TEXELS_PER_PIXEL, TEXELS_PER_PIXEL)
        }
        _ => {
            let (x, y) = (c as u32 % 16 * 16, c as u32 / 16 % 16 * 16);
            match opaque_columns(unicode, x, y, 16) {
                Some((start, end)) => {
                    let bitmap = RgbaImage::from_fn(end + 1 - start, GLYPH_HEIGHT, |px, py| {
                        *unicode.get_pixel(x + start + px, y + py)
                    });
                    (bitmap, end + 1 - start + TEXELS_PER_PIXEL, 1)
                }
                None => (blank(), 0, 0),
            }
        }
    }
}

fn opaque_columns(image: &RgbaImage, x: u32, y: u32, size: u32) -> Option<(u32, u32)> {
    let opaque = |&column: &u32| (0..size).any(|row| image.get_pixel(x + column, y + row)[3] > 0);
    Some(((0..size).find(opaque)?, (0..size).rev().find(opaque)?))
}
