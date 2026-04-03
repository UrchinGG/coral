use image::{ImageEncoder, ImageError, RgbaImage};


#[derive(Debug, Clone)]
pub enum OutputType {
    FullBody { width: u32, height: u32 },
    Head { width: u32, height: u32 },
    Face { size: u32 },
}


impl OutputType {
    pub fn full_body(width: u32, height: u32) -> Self { Self::FullBody { width, height } }
    pub fn head(width: u32, height: u32) -> Self { Self::Head { width, height } }
    pub fn face(size: u32) -> Self { Self::Face { size } }
}


pub struct RenderOutput {
    pub image: RgbaImage,
}


impl RenderOutput {
    pub fn into_image(self) -> RgbaImage { self.image }

    pub fn to_png_bytes(&self) -> Result<Vec<u8>, ImageError> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &self.image,
                self.image.width(),
                self.image.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        Ok(bytes)
    }
}
