use mctext::{Canvas, LINE_HEIGHT, LayoutOptions, MCText, RenderOptions, Sampling};

use super::font;

const UNITS_PER_FONT_SIZE: f32 = 12.0;
const GLYPH_TOP_UNITS: f32 = 2.0;

pub struct TextRenderer;

impl TextRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn measure(&self, text: &MCText, font_size: f32) -> (f32, f32) {
        let (width, height) = font().measure(text);
        let lines = height / LINE_HEIGHT;
        (width * pixels_per_unit(font_size), lines * font_size)
    }

    pub fn draw(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        text: &MCText,
        font_size: f32,
        shadow: bool,
    ) {
        let scale = pixels_per_unit(font_size);
        let font = font();
        let layout = font.layout(text, &LayoutOptions::default());
        let options = RenderOptions {
            scale,
            shadow,
            sampling: Sampling::Area,
            ..RenderOptions::default()
        };
        let mut canvas = Canvas::new(buffer, width, height);
        font.render(
            &layout,
            &mut canvas,
            x,
            y + GLYPH_TOP_UNITS * scale,
            &options,
        );
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn pixels_per_unit(font_size: f32) -> f32 {
    font_size / UNITS_PER_FONT_SIZE
}
