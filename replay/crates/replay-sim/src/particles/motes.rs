use std::f32::consts::{PI, TAU};

use glam::{Vec3, vec3};

use super::{Particle, Sprite, Terrain};
use crate::random::Random;

pub(super) const EXPLODE: i32 = 0;
pub(super) const LARGE_EXPLODE: i32 = 1;
pub(super) const HUGE_EXPLOSION: i32 = 2;
const FIREWORKS_SPARK: i32 = 3;
const BUBBLE: i32 = 4;
const SPLASH: i32 = 5;
const SUSPENDED: i32 = 7;
const DEPTH_SUSPEND: i32 = 8;
pub(super) const CRIT: i32 = 9;
pub(super) const MAGIC_CRIT: i32 = 10;
pub(super) const SMOKE: i32 = 11;
const LARGE_SMOKE: i32 = 12;
pub(super) const SPELL: i32 = 13;
pub(super) const INSTANT_SPELL: i32 = 14;
const MOB_SPELL: i32 = 15;
const MOB_SPELL_AMBIENT: i32 = 16;
const WITCH_MAGIC: i32 = 17;
const DRIP_WATER: i32 = 18;
const DRIP_LAVA: i32 = 19;
const ANGRY_VILLAGER: i32 = 20;
pub(super) const HAPPY_VILLAGER: i32 = 21;
const TOWN_AURA: i32 = 22;
const NOTE: i32 = 23;
const PORTAL: i32 = 24;
const ENCHANTMENT_TABLE: i32 = 25;
pub(super) const FLAME: i32 = 26;
const LAVA: i32 = 27;
const FOOTSTEP: i32 = 28;
const CLOUD: i32 = 29;
const REDDUST: i32 = 30;
const SNOWBALL_POOF: i32 = 31;
const SNOW_SHOVEL: i32 = 32;
const SLIME: i32 = 33;
const HEART: i32 = 34;
pub(super) const ITEM_CRACK: i32 = 36;
const BLOCK_CRACK: i32 = 37;
const BLOCK_DUST: i32 = 38;
const DROPLET: i32 = 39;
const SNOWBALL: i16 = 332;
const SLIME_BALL: i16 = 341;
const GRAVITY: f32 = 0.04;
const FIREWORK_TILE: u8 = 160;

#[derive(Clone, Copy)]
pub(super) struct Mote {
    pos: Vec3,
    prev: Vec3,
    pub(super) velocity: Vec3,
    age: u32,
    life: u32,
    scale: f32,
    pub(super) color: Vec3,
    alpha: f32,
    rise: f32,
    drag: f32,
    clips: bool,
    grounded: bool,
    sprite: Sprite,
    flipbook: Option<u8>,
    motion: Motion,
    grow: bool,
    shrink: f32,
}

#[derive(Clone, Copy)]
enum Motion {
    Still,
    Free,
    Drift,
    Rain,
    Crit,
    Bubble,
    Portal(Vec3),
    Enchant(Vec3),
    Spark(Spark),
}

#[derive(Clone, Copy, Default)]
pub(super) struct Spark {
    pub(super) fade: Option<Vec3>,
    pub(super) trail: bool,
    pub(super) twinkle: bool,
}

pub(super) fn create(
    kind: i32,
    random: &mut Random,
    at: Vec3,
    v: Vec3,
    item: Option<(i32, i32)>,
) -> Option<Mote> {
    let mote = Mote::new(random, at);
    Some(match kind {
        EXPLODE => {
            let mut mote = mote.thrown(random, v);
            mote.velocity = v + random.cube() * 0.05;
            mote.color = Vec3::splat(random.next() * 0.3 + 0.7);
            mote.scale = random.next() * random.next() * 6.0 + 1.0;
            mote.life = lifetime(random, 16.0, 0.2) + 2;
            Mote {
                rise: 0.004,
                drag: 0.9,
                flipbook: Some(0),
                ..mote
            }
        }
        LARGE_EXPLODE => Mote {
            life: 6 + random.below(4) as u32,
            color: Vec3::splat(random.next() * 0.6 + 0.4),
            scale: 20.0 * (1.0 - v.x * 0.5),
            sprite: Sprite::Explosion(0),
            motion: Motion::Still,
            ..mote
        },
        BUBBLE => {
            let mut mote = mote.thrown(random, v);
            mote.sprite = Sprite::Tile(32);
            mote.scale *= random.next() * 0.6 + 0.2;
            mote.velocity = v * 0.2 + random.cube() * 0.02;
            mote.life = lifetime(random, 8.0, 0.2);
            Mote {
                rise: 0.002,
                drag: 0.85,
                motion: Motion::Bubble,
                ..mote
            }
        }
        SPLASH | DROPLET => {
            let mut mote = mote.thrown(random, Vec3::ZERO);
            mote.velocity *= vec3(0.3, 0.0, 0.3);
            mote.velocity.y = random.next() * 0.2 + 0.1;
            let tile = 19 + random.below(4) as u8;
            mote.life = lifetime(random, 8.0, 0.2);
            let splash = kind == SPLASH;
            if splash && v.y == 0.0 && (v.x != 0.0 || v.z != 0.0) {
                mote.velocity = v + Vec3::Y * 0.1;
            }
            Mote {
                sprite: Sprite::Tile(tile + u8::from(splash)),
                rise: if splash { -0.04 } else { -0.06 },
                motion: Motion::Rain,
                ..mote
            }
        }
        CRIT | MAGIC_CRIT => {
            let mut mote = mote.thrown(random, Vec3::ZERO);
            mote.velocity = mote.velocity * 0.1 + v * 0.4;
            mote.color = Vec3::splat(random.next() * 0.3 + 0.6);
            mote.scale *= 0.75;
            mote.life = lifetime(random, 6.0, 0.6).max(1);
            mote.sprite = Sprite::Tile(65);
            if kind == MAGIC_CRIT {
                mote.color *= vec3(0.3, 0.8, 1.0);
                mote.sprite = Sprite::Tile(66);
            }
            Mote {
                drag: 0.7,
                motion: Motion::Crit,
                grow: true,
                ..mote
            }
        }
        SMOKE | LARGE_SMOKE | CLOUD | SNOW_SHOVEL => {
            let smoke = matches!(kind, SMOKE | LARGE_SMOKE);
            let factor = if matches!(kind, LARGE_SMOKE | CLOUD) {
                2.5
            } else {
                1.0
            };
            let mut mote = mote.thrown(random, Vec3::ZERO);
            mote.velocity = mote.velocity * 0.1 + v;
            let shade = random.next() * 0.3;
            mote.color = Vec3::splat(if smoke { shade } else { 1.0 - shade });
            mote.scale *= 0.75 * factor;
            let spread = if smoke { 0.2 } else { 0.3 };
            mote.life = ((8.0 / (random.next() * 0.8 + spread) * factor) as u32).max(1);
            mote.grow = true;
            let rise = match kind {
                SMOKE | LARGE_SMOKE => 0.004,
                SNOW_SHOVEL => -0.03,
                _ => 0.0,
            };
            mote.drifting(rise, 0.96, Some(0))
        }
        SPELL | INSTANT_SPELL | MOB_SPELL | MOB_SPELL_AMBIENT | WITCH_MAGIC => {
            let mut mote = mote.thrown(random, v);
            mote.velocity.y *= 0.2;
            if v.x == 0.0 && v.z == 0.0 {
                mote.velocity *= vec3(0.1, 1.0, 0.1);
            }
            mote.scale *= 0.75;
            mote.life = lifetime(random, 8.0, 0.2);
            match kind {
                MOB_SPELL | MOB_SPELL_AMBIENT => {
                    mote.color = v;
                    mote.velocity = Vec3::ZERO;
                    if kind == MOB_SPELL_AMBIENT {
                        mote.alpha = 0.15;
                    }
                }
                WITCH_MAGIC => {
                    let shade = random.next() * 0.5 + 0.35;
                    mote.color = vec3(shade, 0.0, shade);
                }
                _ => {}
            }
            let tiles = if kind == INSTANT_SPELL { 144 } else { 128 };
            mote.drifting(0.004, 0.96, Some(tiles))
        }
        REDDUST => {
            let mut mote = mote.thrown(random, Vec3::ZERO);
            mote.velocity *= 0.1;
            let red = if v.x == 0.0 { 1.0 } else { v.x };
            let shade = random.next() * 0.4 + 0.6;
            let mut channel = |value: f32| (random.next() * 0.2 + 0.8) * value * shade;
            mote.color = vec3(channel(red), channel(v.y), channel(v.z));
            mote.scale *= 0.75;
            mote.life = lifetime(random, 8.0, 0.2).max(1);
            mote.grow = true;
            mote.drifting(0.0, 0.96, Some(0))
        }
        HAPPY_VILLAGER | TOWN_AURA | SUSPENDED | DEPTH_SUSPEND => {
            let mut mote = mote.thrown(random, v);
            mote.color = match kind {
                HAPPY_VILLAGER => Vec3::ONE,
                SUSPENDED => vec3(0.4, 0.4, 0.7),
                _ => Vec3::splat(random.next() * 0.1 + 0.2),
            };
            mote.sprite = Sprite::Tile(if kind == HAPPY_VILLAGER { 82 } else { 0 });
            mote.scale *= random.next() * 0.6 + 0.5;
            mote.velocity *= 0.02;
            mote.life = lifetime(random, 20.0, 0.2);
            Mote {
                drag: 0.99,
                clips: false,
                ..mote
            }
        }
        HEART | ANGRY_VILLAGER | NOTE => {
            let mut mote = mote.thrown(random, Vec3::ZERO);
            mote.velocity *= 0.01;
            mote.scale *= 1.5;
            mote.grow = true;
            if kind == NOTE {
                let hue = |offset: f32| ((v.x + offset) * TAU).sin() * 0.65 + 0.35;
                mote.velocity.y += 0.2;
                mote.color = vec3(hue(0.0), hue(1.0 / 3.0), hue(2.0 / 3.0));
                mote.life = 6;
                mote.sprite = Sprite::Tile(64);
                mote.drifting(0.0, 0.66, None)
            } else {
                let angry = kind == ANGRY_VILLAGER;
                mote.velocity.y += if angry { 1.1 } else { 0.1 };
                mote.life = 16;
                mote.sprite = Sprite::Tile(if angry { 81 } else { 80 });
                mote.drifting(0.0, 0.86, None)
            }
        }
        PORTAL | ENCHANTMENT_TABLE => {
            let shade = random.next() * 0.6 + 0.4;
            let (color, tile, life, motion) = if kind == PORTAL {
                let tile = random.below(8) as u8;
                let life = (random.next() * 10.0) as u32 + 40;
                (vec3(0.9, 0.3, 1.0), tile, life, Motion::Portal(at))
            } else {
                let tile = 225 + random.below(26) as u8;
                let life = (random.next() * 10.0) as u32 + 30;
                (vec3(0.9, 0.9, 1.0), tile, life, Motion::Enchant(at))
            };
            Mote {
                velocity: v,
                color: color * shade,
                sprite: Sprite::Tile(tile),
                life,
                scale: random.next() * 0.2 + 0.5,
                clips: false,
                motion,
                ..mote
            }
        }
        FLAME => {
            let mut mote = mote.thrown(random, v);
            mote.velocity = mote.velocity * 0.01 + v;
            mote.life = lifetime(random, 8.0, 0.2) + 4;
            Mote {
                sprite: Sprite::Tile(48),
                drag: 0.96,
                clips: false,
                shrink: 0.5,
                ..mote
            }
        }
        LAVA => {
            let mut mote = mote.thrown(random, v);
            mote.velocity *= vec3(0.8, 0.0, 0.8);
            mote.velocity.y = random.next() * 0.4 + 0.05;
            mote.scale *= random.next() * 2.0 + 0.2;
            mote.life = lifetime(random, 16.0, 0.2);
            Mote {
                sprite: Sprite::Tile(49),
                rise: -0.03,
                drag: 0.999,
                shrink: 1.0,
                ..mote
            }
        }
        DRIP_WATER | DRIP_LAVA => Mote {
            sprite: Sprite::Tile(113),
            rise: -GRAVITY * 0.06,
            life: lifetime(random, 64.0, 0.2),
            color: if kind == DRIP_WATER {
                vec3(0.0, 0.0, 1.0)
            } else {
                vec3(1.0, 0.16, 0.04)
            },
            ..mote
        },
        FOOTSTEP => Mote {
            sprite: Sprite::Footprint,
            scale: 1.25,
            life: 200,
            motion: Motion::Still,
            ..mote
        },
        SNOWBALL_POOF => breaking(random, at, SNOWBALL, 0),
        SLIME => breaking(random, at, SLIME_BALL, 0),
        ITEM_CRACK => {
            let (id, damage) = item?;
            let mut mote = breaking(random, at, id as i16, damage as i16);
            mote.velocity = mote.velocity * 0.1 + v;
            mote
        }
        BLOCK_CRACK | BLOCK_DUST => {
            let (id, meta) = item?;
            let mut mote = digging(random, at, v, (id << 4 | meta & 15) as u16);
            if kind == BLOCK_DUST {
                mote.velocity = v;
            }
            mote
        }
        FIREWORKS_SPARK => spark(random, at, v, Spark::default(), None),
        _ => return None,
    })
}

pub(super) fn digging(random: &mut Random, at: Vec3, v: Vec3, state: u16) -> Mote {
    let mote = Mote::new(random, at).thrown(random, v);
    Mote {
        rise: -GRAVITY,
        color: Vec3::splat(0.6),
        scale: mote.scale / 2.0,
        sprite: Sprite::Block(state, corner(random)),
        ..mote
    }
}

pub(super) fn spark(
    random: &mut Random,
    at: Vec3,
    v: Vec3,
    spark: Spark,
    color: Option<Vec3>,
) -> Mote {
    let mote = Mote::new(random, at);
    Mote {
        velocity: v,
        scale: mote.scale * 0.75,
        life: 48 + random.below(12) as u32,
        alpha: 0.99,
        color: color.unwrap_or(Vec3::ONE),
        rise: -0.004,
        drag: 0.91,
        flipbook: Some(FIREWORK_TILE),
        motion: Motion::Spark(spark),
        ..mote
    }
}

pub(super) fn flash(random: &mut Random, at: Vec3, color: Vec3) -> Mote {
    Mote {
        color,
        life: 4,
        sprite: Sprite::Flash,
        motion: Motion::Still,
        ..Mote::new(random, at)
    }
}

fn breaking(random: &mut Random, at: Vec3, id: i16, damage: i16) -> Mote {
    let mote = Mote::new(random, at).thrown(random, Vec3::ZERO);
    Mote {
        rise: -GRAVITY,
        scale: mote.scale / 2.0,
        sprite: Sprite::Item(id, damage, corner(random)),
        ..mote
    }
}

fn corner(random: &mut Random) -> [f32; 2] {
    [random.next() * 3.0, random.next() * 3.0]
}

fn lifetime(random: &mut Random, ticks: f32, least: f32) -> u32 {
    (ticks / (random.next() * 0.8 + least)) as u32
}

impl Mote {
    fn new(random: &mut Random, at: Vec3) -> Mote {
        Mote {
            pos: at,
            prev: at,
            velocity: Vec3::ZERO,
            age: 0,
            life: (4.0 / (random.next() * 0.9 + 0.1)) as u32,
            scale: (random.next() * 0.5 + 0.5) * 2.0,
            color: Vec3::ONE,
            alpha: 1.0,
            rise: 0.0,
            drag: 0.98,
            clips: true,
            grounded: false,
            sprite: Sprite::Tile(0),
            flipbook: None,
            motion: Motion::Free,
            grow: false,
            shrink: 0.0,
        }
    }

    fn thrown(mut self, random: &mut Random, v: Vec3) -> Mote {
        let scattered = v + random.cube() * 0.4;
        let speed = (random.next() + random.next() + 1.0) * 0.15;
        self.velocity = scattered.normalize_or_zero() * speed * 0.4 + Vec3::Y * 0.1;
        self
    }

    fn drifting(self, rise: f32, drag: f32, flipbook: Option<u8>) -> Mote {
        Mote {
            rise,
            drag,
            flipbook,
            motion: Motion::Drift,
            ..self
        }
    }

    pub(super) fn step(
        &mut self,
        terrain: &Terrain,
        random: &mut Random,
        children: &mut Vec<Mote>,
    ) -> bool {
        if self.age >= self.life {
            return false;
        }
        let progress = self.age as f32 / self.life as f32;
        self.age += 1;
        self.prev = self.pos;
        match self.motion {
            Motion::Still => return true,
            Motion::Portal(start) => {
                let curve = 1.0 + progress - progress * progress * 2.0;
                self.pos = start + self.velocity * curve + Vec3::Y * (1.0 - progress);
                return true;
            }
            Motion::Enchant(start) => {
                let back = 1.0 - progress;
                self.pos = start + self.velocity * back - Vec3::Y * (1.0 - back * back) * 1.2;
                return true;
            }
            _ => {}
        }
        self.velocity.y += self.rise;
        let before = self.pos.y;
        self.coast(terrain);
        match self.motion {
            Motion::Drift if self.pos.y == before => self.velocity *= vec3(1.1, 1.0, 1.1),
            Motion::Crit => {
                self.color *= vec3(1.0, 0.96, 0.9);
                self.velocity.y -= 0.02;
            }
            Motion::Rain if self.grounded && random.next() < 0.5 => return false,
            Motion::Bubble if !terrain.water(self.pos) => return false,
            Motion::Spark(spark) => self.sparkle(spark, random, children),
            _ => {}
        }
        true
    }

    fn sparkle(&mut self, style: Spark, random: &mut Random, children: &mut Vec<Mote>) {
        let (age, life) = (self.age as f32, self.life as f32);
        if age > life / 2.0 {
            self.alpha = 1.0 - (age - life / 2.0) / life;
            if let Some(fade) = style.fade {
                self.color += (fade - self.color) * 0.2;
            }
        }
        if style.trail && age < life / 2.0 && (self.age + self.life).is_multiple_of(2) {
            let trail = Spark {
                trail: false,
                ..style
            };
            let mut child = spark(random, self.pos, Vec3::ZERO, trail, Some(self.color));
            child.age = child.life / 2;
            children.push(child);
        }
    }

    fn coast(&mut self, terrain: &Terrain) {
        if self.clips {
            self.collide(terrain);
        } else {
            self.pos += self.velocity;
        }
        self.velocity *= self.drag;
        if self.grounded {
            self.velocity *= vec3(0.7, 1.0, 0.7);
        }
    }

    fn collide(&mut self, terrain: &Terrain) {
        let Vec3 { x, y, z } = self.pos;
        let next_y = y + self.velocity.y;
        if self.velocity.y < 0.0 && terrain.solid(vec3(x, next_y, z)) {
            self.pos.y = next_y.floor() + 1.0;
            self.velocity.y = 0.0;
            self.grounded = true;
        } else if self.velocity.y > 0.0 && terrain.solid(vec3(x, next_y + 0.2, z)) {
            self.velocity.y = 0.0;
        } else {
            self.pos.y = next_y;
            self.grounded = false;
        }
        let feet = self.pos.y + 0.05;
        let next_x = x + self.velocity.x;
        if terrain.solid(vec3(next_x, feet, z)) {
            self.velocity.x = 0.0;
        } else {
            self.pos.x = next_x;
        }
        let next_z = z + self.velocity.z;
        if terrain.solid(vec3(self.pos.x, feet, next_z)) {
            self.velocity.z = 0.0;
        } else {
            self.pos.z = next_z;
        }
    }

    pub(super) fn particle(&self, partial: f32) -> Option<Particle> {
        let (age, life) = (self.age, self.life.max(1));
        if let Motion::Spark(spark) = self.motion
            && spark.twinkle
            && age as f32 >= life as f32 / 3.0
            && (age + life) / 3 % 2 != 0
        {
            return None;
        }
        let progress = (age as f32 + partial) / life as f32;
        let mut size = 0.1 * self.scale;
        if self.grow {
            size *= (progress * 32.0).clamp(0.0, 1.0);
        }
        size *= 1.0 - progress * progress * self.shrink;
        let mut alpha = self.alpha;
        let sprite = match (self.sprite, self.flipbook) {
            (_, Some(first)) => Sprite::Tile(first + 7u8.saturating_sub((age * 8 / life) as u8)),
            (Sprite::Explosion(_), _) => Sprite::Explosion((progress * 15.0).min(15.0) as u8),
            (Sprite::Footprint, _) => {
                alpha = (2.0 - 2.0 * progress * progress).min(1.0) * 0.2;
                Sprite::Footprint
            }
            (Sprite::Flash, _) => {
                let phase = (age as f32 + partial - 1.0) * 0.25;
                size = 7.1 * (phase * PI).sin().max(0.0) * 0.5;
                alpha = (0.6 - phase * 0.5).max(0.0);
                Sprite::Flash
            }
            (sprite, None) => sprite,
        };
        let flat = sprite == Sprite::Footprint;
        let lift = if flat { 0.01 } else { 0.0 };
        Some(Particle {
            position: self.prev.lerp(self.pos, partial) + Vec3::Y * lift,
            size,
            color: self.color.extend(alpha),
            sprite,
            flat,
        })
    }
}
