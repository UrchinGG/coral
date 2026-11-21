use std::collections::HashSet;

use glam::{IVec3, Vec3};
use replay_model::{Effect, EffectKind, EventKind, FireworkShape, Object, ObjectKind, Replay};

use crate::random::Random;
use crate::{BLOCK_BREAK, LID_PER_MS, firework_position, lid, lids};

const DOOR: i32 = 1003;
const CHEST_DEDUP_MS: f32 = 200.0;
const FIREWORK_VOLUME: f32 = 20.0;
const TWINKLE_DELAY_MS: f32 = 100.0;
const WORLD_SOUNDS: [(i32, &str, f32, Pitch); 18] = [
    (1000, "random.click", 1.0, Pitch::Fixed(1.0)),
    (1001, "random.click", 1.0, Pitch::Fixed(1.2)),
    (1002, "random.bow", 1.0, Pitch::Fixed(1.2)),
    (1004, "random.fizz", 0.5, Pitch::Wobble(2.6, 0.8)),
    (1007, "mob.ghast.charge", 10.0, Pitch::Wobble(1.0, 0.2)),
    (1008, "mob.ghast.fireball", 10.0, Pitch::Wobble(1.0, 0.2)),
    (1009, "mob.ghast.fireball", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1010, "mob.zombie.wood", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1011, "mob.zombie.metal", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1012, "mob.zombie.woodbreak", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1014, "mob.wither.shoot", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1015, "mob.bat.takeoff", 0.05, Pitch::Wobble(1.0, 0.1)),
    (1016, "mob.zombie.infect", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1017, "mob.zombie.unfect", 2.0, Pitch::Wobble(1.0, 0.2)),
    (1020, "random.anvil_break", 1.0, Pitch::Slight),
    (1021, "random.anvil_use", 1.0, Pitch::Slight),
    (1022, "random.anvil_land", 0.3, Pitch::Slight),
    (2002, "game.potion.smash", 1.0, Pitch::Slight),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    pub t: f32,
    pub name: usize,
    pub origin: Origin,
    pub volume: f32,
    pub pitch: f32,
    pub far: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Origin {
    Point(Vec3),
    Player(usize),
}

pub struct Sounds {
    pub names: Vec<String>,
    sounds: Vec<Sound>,
}

enum Pitch {
    Fixed(f32),
    Wobble(f32, f32),
    Slight,
}

impl Sounds {
    pub fn new(replay: &Replay) -> Sounds {
        let mut sounds = Sounds {
            names: Vec::new(),
            sounds: Vec::new(),
        };
        let mut random = Random::new(1);
        for effect in &replay.effects {
            let t = effect.t as f32;
            match effect.kind {
                EffectKind::Sound {
                    ref name,
                    pos,
                    volume,
                    pitch,
                } => {
                    sounds.push(t, name, Origin::Point(pos.into()), volume, pitch);
                }
                EffectKind::World { id, pos, data } => {
                    if let Some((name, volume, pitch)) = world_sound(id, data, &mut random) {
                        sounds.push(t, name, center(pos), volume, pitch);
                    }
                }
                _ => {}
            }
        }
        for event in &replay.events {
            let (name, volume, pitch, player) = match event.kind {
                EventKind::Hurt(player) => (
                    "game.player.hurt",
                    1.0,
                    Pitch::Wobble(1.0, 0.2).sample(&mut random),
                    player,
                ),
                EventKind::Dead(player) => (
                    "game.player.die",
                    1.0,
                    Pitch::Wobble(1.0, 0.2).sample(&mut random),
                    player,
                ),
                EventKind::Collect { player, .. } => (
                    "random.pop",
                    0.2,
                    (random.difference() * 0.7 + 1.0) * 2.0,
                    player,
                ),
                _ => continue,
            };
            sounds.push(event.t as f32, name, Origin::Player(player), volume, pitch);
        }
        sounds.chests(&replay.effects, &mut random);
        for object in &replay.objects {
            sounds.firework(object, &mut random);
        }
        sounds.sounds.sort_by(|a, b| a.t.total_cmp(&b.t));
        sounds
    }

    pub fn between(&self, after: f32, until: f32) -> &[Sound] {
        let start = self.sounds.partition_point(|sound| sound.t <= after);
        let end = self.sounds.partition_point(|sound| sound.t <= until);
        &self.sounds[start..end.max(start)]
    }

    fn chests(&mut self, effects: &[Effect], random: &mut Random) {
        let lids = lids(effects);
        let key = |pos: [i32; 3], name, at: f32| (name, (at / CHEST_DEDUP_MS).round() as i32, pos);
        let mut heard = HashSet::new();
        for (&[x, y, z], events) in &lids {
            let mut users = 0;
            for &(t, viewers) in events {
                let t = t as f32;
                let angle = lid(events, t - 1.0);
                let sound = if viewers > 0 && users == 0 && angle == 0.0 {
                    Some(("random.chestopen", t))
                } else if viewers == 0 && users > 0 && angle >= 0.5 {
                    Some(("random.chestclosed", t + (angle - 0.5) / LID_PER_MS))
                } else {
                    None
                };
                users = viewers;
                if let Some((name, at)) = sound
                    && heard.insert(key([x, y, z], name, at))
                    && !heard.contains(&key([x - 1, y, z], name, at))
                    && !heard.contains(&key([x, y, z - 1], name, at))
                {
                    self.push(
                        at,
                        name,
                        center([x, y, z]),
                        0.5,
                        Pitch::Slight.sample(random),
                    );
                }
            }
        }
    }

    fn firework(&mut self, object: &Object, random: &mut Random) {
        let (ObjectKind::Firework, Some(burst)) = (object.kind, object.burst) else {
            return;
        };
        let explosions = &object.explosions;
        if explosions.is_empty() {
            return;
        }
        let burst = burst as f32;
        let at = Origin::Point(firework_position(object, burst));
        let large = explosions.len() >= 3
            || explosions
                .iter()
                .any(|explosion| explosion.shape == FireworkShape::LargeBall);
        let blast = if large {
            "fireworks.largeBlast"
        } else {
            "fireworks.blast"
        };
        let pitch = random.next() * 0.1 + 0.95;
        self.firework_sound(burst, blast, at, pitch);
        if explosions.iter().any(|explosion| explosion.flicker) {
            let twinkle = burst + explosions.len() as f32 * TWINKLE_DELAY_MS;
            let pitch = random.next() * 0.15 + 0.9;
            self.firework_sound(twinkle, "fireworks.twinkle", at, pitch);
        }
    }

    fn firework_sound(&mut self, t: f32, name: &str, origin: Origin, pitch: f32) {
        let far = self.index(&format!("{name}_far"));
        self.push(t, name, origin, FIREWORK_VOLUME, pitch).far = Some(far);
    }

    fn push(&mut self, t: f32, name: &str, origin: Origin, volume: f32, pitch: f32) -> &mut Sound {
        let name = self.index(name);
        let pushed = self.sounds.len();
        self.sounds.push(Sound {
            t,
            name,
            origin,
            volume,
            pitch,
            far: None,
        });
        &mut self.sounds[pushed]
    }

    fn index(&mut self, name: &str) -> usize {
        self.names
            .iter()
            .position(|known| known == name)
            .unwrap_or_else(|| {
                self.names.push(name.to_owned());
                self.names.len() - 1
            })
    }
}

impl Pitch {
    fn sample(&self, random: &mut Random) -> f32 {
        match *self {
            Pitch::Fixed(pitch) => pitch,
            Pitch::Wobble(center, spread) => center + random.difference() * spread,
            Pitch::Slight => random.next() * 0.1 + 0.9,
        }
    }
}

fn world_sound(id: i32, data: i32, random: &mut Random) -> Option<(&'static str, f32, f32)> {
    match id {
        BLOCK_BREAK => {
            let (name, volume, pitch) = break_sound(data & 4095);
            Some((name, (volume + 1.0) / 2.0, pitch * 0.8))
        }
        DOOR => {
            let name = if random.next() < 0.5 {
                "random.door_open"
            } else {
                "random.door_close"
            };
            Some((name, 1.0, Pitch::Slight.sample(random)))
        }
        _ => WORLD_SOUNDS
            .iter()
            .find(|&&(known, ..)| known == id)
            .map(|(_, name, volume, pitch)| (*name, *volume, pitch.sample(random))),
    }
}

fn break_sound(block: i32) -> (&'static str, f32, f32) {
    match block {
        35 | 92 | 171 => ("dig.cloth", 1.0, 1.0),
        20 | 79 | 89 | 90 | 95 | 102 | 123 | 124 | 138 | 160 | 169 | 174 => ("dig.glass", 1.0, 1.0),
        12 | 88 => ("dig.sand", 1.0, 1.0),
        78 | 80 => ("dig.snow", 1.0, 1.0),
        3 | 13 | 60 | 82 => ("dig.gravel", 1.0, 1.0),
        2
        | 6
        | 18
        | 19
        | 30..=32
        | 37
        | 38
        | 46
        | 59
        | 83
        | 104..=106
        | 110
        | 111
        | 115
        | 140..=142
        | 161
        | 170
        | 175 => ("dig.grass", 1.0, 1.0),
        5
        | 17
        | 25
        | 26
        | 47
        | 53
        | 54
        | 58
        | 63..=65
        | 68
        | 69
        | 72
        | 84..=86
        | 91
        | 96
        | 99
        | 100
        | 103
        | 107
        | 125..=127
        | 130
        | 134..=136
        | 143
        | 146
        | 151
        | 162..=164
        | 176..=178
        | 183..=197 => ("dig.wood", 1.0, 1.0),
        27 | 28 | 41 | 42 | 57 | 66 | 71 | 101 | 118 | 133 | 147 | 148 | 152 | 154 | 157 | 167 => {
            ("dig.stone", 1.0, 1.5)
        }
        145 => ("dig.stone", 0.3, 1.0),
        165 => ("mob.slime.big", 1.0, 1.0),
        _ => ("dig.stone", 1.0, 1.0),
    }
}

fn center(pos: [i32; 3]) -> Origin {
    Origin::Point(IVec3::from(pos).as_vec3() + 0.5)
}
