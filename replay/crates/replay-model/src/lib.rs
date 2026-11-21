mod build;
mod clock;
mod effect;
mod item;
mod object;
mod roster;
mod scoreboard;
mod world;

use replay_protocol::{Segment, legacy_segments};

pub use build::build;
pub use effect::{Effect, EffectKind};
pub use item::{Item, Material};
pub use object::{FireworkExplosion, FireworkShape, Look, Object, ObjectKind, Path};
pub use world::{BlockChanges, SectionLoads, World, section_index, section_local};

pub struct Replay {
    pub info: Info,
    pub players: Vec<Player>,
    pub items: Vec<Item>,
    pub objects: Vec<Object>,
    pub events: Vec<Event>,
    pub effects: Vec<Effect>,
    pub daytime: Vec<(u32, u16)>,
    pub world: World,
}

pub struct Info {
    pub duration: u32,
    pub start: u32,
    pub stable_start: u32,
    pub server_name: Option<String>,
    pub game: Option<String>,
    pub mode: Option<String>,
    pub map: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
}

#[derive(Default)]
pub struct Player {
    pub username: String,
    pub uuid: u128,
    pub skin_hash: Option<String>,
    pub skin_owner: Option<String>,
    pub slim: bool,
    pub team_color: u32,
    pub moves: Moves,
    pub presence: Vec<(u32, bool)>,
    pub equipment: Vec<(u32, Equipment)>,
    pub states: Vec<(u32, EntityState)>,
    pub riding: Vec<(u32, Option<usize>)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Equipment {
    pub held: Option<u32>,
    pub boots: Option<u32>,
    pub leggings: Option<u32>,
    pub chestplate: Option<u32>,
    pub helmet: Option<u32>,
}

#[derive(Default)]
pub struct Moves {
    pub t: Vec<u32>,
    pub pos: Vec<[f32; 3]>,
    pub yaw: Vec<f32>,
    pub pitch: Vec<f32>,
    pub head_yaw: Vec<f32>,
    pub kind: Vec<MoveKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Spawn,
    Relative,
    Look,
    LookAndRelative,
    Teleport,
    HeadLook,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EntityState {
    pub flags: u8,
    pub health: Option<f32>,
    pub potion_color: i32,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub t: u32,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventKind {
    Chat {
        text: String,
        segments: Vec<Segment>,
    },
    BlockBreak {
        breaker: i32,
        player: Option<usize>,
        pos: [i32; 3],
        stage: i8,
    },
    Collect {
        player: usize,
        collected: i32,
    },
    Swing(usize),
    Hurt(usize),
    Dead(usize),
}

fn strip(text: &str) -> String {
    legacy_segments(text, 0xFFFFFF)
        .into_iter()
        .map(|segment| segment.text)
        .collect()
}
