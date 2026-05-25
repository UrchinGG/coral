use glam::Vec3;

#[derive(Debug, Clone, Copy, Default)]
pub struct Rotation {
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
}

impl Rotation {
    pub const ZERO: Self = Self {
        pitch: 0.0,
        yaw: 0.0,
        roll: 0.0,
    };

    pub fn new(pitch: f32, yaw: f32, roll: f32) -> Self {
        Self { pitch, yaw, roll }
    }

    pub fn to_radians(&self) -> Vec3 {
        Vec3::new(
            self.pitch.to_radians(),
            self.yaw.to_radians(),
            self.roll.to_radians(),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub head: Rotation,
    pub body: Rotation,
    pub left_arm: Rotation,
    pub right_arm: Rotation,
    pub left_leg: Rotation,
    pub right_leg: Rotation,
}

impl Default for Pose {
    fn default() -> Self {
        Self::standing()
    }
}

impl Pose {
    pub fn standing() -> Self {
        Self {
            head: Rotation::ZERO,
            body: Rotation::ZERO,
            left_arm: Rotation::new(0.0, 0.0, 3.0),
            right_arm: Rotation::new(0.0, 0.0, -3.0),
            left_leg: Rotation::ZERO,
            right_leg: Rotation::ZERO,
        }
    }

    pub fn walking(t: f32) -> Self {
        let swing = (t * std::f32::consts::PI * 2.0).sin() * 30.0;
        Self {
            head: Rotation::ZERO,
            body: Rotation::ZERO,
            left_arm: Rotation::new(-swing, 0.0, 3.0),
            right_arm: Rotation::new(swing, 0.0, -3.0),
            left_leg: Rotation::new(swing, 0.0, 0.0),
            right_leg: Rotation::new(-swing, 0.0, 0.0),
        }
    }

    pub fn custom() -> PoseBuilder {
        PoseBuilder::default()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PoseBuilder {
    pose: Pose,
}

impl PoseBuilder {
    pub fn head(mut self, r: Rotation) -> Self {
        self.pose.head = r;
        self
    }
    pub fn body(mut self, r: Rotation) -> Self {
        self.pose.body = r;
        self
    }
    pub fn left_arm(mut self, r: Rotation) -> Self {
        self.pose.left_arm = r;
        self
    }
    pub fn right_arm(mut self, r: Rotation) -> Self {
        self.pose.right_arm = r;
        self
    }
    pub fn left_leg(mut self, r: Rotation) -> Self {
        self.pose.left_leg = r;
        self
    }
    pub fn right_leg(mut self, r: Rotation) -> Self {
        self.pose.right_leg = r;
        self
    }
    pub fn build(self) -> Pose {
        self.pose
    }
}
