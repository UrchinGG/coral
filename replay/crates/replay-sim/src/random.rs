use std::f32::consts::TAU;

use glam::{Vec3, vec3};

pub(crate) struct Random(u32);

impl Random {
    pub(crate) fn new(seed: u32) -> Random {
        Random(seed)
    }

    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut value = self.0;
        value = (value ^ value >> 15).wrapping_mul(value | 1);
        value ^= value.wrapping_add((value ^ value >> 7).wrapping_mul(value | 61));
        ((value ^ value >> 14) >> 8) as f32 / 16_777_216.0
    }

    pub(crate) fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }

    pub(crate) fn difference(&mut self) -> f32 {
        self.next() - self.next()
    }

    pub(crate) fn below(&mut self, bound: usize) -> usize {
        (self.next() * bound as f32) as usize
    }

    pub(crate) fn gaussian(&mut self) -> f32 {
        let radius = (-2.0 * self.next().max(1e-12).ln()).sqrt();
        radius * (TAU * self.next()).cos()
    }

    pub(crate) fn cube(&mut self) -> Vec3 {
        vec3(self.signed(), self.signed(), self.signed())
    }

    pub(crate) fn gaussians(&mut self) -> Vec3 {
        vec3(self.gaussian(), self.gaussian(), self.gaussian())
    }
}
