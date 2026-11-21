use std::collections::HashMap;

use glam::{IVec3, Mat4, Vec2, Vec3};
use replay_model::{Event, EventKind};
use replay_render::{WHITE, quad};
use replay_sim::TICK_MS;
use replay_world::WorldState;

use crate::draw::Draw;

const FORGOTTEN_MS: f32 = 400.0 * TICK_MS;
const LIFT: f32 = 1.002;

pub struct Cracks(Vec<Vec<(u32, [i32; 3], i8)>>);

impl Cracks {
    pub fn new(events: &[Event]) -> Cracks {
        let mut breakers: HashMap<i32, Vec<_>> = HashMap::new();
        for event in events {
            if let EventKind::BlockBreak {
                breaker,
                pos,
                stage,
                ..
            } = event.kind
            {
                breakers
                    .entry(breaker)
                    .or_default()
                    .push((event.t, pos, stage));
            }
        }
        Cracks(breakers.into_values().collect())
    }

    pub fn draw(&self, draw: &mut Draw, world: &WorldState) {
        let models = &draw.assets.models;
        for breaks in &self.0 {
            let Some(&(at, pos, stage)) =
                breaks[..breaks.partition_point(|&(at, ..)| at as f32 <= draw.t)].last()
            else {
                continue;
            };
            let Some(&layer) = usize::try_from(stage)
                .ok()
                .and_then(|stage| models.destroy.get(stage))
            else {
                continue;
            };
            let state = world.block(pos);
            if state == 0 || draw.t - at as f32 > FORGOTTEN_MS {
                continue;
            }
            let transform = Mat4::from_translation(IVec3::from(pos).as_vec3() + 0.5)
                * Mat4::from_scale(Vec3::splat(LIFT))
                * Mat4::from_translation(Vec3::splat(-0.5));
            let [x, y, z] = pos;
            let form = models.form(state, pos, |dx, dy, dz| {
                world.block([x + dx, y + dy, z + dz])
            });
            let quads = models.quads(form, pos);
            draw.scene.cracks.extend(quads.iter().flat_map(|face| {
                let corners = face
                    .corners
                    .map(|corner| transform.transform_point3(corner.into()));
                quad(corners, face.uv.map(Vec2::from), layer.into(), WHITE)
            }));
        }
    }
}
