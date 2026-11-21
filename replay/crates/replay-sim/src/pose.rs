use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Mat4, Vec3, vec3};

use crate::{PlayerFrame, TICK_MS, wrap};

const FEET: f32 = 24.125;
pub const SNEAK_DROP: f32 = 0.125;
const SNEAK_SHIFT: f32 = 3.2;
const HAND: Vec3 = vec3(-1.0, 7.0, 1.0);
const SNEAK_HAND: f32 = 3.25;
const BLOCK: f32 = 16.0;
const DEATH_TICKS: f32 = 20.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub root: Mat4,
    pub head: Mat4,
    pub body: Mat4,
    pub right_arm: Mat4,
    pub left_arm: Mat4,
    pub right_leg: Mat4,
    pub left_leg: Mat4,
    pub held_item: Mat4,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Empty,
    Holding,
    Blocking,
    Aiming,
}

#[derive(Clone, Copy, Default)]
struct Part {
    pivot: Vec3,
    angles: Vec3,
}

impl Pose {
    pub(crate) fn new(frame: &PlayerFrame, hand: Hand, t: f32) -> Pose {
        let sneaking = frame.sneaking();
        let limb = frame.limb_swing * 0.6662;
        let amount = frame.limb_amount.min(1.0);
        let (leg_y, leg_z) = if sneaking { (9.0, 4.0) } else { (12.0, 0.1) };
        let head = Part {
            pivot: Vec3::Y * if sneaking { 1.0 } else { 0.0 },
            angles: vec3(frame.pitch, wrap(frame.head_yaw - frame.body_yaw), 0.0),
        };
        let mut body = Part::default();
        let mut right_arm = Part::new(vec3(-5.0, 2.0, 0.0), (limb + PI).cos() * amount);
        let mut left_arm = Part::new(vec3(5.0, 2.0, 0.0), limb.cos() * amount);
        let right_leg = Part::new(vec3(-1.9, leg_y, leg_z), limb.cos() * 1.4 * amount);
        let left_leg = Part::new(vec3(1.9, leg_y, leg_z), (limb + PI).cos() * 1.4 * amount);

        match hand {
            Hand::Empty => {}
            Hand::Holding | Hand::Aiming => {
                right_arm.angles.x = right_arm.angles.x * 0.5 - PI / 10.0
            }
            Hand::Blocking => {
                right_arm.angles.x = right_arm.angles.x * 0.5 - PI * 3.0 / 10.0;
                right_arm.angles.y = -PI / 6.0;
            }
        }
        if let Some(progress) = frame.swing {
            let twist = (progress.sqrt() * TAU).sin() * 0.2;
            let arc = (progress * PI).sin();
            let eased = 1.0 - (1.0 - progress).powi(4);
            body.angles.y = twist;
            right_arm.pivot = vec3(-twist.cos() * 5.0, 2.0, twist.sin() * 5.0);
            left_arm.pivot = vec3(twist.cos() * 5.0, 2.0, -twist.sin() * 5.0);
            left_arm.angles += vec3(twist, twist, 0.0);
            right_arm.angles += vec3(
                -(eased * PI).sin() * 1.2 + arc * (head.angles.x - 0.7) * 0.75,
                twist * 3.0,
                -arc * 0.4,
            );
        }
        if sneaking {
            body.angles.x = 0.5;
            right_arm.angles.x += 0.4;
            left_arm.angles.x += 0.4;
        }
        if hand == Hand::Aiming {
            right_arm.angles = vec3(head.angles.x - FRAC_PI_2, head.angles.y - 0.1, 0.0);
            left_arm.angles = vec3(head.angles.x - FRAC_PI_2, head.angles.y + 0.5, 0.0);
        }
        let age = t / TICK_MS;
        let sway = vec3(
            (age * 0.067).sin() * 0.05,
            0.0,
            (age * 0.09).cos() * 0.05 + 0.05,
        );
        right_arm.angles += sway;
        left_arm.angles -= sway;

        let corpse = frame.death.map_or(0.0, |progress| {
            ((progress * DEATH_TICKS - 1.0) / DEATH_TICKS * 1.6)
                .clamp(0.0, 1.0)
                .sqrt()
                * FRAC_PI_2
        });
        let (drop, shift, lift) = if sneaking {
            (SNEAK_DROP, SNEAK_SHIFT, SNEAK_HAND)
        } else {
            (0.0, 0.0, 0.0)
        };
        let right_arm = right_arm.matrix();
        Pose {
            root: root(
                frame.position - Vec3::Y * drop,
                frame.body_yaw,
                corpse,
                0.9375,
            ) * Mat4::from_translation(Vec3::Y * shift),
            head: head.matrix(),
            body: body.matrix(),
            right_arm,
            left_arm: left_arm.matrix(),
            right_leg: right_leg.matrix(),
            left_leg: left_leg.matrix(),
            held_item: Mat4::from_translation(Vec3::Y * -shift)
                * held_item(right_arm * Mat4::from_translation(Vec3::Y * lift)),
        }
    }
}

impl Part {
    fn new(pivot: Vec3, pitch: f32) -> Part {
        Part {
            pivot,
            angles: Vec3::X * pitch,
        }
    }

    fn matrix(self) -> Mat4 {
        part(self.pivot, self.angles)
    }
}

pub fn part(pivot: Vec3, angles: Vec3) -> Mat4 {
    Mat4::from_translation(pivot)
        * Mat4::from_rotation_z(angles.z)
        * Mat4::from_rotation_y(angles.y)
        * Mat4::from_rotation_x(angles.x)
}

pub fn root(position: Vec3, yaw: f32, roll: f32, scale: f32) -> Mat4 {
    let scale = scale / 16.0;
    Mat4::from_translation(position)
        * Mat4::from_rotation_y(PI - yaw)
        * Mat4::from_rotation_z(roll)
        * Mat4::from_scale(vec3(-scale, -scale, scale))
        * Mat4::from_translation(Vec3::Y * -FEET)
}

pub fn held_item(right_arm: Mat4) -> Mat4 {
    right_arm * Mat4::from_translation(HAND) * Mat4::from_scale(Vec3::splat(BLOCK))
}
