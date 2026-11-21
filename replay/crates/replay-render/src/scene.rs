use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};

pub const SHEET: i32 = -1;
pub const PLAIN: i32 = -2;
pub const WHITE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Cuboid {
    pub transform: [[f32; 4]; 4],
    pub origin: Vec3,
    pub inflate: f32,
    pub size: Vec3,
    pub layer: i32,
    pub uv: Vec2,
    pub mirror: u32,
    pub tint: u32,
    pub flash: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: Vec3,
    pub uv: Vec2,
    pub layer: i32,
    pub tint: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Billboard {
    pub center: Vec3,
    pub half: f32,
    pub uv: [f32; 4],
    pub layer: i32,
    pub tint: u32,
    pub flat: u32,
}

#[derive(Default)]
pub struct Scene {
    pub time: f32,
    pub focus: Option<(Vec3, f32)>,
    pub cuboids: Vec<Cuboid>,
    pub glint_cuboids: Vec<Cuboid>,
    pub solid: Vec<MeshVertex>,
    pub glint_meshes: Vec<MeshVertex>,
    pub cracks: Vec<MeshVertex>,
    pub xray: Vec<MeshVertex>,
    pub blended: Vec<MeshVertex>,
    pub particles: Vec<Billboard>,
}

impl Scene {
    pub fn clear(&mut self) {
        self.cuboids.clear();
        self.glint_cuboids.clear();
        self.particles.clear();
        for list in [
            &mut self.solid,
            &mut self.glint_meshes,
            &mut self.cracks,
            &mut self.xray,
            &mut self.blended,
        ] {
            list.clear();
        }
    }
}

impl Cuboid {
    pub fn new(transform: Mat4, origin: Vec3, size: Vec3) -> Cuboid {
        Cuboid {
            transform: transform.to_cols_array_2d(),
            origin,
            inflate: 0.0,
            size,
            layer: SHEET,
            uv: Vec2::ZERO,
            mirror: 0,
            tint: WHITE,
            flash: 0,
        }
    }
}

pub fn quad(corners: [Vec3; 4], uv: [Vec2; 4], layer: i32, tint: u32) -> [MeshVertex; 4] {
    [0, 1, 2, 3].map(|corner| MeshVertex {
        position: corners[corner],
        uv: uv[corner],
        layer,
        tint,
    })
}
