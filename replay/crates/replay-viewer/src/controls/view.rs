use glam::{IVec3, Quat, Vec3};
use replay_render::Camera;
use replay_sim::wrap;

use super::{FOLLOW_REST_DISTANCE, FOV};

const MAP_PITCH: [f32; 2] = [-70f32.to_radians(), 89f32.to_radians()];

pub struct Area {
    pub(super) center: Vec3,
    pub(super) radius: f32,
    pub(super) corners: Vec<Vec3>,
}

pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Clone, Copy)]
pub(super) struct View {
    pub(super) position: Vec3,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Orbit {
    pub(super) target: Vec3,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) distance: f32,
    pub(super) reach: f32,
}

pub(super) struct Glide {
    pub(super) from: Camera,
    pub(super) elapsed: f32,
    pub(super) duration: f32,
}

impl Area {
    pub fn new(sections: impl Iterator<Item = IVec3>) -> Area {
        let corners: Vec<Vec3> = sections
            .flat_map(|section| {
                (0..8).map(move |corner| {
                    ((section + IVec3::new(corner & 1, corner >> 1 & 1, corner >> 2)) * 16)
                        .as_vec3()
                })
            })
            .collect();
        let min = corners.iter().copied().reduce(Vec3::min);
        let max = corners.iter().copied().reduce(Vec3::max);
        let (center, radius) = min.zip(max).map_or((Vec3::ZERO, 80.0), |(min, max)| {
            let size = max - min;
            ((min + max) / 2.0, (size.x.hypot(size.z) / 2.0).max(24.0))
        });
        Area {
            center,
            radius,
            corners,
        }
    }

    pub(super) fn distance(&self, aspect: f32) -> f32 {
        let half = (FOV / 2.0).min(((FOV / 2.0).tan() * aspect).atan());
        self.radius / half.sin() * 0.8
    }
}

impl Ray {
    pub fn at(&self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }

    pub(super) fn plane(&self, height: f32) -> Option<Vec3> {
        let distance = (height - self.origin.y) / self.direction.y;
        (distance.is_finite() && distance >= 0.0).then(|| self.at(distance))
    }
}

impl View {
    pub(super) fn of(camera: Camera) -> View {
        View {
            position: camera.position,
            yaw: camera.yaw,
            pitch: camera.pitch,
        }
    }

    pub(super) fn looking(position: Vec3, direction: Vec3) -> View {
        let direction = direction.normalize();
        View {
            position,
            yaw: (-direction.x).atan2(direction.z),
            pitch: (-direction.y).asin(),
        }
    }

    pub(super) fn camera(&self, fov_y: f32) -> Camera {
        Camera {
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            fov_y,
        }
    }

    pub(super) fn forward(&self) -> Vec3 {
        forward(self.yaw, self.pitch)
    }

    fn rotation(&self) -> Quat {
        Quat::from_rotation_y(-self.yaw) * Quat::from_rotation_x(self.pitch)
    }

    pub(super) fn orbit(&mut self, pivot: Vec3, yaw: f32, pitch: f32) {
        let before = self.rotation().inverse();
        self.yaw += yaw;
        self.pitch = (self.pitch + pitch).clamp(MAP_PITCH[0], MAP_PITCH[1]);
        self.position = pivot + self.rotation() * before * (self.position - pivot);
    }
}

impl Orbit {
    pub(super) fn holding(camera: Camera) -> Orbit {
        let distance = FOLLOW_REST_DISTANCE;
        Orbit {
            target: camera.position + forward(camera.yaw, camera.pitch) * distance,
            yaw: camera.yaw,
            pitch: camera.pitch,
            distance,
            reach: distance,
        }
    }

    pub(super) fn camera(&self) -> Camera {
        Camera {
            position: self.target - forward(self.yaw, self.pitch) * self.reach,
            yaw: self.yaw,
            pitch: self.pitch,
            fov_y: FOV,
        }
    }
}

impl Glide {
    pub(super) fn blend(&self, goal: Camera) -> Camera {
        let linear = (self.elapsed / self.duration).min(1.0);
        let k = if linear < 0.5 {
            4.0 * linear.powi(3)
        } else {
            1.0 - (2.0 - 2.0 * linear).powi(3) / 2.0
        };
        let from = self.from;
        Camera {
            position: from.position.lerp(goal.position, k),
            yaw: from.yaw + wrap(goal.yaw - from.yaw) * k,
            pitch: from.pitch + (goal.pitch - from.pitch) * k,
            fov_y: from.fov_y + (goal.fov_y - from.fov_y) * k,
        }
    }
}

pub(super) fn forward(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

pub(super) fn right(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.cos(), 0.0, -yaw.sin())
}
