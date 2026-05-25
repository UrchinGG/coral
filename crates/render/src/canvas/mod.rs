use std::sync::OnceLock;

mod canvas;
mod color;
mod context;
mod shape;
mod shapes;
mod text;

pub use canvas::Canvas;
pub use color::{BLACK, BOX_BACKGROUND, CANVAS_BACKGROUND, TRANSPARENT, WHITE, blend, rgb, rgba};
pub use context::DrawContext;
pub use mctext::{FontSystem, FontVariant, MCText, NamedColor};
pub use shape::Shape;
pub use shapes::*;
pub use text::TextRenderer;

pub use image::{DynamicImage, Rgba, RgbaImage};

static FONT_SYSTEM: OnceLock<FontSystem> = OnceLock::new();

pub fn init() {
    let fs = FONT_SYSTEM.get_or_init(FontSystem::modern);
    for variant in [
        FontVariant::Regular,
        FontVariant::Bold,
        FontVariant::Italic,
        FontVariant::BoldItalic,
    ] {
        fs.measure_char('A', 16.0, variant);
    }
}

pub(crate) fn font_system() -> &'static FontSystem {
    FONT_SYSTEM
        .get()
        .expect("render::canvas::init() must be called before rendering")
}
