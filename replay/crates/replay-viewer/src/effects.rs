use std::f32::consts::PI;

use glam::{Mat4, Vec3, vec2, vec3};
use replay_assets::{ItemLook, Rect};
use replay_render::{Billboard, SHEET, WHITE, quad};
use replay_sim::{MAX_PARTICLES, Particle, Sprite, TICK_MS};

use crate::assets::Assets;
use crate::draw::Draw;

const FIRE_FRAMES: u32 = 32;
const FIRE_LAYERS: [&str; 2] = ["blocks/fire_layer_0", "blocks/fire_layer_1"];
const FIRE_SHEET: f32 = 1.4;
const FIRE_STEP: f32 = 0.45;

pub fn particles(draw: &mut Draw, particles: impl Iterator<Item = Particle>) {
    let assets = draw.assets;
    let billboards = particles
        .take(MAX_PARTICLES)
        .filter_map(|particle| billboard(assets, particle));
    draw.scene.particles.extend(billboards);
}

pub fn fire(draw: &mut Draw, feet: Vec3, width: f32, height: f32) {
    let layers = FIRE_LAYERS.map(|path| draw.assets.atlas.rect(path).unwrap_or_default());
    let frame = (draw.t / TICK_MS) as u32 % FIRE_FRAMES;
    let scale = width * FIRE_SHEET;
    let facing = Mat4::from_translation(feet)
        * Mat4::from_rotation_y(PI - draw.camera.yaw)
        * Mat4::from_scale(Vec3::splat(scale));
    let span = height / scale;
    let sheets = (span / FIRE_STEP).ceil() as usize;
    for index in 0..sheets {
        let rect = layers[index % 2];
        let size = f32::from(rect.width);
        let origin = vec2(rect.x.into(), f32::from(rect.y) + (frame as f32) * size);
        let (left, right) = if index / 2 % 2 == 0 {
            (size, 0.0)
        } else {
            (0.0, size)
        };
        let step = index as f32;
        let half = 0.5 * 0.9_f32.powi(index as i32);
        let bottom = step * FIRE_STEP;
        let top = bottom + FIRE_SHEET;
        let depth = 0.3 - span.floor() * 0.02 - step * 0.03;
        let corners = [
            vec3(half, bottom, depth),
            vec3(-half, bottom, depth),
            vec3(-half, top, depth),
            vec3(half, top, depth),
        ]
        .map(|corner| facing.transform_point3(corner));
        let uv = [
            vec2(right, size),
            vec2(left, size),
            vec2(left, 0.0),
            vec2(right, 0.0),
        ]
        .map(|corner| origin + corner);
        draw.scene.blended.extend(quad(corners, uv, SHEET, WHITE));
    }
}

fn billboard(assets: &Assets, particle: Particle) -> Option<Billboard> {
    let sheet = |path: &str, [x0, y0, x1, y1]: [f32; 4]| {
        let Rect {
            x,
            y,
            width,
            height,
        } = assets.atlas.rect(path).unwrap_or_default();
        let [x, y, width, height] = [x, y, width, height].map(f32::from);
        (
            SHEET,
            [
                x + x0 * width,
                y + y0 * height,
                x + x1 * width,
                y + y1 * height,
            ],
        )
    };
    let block = |state: u16| {
        let particle = assets.models.block(state).particle;
        (particle != u16::MAX).then(|| i32::from(particle))
    };
    let (layer, uv) = match particle.sprite {
        Sprite::Tile(tile) => sheet("particle/particles", cell(tile, 16)),
        Sprite::Explosion(frame) => sheet("entity/explosion", cell(frame, 4)),
        Sprite::Footprint => sheet("particle/footprint", [0.0, 0.0, 1.0, 1.0]),
        Sprite::Flash => sheet("particle/particles", [0.25, 0.125, 0.5, 0.375]),
        Sprite::Block(state, corner) => (block(state)?, quarter(corner)),
        Sprite::Item(id, damage, corner) => {
            let layer = match assets.items.look(id, damage)? {
                ItemLook::Block(state) => block(state)?,
                ItemLook::Sprite(sprite, _) => assets.first_item_layer + i32::from(sprite),
            };
            (layer, quarter(corner))
        }
    };
    Some(Billboard {
        center: particle.position,
        half: particle.size,
        uv,
        layer,
        tint: u32::from_be_bytes(
            particle
                .color
                .to_array()
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8),
        ),
        flat: particle.flat.into(),
    })
}

fn cell(index: u8, columns: u8) -> [f32; 4] {
    let (column, row) = (index % columns, index / columns);
    let [x, y, size] = [column, row, columns].map(f32::from);
    [x / size, y / size, (x + 1.0) / size, (y + 1.0) / size]
}

fn quarter([x, y]: [f32; 2]) -> [f32; 4] {
    [x / 4.0, y / 4.0, (x + 1.0) / 4.0, (y + 1.0) / 4.0]
}
