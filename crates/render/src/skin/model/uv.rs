use glam::Vec2;

#[derive(Debug, Clone, Copy)]
pub struct FaceUv {
    pub top_left: Vec2,
    pub top_right: Vec2,
    pub bottom_left: Vec2,
    pub bottom_right: Vec2,
}

impl FaceUv {
    pub fn new(x1: u16, y1: u16, x2: u16, y2: u16) -> Self {
        let (x1, y1, x2, y2) = (x1 as f32, y1 as f32, x2 as f32, y2 as f32);
        Self {
            top_left: Vec2::new(x1 / 64.0, y1 / 64.0),
            top_right: Vec2::new(x2 / 64.0, y1 / 64.0),
            bottom_left: Vec2::new(x1 / 64.0, y2 / 64.0),
            bottom_right: Vec2::new(x2 / 64.0, y2 / 64.0),
        }
    }

    pub fn flip_horizontal(&self) -> Self {
        Self {
            top_left: self.top_right,
            top_right: self.top_left,
            bottom_left: self.bottom_right,
            bottom_right: self.bottom_left,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CubeFaceUvs {
    pub front: FaceUv,
    pub back: FaceUv,
    pub left: FaceUv,
    pub right: FaceUv,
    pub top: FaceUv,
    pub bottom: FaceUv,
}

pub fn box_uv(u: u16, v: u16, [w, h, d]: [u16; 3]) -> CubeFaceUvs {
    CubeFaceUvs {
        top: FaceUv::new(u + d, v, u + d + w, v + d),
        bottom: FaceUv::new(u + d + w, v, u + d + w + w, v + d),
        right: FaceUv::new(u, v + d, u + d, v + d + h),
        front: FaceUv::new(u + d, v + d, u + d + w, v + d + h),
        left: FaceUv::new(u + d + w, v + d, u + d + w + d, v + d + h),
        back: FaceUv::new(u + d + w + d, v + d, u + d + w + d + w, v + d + h),
    }
}
