use std::collections::{BTreeMap, HashMap};
use std::f32::consts::FRAC_PI_2;

use glam::{IVec3, Mat4, Vec3, vec3};
use replay_model::Effect;
use replay_world::{WorldState, section_local};

use crate::draw::Draw;
use crate::model::{Paint, cube};

const CHEST: u16 = 54;
const ENDER_CHEST: u16 = 130;
const TRAPPED_CHEST: u16 = 146;

pub struct Chests {
    lids: BTreeMap<[i32; 3], Vec<(u32, u8)>>,
    found: HashMap<[i32; 3], Vec<[i32; 3]>>,
}

impl Chests {
    pub fn new(effects: &[Effect]) -> Chests {
        Chests {
            lids: replay_sim::lids(effects),
            found: HashMap::new(),
        }
    }

    pub fn scan(&mut self, world: &WorldState, section: [i32; 3]) {
        let origin = IVec3::from(section) * 16;
        let chests: Vec<[i32; 3]> = world
            .section(section)
            .into_iter()
            .flat_map(|blocks| blocks.iter().enumerate())
            .filter(|&(_, &state)| matches!(state >> 4, CHEST | ENDER_CHEST | TRAPPED_CHEST))
            .map(|(index, _)| (origin + IVec3::from(section_local(index))).into())
            .collect();
        if chests.is_empty() {
            self.found.remove(&section);
        } else {
            self.found.insert(section, chests);
        }
    }

    pub fn draw(&self, draw: &mut Draw, world: &WorldState) {
        for &pos in self.found.values().flatten() {
            self.chest(draw, world, pos);
        }
    }

    fn chest(&self, draw: &mut Draw, world: &WorldState, [x, y, z]: [i32; 3]) {
        draw.light_at(IVec3::new(x, y, z).as_vec3());
        let state = world.block([x, y, z]);
        let id = state >> 4;
        let pairs = id != ENDER_CHEST;
        let same = |dx: i32, dz: i32| pairs && world.block([x + dx, y, z + dz]) >> 4 == id;
        if same(-1, 0) || same(0, -1) {
            return;
        }
        let partner = [[1, 0], [0, 1]].into_iter().find(|&[dx, dz]| same(dx, dz));
        let texture = match (id, partner.is_some()) {
            (ENDER_CHEST, _) => "entity/chest/ender",
            (TRAPPED_CHEST, false) => "entity/chest/trapped",
            (TRAPPED_CHEST, true) => "entity/chest/trapped_double",
            (_, false) => "entity/chest/normal",
            (_, true) => "entity/chest/normal_double",
        };
        let facing = state & 15;
        let turn: f32 = match facing {
            2 => 180.0,
            4 => 90.0,
            5 => -90.0,
            _ => 0.0,
        };
        let shift = match (facing, partner) {
            (2, Some([1, 0])) => Vec3::X,
            (5, Some([0, 1])) => Vec3::NEG_Z,
            _ => Vec3::ZERO,
        };
        let transform = Mat4::from_translation(vec3(x as f32, y as f32 + 1.0, z as f32 + 1.0))
            * Mat4::from_scale(vec3(1.0, -1.0, -1.0))
            * Mat4::from_translation(Vec3::splat(0.5) + shift)
            * Mat4::from_rotation_y(turn.to_radians())
            * Mat4::from_scale(Vec3::splat(1.0 / 16.0))
            * Mat4::from_translation(Vec3::splat(-8.0));

        let open = |[dx, dz]: [i32; 2]| {
            self.lids
                .get(&[x + dx, y, z + dz])
                .map_or(0.0, |events| replay_sim::lid(events, draw.t))
        };
        let open = partner.map_or(open([0, 0]), |offset| open([0, 0]).max(open(offset)));
        let angle = -(1.0 - (1.0 - open).powi(3)) * FRAC_PI_2;
        let (width, knob) = if partner.is_some() {
            (30.0, 16.0)
        } else {
            (14.0, 8.0)
        };
        let paint = Paint::sheet(draw.assets.atlas.rect(texture));
        let parts = [
            (
                replay_sim::part(vec3(1.0, 6.0, 1.0), Vec3::ZERO),
                cube(0.0, 19.0, [0.0, 0.0, 0.0], [width, 10.0, 14.0]),
            ),
            (
                replay_sim::part(vec3(1.0, 7.0, 15.0), Vec3::X * angle),
                cube(0.0, 0.0, [0.0, -5.0, -14.0], [width, 5.0, 14.0]),
            ),
            (
                replay_sim::part(vec3(knob, 7.0, 15.0), Vec3::X * angle),
                cube(0.0, 0.0, [-1.0, -2.0, -15.0], [2.0, 4.0, 1.0]),
            ),
        ];
        for (part, model) in parts {
            draw.cuboids(transform * part, paint, &[model]);
        }
    }
}
