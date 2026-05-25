use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn for_full_body() -> Self {
        Self {
            position: Vec3::new(18.0, 26.0, -40.0),
            target: Vec3::new(0.0, 16.0, 0.0),
            up: Vec3::Y,
            fov: 45.0,
            near: 0.1,
            far: 200.0,
        }
    }

    pub fn for_head() -> Self {
        Self {
            position: Vec3::new(-10.0, 28.0, -20.0),
            target: Vec3::new(0.0, 28.0, 0.0),
            up: Vec3::Y,
            fov: 30.0,
            near: 0.1,
            far: 200.0,
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov.to_radians(), aspect, self.near, self.far)
    }

    pub fn view_projection_matrix(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }
}
