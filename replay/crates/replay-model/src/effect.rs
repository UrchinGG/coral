use replay_protocol::Clientbound;

use crate::item::{known_block, known_item};

const ITEM_CRACK: i32 = 36;
const BLOCK_CRACK: i32 = 37;
const BLOCK_DUST: i32 = 38;
const BLOCK_BREAK: i32 = 2001;
const LID_BLOCKS: [i32; 3] = [54, 130, 146];

#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub t: u32,
    pub kind: EffectKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectKind {
    Particle {
        id: i32,
        pos: [f32; 3],
        offset: [f32; 3],
        speed: f32,
        count: i32,
        item: Option<(i32, i32)>,
    },
    Sound {
        name: String,
        pos: [f32; 3],
        volume: f32,
        pitch: f32,
    },
    World {
        id: i32,
        pos: [i32; 3],
        data: i32,
    },
    Lid {
        pos: [i32; 3],
        viewers: u8,
    },
    Crit {
        magic: bool,
        player: Option<usize>,
        pos: [f32; 3],
        height: f32,
    },
    Explosion {
        pos: [f32; 3],
        radius: f32,
        blocks: Vec<[i32; 3]>,
    },
}

impl EffectKind {
    pub(crate) fn of(packet: Clientbound) -> Option<EffectKind> {
        Some(match packet {
            Clientbound::Particle {
                id,
                pos,
                offset,
                speed,
                count,
                data,
                ..
            } => EffectKind::Particle {
                id,
                pos,
                offset,
                speed,
                count,
                item: match (id, data.as_slice()) {
                    (ITEM_CRACK, &[item, meta]) if known_block(item) || known_item(item) => {
                        Some((item, meta))
                    }
                    (BLOCK_CRACK | BLOCK_DUST, &[state]) if known_block(state & 4095) => {
                        Some((state & 4095, state >> 12))
                    }
                    (ITEM_CRACK | BLOCK_CRACK | BLOCK_DUST, _) => return None,
                    _ => None,
                },
            },
            Clientbound::NamedSound {
                name,
                pos,
                volume,
                pitch,
            } if !name.is_empty() => EffectKind::Sound {
                name,
                pos: pos.map(|axis| axis as f32),
                volume,
                pitch: f32::from(pitch) / 63.0,
            },
            Clientbound::Effect { id, pos, data, .. }
                if id != 0 && (id != BLOCK_BREAK || known_block(data & 4095)) =>
            {
                EffectKind::World { id, pos, data }
            }
            Clientbound::BlockAction {
                pos,
                action: 1,
                param,
                block,
            } if LID_BLOCKS.contains(&block) => EffectKind::Lid {
                pos,
                viewers: param,
            },
            _ => return None,
        })
    }
}
