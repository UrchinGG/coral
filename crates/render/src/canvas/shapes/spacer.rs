use crate::canvas::{context::DrawContext, shape::Shape};

pub struct Spacer {
    width: u32,
    height: u32,
}

impl Spacer {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
    pub fn horizontal(width: u32) -> Self {
        Self { width, height: 0 }
    }
    pub fn vertical(height: u32) -> Self {
        Self { width: 0, height }
    }
}

impl Shape for Spacer {
    fn draw(&self, _ctx: &mut DrawContext) {}
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
