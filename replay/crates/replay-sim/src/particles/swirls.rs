use glam::vec3;

use super::{LONGEST_TICKS, Particle, Sprite, TICK_MS, rgb, tick_seed};
use crate::random::Random;
use crate::{INVISIBLE, PLAYER_HEIGHT, PLAYER_WIDTH, Players, latest};

const SPELL_TILE: u8 = 128;

impl Players {
    pub fn swirls(&self, t: f32, hidden: Option<usize>) -> impl Iterator<Item = Particle> + '_ {
        let now = (t / TICK_MS) as u32;
        (0..self.tracks.len())
            .filter(move |&index| Some(index) != hidden)
            .flat_map(move |index| {
                (now.saturating_sub(LONGEST_TICKS)..=now)
                    .filter_map(move |tick| self.swirl(index, tick, t))
            })
    }

    fn swirl(&self, index: usize, tick: u32, t: f32) -> Option<Particle> {
        let born = tick as f32 * TICK_MS;
        let state =
            *latest(&self.tracks[index].states, born).filter(|state| state.potion_color != 0)?;
        let frame = self.sample(index, born)?;
        let mut random = Random::new(tick_seed(index, tick));
        let spawns = if state.flags & INVISIBLE != 0 {
            random.next() * 15.0 < 1.0
        } else {
            random.next() < 0.5
        };
        if !spawns {
            return None;
        }
        let color = rgb(state.potion_color as u32);
        let mut velocity = vec3(
            0.5 - random.next() + random.signed() * 0.4,
            color.y + random.signed() * 0.4,
            0.5 - random.next() + random.signed() * 0.4,
        );
        let speed =
            (random.next() + random.next() + 1.0) * 0.15 * 0.4 / velocity.length().max(1e-6);
        velocity = vec3(
            velocity.x * speed,
            (velocity.y * speed + 0.1) * 0.2,
            velocity.z * speed,
        );
        let life = (8.0 / (random.next() * 0.8 + 0.2)) as u32;
        let age = (t - born) / TICK_MS;
        let ticks = age as u32;
        if ticks >= life {
            return None;
        }
        let mut position = frame.position
            + vec3(
                (random.next() - 0.5) * PLAYER_WIDTH,
                random.next() * PLAYER_HEIGHT,
                (random.next() - 0.5) * PLAYER_WIDTH,
            );
        for _ in 0..ticks {
            velocity.y += 0.004;
            position += velocity;
            velocity *= 0.96;
        }
        Some(Particle {
            position: position + velocity * age.fract(),
            size: 0.1 * (random.next() * 0.5 + 0.5) * 2.0 * 0.75,
            color: color.extend(1.0),
            sprite: Sprite::Tile(SPELL_TILE + 7 - (ticks * 8 / life) as u8),
            flat: false,
        })
    }
}
