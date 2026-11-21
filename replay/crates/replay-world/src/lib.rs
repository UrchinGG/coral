mod blocks;
mod encoding;
mod light;
mod mesh;
mod rules;
mod state;

use bytemuck::{Pod, Zeroable};

pub use blocks::{Block, BlockModels, Face, Flags, Form, Quad, Rule};
pub use light::{Lighting, OPEN_SKY};
pub use mesh::{CUBE_FACES, mesh_section};
pub use replay_model::{section_index, section_local};
pub use state::{Section, WorldState};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub layer: u32,
    pub light: [u8; 4],
    pub tint: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct SectionMesh {
    pub opaque: Geometry,
    pub translucent: Geometry,
}

fn grid(low: [i32; 3], high: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
    (low[1]..=high[1]).flat_map(move |y| {
        (low[2]..=high[2]).flat_map(move |z| (low[0]..=high[0]).map(move |x| [x, y, z]))
    })
}

fn border_offsets(local: [i32; 3]) -> ([i32; 3], [i32; 3]) {
    (
        local.map(|at| -i32::from(at == 0)),
        local.map(|at| i32::from(at == 15)),
    )
}
