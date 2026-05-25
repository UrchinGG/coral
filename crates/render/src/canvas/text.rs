use mctext::{
    LayoutEngine, LayoutOptions, MCText, SoftwareRenderer, TextRenderer as McTextRenderer,
};

use super::font_system;

pub struct TextRenderer;

impl TextRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn measure(&self, text: &MCText, size: f32) -> (f32, f32) {
        LayoutEngine::new(font_system()).measure(text, size)
    }

    pub fn draw(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        x: f32,
        y: f32,
        text: &MCText,
        size: f32,
        shadow: bool,
    ) {
        let options = LayoutOptions::new(size).with_shadow(shadow);
        let engine = LayoutEngine::new(font_system());
        let layout = engine.layout_at(text, x, y, &options);
        let mut renderer =
            SoftwareRenderer::new(font_system(), buffer, width as usize, height as usize);
        let _ = renderer.render_layout(&layout);
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
