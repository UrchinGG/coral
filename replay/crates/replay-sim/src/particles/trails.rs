use glam::{Vec3, vec3};
use replay_model::{Object, ObjectKind};
use replay_world::{BlockModels, WorldState};

use super::motes::{self, CRIT, SMOKE};
use super::{LONGEST_TICKS, Particle, TICK_MS, Terrain, tick_seed};
use crate::random::Random;

pub fn trail<'a>(
    object: &'a Object,
    index: usize,
    t: f32,
    position: impl Fn(f32) -> Option<Vec3> + 'a,
    world: &'a WorldState,
    models: &'a BlockModels,
) -> Option<impl Iterator<Item = Particle> + 'a> {
    let smokes = matches!(
        object.kind,
        ObjectKind::Tnt | ObjectKind::Fireball | ObjectKind::SmallFireball
    );
    let critical = object.kind == ObjectKind::Arrow && object.critical;
    let terrain = Terrain { world, models };
    let now = (t / TICK_MS) as u32;
    let ticks = (now.saturating_sub(LONGEST_TICKS)..=now).flat_map(move |tick| {
        let born = tick as f32 * TICK_MS;
        let mut random = Random::new(tick_seed(index, tick));
        let mut motes = Vec::new();
        if let Some(at) = position(born) {
            if smokes {
                let at = at + Vec3::Y * 0.5;
                motes.extend(motes::create(SMOKE, &mut random, at, Vec3::ZERO, None));
            } else if let Some(motion) = position(born + TICK_MS).map(|next| next - at)
                && motion != Vec3::ZERO
            {
                let velocity = vec3(-motion.x, 0.2 - motion.y, -motion.z);
                for k in 0..4 {
                    let at = at + motion * k as f32 / 4.0;
                    motes.extend(motes::create(CRIT, &mut random, at, velocity, None));
                }
            }
        }
        let age = (t - born) / TICK_MS;
        let mut children = Vec::new();
        for _ in 0..age as u32 {
            motes.retain_mut(|mote| mote.step(&terrain, &mut random, &mut children));
        }
        motes
            .into_iter()
            .filter_map(move |mote| mote.particle(age.fract()))
    });
    (smokes || critical).then_some(ticks)
}
