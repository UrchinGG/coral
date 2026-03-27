use image::{DynamicImage, RgbaImage};

use super::{Result, SkinError};


pub struct Skin {
    pub(crate) texture: RgbaImage,
    pub(crate) slim: bool,
}


impl Skin {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_dynamic(image::load_from_memory(data)?)
    }

    pub fn from_image(image: RgbaImage) -> Result<Self> { Self::validate(image) }

    pub fn from_dynamic(image: DynamicImage) -> Result<Self> { Self::validate(image.into_rgba8()) }

    fn validate(texture: RgbaImage) -> Result<Self> {
        if texture.width() != 64 || texture.height() != 64 {
            return Err(SkinError::InvalidDimensions(texture.width(), texture.height()));
        }
        let slim = texture.get_pixel(54, 20).0[3] == 0;
        Ok(Self { texture, slim })
    }

    pub fn is_slim(&self) -> bool { self.slim }

    pub fn set_slim(&mut self, slim: bool) { self.slim = slim; }

    pub fn texture(&self) -> &RgbaImage { &self.texture }
}
