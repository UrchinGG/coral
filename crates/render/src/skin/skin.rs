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

    pub fn from_image(image: RgbaImage) -> Result<Self> {
        Self::validate(image)
    }

    pub fn from_dynamic(image: DynamicImage) -> Result<Self> {
        Self::validate(image.into_rgba8())
    }

    fn validate(texture: RgbaImage) -> Result<Self> {
        let texture = match (texture.width(), texture.height()) {
            (64, 64) => texture,
            (64, 32) => upgrade_legacy(&texture),
            (w, h) => return Err(SkinError::InvalidDimensions(w, h)),
        };
        let slim = texture.get_pixel(54, 20).0[3] == 0;
        Ok(Self { texture, slim })
    }

    pub fn is_slim(&self) -> bool {
        self.slim
    }

    pub fn set_slim(&mut self, slim: bool) {
        self.slim = slim;
    }

    pub fn texture(&self) -> &RgbaImage {
        &self.texture
    }
}

fn upgrade_legacy(src: &RgbaImage) -> RgbaImage {
    use image::Rgba;
    let mut out = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
    for y in 0..16 {
        for x in 0..32 {
            out.put_pixel(x, y, *src.get_pixel(x, y));
        }
    }
    for y in 16..32 {
        for x in 0..64 {
            out.put_pixel(x, y, *src.get_pixel(x, y));
        }
    }
    mirror_blit(src, &mut out, (40, 16, 16, 16), (32, 48));
    mirror_blit(src, &mut out, (0, 16, 16, 16), (16, 48));
    out
}

fn mirror_blit(
    src: &RgbaImage,
    dst: &mut RgbaImage,
    src_rect: (u32, u32, u32, u32),
    dst_origin: (u32, u32),
) {
    let (sx, sy, w, h) = src_rect;
    let (dx, dy) = dst_origin;
    for y in 0..h {
        for x in 0..w {
            let mirrored_x = w - 1 - x;
            dst.put_pixel(dx + x, dy + y, *src.get_pixel(sx + mirrored_x, sy + y));
        }
    }
}
