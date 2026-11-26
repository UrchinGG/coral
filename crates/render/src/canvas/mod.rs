use std::sync::OnceLock;

use mctext::Font;

mod canvas;
mod color;
mod context;
mod shape;
mod shapes;
mod text;

pub use canvas::Canvas;
pub use color::{BLACK, BOX_BACKGROUND, CANVAS_BACKGROUND, TRANSPARENT, WHITE, blend, rgb, rgba};
pub use context::DrawContext;
pub use mctext::{MCText, NamedColor};
pub use shape::Shape;
pub use shapes::*;
pub use text::TextRenderer;

pub use image::{DynamicImage, Rgba, RgbaImage};

static FONT: OnceLock<Font> = OnceLock::new();

pub fn init() {
    FONT.get_or_init(|| Font::modern().expect("bundled Minecraft font loads"));
}

pub(crate) fn font() -> &'static Font {
    FONT.get()
        .expect("render::canvas::init() must be called before rendering")
}
