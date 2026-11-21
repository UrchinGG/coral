use std::collections::HashMap;

use replay_protocol::{Column, Section};

const DEDUP_MS: u32 = 75;

#[derive(Default)]
pub struct World {
    pub sections: Vec<[u16; 4096]>,
    pub initial: Vec<([i32; 3], u32)>,
    pub loads: SectionLoads,
    pub changes: BlockChanges,
}

#[derive(Default)]
pub struct SectionLoads {
    pub t: Vec<u32>,
    pub order: Vec<u32>,
    pub pos: Vec<[i32; 3]>,
    pub section: Vec<Option<u32>>,
}

#[derive(Default)]
pub struct BlockChanges {
    pub t: Vec<u32>,
    pub order: Vec<u32>,
    pub pos: Vec<[i32; 3]>,
    pub state: Vec<u16>,
}

impl World {
    fn next_order(&self) -> u32 {
        (self.loads.t.len() + self.changes.t.len()) as u32
    }

    fn push_load(&mut self, t: u32, pos: [i32; 3], section: Option<u32>) {
        let order = self.next_order();
        self.loads.t.push(t);
        self.loads.order.push(order);
        self.loads.pos.push(pos);
        self.loads.section.push(section);
    }

    fn push_change(&mut self, t: u32, pos: [i32; 3], state: u16) {
        let order = self.next_order();
        self.changes.t.push(t);
        self.changes.order.push(order);
        self.changes.pos.push(pos);
        self.changes.state.push(state);
    }
}

#[derive(Default)]
pub(crate) struct WorldLog {
    columns: HashMap<[i32; 2], [Option<Section>; 16]>,
    last_change: HashMap<[i32; 3], (u32, u16)>,
    started: bool,
    initial_sealed: bool,
    world: World,
}

impl WorldLog {
    pub fn begin(&mut self) {
        self.started = true;
        let mut initial: Vec<_> = self
            .columns
            .iter()
            .flat_map(|(&[x, z], sections)| {
                sections
                    .iter()
                    .zip(0..)
                    .filter_map(move |(section, y)| Some(([x, y, z], section.as_deref()?)))
            })
            .collect();
        initial.sort_unstable_by_key(|&(pos, _)| pos);
        for (pos, blocks) in initial {
            let index = store(&mut self.world.sections, blocks);
            self.world.initial.push((pos, index));
        }
    }

    pub fn clear(&mut self, t: u32) {
        if self.started {
            for (&[x, z], sections) in &self.columns {
                for (_, y) in sections
                    .iter()
                    .zip(0..)
                    .filter(|(section, _)| section.is_some())
                {
                    self.world.push_load(t, [x, y, z], None);
                }
            }
        }
        self.columns.clear();
        self.last_change.clear();
        self.initial_sealed = self.started;
    }

    pub fn load(&mut self, column: Column, t: u32) {
        if column.full && column.sections.iter().all(Option::is_none) {
            return;
        }
        let first = self.started
            && !self.initial_sealed
            && !self.columns.contains_key(&[column.x, column.z]);
        let current = self.columns.entry([column.x, column.z]).or_default();
        for ((slot, section), y) in current.iter_mut().zip(column.sections).zip(0..) {
            if !column.full && section.is_none() {
                continue;
            }
            let section = section.filter(|blocks| blocks.iter().any(|&state| state != 0));
            if *slot == section {
                continue;
            }
            if self.started {
                let pos = [column.x, y, column.z];
                let index = section
                    .as_deref()
                    .map(|blocks| store(&mut self.world.sections, blocks));
                match index {
                    Some(index) if first => self.world.initial.push((pos, index)),
                    _ => self.world.push_load(t, pos, index),
                }
            }
            *slot = section;
        }
    }

    pub fn set(&mut self, pos: [i32; 3], state: u16, t: u32) {
        let [x, y, z] = pos;
        let Some(slot) = self
            .columns
            .get_mut(&[x >> 4, z >> 4])
            .and_then(|sections| sections.get_mut(usize::try_from(y >> 4).ok()?))
        else {
            return;
        };
        slot.get_or_insert_with(|| Box::new([0; 4096]))[section_index(pos)] = state;
        let repeated = self
            .last_change
            .get(&pos)
            .is_some_and(|&(last, last_state)| last_state == state && t - last < DEDUP_MS);
        if self.started && !repeated {
            self.last_change.insert(pos, (t, state));
            self.world.push_change(t, pos, state);
        }
    }

    pub fn finish(self) -> World {
        self.world
    }
}

pub fn section_index([x, y, z]: [i32; 3]) -> usize {
    ((y & 15) << 8 | (z & 15) << 4 | (x & 15)) as usize
}

pub fn section_local(index: usize) -> [i32; 3] {
    let index = index as i32;
    [index & 15, index >> 8, index >> 4 & 15]
}

fn store(sections: &mut Vec<[u16; 4096]>, blocks: &[u16; 4096]) -> u32 {
    sections.push(*blocks);
    sections.len() as u32 - 1
}
