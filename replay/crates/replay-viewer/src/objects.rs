use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use glam::{Mat4, Vec3, vec2, vec3};
use replay_assets::ItemLook;
use replay_model::{Equipment, Look, Object, ObjectKind, Path};
use replay_render::{PLAIN, SHEET, WHITE, quad};
use replay_sim::{Particle, Pose, TICK_MS, firework_position, latest, since, wrap};
use replay_world::{BlockModels, Flags, WorldState};

use crate::draw::{self, Draw};
use crate::mobs::{self, Mob};
use crate::{effects, items, players, text};

const MAX_SIMULATED_TICKS: u32 = 400;
const MAX_EASED_ERROR: f32 = 1.0;
const MAX_RIDING_DEPTH: u8 = 4;
const SEAT: f32 = 0.75;
const NAME_LIFT: f32 = 0.5;
const HOVER_PHASE: f32 = 2.399_963;
const TNT: u16 = 46 << 4;
const TNT_FUSE_TICKS: f32 = 80.0;
const BIRCH_PLANKS: u16 = 5 << 4 | 2;
const SNOWBALL: i16 = 332;
const EGG: i16 = 344;
const ENDER_PEARL: i16 = 368;
const POTION: i16 = 373;
const SPLASH: i16 = 0x4000;
const FIRE_CHARGE: i16 = 385;
const FIREWORK: i16 = 401;
const ARROW_PIXEL: f32 = 0.05625;
const STAND_ARM: Vec3 = vec3(-15.0, 0.0, 10.0);
const FRAME_BOXES: [([f32; 3], [f32; 3], bool); 5] = [
    ([3.0, 3.0, 15.5], [13.0, 13.0, 16.0], true),
    ([2.0, 2.0, 15.0], [14.0, 3.0, 16.0], false),
    ([2.0, 13.0, 15.0], [14.0, 14.0, 16.0], false),
    ([2.0, 3.0, 15.0], [3.0, 13.0, 16.0], false),
    ([13.0, 3.0, 15.0], [14.0, 13.0, 16.0], false),
];

#[derive(Clone, Copy)]
struct Physics {
    gravity: f32,
    drag: f32,
    gravity_first: bool,
    sticks: bool,
}

pub struct Objects {
    list: Vec<Object>,
    walked: Vec<Vec<f32>>,
}

impl Objects {
    pub fn new(list: Vec<Object>) -> Objects {
        let walked = list
            .iter()
            .map(|object| {
                let mut total = 0.0;
                let steps = object.path.pos.windows(2).map(|pair| {
                    total += vec2(pair[1][0] - pair[0][0], pair[1][2] - pair[0][2]).length();
                    total
                });
                std::iter::once(0.0).chain(steps).collect()
            })
            .collect();
        Objects { list, walked }
    }

    pub fn draw(&self, draw: &mut Draw, world: &WorldState) {
        let t = draw.t;
        let solid = solid(world, &draw.assets.models);
        for (index, object) in self.list.iter().enumerate() {
            if !self.visible(index, t) {
                continue;
            }
            let position = self.position(index, t, &solid, 0);
            draw.light_at(position);
            let age = (t - object.path.t[0] as f32) / TICK_MS;
            let look = look(&object.path, t);
            match object.kind {
                ObjectKind::Item => {
                    if let Some((_, item)) = draw.item(object.item) {
                        items::dropped(draw, item, position, age, index as f32 * HOVER_PHASE % TAU);
                    }
                }
                ObjectKind::Tnt => tnt(draw, position, age),
                ObjectKind::FallingBlock => {
                    if let Some(state) = object.block {
                        draw.block(
                            state,
                            Mat4::from_translation(position - vec3(0.5, 0.0, 0.5)),
                        );
                    }
                }
                ObjectKind::Arrow => arrow(draw, position, look),
                ObjectKind::Snowball => items::thrown(draw, SNOWBALL, 0, position),
                ObjectKind::Egg => items::thrown(draw, EGG, 0, position),
                ObjectKind::EnderPearl => items::thrown(draw, ENDER_PEARL, 0, position),
                ObjectKind::Potion => items::thrown(draw, POTION, SPLASH, position),
                ObjectKind::Firework => items::thrown(draw, FIREWORK, 0, position),
                ObjectKind::Fireball => fireball(draw, position, 2.0),
                ObjectKind::SmallFireball => fireball(draw, position, 0.5),
                ObjectKind::ItemFrame => item_frame(draw, object, position),
                ObjectKind::ArmorStand => armor_stand(draw, object, position, look),
                ObjectKind::EnderDragon => {}
                kind if !object.invisible => {
                    let mob = self.mob(index, position, look, age, t);
                    match kind {
                        ObjectKind::Villager => mobs::villager(draw, &mob, object.profession),
                        ObjectKind::IronGolem => mobs::iron_golem(
                            draw,
                            &mob,
                            since(&object.attacks, t).map(|ms| ms / TICK_MS),
                        ),
                        ObjectKind::Silverfish => mobs::silverfish(draw, &mob),
                        ObjectKind::Bat => mobs::bat(draw, &mob),
                        _ => {}
                    }
                }
                _ => {}
            }
            if latest(&object.burning, t) == Some(&true) {
                effects::fire(draw, position, object.kind.width(), object.kind.height());
            }
            if let Some(name) = latest(&object.names, t).and_then(Option::as_deref) {
                let height = match object.kind {
                    ObjectKind::ArmorStand if object.marker => 0.0,
                    ObjectKind::ArmorStand if object.small => object.kind.height() / 2.0,
                    kind => kind.height(),
                };
                let anchor = position + Vec3::Y * (height + NAME_LIFT);
                text::name_tag(draw, anchor, name, 0xFFFFFF, false);
            }
        }
    }

    pub fn trails<'a>(
        &'a self,
        t: f32,
        world: &'a WorldState,
        models: &'a BlockModels,
    ) -> impl Iterator<Item = Particle> + 'a {
        let solid = solid(world, models);
        self.list
            .iter()
            .enumerate()
            .filter_map(move |(index, object)| {
                let position = move |at: f32| {
                    self.visible(index, at)
                        .then(|| self.position(index, at, &solid, 0))
                };
                replay_sim::trail(object, index, t, position, world, models)
            })
            .flatten()
    }

    fn visible(&self, index: usize, t: f32) -> bool {
        let object = &self.list[index];
        object
            .path
            .t
            .first()
            .is_some_and(|&start| start as f32 <= t)
            && object.end.is_none_or(|end| t < end as f32)
    }

    pub fn seat(
        &self,
        vehicle: usize,
        t: f32,
        world: &WorldState,
        models: &BlockModels,
    ) -> Option<Vec3> {
        self.mount(vehicle, t, &solid(world, models), 0)
    }

    fn mount(
        &self,
        vehicle: usize,
        t: f32,
        solid: &impl Fn(Vec3) -> bool,
        depth: u8,
    ) -> Option<Vec3> {
        let seat = self.list.get(vehicle)?.kind.height() * SEAT;
        (depth < MAX_RIDING_DEPTH && self.visible(vehicle, t))
            .then(|| self.position(vehicle, t, solid, depth + 1) + Vec3::Y * seat)
    }

    fn position(&self, index: usize, t: f32, solid: &impl Fn(Vec3) -> bool, depth: u8) -> Vec3 {
        let object = &self.list[index];
        let vehicle = latest(&object.riding, t).copied().flatten();
        if let Some(seat) = vehicle.and_then(|vehicle| self.mount(vehicle, t, solid, depth)) {
            return seat;
        }
        if object.kind == ObjectKind::Firework {
            return firework_position(object, t);
        }
        match physics(object.kind) {
            Some(physics) if !object.velocity.is_empty() => simulated(object, t, physics, solid),
            _ => {
                let extrapolate = matches!(
                    object.kind,
                    ObjectKind::Fireball | ObjectKind::SmallFireball
                );
                interpolated(&object.path, t, extrapolate)
            }
        }
    }

    fn mob(&self, index: usize, position: Vec3, look: Look, age: f32, t: f32) -> Mob {
        let object = &self.list[index];
        let path = &object.path;
        let walked = &self.walked[index];
        let next = path.t.partition_point(|&at| at as f32 <= t);
        let current = next - 1;
        let (distance, speed) = match path.t.get(next) {
            Some(&until) => {
                let span = (until - path.t[current]).max(1) as f32;
                let step = walked[next] - walked[current];
                let progress = ((t - path.t[current] as f32) / span).min(1.0);
                let speed = if span > 1000.0 {
                    0.0
                } else {
                    step / (span / TICK_MS)
                };
                (walked[current] + step * progress, speed)
            }
            None => (walked[current], 0.0),
        };
        let (scale, lift) = match object.kind {
            ObjectKind::Villager => (0.9375, 0.0),
            ObjectKind::Bat => (0.35, (age * 0.3).cos() * 0.1),
            _ => (1.0, 0.0),
        };
        Mob {
            root: replay_sim::root(position + Vec3::Y * lift, look.yaw, 0.0, scale),
            yaw: wrap(look.head_yaw - look.yaw),
            pitch: look.pitch,
            limb_swing: distance * 4.0,
            limb_amount: (speed * 4.0).min(1.0),
            age,
        }
    }
}

fn solid<'a>(world: &'a WorldState, models: &'a BlockModels) -> impl Fn(Vec3) -> bool + Copy + 'a {
    move |at: Vec3| {
        let state = world.block(at.floor().as_ivec3().into());
        models.block(state).flags.has(Flags::SOLID)
    }
}

fn tnt(draw: &mut Draw, position: Vec3, age: f32) {
    let fuse = TNT_FUSE_TICKS - age;
    let swell = (1.0 - (fuse + 1.0) / 10.0).clamp(0.0, 1.0).powi(4);
    let transform = Mat4::from_translation(position + Vec3::Y * 0.5)
        * Mat4::from_scale(Vec3::splat(1.0 + swell * 0.3))
        * Mat4::from_translation(Vec3::splat(-0.5));
    draw.block(TNT, transform);
    if (fuse.floor() as i32 / 5) % 2 == 0 {
        let alpha = ((1.0 - (fuse + 1.0) / 100.0) * 0.8).clamp(0.0, 1.0);
        let white = 0xFFFFFF00 | (alpha * 255.0) as u32;
        let flash = draw::block_box(
            Vec3::splat(-0.02),
            Vec3::splat(16.02),
            PLAIN,
            white,
            transform,
        );
        draw.scene.blended.extend(flash);
    }
}

fn arrow(draw: &mut Draw, position: Vec3, look: Look) {
    let rect = draw.assets.atlas.rect("entity/arrow").unwrap_or_default();
    let origin = vec2(rect.x.into(), rect.y.into());
    let size = vec2(rect.width.into(), rect.height.into());
    let transform = Mat4::from_translation(position)
        * Mat4::from_rotation_y(look.yaw - FRAC_PI_2)
        * Mat4::from_rotation_z(look.pitch)
        * Mat4::from_rotation_x(FRAC_PI_4)
        * Mat4::from_scale(Vec3::splat(ARROW_PIXEL))
        * Mat4::from_translation(Vec3::X * -4.0);
    let uv = |[u0, v0, u1, v1]: [f32; 4]| {
        [vec2(u0, v0), vec2(u1, v0), vec2(u1, v1), vec2(u0, v1)]
            .map(|corner| origin + corner * size)
    };
    let shaft = uv([0.0, 0.0, 0.5, 5.0 / 32.0]);
    let fletching = [
        vec3(-7.0, -2.0, -2.0),
        vec3(-7.0, -2.0, 2.0),
        vec3(-7.0, 2.0, 2.0),
        vec3(-7.0, 2.0, -2.0),
    ];
    let planes = [FRAC_PI_2, PI].map(|angle| {
        let turn = Mat4::from_rotation_x(angle);
        [
            vec3(-8.0, -2.0, 0.0),
            vec3(8.0, -2.0, 0.0),
            vec3(8.0, 2.0, 0.0),
            vec3(-8.0, 2.0, 0.0),
        ]
        .map(|corner| turn.transform_point3(corner))
    });
    let faces = [(fletching, uv([0.0, 5.0 / 32.0, 0.15625, 10.0 / 32.0]))]
        .into_iter()
        .chain(planes.map(|plane| (plane, shaft)));
    for (corners, uv) in faces {
        let corners = corners.map(|corner| transform.transform_point3(corner));
        draw.scene
            .solid
            .extend(quad(corners, uv, SHEET, draw.light));
    }
}

fn fireball(draw: &mut Draw, position: Vec3, size: f32) {
    let Some(ItemLook::Sprite(sprite, _)) = draw.assets.items.look(FIRE_CHARGE, 0) else {
        return;
    };
    let billboard = draw.camera.billboard();
    let at = |x: f32, y: f32| position + (billboard.x_axis * x + billboard.y_axis * y) * size;
    let corners = [
        at(-0.5, -0.25),
        at(0.5, -0.25),
        at(0.5, 0.75),
        at(-0.5, 0.75),
    ];
    let uv = [
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
        vec2(1.0, 0.0),
        vec2(0.0, 0.0),
    ];
    let layer = draw.assets.first_item_layer + i32::from(sprite);
    draw.scene.solid.extend(quad(corners, uv, layer, WHITE));
}

fn item_frame(draw: &mut Draw, object: &Object, position: Vec3) {
    let yaw = f32::from(object.facing) * FRAC_PI_2;
    let center = Mat4::from_translation(position + 0.5) * Mat4::from_rotation_y(PI - yaw);
    let model = center * Mat4::from_translation(Vec3::splat(-0.5));
    let back = i32::from(draw.assets.models.item_frame);
    let wood = draw
        .assets
        .models
        .item(BIRCH_PLANKS)
        .0
        .first()
        .map_or(0, |quad| quad.layer.into());
    for (from, to, backing) in FRAME_BOXES {
        draw.block_box(
            from.into(),
            to.into(),
            if backing { back } else { wood },
            model,
        );
    }
    if let Some((_, item)) = draw.item(object.item) {
        let frame = center
            * Mat4::from_translation(Vec3::Z * 0.4375)
            * Mat4::from_rotation_z(f32::from(object.rotation) * FRAC_PI_4);
        items::framed(draw, item, frame);
    }
}

fn armor_stand(draw: &mut Draw, object: &Object, position: Vec3, look: Look) {
    let scale = if object.small { 0.5 } else { 1.0 };
    let right_arm = replay_sim::part(vec3(-5.0, 2.0, 0.0), STAND_ARM * (PI / 180.0));
    let pose = Pose {
        root: replay_sim::root(position, look.yaw, 0.0, scale),
        right_arm,
        held_item: replay_sim::held_item(right_arm),
        ..Pose::default()
    };
    let equipment = Equipment {
        held: object.hand,
        helmet: object.helmet,
        ..Equipment::default()
    };
    players::wear(draw, &pose, &equipment, false);
}

fn physics(kind: ObjectKind) -> Option<Physics> {
    let (gravity, drag, gravity_first, sticks) = match kind {
        ObjectKind::Item | ObjectKind::Tnt | ObjectKind::FallingBlock => (0.04, 0.98, true, false),
        ObjectKind::Arrow | ObjectKind::Potion => (0.05, 0.99, false, true),
        ObjectKind::Snowball | ObjectKind::Egg | ObjectKind::EnderPearl => {
            (0.03, 0.99, false, true)
        }
        _ => return None,
    };
    Some(Physics {
        gravity,
        drag,
        gravity_first,
        sticks,
    })
}

fn simulated(object: &Object, t: f32, physics: Physics, solid: &impl Fn(Vec3) -> bool) -> Vec3 {
    let path = &object.path;
    let next = path.t.partition_point(|&at| at as f32 <= t);
    let current = next - 1;
    let start_t = path.t[current];
    let start = Vec3::from(path.pos[current]);
    let target = path
        .t
        .get(next)
        .map(|&until| (until, Vec3::from(path.pos[next])));
    let span = target.map_or(MAX_SIMULATED_TICKS, |(until, _)| {
        (until - start_t)
            .div_ceil(TICK_MS as u32)
            .clamp(1, MAX_SIMULATED_TICKS)
    });
    let elapsed = ((t - start_t as f32) / TICK_MS).clamp(0.0, span as f32);
    let whole = elapsed as u32;
    let samples = [whole, (whole + 1).min(span), span];

    let mut velocity = velocity_at(object, start_t, physics);
    let mut update = object.velocity.partition_point(|&(at, _)| at <= start_t);
    let mut position = start;
    let mut found = samples.map(|tick| (tick == 0).then_some(start));
    for tick in 1..=span {
        let at = start_t + tick * TICK_MS as u32;
        while let Some(&(_, change)) = object
            .velocity
            .get(update)
            .filter(|&&(changed, _)| changed <= at)
        {
            velocity = change.into();
            update += 1;
        }
        let rests = step(&mut position, &mut velocity, physics, solid);
        for (sample, &wanted) in found.iter_mut().zip(&samples) {
            if wanted == tick {
                *sample = Some(position);
            }
        }
        let quiet = object
            .velocity
            .get(update)
            .is_none_or(|&(changed, _)| changed > start_t + span * TICK_MS as u32);
        if rests && quiet {
            break;
        }
    }
    let [from, to, end] = found.map(|sample| sample.unwrap_or(position));
    let eased = |sample: Vec3, tick: u32| match target {
        Some((_, goal)) => {
            let share = tick as f32 / span as f32;
            let error = goal - end;
            if error.length() > MAX_EASED_ERROR {
                start.lerp(goal, share)
            } else {
                sample + error * share
            }
        }
        None => sample,
    };
    eased(from, samples[0]).lerp(eased(to, samples[1]), elapsed - whole as f32)
}

fn step(
    position: &mut Vec3,
    velocity: &mut Vec3,
    physics: Physics,
    solid: &impl Fn(Vec3) -> bool,
) -> bool {
    let before = *position;
    if physics.gravity_first {
        velocity.y -= physics.gravity;
    }
    let next = *position + *velocity;
    if solid(next) {
        if physics.sticks {
            *velocity = Vec3::ZERO;
            return true;
        }
        if velocity.y < 0.0 && !solid(vec3(next.x, position.y, next.z)) {
            *position = vec3(next.x, next.y.floor() + 1.0, next.z);
            *velocity *= vec3(0.588, 0.0, 0.588);
        } else {
            *velocity = vec3(0.0, velocity.y.min(0.0), 0.0);
        }
    } else {
        *position = next;
    }
    *velocity *= physics.drag;
    if !physics.gravity_first {
        velocity.y -= physics.gravity;
    }
    *position == before && velocity.length_squared() < 1e-8
}

fn velocity_at(object: &Object, t: u32, physics: Physics) -> Vec3 {
    let Some(&(changed, velocity)) =
        object.velocity[..object.velocity.partition_point(|&(at, _)| at <= t)].last()
    else {
        return Vec3::ZERO;
    };
    let ticks = ((t - changed) as f32 / TICK_MS)
        .round()
        .min(MAX_SIMULATED_TICKS as f32) as u32;
    (0..ticks).fold(Vec3::from(velocity), |mut velocity, _| {
        if physics.gravity_first {
            velocity.y -= physics.gravity;
        }
        velocity *= physics.drag;
        if !physics.gravity_first {
            velocity.y -= physics.gravity;
        }
        velocity
    })
}

fn interpolated(path: &Path, t: f32, extrapolate: bool) -> Vec3 {
    let next = path.t.partition_point(|&at| at as f32 <= t);
    let current = next - 1;
    let from = Vec3::from(path.pos[current]);
    let since = t - path.t[current] as f32;
    match path.t.get(next) {
        Some(&until) => {
            let span = (until - path.t[current]).max(1) as f32;
            from.lerp(path.pos[next].into(), (since / span).clamp(0.0, 1.0))
        }
        None if extrapolate && current > 0 => {
            let previous = current - 1;
            let span = (path.t[current] - path.t[previous]).max(1) as f32;
            from + (from - Vec3::from(path.pos[previous])) * since / span
        }
        None => from,
    }
}

fn look(path: &Path, t: f32) -> Look {
    let next = path.t.partition_point(|&at| at as f32 <= t);
    let current = next.saturating_sub(1);
    let Some(&from) = path.look.get(current) else {
        return Look {
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
        };
    };
    let Some((&to, &until)) = path.look.get(next).zip(path.t.get(next)) else {
        return from;
    };
    let span = (until - path.t[current]).max(1) as f32;
    let alpha = ((t - path.t[current] as f32) / span).clamp(0.0, 1.0);
    let turn = |from: f32, to: f32| from + wrap(to - from) * alpha;
    Look {
        yaw: turn(from.yaw, to.yaw),
        pitch: turn(from.pitch, to.pitch),
        head_yaw: turn(from.head_yaw, to.head_yaw),
    }
}
