use super::{context::DrawContext, text::TextRenderer};

pub trait Shape {
    fn draw(&self, ctx: &mut DrawContext);
    fn size(&self) -> (u32, u32);
    fn measure(&self, _renderer: &TextRenderer) -> (u32, u32) {
        self.size()
    }
}
