mod fireworks;
mod motes;
mod swirls;
mod trails;

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use glam::{IVec3, Vec3, Vec4, vec3};
use replay_model::{EffectKind, FireworkExplosion, ObjectKind, Replay};
use replay_world::{BlockModels, Flags, WorldState};

use crate::random::Random;
use crate::{BLOCK_BREAK, TICK_MS};
use motes::{
    CRIT, EXPLODE, FLAME, HAPPY_VILLAGER, HUGE_EXPLOSION, INSTANT_SPELL, ITEM_CRACK, LARGE_EXPLODE,
    MAGIC_CRIT, Mote, SMOKE, SPELL,
};

pub use fireworks::position as firework_position;
pub use trails::trail;

pub const MAX_PARTICLES: usize = 6000;
const LONGEST_TICKS: u32 = 40;
const WINDOW_MS: f32 = 12_000.0;
const MAX_TICKS: u32 = (WINDOW_MS / TICK_MS) as u32;
const MAX_COUNT: i32 = 500;
const HUGE_TICKS: u32 = 8;
const CRIT_TICKS: u32 = 3;
const DISPENSE: i32 = 2000;
const SPLASH_POTION: i32 = 2002;
const SPAWNER: i32 = 2004;
const BONEMEAL: i32 = 2005;
const POTION: i32 = 373;
const POTION_COLORS: [u32; 16] = [
    0x385DC6, 0xCD5CAB, 0x7CAFC6, 0xE49A3A, 0x4E9331, 0xF82423, 0x1F1FA1, 0x385DC6, 0x484D48,
    0x932423, 0x5A6C81, 0x22FF4C, 0x430A09, 0x2E5299, 0x7F8392, 0x385DC6,
];
const INSTANT_POTIONS: [i32; 2] = [5, 12];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub position: Vec3,
    pub size: f32,
    pub color: Vec4,
    pub sprite: Sprite,
    pub flat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sprite {
    Tile(u8),
    Explosion(u8),
    Footprint,
    Flash,
    Block(u16, [f32; 2]),
    Item(i16, i16, [f32; 2]),
}

pub struct Particles {
    entries: Vec<Entry>,
    runs: BTreeMap<usize, Run>,
    shown: Vec<Particle>,
}

struct Entry {
    t: u32,
    seed: u32,
    source: Source,
}

enum Source {
    Effect(EffectKind),
    Firework {
        trail: Vec<Vec3>,
        explosions: Vec<FireworkExplosion>,
    },
}

struct Run {
    tick: u32,
    random: Random,
    motes: Vec<Mote>,
}

struct Terrain<'a> {
    world: &'a WorldState,
    models: &'a BlockModels,
}

impl Particles {
    pub fn new(replay: &Replay) -> Particles {
        let effects = replay
            .effects
            .iter()
            .enumerate()
            .filter(|(_, effect)| makes_particles(&effect.kind))
            .flat_map(|(index, effect)| {
                let seed = seed(index, 2_654_435_761, 7);
                let kinds = match effect.kind {
                    EffectKind::Particle {
                        id: HUGE_EXPLOSION,
                        pos,
                        offset,
                        count,
                        ..
                    } if count > 0 => {
                        let mut random = Random::new(seed);
                        (0..count.min(MAX_COUNT))
                            .map(|_| EffectKind::Particle {
                                id: HUGE_EXPLOSION,
                                pos: (Vec3::from(pos) + random.gaussians() * Vec3::from(offset))
                                    .into(),
                                offset: [0.0; 3],
                                speed: 0.0,
                                count: 0,
                                item: None,
                            })
                            .collect()
                    }
                    _ => vec![effect.kind.clone()],
                };
                kinds.into_iter().enumerate().map(move |(n, kind)| Entry {
                    t: effect.t,
                    seed: seed.wrapping_add(n as u32),
                    source: Source::Effect(kind),
                })
            });
        let fireworks = replay
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| object.kind == ObjectKind::Firework)
            .filter_map(|(index, object)| {
                let (t, source) = fireworks::source(object)?;
                Some(Entry {
                    t,
                    seed: seed(index, 40_503, 11),
                    source,
                })
            });
        let mut entries: Vec<Entry> = effects.chain(fireworks).collect();
        entries.sort_by_key(|entry| entry.t);
        Particles {
            entries,
            runs: BTreeMap::new(),
            shown: Vec::new(),
        }
    }

    pub fn update(&mut self, t: f32, world: &WorldState, models: &BlockModels) -> &[Particle] {
        let terrain = Terrain { world, models };
        let Particles {
            entries,
            runs,
            shown,
        } = self;
        let first = entries.partition_point(|entry| (entry.t as f32) < t - WINDOW_MS);
        let last = entries.partition_point(|entry| entry.t as f32 <= t);
        let age = |entry: &Entry| (t - entry.t as f32) / TICK_MS;
        runs.retain(|&index, run| {
            (first..last).contains(&index) && run.tick as f32 <= age(&entries[index])
        });
        shown.clear();
        for (index, entry) in entries.iter().enumerate().take(last).skip(first) {
            let age = age(entry);
            let run = runs.entry(index).or_insert_with(|| Run::start(entry));
            run.advance(&entry.source, age as u32, &terrain);
            let room = MAX_PARTICLES - shown.len();
            shown.extend(
                run.motes
                    .iter()
                    .filter_map(|mote| mote.particle(age.fract()))
                    .take(room),
            );
        }
        shown
    }
}

impl Run {
    fn start(entry: &Entry) -> Run {
        let mut run = Run {
            tick: 0,
            random: Random::new(entry.seed),
            motes: Vec::new(),
        };
        entry.source.emit(0, &mut run.random, &mut run.motes);
        run
    }

    fn advance(&mut self, source: &Source, target: u32, terrain: &Terrain) {
        let last = source.last_tick();
        let mut children = Vec::new();
        while self.tick < target && (self.tick < last || !self.motes.is_empty()) {
            self.tick += 1;
            self.motes
                .retain_mut(|mote| mote.step(terrain, &mut self.random, &mut children));
            self.motes.append(&mut children);
            source.emit(self.tick, &mut self.random, &mut self.motes);
            self.motes.truncate(MAX_PARTICLES);
        }
        self.tick = self.tick.max(target);
    }
}

impl Source {
    fn last_tick(&self) -> u32 {
        match self {
            Source::Effect(EffectKind::Particle {
                id: HUGE_EXPLOSION, ..
            }) => HUGE_TICKS,
            Source::Effect(EffectKind::Explosion { radius, .. }) if *radius >= 2.0 => HUGE_TICKS,
            Source::Effect(EffectKind::Crit { .. }) => CRIT_TICKS - 1,
            Source::Effect(_) => 0,
            Source::Firework { trail, explosions } => {
                let flight = trail.len().saturating_sub(1);
                (flight + explosions.len().saturating_sub(1) * 2) as u32
            }
        }
    }

    fn emit(&self, tick: u32, random: &mut Random, out: &mut Vec<Mote>) {
        match self {
            Source::Effect(kind) => effect(kind, tick, random, out),
            Source::Firework { trail, explosions } => {
                fireworks::emit(trail, explosions, tick, random, out)
            }
        }
    }
}

fn effect(kind: &EffectKind, tick: u32, random: &mut Random, out: &mut Vec<Mote>) {
    match *kind {
        EffectKind::Particle {
            id: HUGE_EXPLOSION,
            pos,
            ..
        } if (1..=HUGE_TICKS).contains(&tick) => huge(pos.into(), tick, random, out),
        EffectKind::Explosion { pos, radius, .. }
            if radius >= 2.0 && (1..=HUGE_TICKS).contains(&tick) =>
        {
            huge(pos.into(), tick, random, out)
        }
        EffectKind::Explosion {
            pos,
            radius,
            ref blocks,
        } if tick == 0 => explosion(pos.into(), radius, blocks, random, out),
        EffectKind::Particle {
            id,
            pos,
            offset,
            speed,
            count,
            item,
        } if tick == 0 => {
            let (pos, offset) = (Vec3::from(pos), Vec3::from(offset));
            if count == 0 {
                out.extend(motes::create(id, random, pos, offset * speed, item));
            }
            for _ in 0..count.min(MAX_COUNT) {
                let at = pos + random.gaussians() * offset;
                let velocity = random.gaussians() * speed;
                out.extend(motes::create(id, random, at, velocity, item));
            }
        }
        EffectKind::Crit {
            magic, pos, height, ..
        } if tick < CRIT_TICKS => {
            let kind = if magic { MAGIC_CRIT } else { CRIT };
            let center = Vec3::from(pos) + Vec3::Y * height / 2.0;
            for _ in 0..16 {
                let direction = loop {
                    let direction = random.cube();
                    if direction.length_squared() <= 1.0 {
                        break direction;
                    }
                };
                let at = center + direction * vec3(0.15, height / 4.0, 0.15);
                out.extend(motes::create(
                    kind,
                    random,
                    at,
                    direction + Vec3::Y * 0.2,
                    None,
                ));
            }
        }
        EffectKind::World { id, pos, data } if tick == 0 => {
            world(id, IVec3::from(pos).as_vec3(), data, random, out)
        }
        _ => {}
    }
}

fn huge(pos: Vec3, tick: u32, random: &mut Random, out: &mut Vec<Mote>) {
    let push = Vec3::X * (tick - 1) as f32 / HUGE_TICKS as f32;
    for _ in 0..6 {
        let scatter = vec3(
            random.difference(),
            random.difference(),
            random.difference(),
        );
        let at = pos + scatter * 4.0;
        out.extend(motes::create(LARGE_EXPLODE, random, at, push, None));
    }
}

fn explosion(
    pos: Vec3,
    radius: f32,
    blocks: &[[i32; 3]],
    random: &mut Random,
    out: &mut Vec<Mote>,
) {
    if radius < 2.0 {
        out.extend(motes::create(LARGE_EXPLODE, random, pos, Vec3::X, None));
    }
    for &block in blocks {
        let at = IVec3::from(block).as_vec3() + vec3(random.next(), random.next(), random.next());
        let away = at - pos;
        let push = 0.5 / (away.length() / radius + 0.1) * (random.next() * random.next() + 0.3);
        let velocity = away.normalize_or_zero() * push;
        out.extend(motes::create(
            EXPLODE,
            random,
            (at + pos) / 2.0,
            velocity,
            None,
        ));
        out.extend(motes::create(SMOKE, random, at, velocity, None));
    }
}

fn world(id: i32, origin: Vec3, data: i32, random: &mut Random, out: &mut Vec<Mote>) {
    let center = origin + 0.5;
    match id {
        DISPENSE => {
            let (dx, dz) = (
                (data.rem_euclid(3) - 1) as f32,
                ((data / 3).rem_euclid(3) - 1) as f32,
            );
            for _ in 0..10 {
                let speed = random.next() * 0.2 + 0.01;
                let at = center
                    + vec3(
                        dx * 0.6 + random.signed() * 0.3 * dz.abs(),
                        random.signed() * 0.3,
                        dz * 0.6 + random.signed() * 0.3 * dx.abs(),
                    );
                let velocity = vec3(dx * speed, -0.03, dz * speed) + random.gaussians() * 0.01;
                out.extend(motes::create(SMOKE, random, at, velocity, None));
            }
        }
        BLOCK_BREAK => {
            let state = ((data & 4095) << 4 | data >> 12 & 15) as u16;
            for i in 0..4 {
                for j in 0..4 {
                    for k in 0..4 {
                        let offset = (vec3(i as f32, j as f32, k as f32) + 0.5) / 4.0;
                        let crumb = motes::digging(random, origin + offset, offset - 0.5, state);
                        out.push(crumb);
                    }
                }
            }
        }
        SPLASH_POTION => {
            let potion = (data & 15) as usize;
            let color = rgb(POTION_COLORS[potion]);
            let kind = if INSTANT_POTIONS.contains(&(data & 15)) {
                INSTANT_SPELL
            } else {
                SPELL
            };
            for _ in 0..8 {
                let velocity = vec3(
                    random.gaussian() * 0.15,
                    random.next() * 0.2,
                    random.gaussian() * 0.15,
                );
                out.extend(motes::create(
                    ITEM_CRACK,
                    random,
                    origin,
                    velocity,
                    Some((POTION, data)),
                ));
            }
            for _ in 0..100 {
                let speed = random.next() * 4.0;
                let (sin, cos) = (random.next() * TAU).sin_cos();
                let velocity = vec3(cos * speed, 0.01 + random.next() * 0.5, sin * speed);
                let at = origin + vec3(velocity.x * 0.1, 0.3, velocity.z * 0.1);
                if let Some(mut mote) = motes::create(kind, random, at, velocity, None) {
                    mote.color = color * (0.75 + random.next() * 0.25);
                    mote.velocity = vec3(
                        mote.velocity.x * speed,
                        (mote.velocity.y - 0.1) * speed + 0.1,
                        mote.velocity.z * speed,
                    );
                    out.push(mote);
                }
            }
        }
        SPAWNER => {
            for _ in 0..20 {
                let at = center + vec3(random.next(), random.next(), random.next()) * 2.0 - 1.0;
                out.extend(motes::create(SMOKE, random, at, Vec3::ZERO, None));
                out.extend(motes::create(FLAME, random, at, Vec3::ZERO, None));
            }
        }
        BONEMEAL => {
            let count = if data == 0 { 15 } else { data.min(MAX_COUNT) };
            for _ in 0..count {
                let at = origin + vec3(random.next(), random.next(), random.next());
                let velocity = random.gaussians() * 0.02;
                out.extend(motes::create(HAPPY_VILLAGER, random, at, velocity, None));
            }
        }
        _ => {}
    }
}

fn makes_particles(kind: &EffectKind) -> bool {
    match kind {
        EffectKind::Particle { .. } | EffectKind::Crit { .. } | EffectKind::Explosion { .. } => {
            true
        }
        EffectKind::World { id, .. } => {
            matches!(
                *id,
                DISPENSE | BLOCK_BREAK | SPLASH_POTION | SPAWNER | BONEMEAL
            )
        }
        _ => false,
    }
}

fn seed(index: usize, multiplier: u32, offset: u32) -> u32 {
    (index as u32).wrapping_mul(multiplier).wrapping_add(offset)
}

fn rgb(color: u32) -> Vec3 {
    let [_, red, green, blue] = color.to_be_bytes();
    vec3(red.into(), green.into(), blue.into()) / 255.0
}

impl Terrain<'_> {
    fn flags(&self, at: Vec3) -> Flags {
        let state = self.world.block(at.floor().as_ivec3().into());
        self.models.block(state).flags
    }

    fn solid(&self, at: Vec3) -> bool {
        self.flags(at).has(Flags::SOLID)
    }

    fn water(&self, at: Vec3) -> bool {
        self.flags(at).has(Flags::WATER)
    }
}

fn tick_seed(index: usize, tick: u32) -> u32 {
    (index as u32).wrapping_mul(0x9E37_79B9) ^ tick.wrapping_mul(0x85EB_CA6B)
}
