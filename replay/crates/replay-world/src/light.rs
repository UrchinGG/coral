use std::array::from_fn;
use std::collections::{HashMap, HashSet};

use crate::{BlockModels, Flags, WorldState, border_offsets, grid, section_index, section_local};

pub const OPEN_SKY: u8 = FULL << 4;
const FULL: u8 = 15;
const REACH: i32 = 15;
const TILE: i32 = 64;
const CLUSTER: i32 = 96;
const SLABS: [u16; 3] = [44, 126, 182];
const STAIRS: [u16; 13] = [
    53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180,
];
const FARMLAND: u16 = 60;

pub struct Lighting {
    sections: HashMap<[i32; 3], Box<[u8; 4096]>>,
    lit: HashSet<[i32; 2]>,
    opacity: Vec<u8>,
    emission: Vec<u8>,
}

struct Volume {
    low: [i32; 3],
    size: [usize; 3],
}

type Bounds = [[i32; 3]; 2];

impl Lighting {
    pub fn new(models: &BlockModels) -> Lighting {
        Lighting {
            sections: HashMap::new(),
            lit: HashSet::new(),
            opacity: (0..4096).map(|state| opacity(state, models)).collect(),
            emission: (0..4096).map(emission).collect(),
        }
    }

    pub fn prepare(&mut self, world: &WorldState, [x, _, z]: [i32; 3]) {
        let tiles = grid([x - 1, 0, z - 1], [x + 1, 0, z + 1])
            .map(|[x, _, z]| [x * 16, z * 16].map(|at| at.div_euclid(TILE) * TILE));
        for [x, z] in tiles {
            if self.lit.insert([x, z]) {
                let tile = [x, z, x + TILE - 1, z + TILE - 1];
                self.relight(world, tile, &mut HashSet::new());
            }
        }
    }

    pub fn section(&self, pos: [i32; 3]) -> Option<&[u8; 4096]> {
        self.sections.get(&pos).map(|cells| &**cells)
    }

    pub fn at(&self, pos: [i32; 3]) -> u8 {
        self.sections
            .get(&pos.map(|at| at >> 4))
            .map_or(OPEN_SKY, |cells| cells[section_index(pos)])
    }

    pub fn update(
        &mut self,
        world: &WorldState,
        areas: impl IntoIterator<Item = [i32; 4]>,
    ) -> HashSet<[i32; 3]> {
        let mut clusters: Vec<[i32; 4]> = Vec::new();
        for area in areas {
            match clusters
                .iter_mut()
                .find(|cluster| fits(union(**cluster, area)))
            {
                Some(cluster) => *cluster = union(*cluster, area),
                None => clusters.push(area),
            }
        }
        let mut changed = HashSet::new();
        for cluster in clusters {
            self.relight(world, cluster, &mut changed);
        }
        changed
    }

    fn relight(
        &mut self,
        world: &WorldState,
        [x0, z0, x1, z1]: [i32; 4],
        changed: &mut HashSet<[i32; 3]>,
    ) {
        let Some([bottom, top]) = world.height() else {
            return;
        };
        let inner = [
            [x0 - REACH, bottom, z0 - REACH],
            [x1 + REACH, top, z1 + REACH],
        ];
        let volume = Volume::around(inner);
        let mut opacity = vec![0; volume.len()];
        for at in volume.shell() {
            opacity[at] = FULL;
        }
        let mut block = vec![0; volume.len()];
        let mut glowing = Vec::new();
        for section in sections(inner) {
            let Some(cells) = world.section(section) else {
                continue;
            };
            for (at, cell) in volume.cells(section, inner) {
                let state = usize::from(cells[cell]);
                opacity[at] = self.opacity.get(state).copied().unwrap_or(0);
                block[at] = self.emission.get(state).copied().unwrap_or(0);
                if block[at] > 0 {
                    glowing.push(at);
                }
            }
        }
        let (mut sky, edge) = volume.skylight(inner, &opacity);
        volume.spread(&mut sky, &opacity, edge);
        volume.spread(&mut block, &opacity, glowing);
        let region = [[x0, bottom, z0], [x1, top, z1]];
        self.store(world, &volume, region, [&sky, &block], changed);
    }

    fn store(
        &mut self,
        world: &WorldState,
        volume: &Volume,
        region: Bounds,
        [sky, block]: [&[u8]; 2],
        changed: &mut HashSet<[i32; 3]>,
    ) {
        for section in sections(region) {
            let mut cells = self
                .sections
                .remove(&section)
                .unwrap_or_else(|| Box::new([OPEN_SKY; 4096]));
            let mut touched = [false; 27];
            for (at, cell) in volume.cells(section, region) {
                let packed = sky[at] << 4 | block[at];
                if cells[cell] != packed {
                    cells[cell] = packed;
                    let (low, high) = border_offsets(section_local(cell));
                    for offset in grid(low, high) {
                        touched[neighbor_slot(offset)] = true;
                    }
                }
            }
            for offset in grid([-1; 3], [1; 3]) {
                let neighbor = from_fn(|axis| section[axis] + offset[axis]);
                if touched[neighbor_slot(offset)] && world.section(neighbor).is_some() {
                    changed.insert(neighbor);
                }
            }
            if cells.iter().any(|&cell| cell != OPEN_SKY) {
                self.sections.insert(section, cells);
            }
        }
    }
}

impl Volume {
    fn around([low, high]: Bounds) -> Volume {
        let low = low.map(|at| at - 1);
        Volume {
            low,
            size: from_fn(|axis| (high[axis] + 2 - low[axis]) as usize),
        }
    }

    fn len(&self) -> usize {
        self.size.iter().product()
    }

    fn strides(&self) -> [usize; 3] {
        [1, self.size[0] * self.size[2], self.size[0]]
    }

    fn index(&self, pos: [i32; 3]) -> usize {
        let strides = self.strides();
        (0..3)
            .map(|axis| (pos[axis] - self.low[axis]) as usize * strides[axis])
            .sum()
    }

    fn shell(&self) -> impl Iterator<Item = usize> + '_ {
        let high = from_fn(|axis| self.low[axis] + self.size[axis] as i32 - 1);
        (0..3)
            .flat_map(move |axis| {
                [self.low[axis], high[axis]].map(move |side| {
                    let (mut from, mut to) = (self.low, high);
                    from[axis] = side;
                    to[axis] = side;
                    grid(from, to)
                })
            })
            .flatten()
            .map(|pos| self.index(pos))
    }

    fn cells(
        &self,
        section: [i32; 3],
        [low, high]: Bounds,
    ) -> impl Iterator<Item = (usize, usize)> + '_ {
        let from: [i32; 3] = from_fn(|axis| low[axis].max(section[axis] * 16));
        let to: [i32; 3] = from_fn(|axis| high[axis].min(section[axis] * 16 + 15));
        let width = (to[0] - from[0] + 1).max(0) as usize;
        (from[1]..=to[1]).flat_map(move |y| {
            (from[2]..=to[2]).flat_map(move |z| {
                let (at, cell) = (self.index([from[0], y, z]), section_index([from[0], y, z]));
                (0..width).map(move |x| (at + x, cell + x))
            })
        })
    }

    fn skylight(&self, [low, high]: Bounds, opacity: &[u8]) -> (Vec<u8>, Vec<usize>) {
        let up = self.strides()[1];
        let depth = high[2] - low[2] + 1;
        let column = |x: i32, z: i32| ((x - low[0]) * depth + z - low[2]) as usize;
        let columns =
            || (low[0]..=high[0]).flat_map(move |x| (low[2]..=high[2]).map(move |z| [x, z]));
        let inside =
            |[x, z]: [i32; 2]| (low[0]..=high[0]).contains(&x) && (low[2]..=high[2]).contains(&z);
        let mut sky = vec![0; self.len()];
        let mut lowest = vec![0; columns().count()];
        for [x, z] in columns() {
            let (mut y, mut at) = (high[1], self.index([x, high[1], z]));
            while opacity[at] == 0 {
                sky[at] = FULL;
                (y, at) = (y - 1, at - up);
            }
            lowest[column(x, z)] = y + 1;
        }
        let mut edge = Vec::new();
        for [x, z] in columns() {
            let open_from = lowest[column(x, z)];
            let shaded_below = [[x - 1, z], [x + 1, z], [x, z - 1], [x, z + 1]]
                .into_iter()
                .filter(|&side| inside(side))
                .map(|[x, z]| lowest[column(x, z)])
                .fold(open_from + 1, i32::max);
            edge.extend((open_from..shaded_below.min(high[1] + 1)).map(|y| self.index([x, y, z])));
        }
        (sky, edge)
    }

    fn spread(&self, light: &mut [u8], opacity: &[u8], mut queue: Vec<usize>) {
        let strides = self.strides();
        while let Some(at) = queue.pop() {
            let level = light[at];
            for neighbor in strides
                .iter()
                .flat_map(|&stride| [at - stride, at + stride])
            {
                let next = level.saturating_sub(opacity[neighbor].max(1));
                if next > light[neighbor] {
                    light[neighbor] = next;
                    queue.push(neighbor);
                }
            }
        }
    }
}

pub(crate) fn reach([x, _, z]: [i32; 3], width: i32) -> [i32; 4] {
    [x - REACH, z - REACH, x + width + REACH, z + width + REACH]
}

pub fn borrows_light(state: u16) -> bool {
    let id = state >> 4;
    SLABS.contains(&id) || STAIRS.contains(&id) || id == FARMLAND
}

fn opacity(state: u16, models: &BlockModels) -> u8 {
    match state >> 4 {
        8 | 9 | 79 => 3,
        18 | 30 | 161 => 1,
        _ if borrows_light(state) || models.block(state).flags.has(Flags::OCCLUDES) => FULL,
        _ => 0,
    }
}

fn emission(state: u16) -> u8 {
    match state >> 4 {
        10 | 11 | 51 | 89 | 91 | 119 | 124 | 138 | 169 => 15,
        50 => 14,
        62 => 13,
        90 => 11,
        74 | 94 | 150 => 9,
        76 | 130 => 7,
        39 | 117 | 120 | 122 => 1,
        _ => 0,
    }
}

fn sections([low, high]: Bounds) -> impl Iterator<Item = [i32; 3]> {
    grid(low.map(|at| at >> 4), high.map(|at| at >> 4))
}

fn neighbor_slot([x, y, z]: [i32; 3]) -> usize {
    ((x + 1) * 9 + (y + 1) * 3 + z + 1) as usize
}

fn union(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

fn fits([x0, z0, x1, z1]: [i32; 4]) -> bool {
    x1 - x0 < CLUSTER && z1 - z0 < CLUSTER
}
