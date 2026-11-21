use std::f32::consts::{PI, TAU};

use glam::{Vec3, Vec4};

const DAY_TICKS: f32 = 24000.0;
const NOON: u16 = 6000;
const LEVELS: u8 = 16;

#[derive(Clone, Copy, Debug)]
pub struct Daylight {
    sun: f32,
}

impl Default for Daylight {
    fn default() -> Daylight {
        Daylight::at(NOON)
    }
}

impl Daylight {
    pub fn at(tick: u16) -> Daylight {
        let turned = (f32::from(tick) / DAY_TICKS - 0.25).rem_euclid(1.0);
        let eased = 1.0 - ((turned * PI).cos() + 1.0) / 2.0;
        let height = ((turned + (eased - turned) / 3.0) * TAU).cos();
        Daylight {
            sun: (1.0 - (1.0 - (height * 2.0 + 0.2)).clamp(0.0, 1.0)) * 0.8 + 0.2,
        }
    }

    pub fn tint(self, packed: u8) -> u32 {
        let [red, green, blue] = self
            .color(packed >> 4, packed & 15)
            .to_array()
            .map(|channel| (channel * 255.0).round() as u8);
        u32::from_be_bytes([red, green, blue, u8::MAX])
    }

    pub(crate) fn lightmap(self) -> [Vec4; 256] {
        std::array::from_fn(|at| {
            let at = at as u8;
            self.color(at / LEVELS, at % LEVELS).extend(1.0)
        })
    }

    fn color(self, sky: u8, block: u8) -> Vec3 {
        let brightness = |level: u8| {
            let dimness = 1.0 - f32::from(level) / 15.0;
            (1.0 - dimness) / (dimness * 3.0 + 1.0)
        };
        let sky = brightness(sky) * (self.sun * 0.95 + 0.05);
        let block = brightness(block) * 1.5;
        let warm = sky * (self.sun * 0.65 + 0.35);
        let mixed = Vec3::new(
            warm + block,
            warm + block * ((block * 0.6 + 0.4) * 0.6 + 0.4),
            sky + block * (block * block * 0.6 + 0.4),
        );
        let settle = |color: Vec3| (color * 0.96 + 0.03).clamp(Vec3::ZERO, Vec3::ONE);
        let settled = settle(mixed);
        settle(Vec3::ONE - (Vec3::ONE - settled).powf(4.0))
    }
}
