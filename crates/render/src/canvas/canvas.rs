use image::{Rgba, RgbaImage};

use super::{color::TRANSPARENT, context::DrawContext, shape::Shape, text::TextRenderer};

pub struct Canvas {
    buffer: RgbaImage,
    renderer: TextRenderer,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            buffer: RgbaImage::from_pixel(width, height, TRANSPARENT),
            renderer: TextRenderer::new(),
        }
    }

    pub fn background(mut self, color: Rgba<u8>) -> Self {
        for pixel in self.buffer.pixels_mut() {
            *pixel = color;
        }
        self
    }

    pub fn draw(mut self, x: i32, y: i32, shape: &impl Shape) -> Self {
        let mut ctx = DrawContext::new(&mut self.buffer, &self.renderer);
        ctx.x = x;
        ctx.y = y;
        shape.draw(&mut ctx);
        self
    }

    pub fn build(self) -> RgbaImage {
        self.buffer
    }
}
