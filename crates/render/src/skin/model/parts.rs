use glam::Vec3;

use super::{CubeFaceUvs, box_uv};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyPart {
    Head,
    HeadLayer,
    Body,
    BodyLayer,
    RightArm,
    RightArmLayer,
    LeftArm,
    LeftArmLayer,
    RightLeg,
    RightLegLayer,
    LeftLeg,
    LeftLegLayer,
}

impl BodyPart {
    pub const BASE_PARTS: [BodyPart; 6] = [
        BodyPart::Head,
        BodyPart::Body,
        BodyPart::RightArm,
        BodyPart::LeftArm,
        BodyPart::RightLeg,
        BodyPart::LeftLeg,
    ];

    pub const LAYER_PARTS: [BodyPart; 6] = [
        BodyPart::HeadLayer,
        BodyPart::BodyLayer,
        BodyPart::RightArmLayer,
        BodyPart::LeftArmLayer,
        BodyPart::RightLegLayer,
        BodyPart::LeftLegLayer,
    ];

    pub fn is_layer(&self) -> bool {
        matches!(
            self,
            BodyPart::HeadLayer
                | BodyPart::BodyLayer
                | BodyPart::RightArmLayer
                | BodyPart::LeftArmLayer
                | BodyPart::RightLegLayer
                | BodyPart::LeftLegLayer
        )
    }

    pub fn position(&self, slim: bool) -> Vec3 {
        match self {
            BodyPart::Head | BodyPart::HeadLayer => Vec3::new(-4.0, 24.0, -4.0),
            BodyPart::Body | BodyPart::BodyLayer => Vec3::new(-4.0, 12.0, -2.0),
            BodyPart::RightArm | BodyPart::RightArmLayer => {
                Vec3::new(if slim { -7.0 } else { -8.0 }, 12.0, -2.0)
            }
            BodyPart::LeftArm | BodyPart::LeftArmLayer => Vec3::new(4.0, 12.0, -2.0),
            BodyPart::RightLeg | BodyPart::RightLegLayer => Vec3::new(-4.0, 0.0, -2.0),
            BodyPart::LeftLeg | BodyPart::LeftLegLayer => Vec3::new(0.0, 0.0, -2.0),
        }
    }

    pub fn size(&self, slim: bool) -> Vec3 {
        let arm_w = if slim { 3.0 } else { 4.0 };
        match self {
            BodyPart::Head | BodyPart::HeadLayer => Vec3::new(8.0, 8.0, 8.0),
            BodyPart::Body | BodyPart::BodyLayer => Vec3::new(8.0, 12.0, 4.0),
            BodyPart::RightArm
            | BodyPart::RightArmLayer
            | BodyPart::LeftArm
            | BodyPart::LeftArmLayer => Vec3::new(arm_w, 12.0, 4.0),
            BodyPart::RightLeg
            | BodyPart::RightLegLayer
            | BodyPart::LeftLeg
            | BodyPart::LeftLegLayer => Vec3::new(4.0, 12.0, 4.0),
        }
    }

    pub fn layer_expansion(&self) -> f32 {
        match self {
            BodyPart::HeadLayer => 0.5,
            _ if self.is_layer() => 0.25,
            _ => 0.0,
        }
    }

    pub fn uvs(&self, slim: bool) -> CubeFaceUvs {
        let arm_w = if slim { 3 } else { 4 };
        match self {
            BodyPart::Head => box_uv(0, 0, [8, 8, 8]),
            BodyPart::HeadLayer => box_uv(32, 0, [8, 8, 8]),
            BodyPart::Body => box_uv(16, 16, [8, 12, 4]),
            BodyPart::BodyLayer => box_uv(16, 32, [8, 12, 4]),
            BodyPart::RightArm => box_uv(40, 16, [arm_w, 12, 4]),
            BodyPart::RightArmLayer => box_uv(40, 32, [arm_w, 12, 4]),
            BodyPart::LeftArm | BodyPart::LeftArmLayer => {
                let base = if matches!(self, BodyPart::LeftArm) {
                    32
                } else {
                    48
                };
                let mut uvs = box_uv(base, 48, [arm_w, 12, 4]);
                uvs.top = uvs.top.flip_horizontal();
                uvs.bottom = uvs.bottom.flip_horizontal();
                uvs
            }
            BodyPart::RightLeg => box_uv(0, 16, [4, 12, 4]),
            BodyPart::RightLegLayer => box_uv(0, 32, [4, 12, 4]),
            BodyPart::LeftLeg | BodyPart::LeftLegLayer => {
                let base = if matches!(self, BodyPart::LeftLeg) {
                    16
                } else {
                    0
                };
                let mut uvs = box_uv(base, 48, [4, 12, 4]);
                uvs.top = uvs.top.flip_horizontal();
                uvs.bottom = uvs.bottom.flip_horizontal();
                uvs
            }
        }
    }

    pub fn pivot(&self, _slim: bool) -> Vec3 {
        match self {
            BodyPart::Head | BodyPart::HeadLayer => Vec3::new(0.0, 24.0, 0.0),
            BodyPart::Body | BodyPart::BodyLayer => Vec3::new(0.0, 12.0, 0.0),
            BodyPart::RightArm | BodyPart::RightArmLayer => Vec3::new(-4.0, 24.0, 0.0),
            BodyPart::LeftArm | BodyPart::LeftArmLayer => Vec3::new(4.0, 24.0, 0.0),
            BodyPart::RightLeg | BodyPart::RightLegLayer => Vec3::new(-2.0, 12.0, 0.0),
            BodyPart::LeftLeg | BodyPart::LeftLegLayer => Vec3::new(2.0, 12.0, 0.0),
        }
    }
}
