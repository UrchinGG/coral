mod model;
mod output;
mod render;
mod skin;

pub use model::{Pose, Rotation};
pub use output::{OutputType, RenderOutput};
pub use render::Renderer;
pub use skin::Skin;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkinError {
    #[error("invalid skin dimensions: expected 64x64, got {0}x{1}")]
    InvalidDimensions(u32, u32),
    #[error("failed to decode image: {0}")]
    ImageDecode(#[from] image::ImageError),
    #[error("render error: {0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, SkinError>;

pub fn render(skin: &Skin, pose: &Pose, output: OutputType) -> Result<RenderOutput> {
    Renderer::new()?.render(skin, pose, output)
}
