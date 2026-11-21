use std::f32::consts::PI;

use glam::{Vec3, vec3};
use replay_model::{FireworkExplosion, FireworkShape, Object};

use super::motes::{self, Mote, Spark};
use super::{MAX_TICKS, Source, TICK_MS, rgb};
use crate::random::Random;

const SHAPE_SPEED: f32 = 0.5;
const STAR: [[f32; 2]; 11] = [
    [0.0, 1.0],
    [0.3455, 0.309],
    [0.9511, 0.309],
    [0.3816, -0.12],
    [0.5878, -0.809],
    [0.0, -0.38],
    [-0.5878, -0.809],
    [-0.3816, -0.12],
    [-0.9511, 0.309],
    [-0.3455, 0.309],
    [0.0, 1.0],
];
const CREEPER: [[f32; 2]; 12] = [
    [0.0, 0.2],
    [0.2, 0.2],
    [0.2, 0.6],
    [0.6, 0.6],
    [0.6, 0.2],
    [0.2, 0.2],
    [0.2, 0.0],
    [0.4, 0.0],
    [0.4, -0.6],
    [0.2, -0.6],
    [0.2, -0.4],
    [0.0, -0.4],
];

pub fn position(object: &Object, t: f32) -> Vec3 {
    let (Some(&start), Some(&origin)) = (object.path.t.first(), object.path.pos.first()) else {
        return Vec3::ZERO;
    };
    let mut position = Vec3::from(origin);
    let mut velocity = object
        .velocity
        .iter()
        .map(|&(_, velocity)| Vec3::from(velocity))
        .find(|&velocity| velocity != Vec3::ZERO)
        .unwrap_or_default();
    let stop = object.burst.or(object.end).map_or(t, |at| t.min(at as f32));
    let ticks = ((stop - start as f32) / TICK_MS).max(0.0);
    let boost = |velocity: Vec3| velocity * vec3(1.15, 1.0, 1.15) + Vec3::Y * 0.04;
    for _ in 0..ticks as u32 {
        velocity = boost(velocity);
        position += velocity;
    }
    position + boost(velocity) * ticks.fract()
}

pub(super) fn source(object: &Object) -> Option<(u32, Source)> {
    let start = *object.path.t.first()?;
    let end = object.burst.or(object.end).unwrap_or(start);
    let flight = ((end.saturating_sub(start) as f32 / TICK_MS).round() as u32).min(MAX_TICKS);
    let trail = (0..=flight)
        .map(|tick| position(object, (start + tick * TICK_MS as u32) as f32))
        .collect();
    let explosions = match object.burst {
        Some(_) => object.explosions.clone(),
        None => Vec::new(),
    };
    Some((start, Source::Firework { trail, explosions }))
}

pub(super) fn emit(
    trail: &[Vec3],
    explosions: &[FireworkExplosion],
    tick: u32,
    random: &mut Random,
    out: &mut Vec<Mote>,
) {
    let Some(&end) = trail.last() else {
        return;
    };
    if tick > 0
        && let Some(&at) = trail.get(tick as usize)
    {
        let drift = vec3(random.gaussian() * 0.05, -0.05, random.gaussian() * 0.05);
        out.push(motes::spark(
            random,
            at - Vec3::Y * 0.3,
            drift,
            Spark::default(),
            None,
        ));
    }
    let since = tick.wrapping_sub(trail.len() as u32 - 1);
    if since.is_multiple_of(2)
        && let Some(explosion) = explosions.get(since as usize / 2)
    {
        burst(explosion, end, random, out);
        let color = explosion
            .colors
            .first()
            .map_or(Vec3::ONE, |&color| rgb(color));
        out.push(motes::flash(random, end, color));
    }
}

fn burst(explosion: &FireworkExplosion, at: Vec3, random: &mut Random, out: &mut Vec<Mote>) {
    let spark = Spark {
        fade: None,
        trail: explosion.trail,
        twinkle: explosion.flicker,
    };
    let mut make = |random: &mut Random, velocity: Vec3| {
        let color = pick(random, &explosion.colors).map_or(Vec3::ONE, rgb);
        let fade = pick(random, &explosion.fade).map(rgb);
        let spark = Spark { fade, ..spark };
        out.push(motes::spark(random, at, velocity, spark, Some(color)));
    };
    match explosion.shape {
        FireworkShape::SmallBall => ball(random, 0.25, 2, &mut make),
        FireworkShape::LargeBall => ball(random, 0.5, 4, &mut make),
        FireworkShape::Star => shaped(random, &STAR, 0.34, &mut make),
        FireworkShape::Creeper => shaped(random, &CREEPER, 0.034, &mut make),
        FireworkShape::Burst => {
            let (lean_x, lean_z) = (random.gaussian() * 0.05, random.gaussian() * 0.05);
            for _ in 0..70 {
                let velocity = vec3(
                    random.gaussian() * 0.15 + lean_x,
                    random.next() * 0.5,
                    random.gaussian() * 0.15 + lean_z,
                );
                make(random, velocity);
            }
        }
    }
}

fn ball(random: &mut Random, speed: f32, size: i32, make: &mut impl FnMut(&mut Random, Vec3)) {
    for i in -size..=size {
        for j in -size..=size {
            let edge = i.abs() == size || j.abs() == size;
            let mut k = -size;
            while k <= size {
                let jitter = vec3(
                    random.difference(),
                    random.difference(),
                    random.difference(),
                );
                let direction = vec3(j as f32, i as f32, k as f32) + jitter * 0.5;
                let length = direction.length() / speed + random.gaussian() * 0.05;
                make(random, direction / length);
                k += if edge { 1 } else { size * 2 };
            }
        }
    }
}

fn shaped(
    random: &mut Random,
    shape: &[[f32; 2]],
    spread: f32,
    make: &mut impl FnMut(&mut Random, Vec3),
) {
    let [x, y] = shape[0];
    make(random, vec3(x, y, 0.0) * SHAPE_SPEED);
    let turn = random.next() * PI;
    for copy in 0..3 {
        let angle = turn + copy as f32 * PI * spread;
        for pair in shape.windows(2) {
            let ([from_x, from_y], [to_x, to_y]) = (pair[0], pair[1]);
            for along in [0.25, 0.5, 0.75, 1.0] {
                let across = (from_x + (to_x - from_x) * along) * SHAPE_SPEED;
                let up = (from_y + (to_y - from_y) * along) * SHAPE_SPEED;
                let (sin, cos) = angle.sin_cos();
                for side in [-1.0, 1.0] {
                    make(random, vec3(across * cos * side, up, across * sin * side));
                }
            }
        }
    }
}

fn pick(random: &mut Random, list: &[u32]) -> Option<u32> {
    list.get(random.below(list.len())).copied()
}
