use std::{f32::consts::FRAC_PI_2, ops::Range};

use glam::{Vec3, Vec3Swizzles};
use replay_model::{MoveKind, Moves, Player};

use crate::{SWING_MS, TICK_MS, since, wrap};

const WHOLE_TICK_MS: u32 = TICK_MS as u32;
const LERP_TICKS: f32 = 3.0;
const MAX_TWIST: f32 = 75f32.to_radians();
const CATCH_UP_TWIST: f32 = 50f32.to_radians();
const WALKING: f32 = 0.0025;

#[derive(Clone, Copy)]
pub struct Tick {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub body_yaw: f32,
    pub limb_swing: f32,
    pub limb_amount: f32,
}

pub struct Life {
    pub spawn: u32,
    pub until: u32,
    start: u32,
    ticks: Vec<Tick>,
}

struct Motion {
    now: Tick,
    target: Vec3,
    target_yaw: f32,
    target_pitch: f32,
    remaining: f32,
}

pub fn lives(player: &Player, swings: &[u32], end: u32) -> Vec<Life> {
    let moves = &player.moves;
    let spawns: Vec<usize> = (0..moves.kind.len())
        .filter(|&index| moves.kind[index] == MoveKind::Spawn)
        .collect();
    let mut spans = Vec::new();
    let mut open = None;
    for &(t, present) in &player.presence {
        spans.extend(open.take().map(|spawn| (spawn, t)));
        open = present.then_some(t);
    }
    spans.extend(open.map(|spawn| (spawn, u32::MAX)));
    let ends = spawns.iter().skip(1).copied().chain([moves.t.len()]);
    spans
        .into_iter()
        .zip(spawns.iter().zip(ends))
        .map(|((spawn, until), (&first, last))| {
            let start = spawn / WHOLE_TICK_MS * WHOLE_TICK_MS;
            Life {
                spawn,
                until,
                start,
                ticks: simulate(moves, first..last, start, until.min(end), swings),
            }
        })
        .collect()
}

fn simulate(
    moves: &Moves,
    range: Range<usize>,
    start: u32,
    last: u32,
    swings: &[u32],
) -> Vec<Tick> {
    let mut motion = Motion::spawn(moves, range.start);
    let mut pending = range.skip(1).peekable();
    (0..=last.saturating_sub(start) / WHOLE_TICK_MS + 1)
        .map(|tick| {
            let now = start + tick * WHOLE_TICK_MS;
            while let Some(index) = pending.next_if(|&index| moves.t[index] <= now) {
                motion.receive(moves, index);
            }
            let swinging =
                since(swings, now as f32).is_some_and(|elapsed| elapsed < SWING_MS as f32);
            motion.tick(swinging)
        })
        .collect()
}

impl Life {
    pub fn sample(&self, t: f32) -> Tick {
        let at = ((t - self.start as f32) / TICK_MS).max(0.0);
        let index = (at as usize).min(self.ticks.len() - 2);
        self.ticks[index].lerp(self.ticks[index + 1], (at - index as f32).min(1.0))
    }
}

impl Tick {
    fn lerp(self, to: Tick, alpha: f32) -> Tick {
        let mix = |from: f32, to: f32| from + (to - from) * alpha;
        Tick {
            position: self.position.lerp(to.position, alpha),
            yaw: mix(self.yaw, to.yaw),
            pitch: mix(self.pitch, to.pitch),
            head_yaw: mix(self.head_yaw, to.head_yaw),
            body_yaw: mix(self.body_yaw, to.body_yaw),
            limb_swing: mix(self.limb_swing, to.limb_swing),
            limb_amount: mix(self.limb_amount, to.limb_amount),
        }
    }
}

impl Motion {
    fn spawn(moves: &Moves, index: usize) -> Motion {
        let position = Vec3::from(moves.pos[index]);
        let yaw = moves.yaw[index];
        let pitch = moves.pitch[index];
        Motion {
            now: Tick {
                position,
                yaw,
                pitch,
                head_yaw: moves.head_yaw[index],
                body_yaw: yaw,
                limb_swing: 0.0,
                limb_amount: 0.0,
            },
            target: position,
            target_yaw: yaw,
            target_pitch: pitch,
            remaining: 0.0,
        }
    }

    fn receive(&mut self, moves: &Moves, index: usize) {
        let now = &mut self.now;
        let kind = moves.kind[index];
        if matches!(
            kind,
            MoveKind::Spawn | MoveKind::Teleport | MoveKind::HeadLook
        ) {
            now.head_yaw += wrap(moves.head_yaw[index] - now.head_yaw);
        }
        if kind == MoveKind::HeadLook {
            return;
        }
        let looked = kind != MoveKind::Relative;
        self.target = moves.pos[index].into();
        self.target_yaw = if looked { moves.yaw[index] } else { now.yaw };
        self.target_pitch = if looked {
            moves.pitch[index]
        } else {
            now.pitch
        };
        self.remaining = LERP_TICKS;
    }

    fn tick(&mut self, swinging: bool) -> Tick {
        let now = &mut self.now;
        let previous = now.position;
        if self.remaining > 0.0 {
            now.position += (self.target - now.position) / self.remaining;
            now.yaw += wrap(self.target_yaw - now.yaw) / self.remaining;
            now.pitch += (self.target_pitch - now.pitch) / self.remaining;
            self.remaining -= 1.0;
        }
        let step = (now.position - previous).xz();
        let facing = if swinging {
            now.yaw
        } else if step.length_squared() > WALKING {
            step.y.atan2(step.x) - FRAC_PI_2
        } else {
            now.body_yaw
        };
        now.body_yaw += wrap(facing - now.body_yaw) * 0.3;
        let twist = wrap(now.yaw - now.body_yaw).clamp(-MAX_TWIST, MAX_TWIST);
        now.body_yaw = now.yaw - twist;
        if twist.abs() > CATCH_UP_TWIST {
            now.body_yaw += twist * 0.2;
        }
        now.limb_amount += ((step.length() * 4.0).min(1.0) - now.limb_amount) * 0.4;
        now.limb_swing += now.limb_amount;
        *now
    }
}
