use std::array::from_fn;

use crate::light::borrows_light;
use crate::{
    Block, BlockModels, Face, Flags, Geometry, Lighting, OPEN_SKY, Quad, SectionMesh, Vertex,
    WorldState, grid, section_index,
};

const MARGIN: i32 = 2;
const PAD: i32 = 16 + MARGIN * 2;
const AO: [f32; 4] = [0.5, 0.68, 0.84, 1.0];
const WHITE: u32 = u32::MAX;
pub const CUBE_FACES: [[[f32; 3]; 4]; 6] = [
    [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
    [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]],
    [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]],
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
    [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]],
    [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
];

#[derive(Clone, Copy, Default)]
struct Light {
    shade: f32,
    sky: f32,
    block: f32,
}

pub fn mesh_section(
    world: &WorldState,
    lighting: &Lighting,
    models: &BlockModels,
    section: [i32; 3],
) -> SectionMesh {
    let mut mesh = SectionMesh::default();
    if world.section(section).is_none() {
        return mesh;
    }
    let states = padded(section, 0, |pos| world.section(pos));
    let blocks = states.iter().map(|&state| models.block(state)).collect();
    let mesher = Mesher {
        models,
        states,
        blocks,
        light: padded(section, OPEN_SKY, |pos| lighting.section(pos)),
    };
    for local in grid([0; 3], [15; 3]) {
        let index = padded_index(local);
        if mesher.states[index] == 0 {
            continue;
        }
        let geometry = if mesher.blocks[index].flags.has(Flags::TRANSLUCENT) {
            &mut mesh.translucent
        } else {
            &mut mesh.opaque
        };
        mesher.block(
            geometry,
            index,
            from_fn(|axis| section[axis] * 16 + local[axis]),
        );
    }
    mesh
}

fn padded<'a, T: Copy + 'a>(
    [x, y, z]: [i32; 3],
    empty: T,
    source: impl Fn([i32; 3]) -> Option<&'a [T; 4096]>,
) -> Vec<T> {
    let mut cells = vec![empty; (PAD * PAD * PAD) as usize];
    for offset in grid([-1; 3], [1; 3]) {
        let Some(section) = source([x + offset[0], y + offset[1], z + offset[2]]) else {
            continue;
        };
        let low = offset.map(|at| if at < 0 { 16 - MARGIN } else { 0 });
        let high = offset.map(|at| if at > 0 { MARGIN - 1 } else { 15 });
        for [x, y, z] in grid(low, high) {
            let index = padded_index([x + offset[0] * 16, y + offset[1] * 16, z + offset[2] * 16]);
            cells[index] = section[section_index([x, y, z])];
        }
    }
    cells
}

fn padded_index([x, y, z]: [i32; 3]) -> usize {
    (((y + MARGIN) * PAD + z + MARGIN) * PAD + x + MARGIN) as usize
}

fn step(index: usize, [dx, dy, dz]: [i32; 3]) -> usize {
    index.wrapping_add_signed(((dy * PAD + dz) * PAD + dx) as isize)
}

struct Mesher<'a> {
    models: &'a BlockModels,
    states: Vec<u16>,
    blocks: Vec<Block>,
    light: Vec<u8>,
}

impl Mesher<'_> {
    fn block(&self, geometry: &mut Geometry, index: usize, pos: [i32; 3]) {
        let flags = self.blocks[index].flags;
        let form = self.models.form(self.states[index], pos, |dx, dy, dz| {
            self.states[step(index, [dx, dy, dz])]
        });
        if flags.has(Flags::WATER) || flags.has(Flags::LAVA) {
            self.fluid(geometry, index, pos, form.tint);
            return;
        }
        for quad in self.models.quads(form, pos) {
            if quad.cull.is_some_and(|face| self.hides(index, face)) {
                continue;
            }
            let shade = if quad.shade { quad.face.shade() } else { 1.0 };
            let light = self
                .smooth(index, quad)
                .unwrap_or_else(|| [Light::flat(self.level(self.base(index, quad))); 4])
                .map(|light| Light {
                    shade: light.shade * shade,
                    ..light
                });
            let tint = if quad.tinted { form.tint } else { WHITE };
            push(
                geometry,
                pos,
                quad.corners,
                quad.uv,
                quad.layer,
                light,
                tint,
            );
        }
    }

    fn hides(&self, index: usize, face: Face) -> bool {
        let neighbor = step(index, face.normal());
        let flags = self.blocks[neighbor].flags;
        flags.has(Flags::OCCLUDES)
            || flags.has(Flags::SELF_CULL) && self.states[neighbor] == self.states[index]
    }

    fn base(&self, index: usize, quad: &Quad) -> usize {
        let axis = quad.face.axis();
        let edge = if quad.face.positive() { 1.0 } else { 0.0 };
        if quad
            .corners
            .iter()
            .all(|corner| (corner[axis] - edge).abs() < 1e-4)
        {
            step(index, quad.face.normal())
        } else {
            index
        }
    }

    fn level(&self, index: usize) -> u8 {
        if !borrows_light(self.states[index]) {
            return self.light[index];
        }
        Face::ALL
            .map(|face| self.light[step(index, face.normal())])
            .into_iter()
            .fold(0, brightest)
    }

    fn smooth(&self, index: usize, quad: &Quad) -> Option<[Light; 4]> {
        let axis = quad.face.axis();
        let plane = quad.corners[0][axis];
        if !quad.smooth
            || quad
                .corners
                .iter()
                .any(|corner| (corner[axis] - plane).abs() > 1e-4)
        {
            return None;
        }
        let base = self.base(index, quad);
        let [u, v] = [(axis + 1) % 3, (axis + 2) % 3];
        let occludes = |offset| self.blocks[step(base, offset)].flags.has(Flags::OCCLUDES);
        let along = |axis: usize, high: usize| {
            let mut offset = [0; 3];
            offset[axis] = high as i32 * 2 - 1;
            offset
        };
        let center = self.level(base);
        let near = |offset| match self.level(step(base, offset)) {
            0 => center,
            level => level,
        };
        let levels: [[Light; 2]; 2] = from_fn(|high_u| {
            from_fn(|high_v| {
                let (side_u, side_v) = (along(u, high_u), along(v, high_v));
                let corner = from_fn(|axis| side_u[axis] + side_v[axis]);
                let blocked = usize::from(occludes(side_u)) + usize::from(occludes(side_v));
                let (shade, diagonal) = if blocked == 2 {
                    (AO[0], near(side_u))
                } else {
                    (
                        AO[3 - blocked - usize::from(occludes(corner))],
                        near(corner),
                    )
                };
                Light::average([center, near(side_u), near(side_v), diagonal], shade)
            })
        });
        Some(quad.corners.map(|corner| {
            let (s, t) = (corner[u].clamp(0.0, 1.0), corner[v].clamp(0.0, 1.0));
            let weights = [
                [(1.0 - s) * (1.0 - t), (1.0 - s) * t],
                [s * (1.0 - t), s * t],
            ];
            Light::blend(levels.as_flattened(), weights.as_flattened())
        }))
    }

    fn fluid(&self, geometry: &mut Geometry, index: usize, pos: [i32; 3], tint: u32) {
        let (kind, layer) = if self.blocks[index].flags.has(Flags::WATER) {
            (Flags::WATER, self.models.water)
        } else {
            (Flags::LAVA, self.models.lava)
        };
        let same = |at: usize| self.blocks[at].flags.has(kind);
        let height = |x: i32, z: i32| {
            let columns = [[x - 1, z - 1], [x - 1, z], [x, z - 1], [x, z]];
            if columns
                .iter()
                .any(|&[dx, dz]| same(step(index, [dx, 1, dz])))
            {
                return 1.0;
            }
            let (total, count) = columns
                .iter()
                .fold((0.0, 0.0), |(total, count), &[dx, dz]| {
                    let column = step(index, [dx, 0, dz]);
                    if same(column) {
                        let level = self.states[column] & 15;
                        let weight = if level == 0 || level >= 8 { 11.0 } else { 1.0 };
                        let height = f32::from(if level >= 8 { 1 } else { level + 1 }) / 9.0;
                        (total + height * weight, count + weight)
                    } else if self.blocks[column].flags.has(Flags::SOLID) {
                        (total, count)
                    } else {
                        (total + 1.0, count + 1.0)
                    }
                });
            1.0 - total / count
        };
        let heights: [[f32; 2]; 2] = from_fn(|x| from_fn(|z| height(x as i32, z as i32)));
        let light = Light::flat(brightest(
            self.light[index],
            self.light[step(index, [0, 1, 0])],
        ));
        for face in Face::ALL {
            let neighbor = step(index, face.normal());
            if same(neighbor)
                || face != Face::Up && self.blocks[neighbor].flags.has(Flags::OCCLUDES)
            {
                continue;
            }
            let corners = CUBE_FACES[face as usize]
                .map(|[x, y, z]| [x, y * heights[x as usize][z as usize], z]);
            let uv = corners.map(|[x, y, z]| match face {
                Face::Up | Face::Down => [x, z],
                Face::East | Face::West => [z, 1.0 - y],
                Face::South | Face::North => [x, 1.0 - y],
            });
            let shaded = Light {
                shade: face.shade(),
                ..light
            };
            push(geometry, pos, corners, uv, layer, [shaded; 4], tint);
        }
    }
}

impl Light {
    fn flat(level: u8) -> Light {
        Light::average([level; 4], 1.0)
    }

    fn average(levels: [u8; 4], shade: f32) -> Light {
        let mean = |channel: fn(u8) -> u8| {
            levels
                .iter()
                .map(|&level| f32::from(channel(level)))
                .sum::<f32>()
                / 4.0
        };
        Light {
            shade,
            sky: mean(|level| level >> 4),
            block: mean(|level| level & 15),
        }
    }

    fn blend(lights: &[Light], weights: &[f32]) -> Light {
        lights
            .iter()
            .zip(weights)
            .fold(Light::default(), |total, (light, &weight)| Light {
                shade: total.shade + light.shade * weight,
                sky: total.sky + light.sky * weight,
                block: total.block + light.block * weight,
            })
    }

    fn packed(self) -> [u8; 4] {
        let unit = |value: f32| (value * 255.0).round() as u8;
        [
            unit(self.shade),
            unit(self.sky / 15.0),
            unit(self.block / 15.0),
            0,
        ]
    }
}

fn brightest(a: u8, b: u8) -> u8 {
    (a & 0xF0).max(b & 0xF0) | (a & 15).max(b & 15)
}

fn push(
    geometry: &mut Geometry,
    pos: [i32; 3],
    corners: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    layer: u16,
    light: [Light; 4],
    tint: u32,
) {
    let first = geometry.vertices.len() as u32;
    geometry.vertices.extend((0..4).map(|corner| Vertex {
        position: from_fn(|axis| (pos[axis] & 15) as f32 + corners[corner][axis]),
        uv: uv[corner],
        layer: u32::from(layer),
        light: light[corner].packed(),
        tint,
    }));
    let shade = light.map(|light| light.shade);
    let order = if shade[0] + shade[2] < shade[1] + shade[3] {
        [1, 2, 3, 1, 3, 0]
    } else {
        [0, 1, 2, 0, 2, 3]
    };
    geometry.indices.extend(order.map(|corner| first + corner));
}
