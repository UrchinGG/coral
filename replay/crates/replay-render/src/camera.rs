use std::f32::consts::PI;

use glam::camera::rh::proj::directx::perspective_infinite_reverse;
use glam::{Mat3, Mat4, Vec3, Vec4};

const NEAR: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
}

impl Camera {
    pub fn billboard(&self) -> Mat3 {
        Mat3::from_rotation_y(PI - self.yaw) * Mat3::from_rotation_x(-self.pitch)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        perspective_infinite_reverse(self.fov_y, aspect, NEAR)
            * Mat4::from_rotation_x(self.pitch)
            * Mat4::from_rotation_y(self.yaw - PI)
    }
}

pub(crate) struct Frustum([Vec4; 5]);

impl Frustum {
    pub(crate) fn new(view_proj: Mat4) -> Self {
        let row = |i| view_proj.row(i);
        Self([
            row(3) + row(0),
            row(3) - row(0),
            row(3) + row(1),
            row(3) - row(1),
            row(3) - row(2),
        ])
    }

    pub(crate) fn intersects(&self, min: Vec3, max: Vec3) -> bool {
        self.0.iter().all(|plane| {
            let normal = plane.truncate();
            let farthest = Vec3::select(normal.cmpgt(Vec3::ZERO), max, min);
            normal.dot(farthest) + plane.w >= 0.0
        })
    }
}
