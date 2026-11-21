mod follow;
mod view;

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use follow::{Follow, clearance, standoff};
use glam::{IVec3, Vec2, Vec3};
use replay_render::Camera;
use replay_sim::{PlayerFrame, wrap};
use view::{Glide, Orbit, View, forward, right};

pub use view::{Area, Ray};

const FOV: f32 = 60f32.to_radians();
const POV_FOV: f32 = 70f32.to_radians();
const ORBIT_SPEED: f32 = 0.9 * TAU;
const FOLLOW_ORBIT_SPEED: f32 = TAU;
const FOLLOW_DAMPING: f32 = 0.08;
const LOOK_SPEED: f32 = 0.00225;
const ZOOM_SPEED: f32 = 0.0022;
const PINCH_ZOOM_SPEED: f32 = 0.01;
const ZOOM_EASE: f32 = 14.0;
const MIN_ZOOM_STEP: f32 = 0.8;
const ZOOM_RANGE: [f32; 2] = [12.0, 400.0];
const MAX_PAN: f32 = 400.0;
const TWIST_START: f32 = 0.14;
const PINCH_RANGE: [f32; 2] = [2.0, 600.0];
const NEAREST_ANCHOR: f32 = 8.0;
const FALLBACK_DEPTH: f32 = 60.0;
const MAP_DIRECTION: Vec3 = Vec3::new(1.0, 1.2, 1.0);
const FRAME_MARGIN: f32 = 0.92;
const FOLLOW_PITCH: [f32; 2] = [0.005 * PI, 89f32.to_radians()];
const FOLLOW_DISTANCE: [f32; 2] = [1.5, 32.0];
const FOLLOW_REST_DISTANCE: f32 = 7.5;
const FOLLOW_REST_PITCH: f32 = 20f32.to_radians();
const FOLLOW_HEIGHT: f32 = 1.2;
const FOLLOW_SNAP: f32 = 16.0;
const VEIL_EASE: f32 = 8.0;
const VEIL_WIDE: f32 = 1.6;
const FOLLOW_TRACK: Vec3 = Vec3::new(10.0, 5.0, 10.0);
const FOLLOW_PROBE: [Vec2; 4] = [
    Vec2::new(-8f32.to_radians(), 0.0),
    Vec2::new(8f32.to_radians(), 0.0),
    Vec2::new(0.0, -8f32.to_radians()),
    Vec2::new(0.0, 8f32.to_radians()),
];
const FOLLOW_JOLT: f32 = 0.5;
const FOLLOW_EASE: [f32; 2] = [8.0, 3.0];
const FOLLOW_AUTO_EASE: [f32; 2] = [3.0, 1.2];
const CAMERA_MARGIN: f32 = 0.3;
const SEE_THROUGH: f32 = 2.0;
const SEE_THROUGH_BOXED: f32 = 8.0;
const INSIDE_SUBJECT: f32 = 1.1;
pub const EYE_HEIGHT: f32 = 1.62;
const FLY_SPEED: f32 = 10.92;
const FLY_SPEED_RANGE: [f32; 2] = [1.0, FLY_SPEED * 4.0];
const FLY_CLIMB: f32 = 7.5 / FLY_SPEED;
const FLY_DRAG: Vec3 = Vec3::new(1.886, 10.217, 1.886);
const SNEAK_EYE_HEIGHT: f32 = 1.54;
const POV_SNAP: f32 = 0.6;
const GLIDE: f32 = 0.7;
const REGLIDE: f32 = 0.45;
const FOCUS_GLIDE: f32 = 0.55;
const FOCUS_REACH: f32 = 36.0;

const FORWARD: &[&str] = &["KeyW", "ArrowUp"];
const BACK: &[&str] = &["KeyS", "ArrowDown"];
const LEFT: &[&str] = &["KeyA"];
const RIGHT: &[&str] = &["KeyD"];
const JUMP: &[&str] = &["Space"];
const SHIFT: &[&str] = &["ShiftLeft", "ShiftRight"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Map,
    Follow,
    Pov,
    Fly,
}

pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

#[derive(Clone, Copy)]
enum Gesture {
    Orbit(Vec3),
    Pan(Vec3),
    Pinch(Pinch),
}

#[derive(Clone, Copy)]
struct Pinch {
    spot: Vec3,
    spread: f32,
    angle: f32,
    twisted: f32,
    turning: bool,
}

pub struct Controls {
    mode: Mode,
    shown: Camera,
    glide: Option<Glide>,
    reduced_motion: bool,
    viewport: Vec2,
    inset: Vec2,
    area: Area,
    map: View,
    gesture: Option<Gesture>,
    touches: Vec<(i32, Vec2)>,
    fingers_moved: bool,
    zooming: Vec3,
    follow: Follow,
    pov: View,
    fly: View,
    fly_speed: f32,
    fly_velocity: Vec3,
    held: HashSet<String>,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Map, Mode::Follow, Mode::Pov, Mode::Fly];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Map => "map",
            Mode::Follow => "follow",
            Mode::Pov => "pov",
            Mode::Fly => "fly",
        }
    }
}

impl Controls {
    pub fn new(width: f32, height: f32) -> Controls {
        let view = View {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
        };
        Controls {
            mode: Mode::Map,
            shown: view.camera(FOV),
            glide: None,
            reduced_motion: false,
            viewport: Vec2::new(width, height).max(Vec2::ONE),
            inset: Vec2::ZERO,
            area: Area::new(std::iter::empty()),
            map: view,
            gesture: None,
            touches: Vec::new(),
            fingers_moved: false,
            zooming: Vec3::ZERO,
            follow: Follow::new(Orbit::holding(view.camera(FOV)), 0.0),
            pov: view,
            fly: view,
            fly_speed: FLY_SPEED,
            fly_velocity: Vec3::ZERO,
            held: HashSet::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn shown(&self) -> Camera {
        self.shown
    }

    pub fn auto(&self) -> bool {
        self.mode == Mode::Follow && self.follow.auto
    }

    pub fn take_over(&mut self) {
        self.follow.auto = false;
        self.follow.turn = Vec2::ZERO;
    }

    pub fn veil(&self) -> f32 {
        if self.mode == Mode::Follow {
            self.follow.veil
        } else {
            0.0
        }
    }

    pub fn inside_subject(&self) -> bool {
        match self.mode {
            Mode::Pov => true,
            Mode::Follow => self.follow.orbit.reach < INSIDE_SUBJECT,
            Mode::Map | Mode::Fly => false,
        }
    }

    pub fn fly_speed(&self) -> f32 {
        self.fly_speed
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Vec2::new(width, height).max(Vec2::ONE);
    }

    pub fn set_insets(&mut self, inset: Vec2) {
        self.inset = inset;
    }

    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    pub fn frame_map(&mut self, area: Area) {
        self.area = area;
        self.map = self.home();
        self.mode = Mode::Map;
        self.glide = None;
        self.gesture = None;
        self.zooming = Vec3::ZERO;
        self.shown = self.camera();
    }

    pub fn fog(&self) -> [f32; 2] {
        let reach = self.reach();
        [
            reach + self.area.radius * 0.5,
            reach + self.area.radius * 3.0,
        ]
    }

    pub fn set_mode(&mut self, mode: Mode, subject: Option<&PlayerFrame>) {
        match mode {
            Mode::Map if self.mode == Mode::Map => self.map = self.home(),
            Mode::Follow => {
                let orbit = subject.map_or(Orbit::holding(self.shown), |frame| Orbit {
                    target: frame.position + Vec3::Y * FOLLOW_HEIGHT,
                    yaw: self.shown.yaw,
                    pitch: FOLLOW_REST_PITCH,
                    distance: FOLLOW_REST_DISTANCE,
                    reach: FOLLOW_REST_DISTANCE,
                });
                self.follow = Follow::new(orbit, self.follow.veil);
            }
            Mode::Pov if subject.is_none() => self.pov = View::of(self.shown),
            Mode::Fly => {
                if self.mode != Mode::Fly {
                    self.fly = View::of(self.shown);
                    self.fly_velocity = Vec3::ZERO;
                }
                if self.mode == Mode::Pov {
                    self.fly.position += Vec3::Y * 0.6 - self.fly.forward() * 2.5;
                }
                self.mode = Mode::Fly;
                self.glide = None;
                return;
            }
            _ => {}
        }
        let duration = if mode == self.mode { REGLIDE } else { GLIDE };
        self.mode = mode;
        self.glide_for(duration);
    }

    pub fn ray(&self, x: f32, y: f32) -> Ray {
        let camera = self.camera();
        let ndc = Vec2::new(x, y) / self.viewport * 2.0 - 1.0;
        let half = (camera.fov_y / 2.0).tan();
        let ahead = forward(camera.yaw, camera.pitch);
        let side = right(camera.yaw);
        let up = side.cross(ahead);
        Ray {
            origin: camera.position,
            direction: (ahead + side * ndc.x * half * self.aspect() - up * ndc.y * half)
                .normalize(),
        }
    }

    pub fn screen(&self, point: Vec3) -> Option<Vec2> {
        let ndc = self
            .shown
            .view_proj(self.aspect())
            .project_point3(point - self.shown.position);
        (ndc.z > 0.0).then(|| (ndc.truncate() * Vec2::new(0.5, -0.5) + 0.5) * self.viewport)
    }

    pub fn listener(&self) -> Listener {
        let camera = self.shown;
        let ahead = forward(camera.yaw, camera.pitch);
        Listener {
            position: camera.position,
            forward: ahead,
            up: right(camera.yaw).cross(ahead),
        }
    }

    pub fn press(&mut self, at: Vec2, pan: bool, pick: impl Fn(&Ray) -> Option<Vec3>) {
        if self.mode != Mode::Map {
            return;
        }
        self.settle();
        let point = if pan { at } else { self.center() };
        let ray = self.ray(point.x, point.y);
        let anchor = self.anchor(&ray, pick(&ray));
        self.gesture = Some(if pan {
            Gesture::Pan(anchor)
        } else {
            Gesture::Orbit(anchor)
        });
    }

    pub fn lift(&mut self) {
        self.gesture = None;
    }

    pub fn touch_down(&mut self, id: i32, at: Vec2, pick: impl Fn(&Ray) -> Option<Vec3>) {
        if self.mode == Mode::Map {
            self.settle();
        }
        self.touches.push((id, at));
        let anchor = |point: Vec2| {
            let ray = self.ray(point.x, point.y);
            self.anchor(&ray, pick(&ray))
        };
        self.gesture = match self.touches[..] {
            [_] => Some(Gesture::Orbit(anchor(self.center()))),
            [(_, a), (_, b)] => Some(Gesture::Pinch(Pinch {
                spot: anchor((a + b) / 2.0),
                spread: a.distance(b).max(1.0),
                angle: (b - a).to_angle(),
                twisted: 0.0,
                turning: false,
            })),
            _ => None,
        };
    }

    pub fn touch_move(&mut self, id: i32, at: Vec2) {
        let Some((_, last)) = self.touches.iter_mut().find(|(known, _)| *known == id) else {
            return;
        };
        let delta = at - std::mem::replace(last, at);
        match self.touches.len() {
            1 => self.drag(delta.x, delta.y, &self.ray(at.x, at.y)),
            2 => self.fingers_moved = true,
            _ => {}
        }
    }

    pub fn touch_up(&mut self, id: i32) {
        let count = self.touches.len();
        self.touches.retain(|(known, _)| *known != id);
        if self.touches.len() < count {
            self.gesture = None;
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32, ray: &Ray) {
        let turn = ORBIT_SPEED / self.viewport.y;
        match (self.mode, self.gesture) {
            (Mode::Map, Some(Gesture::Orbit(pivot))) => {
                self.map.orbit(pivot, dx * turn, dy * turn);
                self.limit();
            }
            (Mode::Map, Some(Gesture::Pan(grab))) => {
                let shift = ray.plane(grab.y).map(|hit| grab - hit);
                if let Some(shift) = shift.filter(|shift| shift.length() <= MAX_PAN) {
                    self.map.position += shift;
                    self.limit();
                }
            }
            (Mode::Follow, _) => {
                self.take_over();
                self.follow.spin += Vec2::new(dx, dy) * FOLLOW_ORBIT_SPEED / self.viewport.y;
            }
            (Mode::Fly, _) => {
                self.fly.yaw += dx * LOOK_SPEED;
                self.fly.pitch =
                    (self.fly.pitch + dy * LOOK_SPEED).clamp(0.01 - FRAC_PI_2, FRAC_PI_2 - 0.01);
            }
            _ => {}
        }
    }

    pub fn wheel(&mut self, delta: f32, pinch: bool, ray: &Ray, block: Option<Vec3>) {
        match self.mode {
            Mode::Map => {
                let speed = if pinch { PINCH_ZOOM_SPEED } else { ZOOM_SPEED };
                let amount = (-delta * speed).clamp(-1.0, 1.0);
                let ground = ray.plane(self.area.center.y);
                let known = [block, ground]
                    .into_iter()
                    .flatten()
                    .map(|hit| hit.distance(ray.origin));
                let step = if amount > 0.0 {
                    let distance = known
                        .reduce(f32::min)
                        .map_or(FALLBACK_DEPTH, |distance| distance.min(ZOOM_RANGE[1]));
                    (distance * (1.0 - (-amount).exp()))
                        .max(MIN_ZOOM_STEP * (amount * 5.0).min(1.0))
                } else {
                    let distance = known.reduce(f32::max).map_or(FALLBACK_DEPTH, |distance| {
                        distance.clamp(ZOOM_RANGE[0], ZOOM_RANGE[1])
                    });
                    distance * (1.0 - (-amount).exp())
                };
                self.zooming += ray.direction * step;
            }
            Mode::Follow => {
                let step = (delta * ZOOM_SPEED).clamp(-1.0, 1.0);
                self.follow.orbit.distance = (self.follow.orbit.distance * step.exp())
                    .clamp(FOLLOW_DISTANCE[0], FOLLOW_DISTANCE[1]);
            }
            Mode::Fly => {
                let factor = if delta < 0.0 { 1.25 } else { 0.8 };
                self.fly_speed =
                    (self.fly_speed * factor).clamp(FLY_SPEED_RANGE[0], FLY_SPEED_RANGE[1]);
            }
            Mode::Pov => {}
        }
    }

    pub fn focus(&mut self, point: Vec3) {
        if self.mode != Mode::Map {
            return;
        }
        let offset = (self.map.position - point).clamp_length_max(FOCUS_REACH);
        self.map = View::looking(point + offset, -offset);
        self.glide_for(FOCUS_GLIDE);
    }

    pub fn key(&mut self, code: String, down: bool) {
        if down {
            self.held.insert(code);
        } else {
            self.held.remove(&code);
        }
    }

    pub fn release(&mut self) {
        self.held.clear();
        self.touches.clear();
        self.gesture = None;
    }

    pub fn update(
        &mut self,
        dt: f32,
        subject: Option<&PlayerFrame>,
        solid: impl Fn(IVec3) -> bool,
    ) -> Camera {
        if std::mem::take(&mut self.fingers_moved)
            && let [(_, a), (_, b)] = self.touches[..]
            && let Some(Gesture::Pinch(pinch)) = self.gesture
        {
            self.pinch(pinch, a, b);
        }
        match self.mode {
            Mode::Map => self.update_map(dt),
            Mode::Fly => self.update_fly(dt),
            Mode::Follow => self.update_follow(dt, subject, &solid),
            Mode::Pov => self.update_pov(dt, subject),
        }
        let goal = self.camera();
        self.shown = self.glide.as_mut().map_or(goal, |glide| {
            glide.elapsed += dt;
            glide.blend(goal)
        });
        self.glide.take_if(|glide| glide.elapsed >= glide.duration);
        self.shown
    }

    fn update_map(&mut self, dt: f32) {
        let boost = if self.holds(SHIFT) { 2.5 } else { 1.0 };
        let height = (self.map.position.y - self.area.center.y).abs();
        let motion = self.steering(self.map.yaw);
        let zoom = self.zooming * (1.0 - (-ZOOM_EASE * dt).exp());
        self.zooming -= zoom;
        self.map.position +=
            zoom + motion.normalize_or_zero() * (height * 1.2).max(10.0) * boost * dt;
        self.limit();
    }

    fn update_fly(&mut self, dt: f32) {
        let goal = self
            .steering(self.fly.yaw)
            .normalize_or_zero()
            .with_y(self.axis(JUMP, SHIFT) * FLY_CLIMB)
            * self.fly_speed;
        let keep = (-FLY_DRAG * dt).exp();
        self.fly_velocity = goal + (self.fly_velocity - goal) * keep;
        self.fly.position += self.fly_velocity * dt;
    }

    fn update_follow(
        &mut self,
        dt: f32,
        subject: Option<&PlayerFrame>,
        solid: &impl Fn(IVec3) -> bool,
    ) {
        let follow = &mut self.follow;
        let spin = follow.spin * (1.0 - (1.0 - FOLLOW_DAMPING).powf(dt * 60.0));
        follow.spin -= spin;
        follow.orbit.yaw += spin.x;
        follow.orbit.pitch = (follow.orbit.pitch + spin.y).clamp(FOLLOW_PITCH[0], FOLLOW_PITCH[1]);
        if let Some(frame) = subject {
            let goal = frame.position + Vec3::Y * FOLLOW_HEIGHT;
            follow.orbit.target = if follow.orbit.target.distance(goal) > FOLLOW_SNAP {
                goal
            } else {
                let ease = Vec3::ONE - (-FOLLOW_TRACK * dt).exp();
                follow.orbit.target + (goal - follow.orbit.target) * ease
            };
            if follow.auto {
                follow.aim(dt, goal, solid);
            }
        }
        let boxed = follow.boxed();
        let Orbit {
            target,
            yaw,
            pitch,
            distance,
            reach,
        } = follow.orbit;
        let open = |tilt: Vec2| {
            standoff(
                target,
                -forward(yaw + tilt.x, pitch + tilt.y),
                distance,
                if boxed {
                    SEE_THROUGH_BOXED
                } else {
                    SEE_THROUGH
                },
                solid,
            )
        };
        let hard = open(Vec2::ZERO);
        if hard < reach - FOLLOW_JOLT {
            follow.scan = 0.0;
        }
        let soft = FOLLOW_PROBE.map(open).into_iter().fold(hard, f32::min);
        let [pull, release] = if follow.auto {
            FOLLOW_AUTO_EASE
        } else {
            FOLLOW_EASE
        };
        let rate = if soft < reach { pull } else { release };
        follow.orbit.reach = (reach + (soft - reach) * (1.0 - (-rate * dt).exp())).min(hard);
        let sight = clearance(target, -forward(yaw, pitch), follow.orbit.reach, solid);
        let veiled = match (sight < follow.orbit.reach - CAMERA_MARGIN, boxed) {
            (false, _) => 0.0,
            (true, false) => 1.0,
            (true, true) => VEIL_WIDE,
        };
        follow.veil += (veiled - follow.veil) * (1.0 - (-VEIL_EASE * dt).exp());
    }

    fn update_pov(&mut self, dt: f32, subject: Option<&PlayerFrame>) {
        let Some(frame) = subject else {
            return;
        };
        let eye = if frame.sneaking() {
            SNEAK_EYE_HEIGHT
        } else {
            EYE_HEIGHT
        };
        let turn = Vec2::new(
            wrap(frame.head_yaw - self.pov.yaw),
            frame.pitch - self.pov.pitch,
        );
        let ease = if self.glide.is_some() || turn.length() > POV_SNAP {
            1.0
        } else {
            1.0 - (-30.0 * dt).exp()
        };
        self.pov = View {
            position: frame.position + Vec3::Y * eye,
            yaw: self.pov.yaw + turn.x * ease,
            pitch: self.pov.pitch + turn.y * ease,
        };
    }

    fn camera(&self) -> Camera {
        match self.mode {
            Mode::Map => self.map.camera(FOV),
            Mode::Follow => self.follow.orbit.camera(),
            Mode::Pov => self.pov.camera(POV_FOV),
            Mode::Fly => self.fly.camera(FOV),
        }
    }

    fn pinch(&mut self, mut pinch: Pinch, a: Vec2, b: Vec2) {
        let spread = a.distance(b).max(1.0);
        let scale = pinch.spread / spread;
        pinch.spread = spread;
        match self.mode {
            Mode::Map => {
                let spot = pinch.spot;
                let middle = (a + b) / 2.0;
                if let Some(hit) = self.ray(middle.x, middle.y).plane(spot.y) {
                    let most = 0.5 * self.map.position.distance(spot);
                    self.map.position += (spot - hit).clamp_length_max(most);
                }
                let angle = (b - a).to_angle();
                let twist = wrap(angle - pinch.angle);
                pinch.angle = angle;
                pinch.twisted += twist;
                pinch.turning |= pinch.twisted.abs() > TWIST_START;
                if pinch.turning {
                    self.map.orbit(spot, twist, 0.0);
                }
                let offset = self.map.position - spot;
                let length = offset.length();
                let reach =
                    (length * scale).clamp(length.min(PINCH_RANGE[0]), length.max(PINCH_RANGE[1]));
                self.map.position = spot + offset.normalize_or_zero() * reach;
                self.limit();
            }
            Mode::Follow => {
                self.follow.orbit.distance = (self.follow.orbit.distance * scale)
                    .clamp(FOLLOW_DISTANCE[0], FOLLOW_DISTANCE[1]);
            }
            Mode::Pov | Mode::Fly => {}
        }
        self.gesture = Some(Gesture::Pinch(pinch));
    }

    fn settle(&mut self) {
        if self.glide.take().is_some() {
            self.map = View::of(self.shown);
        }
        self.zooming = Vec3::ZERO;
    }

    fn glide_for(&mut self, duration: f32) {
        self.gesture = None;
        self.zooming = Vec3::ZERO;
        self.glide = (!self.reduced_motion).then_some(Glide {
            from: self.shown,
            elapsed: 0.0,
            duration,
        });
    }

    fn home(&self) -> View {
        let direction = MAP_DIRECTION.normalize();
        let aspect = self.aspect();
        let free = (Vec2::ONE - self.inset / self.viewport).max(Vec2::splat(0.4));
        let goal = (free - Vec2::ONE) * Vec2::new(1.0, -1.0);
        let half = free * FRAME_MARGIN;
        let side = right(View::looking(Vec3::ZERO, -direction).yaw);
        let up = side.cross(-direction);
        let mut aim = self.area.center;
        let mut distance = self.area.distance(aspect);
        for _ in 0..6 {
            let view = View::looking(aim + direction * distance, -direction);
            let view_proj = view.camera(FOV).view_proj(aspect);
            let Some((low, high)) = self
                .area
                .corners
                .iter()
                .map(|&corner| view_proj.project_point3(corner - view.position).truncate())
                .fold(None, |bounds: Option<(Vec2, Vec2)>, point| {
                    Some(bounds.map_or((point, point), |(low, high)| {
                        (low.min(point), high.max(point))
                    }))
                })
            else {
                break;
            };
            let scale = ((high - low) / (2.0 * half)).max_element().clamp(0.5, 2.0);
            let shift = (low + high) / 2.0 / scale - goal;
            distance *= scale;
            aim += (side * shift.x * aspect + up * shift.y) * (FOV / 2.0).tan() * distance;
        }
        View::looking(aim + direction * distance, -direction)
    }

    fn center(&self) -> Vec2 {
        (self.viewport - self.inset).max(Vec2::ZERO) / 2.0
    }

    fn anchor(&self, ray: &Ray, block: Option<Vec3>) -> Vec3 {
        block.unwrap_or_else(|| {
            let depth = (self.area.center - ray.origin).dot(ray.direction);
            ray.at(depth.max(NEAREST_ANCHOR))
        })
    }

    fn limit(&mut self) {
        let offset = self.map.position - self.area.center;
        self.map.position = self.area.center + offset.clamp_length_max(self.reach() * 1.25);
    }

    fn reach(&self) -> f32 {
        self.area.distance(self.aspect()) * 1.8
    }

    fn aspect(&self) -> f32 {
        self.viewport.x / self.viewport.y
    }

    fn steering(&self, yaw: f32) -> Vec3 {
        forward(yaw, 0.0) * self.axis(FORWARD, BACK) + right(yaw) * self.axis(RIGHT, LEFT)
    }

    fn holds(&self, codes: &[&str]) -> bool {
        codes.iter().any(|&code| self.held.contains(code))
    }

    fn axis(&self, positive: &[&str], negative: &[&str]) -> f32 {
        f32::from(i8::from(self.holds(positive)) - i8::from(self.holds(negative)))
    }
}
