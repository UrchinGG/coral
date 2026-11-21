use std::f32::consts::TAU;

use glam::{IVec3, Vec2, Vec3};
use replay_sim::wrap;

use super::view::{Orbit, forward};
use super::{CAMERA_MARGIN, FOLLOW_HEIGHT, FOLLOW_REST_PITCH};

const AIM_DIRECTIONS: u16 = 24;
const AIM_PITCHES: [f32; 4] = [
    10f32.to_radians(),
    20f32.to_radians(),
    35f32.to_radians(),
    50f32.to_radians(),
];
const AIM_REACH: f32 = 16.0;
const AIM_SIGHT: [f32; 3] = [0.3, 1.0, 1.7];
const AIM_INTERVAL: f32 = 0.15;
const AIM_MARGIN: f32 = 1.0;
const AIM_NUDGE: Vec2 = Vec2::new(20f32.to_radians(), 10f32.to_radians());
const AIM_HEADING: f32 = 0.4;
const AIM_HEADING_EASE: f32 = 2.0;
const AIM_BEHIND_SLACK: f32 = 20f32.to_radians();
const AIM_BEHIND_RATE: f32 = 0.3;
const AIM_BUFFER: f32 = 0.5;
const AIM_BOXED: f32 = 3.0;
const AIM_BOXED_SCANS: u8 = 4;
const AIM_SPREAD: [f32; 3] = [-12f32.to_radians(), 0.0, 12f32.to_radians()];
const AIM_PITCH_COST: f32 = 8.0;
const FOLLOW_AIM_GLIDE: f32 = 0.7;
const AROUND: [Vec3; 7] = [
    Vec3::ZERO,
    Vec3::X,
    Vec3::NEG_X,
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::Z,
    Vec3::NEG_Z,
];
const CLEAR_STEP: f32 = 0.1;

pub(super) struct Follow {
    pub(super) orbit: Orbit,
    pub(super) spin: Vec2,
    pub(super) auto: bool,
    goal: Vec2,
    drift: f32,
    cramped: u8,
    pub(super) veil: f32,
    pub(super) scan: f32,
    pub(super) turn: Vec2,
    heading: Vec3,
}

impl Follow {
    pub(super) fn new(orbit: Orbit, veil: f32) -> Follow {
        Follow {
            orbit,
            spin: Vec2::ZERO,
            auto: true,
            goal: Vec2::new(orbit.yaw, orbit.pitch),
            drift: 0.0,
            cramped: 0,
            veil,
            scan: 0.0,
            turn: Vec2::ZERO,
            heading: Vec3::ZERO,
        }
    }

    pub(super) fn aim(&mut self, dt: f32, goal: Vec3, solid: &impl Fn(IVec3) -> bool) {
        let motion = (goal - self.orbit.target)
            .with_y(0.0)
            .clamp_length_max(AIM_HEADING);
        self.heading = self
            .heading
            .lerp(motion, 1.0 - (-AIM_HEADING_EASE * dt).exp());
        self.scan -= dt;
        if self.scan <= 0.0 {
            self.scan = AIM_INTERVAL;
            self.plan(solid);
        }
        self.goal.x += self.drift * dt;
        let glided = smooth_damp(
            Vec2::new(self.orbit.yaw, self.orbit.pitch),
            self.goal,
            &mut self.turn,
            FOLLOW_AIM_GLIDE,
            dt,
        );
        (self.orbit.yaw, self.orbit.pitch) = glided.into();
    }

    fn plan(&mut self, solid: &impl Fn(IVec3) -> bool) {
        let Orbit {
            target, distance, ..
        } = self.orbit;
        let feet = target - Vec3::Y * FOLLOW_HEIGHT;
        let room = |view: Vec2| {
            AIM_SPREAD
                .into_iter()
                .flat_map(|side| AIM_SIGHT.map(|lift| (-forward(view.x + side, view.y), lift)))
                .map(|(direction, lift)| {
                    clearance(feet + Vec3::Y * lift, direction, AIM_REACH, solid)
                })
                .fold(AIM_REACH, f32::min)
        };
        let held = self.goal;
        let step = TAU / f32::from(AIM_DIRECTIONS);
        let columns: Vec<(Vec2, f32)> = (1..AIM_DIRECTIONS)
            .filter_map(|turn| {
                let around = held.x + f32::from(turn) * step;
                AIM_PITCHES
                    .map(|lift| {
                        let view = Vec2::new(around, lift);
                        (view, room(view))
                    })
                    .into_iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            })
            .collect();
        let held_room = room(held);
        let widest = columns
            .iter()
            .map(|&(_, room)| room)
            .fold(held_room, f32::max);
        self.drift = 0.0;
        self.cramped = if widest < AIM_BOXED {
            self.cramped.saturating_add(1)
        } else {
            0
        };
        if self.boxed() {
            return;
        }
        let wanted = (distance + AIM_BUFFER).min(widest - AIM_MARGIN);
        if held_room < wanted {
            self.goal = escape(&columns, wanted).unwrap_or(held);
            return;
        }
        let score = |view: Vec2| room(view) - (view.y - FOLLOW_REST_PITCH).abs() * AIM_PITCH_COST;
        let [low, .., high] = AIM_PITCHES;
        let floor = score(held) + AIM_MARGIN;
        self.goal = [Vec2::NEG_X, Vec2::X, Vec2::NEG_Y, Vec2::Y]
            .map(|side| {
                let view = held + side * AIM_NUDGE;
                view.with_y(view.y.clamp(low, high))
            })
            .into_iter()
            .map(|view| (view, score(view)))
            .filter(|&(_, value)| value > floor)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(held, |(view, _)| view);
        let heading = self.heading;
        let turn = wrap((-heading.x).atan2(heading.z) - self.goal.x);
        let behind = (turn.abs() - AIM_BEHIND_SLACK).max(0.0);
        let behind = behind * (1.0 + behind);
        let ahead = self.goal + Vec2::X * turn.signum() * AIM_NUDGE;
        if room(ahead) >= wanted {
            self.drift = turn.signum() * behind * AIM_BEHIND_RATE * heading.length() / AIM_HEADING;
        }
    }

    pub(super) fn boxed(&self) -> bool {
        self.cramped >= AIM_BOXED_SCANS
    }
}

pub(super) fn clearance(
    from: Vec3,
    direction: Vec3,
    distance: f32,
    solid: &impl Fn(IVec3) -> bool,
) -> f32 {
    let steps = (distance / CLEAR_STEP) as u32;
    (1..=steps)
        .map(|step| step as f32 * CLEAR_STEP)
        .find(|&travel| solid((from + direction * travel).floor().as_ivec3()))
        .map_or(distance, |travel| (travel - CAMERA_MARGIN).max(0.0))
}

fn escape(columns: &[(Vec2, f32)], wanted: f32) -> Option<Vec2> {
    let clockwise = route(columns.iter().copied(), wanted);
    let counter = route(
        columns
            .iter()
            .rev()
            .map(|&(view, room)| (view - Vec2::X * TAU, room)),
        wanted,
    );
    [clockwise, counter]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.2.cmp(&a.2)))
        .map(|(view, ..)| view)
}

fn route(path: impl Iterator<Item = (Vec2, f32)>, enough: f32) -> Option<(Vec2, f32, usize)> {
    path.scan(f32::INFINITY, |worst, (view, room)| {
        *worst = room.min(*worst);
        Some((view, *worst, room))
    })
    .enumerate()
    .find_map(|(steps, (view, worst, room))| (room >= enough).then_some((view, worst, steps)))
}

fn smooth_damp(from: Vec2, to: Vec2, velocity: &mut Vec2, time: f32, dt: f32) -> Vec2 {
    let omega = 2.0 / time;
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let gap = from - to;
    let pull = (*velocity + gap * omega) * dt;
    *velocity = (*velocity - pull * omega) * decay;
    to + (gap + pull) * decay
}

pub(super) fn standoff(
    from: Vec3,
    direction: Vec3,
    distance: f32,
    through: f32,
    solid: &impl Fn(IVec3) -> bool,
) -> f32 {
    let blocked = |point: Vec3| solid(point.floor().as_ivec3());
    let mut buried = 0.0;
    let mut settled = 0.0;
    for travel in (1..=(distance / CLEAR_STEP) as u32).map(|step| step as f32 * CLEAR_STEP) {
        let point = from + direction * travel;
        if blocked(point) {
            buried += CLEAR_STEP;
            if buried > through {
                break;
            }
        } else if !AROUND
            .iter()
            .any(|&side| blocked(point + side * CAMERA_MARGIN))
        {
            settled = travel;
        }
    }
    settled
}
