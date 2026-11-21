use glam::{Mat4, Vec2, Vec3, vec2, vec3};
use replay_model::Item;
use replay_render::{Camera, Daylight, MeshVertex, Scene, WHITE, quad};
use replay_world::{CUBE_FACES, Face, Lighting};

use crate::assets::Assets;

const SPRITE: usize = 16;
const SPRITE_EDGE_SHADE: f32 = 0.8;

pub struct Draw<'a> {
    pub scene: &'a mut Scene,
    pub assets: &'a Assets,
    pub camera: &'a Camera,
    pub items: &'a [Item],
    pub skulls: &'a [i32],
    pub t: f32,
    pub lighting: &'a Lighting,
    pub daylight: Daylight,
    pub light: u32,
}

impl<'a> Draw<'a> {
    pub fn item(&self, index: Option<u32>) -> Option<(usize, &'a Item)> {
        let index = index? as usize;
        Some((index, self.items.get(index)?))
    }

    pub fn light_at(&mut self, position: Vec3) {
        let level = self.lighting.at(position.floor().as_ivec3().to_array());
        self.light = self.daylight.tint(level);
    }

    pub fn block(&mut self, state: u16, transform: Mat4) {
        let (quads, tint) = self.assets.models.item(state);
        let light = self.light;
        self.scene.solid.extend(quads.iter().flat_map(|face| {
            let shade = if face.shade { face.face.shade() } else { 1.0 };
            let color = if face.tinted { tint } else { WHITE };
            quad(
                face.corners
                    .map(|corner| transform.transform_point3(corner.into())),
                face.uv.map(Vec2::from),
                face.layer.into(),
                lit(shaded(color, shade), light),
            )
        }));
    }

    pub fn sprite(&mut self, sprite: u16, transform: Mat4) {
        let Some(&mask) = self.assets.masks.get(usize::from(sprite)) else {
            return;
        };
        let layer = self.assets.first_item_layer + i32::from(sprite);
        let at = |x: usize, y: usize, z: f32| {
            transform.transform_point3(vec3(x as f32, (SPRITE - y) as f32, z) / SPRITE as f32)
        };
        let opaque = |x: usize, y: usize| x < SPRITE && y < SPRITE && mask[y] >> x & 1 != 0;
        let (back, front) = (7.5, 8.5);
        let whole = [
            vec2(0.0, 0.0),
            vec2(1.0, 0.0),
            vec2(1.0, 1.0),
            vec2(0.0, 1.0),
        ];
        let solid = &mut self.scene.solid;
        for z in [front, back] {
            let corners = [
                at(0, 0, z),
                at(SPRITE, 0, z),
                at(SPRITE, SPRITE, z),
                at(0, SPRITE, z),
            ];
            solid.extend(quad(corners, whole, layer, self.light));
        }
        let edge = lit(shaded(WHITE, SPRITE_EDGE_SHADE), self.light);
        for (y, x) in (0..SPRITE).flat_map(|y| (0..SPRITE).map(move |x| (y, x))) {
            if !opaque(x, y) {
                continue;
            }
            let center = [vec2(x as f32 + 0.5, y as f32 + 0.5) / SPRITE as f32; 4];
            let sides = [
                (x == 0 || !opaque(x - 1, y), [(x, y), (x, y + 1)]),
                (!opaque(x + 1, y), [(x + 1, y), (x + 1, y + 1)]),
                (y == 0 || !opaque(x, y - 1), [(x, y), (x + 1, y)]),
                (!opaque(x, y + 1), [(x, y + 1), (x + 1, y + 1)]),
            ];
            for (open, [(x0, y0), (x1, y1)]) in sides {
                if open {
                    let corners = [
                        at(x0, y0, back),
                        at(x1, y1, back),
                        at(x1, y1, front),
                        at(x0, y0, front),
                    ];
                    solid.extend(quad(corners, center, layer, edge));
                }
            }
        }
    }

    pub fn block_box(&mut self, from: Vec3, to: Vec3, layer: i32, transform: Mat4) {
        self.scene
            .solid
            .extend(block_box(from, to, layer, self.light, transform));
    }
}

pub fn block_box(
    from: Vec3,
    to: Vec3,
    layer: i32,
    tint: u32,
    transform: Mat4,
) -> impl Iterator<Item = MeshVertex> {
    Face::ALL.into_iter().flat_map(move |face| {
        let pixels =
            CUBE_FACES[face as usize].map(|corner| from + (to - from) * Vec3::from(corner));
        let uv = pixels.map(|pixel| {
            let [x, y, z] = pixel.to_array();
            let (u, v) = match face {
                Face::Down => (x, 16.0 - z),
                Face::Up => (x, z),
                Face::North => (16.0 - x, 16.0 - y),
                Face::South => (x, 16.0 - y),
                Face::West => (z, 16.0 - y),
                Face::East => (16.0 - z, 16.0 - y),
            };
            vec2(u, v) / 16.0
        });
        let corners = pixels.map(|pixel| transform.transform_point3(pixel / 16.0));
        quad(corners, uv, layer, shaded(tint, face.shade()))
    })
}

pub fn lit(rgba: u32, light: u32) -> u32 {
    let [red, green, blue, alpha] = rgba.to_be_bytes();
    let [light_red, light_green, light_blue, _] = light.to_be_bytes();
    let scale = |channel: u8, by: u8| (u16::from(channel) * u16::from(by) / 255) as u8;
    u32::from_be_bytes([
        scale(red, light_red),
        scale(green, light_green),
        scale(blue, light_blue),
        alpha,
    ])
}

fn shaded(rgba: u32, shade: f32) -> u32 {
    let [red, green, blue, alpha] = rgba.to_be_bytes();
    let scale = |channel: u8| (f32::from(channel) * shade).round() as u8;
    u32::from_be_bytes([scale(red), scale(green), scale(blue), alpha])
}
