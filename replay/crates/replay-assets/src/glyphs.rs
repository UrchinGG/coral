use crate::{Reader, write_list};

const MAGIC: &[u8; 4] = b"UGL1";
const SYMBOLS: [char; 1] = ['❤'];
const COVERED: usize = 256 + SYMBOLS.len();

pub const TEXELS_PER_PIXEL: u32 = 2;
pub const GLYPH_HEIGHT: u32 = 8 * TEXELS_PER_PIXEL;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub advance: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glyphs {
    pub glyphs: Vec<Glyph>,
}

impl Glyphs {
    pub fn characters() -> impl Iterator<Item = char> {
        (0..=u8::MAX).map(char::from).chain(SYMBOLS)
    }

    pub fn get(&self, c: char, bold: bool) -> Option<Glyph> {
        let code = match c as usize {
            code @ 0..256 => code,
            _ => 256 + SYMBOLS.iter().position(|&symbol| symbol == c)?,
        };
        self.glyphs.get(usize::from(bold) * COVERED + code).copied()
    }

    pub fn width(&self, text: &str) -> u32 {
        let mut chars = text.chars();
        let mut bold = false;
        let mut width = 0;
        while let Some(c) = chars.next() {
            if c == '§' {
                match chars.next() {
                    Some('l' | 'L') => bold = true,
                    Some('r' | 'R') => bold = false,
                    _ => {}
                }
                continue;
            }
            let advance = self
                .get(c, false)
                .map_or(0, |glyph| u32::from(glyph.advance) / TEXELS_PER_PIXEL);
            width += advance + u32::from(bold && advance > 0);
        }
        width
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        write_list(&mut out, &self.glyphs, |out, glyph| {
            out.extend(glyph.x.to_le_bytes());
            out.extend(glyph.y.to_le_bytes());
            out.extend([glyph.width, glyph.advance]);
        });
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Glyphs> {
        let mut reader = Reader::new(bytes, MAGIC)?;
        let glyphs = reader.list(|reader| {
            Some(Glyph {
                x: reader.u16()?,
                y: reader.u16()?,
                width: reader.u8()?,
                advance: reader.u8()?,
            })
        })?;
        (reader.is_empty() && glyphs.len() == 2 * COVERED).then_some(Glyphs { glyphs })
    }
}
