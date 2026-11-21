use std::f32::consts::{FRAC_PI_4, PI};

use glam::{Mat4, Vec3, vec3};

use crate::draw::Draw;
use crate::model::{ModelBox, Paint, cube};

pub struct Mob {
    pub root: Mat4,
    pub yaw: f32,
    pub pitch: f32,
    pub limb_swing: f32,
    pub limb_amount: f32,
    pub age: f32,
}

const VILLAGER_HEAD: [ModelBox; 2] = [
    cube(0.0, 0.0, [-4.0, -10.0, -4.0], [8.0, 10.0, 8.0]),
    cube(24.0, 0.0, [-1.0, -3.0, -6.0], [2.0, 4.0, 2.0]),
];
const VILLAGER_BODY: [ModelBox; 2] = [
    cube(16.0, 20.0, [-4.0, 0.0, -3.0], [8.0, 12.0, 6.0]),
    cube(0.0, 38.0, [-4.0, 0.0, -3.0], [8.0, 18.0, 6.0]).inflated(0.5),
];
const VILLAGER_ARMS: [ModelBox; 3] = [
    cube(44.0, 22.0, [-8.0, -2.0, -2.0], [4.0, 8.0, 4.0]),
    cube(44.0, 22.0, [4.0, -2.0, -2.0], [4.0, 8.0, 4.0]),
    cube(40.0, 38.0, [-4.0, 2.0, -2.0], [8.0, 4.0, 4.0]),
];
const VILLAGER_LEG: ModelBox = cube(0.0, 22.0, [-2.0, 0.0, -2.0], [4.0, 12.0, 4.0]);
const VILLAGER_TEXTURES: [&str; 5] = [
    "entity/villager/farmer",
    "entity/villager/librarian",
    "entity/villager/priest",
    "entity/villager/smith",
    "entity/villager/butcher",
];

const GOLEM_HEAD: [ModelBox; 2] = [
    cube(0.0, 0.0, [-4.0, -12.0, -5.5], [8.0, 10.0, 8.0]),
    cube(24.0, 0.0, [-1.0, -5.0, -7.5], [2.0, 4.0, 2.0]),
];
const GOLEM_BODY: [ModelBox; 2] = [
    cube(0.0, 40.0, [-9.0, -2.0, -6.0], [18.0, 12.0, 11.0]),
    cube(0.0, 70.0, [-4.5, 10.0, -3.0], [9.0, 5.0, 6.0]).inflated(0.5),
];
const GOLEM_RIGHT_ARM: ModelBox = cube(60.0, 21.0, [-13.0, -2.5, -3.0], [4.0, 30.0, 6.0]);
const GOLEM_LEFT_ARM: ModelBox = cube(60.0, 58.0, [9.0, -2.5, -3.0], [4.0, 30.0, 6.0]);
const GOLEM_LEFT_LEG: ModelBox = cube(37.0, 0.0, [-3.5, -3.0, -3.0], [6.0, 16.0, 5.0]);
const GOLEM_RIGHT_LEG: ModelBox = cube(60.0, 0.0, [-3.5, -3.0, -3.0], [6.0, 16.0, 5.0]).mirrored();
const GOLEM_ATTACK_TICKS: f32 = 10.0;

const SILVERFISH_SEGMENTS: [([f32; 3], [f32; 2], f32); 7] = [
    ([3.0, 2.0, 2.0], [0.0, 0.0], -3.5),
    ([4.0, 3.0, 2.0], [0.0, 4.0], -1.5),
    ([6.0, 4.0, 3.0], [0.0, 9.0], 1.0),
    ([3.0, 3.0, 3.0], [0.0, 16.0], 4.0),
    ([2.0, 2.0, 3.0], [0.0, 22.0], 7.0),
    ([2.0, 1.0, 2.0], [11.0, 0.0], 9.5),
    ([1.0, 1.0, 2.0], [13.0, 4.0], 11.5),
];
const SILVERFISH_WINGS: [(usize, f32, ModelBox); 3] = [
    (
        2,
        16.0,
        cube(20.0, 0.0, [-5.0, 0.0, -1.5], [10.0, 8.0, 3.0]),
    ),
    (
        4,
        20.0,
        cube(20.0, 11.0, [-3.0, 0.0, -1.5], [6.0, 4.0, 3.0]),
    ),
    (
        1,
        19.0,
        cube(20.0, 18.0, [-3.0, 0.0, -1.5], [6.0, 5.0, 2.0]),
    ),
];

const BAT_HEAD: [ModelBox; 3] = [
    cube(0.0, 0.0, [-3.0, -3.0, -3.0], [6.0, 6.0, 6.0]),
    cube(24.0, 0.0, [-4.0, -6.0, -2.0], [3.0, 4.0, 1.0]),
    cube(24.0, 0.0, [1.0, -6.0, -2.0], [3.0, 4.0, 1.0]).mirrored(),
];
const BAT_BODY: [ModelBox; 2] = [
    cube(0.0, 16.0, [-3.0, 4.0, -3.0], [6.0, 12.0, 6.0]),
    cube(0.0, 34.0, [-5.0, 16.0, 0.0], [10.0, 6.0, 1.0]),
];
const BAT_WINGS: [(f32, ModelBox, ModelBox); 2] = [
    (
        1.0,
        cube(42.0, 0.0, [-12.0, 1.0, 1.5], [10.0, 16.0, 1.0]),
        cube(24.0, 16.0, [-8.0, 1.0, 0.0], [8.0, 12.0, 1.0]),
    ),
    (
        -1.0,
        cube(42.0, 0.0, [2.0, 1.0, 1.5], [10.0, 16.0, 1.0]).mirrored(),
        cube(24.0, 16.0, [0.0, 1.0, 0.0], [8.0, 12.0, 1.0]).mirrored(),
    ),
];

pub fn villager(draw: &mut Draw, mob: &Mob, profession: i32) {
    let texture = usize::try_from(profession)
        .ok()
        .and_then(|index| VILLAGER_TEXTURES.get(index))
        .unwrap_or(&"entity/villager/villager");
    let paint = Paint::sheet(draw.assets.atlas.rect(texture));
    let swing = (mob.limb_swing * 0.6662).cos() * 1.4 * mob.limb_amount * 0.5;
    let parts: [(Mat4, &[ModelBox]); 5] = [
        (
            replay_sim::part(Vec3::ZERO, vec3(mob.pitch, mob.yaw, 0.0)),
            &VILLAGER_HEAD,
        ),
        (Mat4::IDENTITY, &VILLAGER_BODY),
        (
            replay_sim::part(vec3(0.0, 3.0, -1.0), Vec3::X * -0.75),
            &VILLAGER_ARMS,
        ),
        (
            replay_sim::part(vec3(-2.0, 12.0, 0.0), Vec3::X * swing),
            &[VILLAGER_LEG],
        ),
        (
            replay_sim::part(vec3(2.0, 12.0, 0.0), Vec3::X * -swing),
            &[VILLAGER_LEG.mirrored()],
        ),
    ];
    for (part, boxes) in parts {
        draw.cuboids(mob.root * part, paint, boxes);
    }
}

pub fn iron_golem(draw: &mut Draw, mob: &Mob, attack: Option<f32>) {
    let paint = Paint::sheet(draw.assets.atlas.rect("entity/iron_golem"));
    let walk = triangle(mob.limb_swing, 13.0);
    let [right_arm, left_arm] = match attack.filter(|&ticks| ticks < GOLEM_ATTACK_TICKS) {
        Some(ticks) => [-2.0 + 1.5 * triangle(GOLEM_ATTACK_TICKS - ticks, 10.0); 2],
        None => [-0.2 + 1.5 * walk, -0.2 - 1.5 * walk].map(|angle| angle * mob.limb_amount),
    };
    let leg = 1.5 * walk * mob.limb_amount;
    let shoulders = vec3(0.0, -7.0, 0.0);
    let parts: [(Mat4, &[ModelBox]); 6] = [
        (
            replay_sim::part(vec3(0.0, -7.0, -2.0), vec3(mob.pitch, mob.yaw, 0.0)),
            &GOLEM_HEAD,
        ),
        (Mat4::from_translation(shoulders), &GOLEM_BODY),
        (
            replay_sim::part(shoulders, Vec3::X * right_arm),
            &[GOLEM_RIGHT_ARM],
        ),
        (
            replay_sim::part(shoulders, Vec3::X * left_arm),
            &[GOLEM_LEFT_ARM],
        ),
        (
            replay_sim::part(vec3(-4.0, 11.0, 0.0), Vec3::X * -leg),
            &[GOLEM_LEFT_LEG],
        ),
        (
            replay_sim::part(vec3(5.0, 11.0, 0.0), Vec3::X * leg),
            &[GOLEM_RIGHT_LEG],
        ),
    ];
    for (part, boxes) in parts {
        draw.cuboids(mob.root * part, paint, boxes);
    }
}

pub fn silverfish(draw: &mut Draw, mob: &Mob) {
    let paint = Paint::sheet(draw.assets.atlas.rect("entity/silverfish"));
    let wiggle = |index: usize| {
        let phase = mob.age * 0.9 + index as f32 * 0.15 * PI;
        let offset = (index as f32 - 2.0).abs();
        (
            phase.sin() * PI * 0.2 * offset,
            phase.cos() * PI * 0.05 * (1.0 + offset),
        )
    };
    for (index, (size, [u, v], z)) in SILVERFISH_SEGMENTS.into_iter().enumerate() {
        let (x, yaw) = wiggle(index);
        let part = replay_sim::part(vec3(x, 24.0 - size[1], z), Vec3::Y * yaw);
        let segment = cube(u, v, [-size[0] / 2.0, 0.0, -size[2] / 2.0], size);
        draw.cuboids(mob.root * part, paint, &[segment]);
    }
    for (index, (follows, y, wing)) in SILVERFISH_WINGS.into_iter().enumerate() {
        let (x, yaw) = wiggle(follows);
        let x = if index == 0 { 0.0 } else { x };
        let part = replay_sim::part(vec3(x, y, SILVERFISH_SEGMENTS[follows].2), Vec3::Y * yaw);
        draw.cuboids(mob.root * part, paint, &[wing]);
    }
}

pub fn bat(draw: &mut Draw, mob: &Mob) {
    let paint = Paint::sheet(draw.assets.atlas.rect("entity/bat"));
    let flap = (mob.age * 1.3).cos() * PI * 0.25;
    let head = replay_sim::part(Vec3::ZERO, vec3(mob.pitch, mob.yaw, 0.0));
    draw.cuboids(mob.root * head, paint, &BAT_HEAD);
    let body = mob.root
        * replay_sim::part(
            Vec3::ZERO,
            Vec3::X * (FRAC_PI_4 + (mob.age * 0.1).cos() * 0.15),
        );
    draw.cuboids(body, paint, &BAT_BODY);
    for (side, wing, tip) in BAT_WINGS {
        let wing_part = body * Mat4::from_rotation_y(flap * side);
        draw.cuboids(wing_part, paint, &[wing]);
        let tip_part = replay_sim::part(vec3(-12.0 * side, 1.0, 1.5), Vec3::Y * flap * side * 0.5);
        draw.cuboids(wing_part * tip_part, paint, &[tip]);
    }
}

fn triangle(value: f32, period: f32) -> f32 {
    ((value % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}
