use image::RgbaImage;

use super::text::TextRenderer;

pub struct DrawContext<'a> {
    pub buffer: &'a mut RgbaImage,
    pub renderer: &'a TextRenderer,
    pub x: i32,
    pub y: i32,
}

impl<'a> DrawContext<'a> {
    pub fn new(buffer: &'a mut RgbaImage, renderer: &'a TextRenderer) -> Self {
        Self {
            buffer,
            renderer,
            x: 0,
            y: 0,
        }
    }

    pub fn at(&mut self, dx: i32, dy: i32) -> DrawContext<'_> {
        DrawContext {
            buffer: self.buffer,
            renderer: self.renderer,
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}
