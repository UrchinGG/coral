mod motion;
mod particles;
mod pose;
mod random;
mod sounds;

use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use replay_model::{Effect, EffectKind, EntityState, Equipment, EventKind, Replay};

use motion::Life;
use pose::Hand;

pub use particles::{MAX_PARTICLES, Particle, Particles, Sprite, firework_position, trail};
pub use pose::{Pose, SNEAK_DROP, held_item, part, root};
pub use sounds::{Origin, Sound, Sounds};

pub const TICK_MS: f32 = 50.0;
pub const INVISIBLE: u8 = 0x20;
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT: f32 = 1.8;
const SWING_MS: u32 = 300;
const HURT_MS: u32 = 500;
const DEATH_MS: u32 = 1000;
const SNEAKING: u8 = 0x02;
const USING_ITEM: u8 = 0x10;
const BOW: i16 = 261;
const SWORDS: [i16; 5] = [267, 268, 272, 276, 283];
const LID_PER_MS: f32 = 0.1 / TICK_MS;
const LID_SETTLE_MS: f32 = 1000.0;
const BLOCK_BREAK: i32 = 2001;
const SEAT_DEPTH: f32 = 0.35;

pub struct Players {
    tracks: Vec<Track>,
    items: Vec<i16>,
}

#[derive(Default)]
struct Track {
    lives: Vec<Life>,
    equipment: Vec<(u32, Equipment)>,
    states: Vec<(u32, EntityState)>,
    riding: Vec<(u32, Option<usize>)>,
    swings: Vec<u32>,
    hurts: Vec<u32>,
    deaths: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerFrame {
    pub position: Vec3,
    pub yaw: f32,
    pub head_yaw: f32,
    pub body_yaw: f32,
    pub pitch: f32,
    pub limb_swing: f32,
    pub limb_amount: f32,
    pub state: EntityState,
    pub equipment: Equipment,
    pub swing: Option<f32>,
    pub hurt: Option<f32>,
    pub death: Option<f32>,
    pub pose: Pose,
}

impl Players {
    pub fn new(replay: &Replay) -> Players {
        let mut tracks: Vec<Track> = replay
            .players
            .iter()
            .map(|player| Track {
                equipment: player.equipment.clone(),
                states: player.states.clone(),
                riding: player.riding.clone(),
                ..Track::default()
            })
            .collect();
        for event in &replay.events {
            let t = event.t;
            match event.kind {
                EventKind::Swing(player) => {
                    if let Some(track) = tracks.get_mut(player)
                        && track
                            .swings
                            .last()
                            .is_none_or(|&last| t >= last + SWING_MS / 2)
                    {
                        track.swings.push(t);
                    }
                }
                EventKind::Hurt(player) => {
                    if let Some(track) = tracks.get_mut(player) {
                        track.hurts.push(t);
                    }
                }
                EventKind::Dead(player) => {
                    if let Some(track) = tracks.get_mut(player) {
                        track.deaths.push(t);
                    }
                }
                _ => {}
            }
        }
        for (track, player) in tracks.iter_mut().zip(&replay.players) {
            track.lives = motion::lives(player, &track.swings, replay.info.duration);
        }
        Players {
            tracks,
            items: replay.items.iter().map(|item| item.id).collect(),
        }
    }

    pub fn sample(&self, index: usize, t: f32) -> Option<PlayerFrame> {
        let track = self.tracks.get(index)?;
        let life = track.lives[..track.lives.partition_point(|life| life.spawn as f32 <= t)]
            .last()
            .filter(|life| t < life.until as f32)?;
        let alive = t - life.spawn as f32;
        let recent = |times: &[u32], duration: u32| {
            let duration = duration as f32;
            since(times, t)
                .filter(|&elapsed| elapsed < duration && elapsed <= alive)
                .map(|elapsed| elapsed / duration)
        };
        let motion = life.sample(t);
        let state = latest(&track.states, t).copied().unwrap_or_default();
        let equipment = latest(&track.equipment, t).copied().unwrap_or_default();
        let using = state.flags & USING_ITEM != 0;
        let hand = match equipment
            .held
            .and_then(|item| self.items.get(item as usize))
        {
            None => Hand::Empty,
            Some(&BOW) if using => Hand::Aiming,
            Some(id) if using && SWORDS.contains(id) => Hand::Blocking,
            Some(_) => Hand::Holding,
        };
        let mut frame = PlayerFrame {
            position: motion.position,
            yaw: wrap(motion.yaw),
            head_yaw: wrap(motion.head_yaw),
            body_yaw: wrap(motion.body_yaw),
            pitch: motion.pitch,
            limb_swing: motion.limb_swing,
            limb_amount: motion.limb_amount,
            state,
            equipment,
            swing: recent(&track.swings, SWING_MS),
            hurt: recent(&track.hurts, HURT_MS),
            death: recent(&track.deaths, DEATH_MS),
            pose: Pose::default(),
        };
        frame.pose = Pose::new(&frame, hand, t);
        Some(frame)
    }

    pub fn vehicle(&self, index: usize, t: f32) -> Option<usize> {
        latest(&self.tracks.get(index)?.riding, t)
            .copied()
            .flatten()
    }
}

impl PlayerFrame {
    pub fn sneaking(&self) -> bool {
        self.state.flags & SNEAKING != 0
    }

    pub fn seated(mut self, seat: Vec3) -> PlayerFrame {
        let position = seat - Vec3::Y * SEAT_DEPTH;
        self.pose.root = Mat4::from_translation(position - self.position) * self.pose.root;
        self.position = position;
        self
    }
}

pub fn since(times: &[u32], t: f32) -> Option<f32> {
    times[..times.partition_point(|&at| at as f32 <= t)]
        .last()
        .map(|&at| t - at as f32)
}

pub fn latest<T>(entries: &[(u32, T)], t: f32) -> Option<&T> {
    entries[..entries.partition_point(|(at, _)| *at as f32 <= t)]
        .last()
        .map(|(_, value)| value)
}

pub fn lids(effects: &[Effect]) -> BTreeMap<[i32; 3], Vec<(u32, u8)>> {
    let mut lids: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for effect in effects {
        if let EffectKind::Lid { pos, viewers } = effect.kind {
            lids.entry(pos).or_default().push((effect.t, viewers));
        }
    }
    lids
}

pub fn lid(events: &[(u32, u8)], t: f32) -> f32 {
    let end = events.partition_point(|&(at, _)| at as f32 <= t);
    let start = events[..end]
        .partition_point(|&(at, _)| (at as f32) < t - LID_SETTLE_MS)
        .saturating_sub(1);
    let settled = start
        .checked_sub(1)
        .is_some_and(|before| events[before].1 > 0);
    let mut open = if settled { 1.0 } else { 0.0 };
    for (index, &(at, viewers)) in events[start..end].iter().enumerate() {
        let until = events
            .get(start + index + 1)
            .map_or(t, |&(next, _)| (next as f32).min(t));
        let change = (until - at as f32).max(0.0) * LID_PER_MS;
        open = if viewers > 0 {
            (open + change).min(1.0)
        } else {
            (open - change).max(0.0)
        };
    }
    open
}

pub fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}
