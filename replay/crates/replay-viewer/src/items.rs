use std::f32::consts::PI;

use glam::{Mat4, Vec3, vec3};
use replay_assets::{Hold, ItemLook};
use replay_model::Item;
use replay_render::Scene;

use crate::draw::Draw;
use crate::model::{ModelBox, Paint, cube};

const SKULL: i16 = 397;
const SKULL_SCALE: f32 = 1.1875;
const HEAD_BLOCK: f32 = 0.625;
const SKULL_HEAD: [ModelBox; 1] = [cube(0.0, 0.0, [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0])];
const HUMANOID_HEAD: [ModelBox; 2] = [
    SKULL_HEAD[0],
    cube(32.0, 0.0, [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]).inflated(0.25),
];

type Display = ([f32; 3], [f32; 3], f32);

const BLOCK: Display = ([0.0, 1.5, -2.75], [10.0, -45.0, 170.0], 0.375);
const GENERATED: Display = ([0.0, 1.0, -3.0], [-90.0, 0.0, 0.0], 0.55);
const HANDHELD: Display = ([0.0, 1.25, -3.5], [0.0, 90.0, -35.0], 0.85);
const BOW: Display = ([0.75, 0.0, 0.25], [5.0, 80.0, -45.0], 1.0);

pub fn held(draw: &mut Draw, item: &Item, hand: Mat4) {
    let Some(look) = draw.assets.items.look(item.id, item.damage) else {
        return;
    };
    let display = match look {
        ItemLook::Block(_) => display(BLOCK),
        ItemLook::Sprite(_, Hold::Generated) => display(GENERATED),
        ItemLook::Sprite(_, Hold::Handheld | Hold::Sword) => display(HANDHELD),
        ItemLook::Sprite(_, Hold::Bow) => display(BOW),
    };
    let transform = hand * Mat4::from_scale(Vec3::splat(2.0)) * display;
    render(draw, look, transform, item.enchanted);
}

pub fn dropped(draw: &mut Draw, item: &Item, position: Vec3, age: f32, phase: f32) {
    let Some(look) = draw.assets.items.look(item.id, item.damage) else {
        return;
    };
    let bob = (age / 10.0 + phase).sin() * 0.1 + 0.35;
    let size = if matches!(look, ItemLook::Block(_)) {
        0.5
    } else {
        1.0
    };
    let transform = Mat4::from_translation(position + Vec3::Y * bob)
        * Mat4::from_rotation_y(age / 20.0 + phase)
        * Mat4::from_scale(Vec3::splat(size));
    render(draw, look, transform, item.enchanted);
}

pub fn thrown(draw: &mut Draw, id: i16, damage: i16, position: Vec3) {
    if let Some(look) = draw.assets.items.look(id, damage) {
        let facing = Mat4::from_mat3(draw.camera.billboard());
        render(draw, look, Mat4::from_translation(position) * facing, false);
    }
}

pub fn framed(draw: &mut Draw, item: &Item, frame: Mat4) {
    let Some(look) = draw.assets.items.look(item.id, item.damage) else {
        return;
    };
    let (turn, size) = match look {
        ItemLook::Block(_) => (0.0, 0.5),
        ItemLook::Sprite(..) => (PI, 1.0),
    };
    let transform = frame * Mat4::from_scale(Vec3::splat(size)) * Mat4::from_rotation_y(turn);
    render(draw, look, transform, item.enchanted);
}

pub fn on_head(draw: &mut Draw, index: usize, item: &Item, head: Mat4) {
    if item.id == SKULL {
        let sheet = |path| Paint::sheet(draw.assets.atlas.rect(path));
        let (paint, boxes) = match item.damage {
            1 => (sheet("entity/skeleton/wither_skeleton"), &SKULL_HEAD[..]),
            2 => (sheet("entity/zombie/zombie"), &HUMANOID_HEAD[..]),
            3 => (
                Paint::skin(draw.skulls.get(index).copied().unwrap_or(0)),
                &HUMANOID_HEAD[..],
            ),
            4 => (sheet("entity/creeper/creeper"), &SKULL_HEAD[..]),
            _ => (sheet("entity/skeleton/skeleton"), &SKULL_HEAD[..]),
        };
        draw.cuboids(
            head * Mat4::from_scale(Vec3::splat(SKULL_SCALE)),
            paint,
            boxes,
        );
    } else if let Some(look @ ItemLook::Block(_)) = draw.assets.items.look(item.id, item.damage) {
        let transform = head
            * Mat4::from_scale(Vec3::splat(16.0))
            * Mat4::from_translation(Vec3::Y * -0.25)
            * Mat4::from_rotation_y(PI)
            * Mat4::from_scale(vec3(HEAD_BLOCK, -HEAD_BLOCK, -HEAD_BLOCK) * 2.0);
        render(draw, look, transform, item.enchanted);
    }
}

fn render(draw: &mut Draw, look: ItemLook, transform: Mat4, enchanted: bool) {
    let model =
        transform * Mat4::from_scale(Vec3::splat(0.5)) * Mat4::from_translation(Vec3::splat(-0.5));
    let start = draw.scene.solid.len();
    match look {
        ItemLook::Block(state) => draw.block(state, model),
        ItemLook::Sprite(sprite, _) => draw.sprite(sprite, model),
    }
    if enchanted {
        let Scene {
            solid,
            glint_meshes,
            ..
        } = &mut *draw.scene;
        glint_meshes.extend_from_slice(&solid[start..]);
    }
}

fn display((translation, [x, y, z], scale): Display) -> Mat4 {
    Mat4::from_translation(Vec3::from(translation) / 16.0)
        * Mat4::from_rotation_y(y.to_radians())
        * Mat4::from_rotation_x(x.to_radians())
        * Mat4::from_rotation_z(z.to_radians())
        * Mat4::from_scale(Vec3::splat(scale))
}
