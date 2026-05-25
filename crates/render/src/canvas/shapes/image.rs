use image::DynamicImage;

use crate::canvas::{context::DrawContext, shape::Shape};

pub struct Image<'a> {
    image: &'a DynamicImage,
    width: Option<u32>,
    height: Option<u32>,
}

impl<'a> Image<'a> {
    pub fn new(image: &'a DynamicImage) -> Self {
        Self {
            image,
            width: None,
            height: None,
        }
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}

impl Shape for Image<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        let rgba = self
            .image
            .as_rgba8()
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or_else(|| std::borrow::Cow::Owned(self.image.to_rgba8()));
        match (self.width, self.height) {
            (Some(w), Some(h)) => {
                let resized =
                    image::imageops::resize(&*rgba, w, h, image::imageops::FilterType::Triangle);
                image::imageops::overlay(ctx.buffer, &resized, ctx.x as i64, ctx.y as i64);
            }
            _ => {
                image::imageops::overlay(ctx.buffer, &*rgba, ctx.x as i64, ctx.y as i64);
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        match (self.width, self.height) {
            (Some(w), Some(h)) => (w, h),
            _ => (self.image.width(), self.image.height()),
        }
    }
}
