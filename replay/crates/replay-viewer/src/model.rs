use glam::{Mat4, Vec2};
use replay_assets::Rect;
use replay_render::{Cuboid, SHEET, Scene, WHITE};

use crate::draw::{Draw, lit};

#[derive(Clone, Copy)]
pub struct ModelBox {
    uv: [f32; 2],
    origin: [f32; 3],
    size: [f32; 3],
    inflate: f32,
    mirror: bool,
}

pub const fn cube(u: f32, v: f32, origin: [f32; 3], size: [f32; 3]) -> ModelBox {
    ModelBox {
        uv: [u, v],
        origin,
        size,
        inflate: 0.0,
        mirror: false,
    }
}

impl ModelBox {
    pub const fn inflated(self, by: f32) -> ModelBox {
        ModelBox {
            inflate: self.inflate + by,
            ..self
        }
    }

    pub const fn mirrored(self) -> ModelBox {
        ModelBox {
            mirror: true,
            ..self
        }
    }
}

#[derive(Clone, Copy)]
pub struct Paint {
    layer: i32,
    origin: Vec2,
    tint: u32,
    flash: u32,
    glint: bool,
}

impl Paint {
    pub fn sheet(rect: Option<Rect>) -> Paint {
        let rect = rect.unwrap_or_default();
        Paint {
            layer: SHEET,
            origin: Vec2::new(rect.x.into(), rect.y.into()),
            tint: WHITE,
            flash: 0,
            glint: false,
        }
    }

    pub fn skin(layer: i32) -> Paint {
        Paint {
            layer,
            origin: Vec2::ZERO,
            tint: WHITE,
            flash: 0,
            glint: false,
        }
    }

    pub fn tinted(self, tint: u32) -> Paint {
        Paint { tint, ..self }
    }

    pub fn flashed(self, flash: u32) -> Paint {
        Paint { flash, ..self }
    }

    pub fn glinting(self, glint: bool) -> Paint {
        Paint { glint, ..self }
    }
}

impl Draw<'_> {
    pub fn cuboids(&mut self, transform: Mat4, paint: Paint, boxes: &[ModelBox]) {
        let start = self.scene.cuboids.len();
        self.scene.cuboids.extend(boxes.iter().map(|model| Cuboid {
            inflate: model.inflate,
            layer: paint.layer,
            uv: paint.origin + Vec2::from(model.uv),
            mirror: model.mirror.into(),
            tint: lit(paint.tint, self.light),
            flash: paint.flash,
            ..Cuboid::new(transform, model.origin.into(), model.size.into())
        }));
        if paint.glint {
            let Scene {
                cuboids,
                glint_cuboids,
                ..
            } = &mut *self.scene;
            glint_cuboids.extend(cuboids[start..].iter().map(|cuboid| Cuboid {
                uv: cuboid.uv - paint.origin,
                ..*cuboid
            }));
        }
    }
}
