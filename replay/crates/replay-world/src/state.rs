use std::array::from_fn;
use std::collections::{HashMap, HashSet};

use replay_model::{World, section_index, section_local};

use crate::light::reach;
use crate::{border_offsets, grid};

pub type Section = [u16; 4096];

pub struct WorldState {
    pool: Vec<Section>,
    initial: HashMap<[i32; 3], u32>,
    events: Vec<Event>,
    history: HashMap<[i32; 3], Vec<usize>>,
    sections: HashMap<[i32; 3], Box<Section>>,
    applied: usize,
    dirty: HashSet<[i32; 3]>,
    stale_light: Vec<[i32; 4]>,
}

#[derive(Clone, Copy)]
struct Event {
    t: u32,
    order: u32,
    section: [i32; 3],
    change: Change,
}

#[derive(Clone, Copy)]
enum Change {
    Load(Option<u32>),
    Block { index: usize, state: u16 },
}

impl WorldState {
    pub fn new(world: World) -> WorldState {
        let loads = world
            .loads
            .t
            .iter()
            .zip(world.loads.order)
            .zip(world.loads.pos)
            .zip(world.loads.section);
        let changes = world
            .changes
            .t
            .iter()
            .zip(world.changes.order)
            .zip(world.changes.pos)
            .zip(world.changes.state);
        let mut events: Vec<Event> = loads
            .map(|(((&t, order), section), load)| Event {
                t,
                order,
                section,
                change: Change::Load(load),
            })
            .chain(changes.map(|(((&t, order), pos), state)| Event {
                t,
                order,
                section: pos.map(|at| at >> 4),
                change: Change::Block {
                    index: section_index(pos),
                    state,
                },
            }))
            .collect();
        events.sort_by_key(|event| (event.t, event.order));
        let mut history: HashMap<_, Vec<_>> = HashMap::new();
        for (index, event) in events.iter().enumerate() {
            history.entry(event.section).or_default().push(index);
        }
        let initial: HashMap<_, _> = world.initial.into_iter().collect();
        let mut state = WorldState {
            pool: world.sections,
            events,
            history,
            sections: HashMap::new(),
            applied: 0,
            dirty: initial.keys().copied().collect(),
            stale_light: Vec::new(),
            initial,
        };
        for section in state.dirty.clone() {
            state.restore(section);
        }
        state
    }

    pub fn section(&self, pos: [i32; 3]) -> Option<&Section> {
        self.sections.get(&pos).map(|section| &**section)
    }

    pub fn height(&self) -> Option<[i32; 2]> {
        let levels = self.sections.keys().map(|&[_, y, _]| y);
        Some([levels.clone().min()? * 16, levels.max()? * 16 + 15])
    }

    pub fn block(&self, pos: [i32; 3]) -> u16 {
        self.section(pos.map(|at| at >> 4))
            .map_or(0, |section| section[section_index(pos)])
    }

    pub fn seek(&mut self, t: u32) {
        let target = self.events.partition_point(|event| event.t <= t);
        if target >= self.applied {
            self.advance(target)
        } else {
            self.rewind(target)
        }
        self.applied = target;
    }

    fn advance(&mut self, target: usize) {
        for index in self.applied..target {
            let Event {
                section, change, ..
            } = self.events[index];
            self.apply(section, change);
            let (stale_area, (low, high)) = match change {
                Change::Load(_) => (reach(section.map(|at| at * 16), 15), ([-1; 3], [1; 3])),
                Change::Block { index, .. } => {
                    let local = section_local(index);
                    let cell = from_fn(|axis| section[axis] * 16 + local[axis]);
                    (reach(cell, 0), border_offsets(local))
                }
            };
            self.stale_light.push(stale_area);
            self.mark(section, low, high);
        }
    }

    fn rewind(&mut self, target: usize) {
        let affected: HashSet<_> = self.events[target..self.applied]
            .iter()
            .map(|event| event.section)
            .collect();
        for section in affected {
            self.restore(section);
            let history = &self.history[&section];
            let replayed: Vec<_> = history[..history.partition_point(|&index| index < target)]
                .iter()
                .map(|&index| self.events[index].change)
                .collect();
            for change in replayed {
                self.apply(section, change);
            }
            self.mark(section, [-1; 3], [1; 3]);
            self.stale_light.push(reach(section.map(|at| at * 16), 15));
        }
    }

    pub fn drain_dirty(&mut self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.dirty.drain()
    }

    pub fn drain_stale_light(&mut self) -> impl Iterator<Item = [i32; 4]> + '_ {
        self.stale_light.drain(..)
    }

    fn restore(&mut self, section: [i32; 3]) {
        self.apply(section, Change::Load(self.initial.get(&section).copied()));
    }

    fn apply(&mut self, section: [i32; 3], change: Change) {
        match change {
            Change::Load(load) => {
                if let Some(blocks) = load.and_then(|index| self.pool.get(index as usize)) {
                    self.sections.insert(section, Box::new(*blocks));
                } else {
                    self.sections.remove(&section);
                }
            }
            Change::Block { index, state } => {
                self.sections
                    .entry(section)
                    .or_insert_with(|| Box::new([0; 4096]))[index] = state;
            }
        }
    }

    fn mark(&mut self, [x, y, z]: [i32; 3], low: [i32; 3], high: [i32; 3]) {
        for [dx, dy, dz] in grid(low, high) {
            let pos = [x + dx, y + dy, z + dz];
            if [dx, dy, dz] == [0; 3] || self.sections.contains_key(&pos) {
                self.dirty.insert(pos);
            }
        }
    }
}
