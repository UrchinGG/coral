use crate::{Column, Metadata, Reader, Slot, Textures, decode_textures};

#[derive(Debug)]
pub enum Clientbound {
    JoinGame {
        entity: i32,
        gamemode: u8,
        dimension: i8,
        difficulty: u8,
        max_players: u8,
        level_type: String,
        reduced_debug_info: bool,
    },
    Chat {
        json: String,
        position: i8,
    },
    TimeUpdate {
        time_of_day: i64,
    },
    EntityEquipment {
        entity: i32,
        slot: i16,
        item: Option<Slot>,
    },
    Respawn {
        dimension: i32,
        difficulty: u8,
        gamemode: u8,
        level_type: String,
    },
    Animation {
        entity: i32,
        animation: u8,
    },
    SpawnPlayer {
        entity: i32,
        uuid: u128,
        pos: [f64; 3],
        yaw: i8,
        pitch: i8,
        current_item: i16,
        metadata: Metadata,
    },
    CollectItem {
        collected: i32,
        collector: i32,
    },
    SpawnObject {
        entity: i32,
        kind: u8,
        pos: [f64; 3],
        pitch: i8,
        yaw: i8,
        data: i32,
        velocity: Option<[i16; 3]>,
    },
    SpawnMob {
        entity: i32,
        kind: u8,
        pos: [f64; 3],
        yaw: i8,
        pitch: i8,
        head_yaw: i8,
        velocity: [i16; 3],
        metadata: Metadata,
    },
    EntityVelocity {
        entity: i32,
        velocity: [i16; 3],
    },
    DestroyEntities(Vec<i32>),
    EntityRelativeMove {
        entity: i32,
        delta: [f64; 3],
        on_ground: bool,
    },
    EntityLook {
        entity: i32,
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    },
    EntityLookAndRelativeMove {
        entity: i32,
        delta: [f64; 3],
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    },
    EntityTeleport {
        entity: i32,
        pos: [f64; 3],
        yaw: i8,
        pitch: i8,
        on_ground: bool,
    },
    EntityHeadLook {
        entity: i32,
        head_yaw: i8,
    },
    EntityStatus {
        entity: i32,
        status: i8,
    },
    AttachEntity {
        entity: i32,
        vehicle: i32,
        leash: bool,
    },
    EntityMetadata {
        entity: i32,
        metadata: Metadata,
    },
    ChunkData(Column),
    MultiBlockChange(Vec<([i32; 3], u16)>),
    BlockChange {
        pos: [i32; 3],
        state: u16,
    },
    BlockAction {
        pos: [i32; 3],
        action: u8,
        param: u8,
        block: i32,
    },
    BlockBreakAnimation {
        breaker: i32,
        pos: [i32; 3],
        stage: i8,
    },
    MapChunkBulk(Vec<Column>),
    Explosion {
        pos: [f32; 3],
        radius: f32,
        blocks: Vec<[i32; 3]>,
        knockback: [f32; 3],
    },
    Effect {
        id: i32,
        pos: [i32; 3],
        data: i32,
        disable_relative_volume: bool,
    },
    NamedSound {
        name: String,
        pos: [f64; 3],
        volume: f32,
        pitch: u8,
    },
    Particle {
        id: i32,
        long_distance: bool,
        pos: [f32; 3],
        offset: [f32; 3],
        speed: f32,
        count: i32,
        data: Vec<i32>,
    },
    PlayerListItem(Vec<(u128, PlayerListAction)>),
    ScoreboardObjective {
        name: String,
        removed: bool,
    },
    UpdateScore {
        entry: String,
        objective: String,
        value: Option<i32>,
    },
    DisplayScoreboard {
        position: i8,
        objective: String,
    },
    Teams {
        name: String,
        action: TeamAction,
    },
}

#[derive(Debug)]
pub enum PlayerListAction {
    Add {
        name: String,
        textures: Option<Textures>,
        gamemode: i32,
        ping: i32,
        display_name: Option<String>,
    },
    UpdateGamemode(i32),
    UpdateLatency(i32),
    UpdateDisplayName(Option<String>),
    Remove,
}

#[derive(Debug)]
pub enum TeamAction {
    Create {
        info: TeamInfo,
        players: Vec<String>,
    },
    Remove,
    Update(TeamInfo),
    AddPlayers(Vec<String>),
    RemovePlayers(Vec<String>),
}

#[derive(Debug)]
pub struct TeamInfo {
    pub display_name: String,
    pub prefix: String,
    pub suffix: String,
    pub friendly_fire: i8,
    pub name_tag_visibility: String,
    pub color: i8,
}

pub fn decode(id: i32, body: &[u8]) -> Option<Clientbound> {
    let mut r = Reader::new(body);
    let packet = match id {
        0x01 => Clientbound::JoinGame {
            entity: r.i32()?,
            gamemode: r.u8()?,
            dimension: r.i8()?,
            difficulty: r.u8()?,
            max_players: r.u8()?,
            level_type: r.string()?,
            reduced_debug_info: r.bool()?,
        },
        0x02 => Clientbound::Chat {
            json: r.string()?,
            position: r.i8()?,
        },
        0x03 => {
            r.i64()?;
            Clientbound::TimeUpdate {
                time_of_day: r.i64()?,
            }
        }
        0x04 => Clientbound::EntityEquipment {
            entity: r.varint()?,
            slot: r.i16()?,
            item: r.slot()?,
        },
        0x07 => Clientbound::Respawn {
            dimension: r.i32()?,
            difficulty: r.u8()?,
            gamemode: r.u8()?,
            level_type: r.string()?,
        },
        0x0B => Clientbound::Animation {
            entity: r.varint()?,
            animation: r.u8()?,
        },
        0x0C => Clientbound::SpawnPlayer {
            entity: r.varint()?,
            uuid: r.uuid()?,
            pos: fixed_point(&mut r)?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            current_item: r.i16()?,
            metadata: r.metadata()?,
        },
        0x0D => Clientbound::CollectItem {
            collected: r.varint()?,
            collector: r.varint()?,
        },
        0x0E => {
            let (entity, kind, pos) = (r.varint()?, r.u8()?, fixed_point(&mut r)?);
            let (pitch, yaw, data) = (r.i8()?, r.i8()?, r.i32()?);
            let velocity = if data == 0 {
                None
            } else {
                Some(r.triple(Reader::i16)?)
            };
            Clientbound::SpawnObject {
                entity,
                kind,
                pos,
                pitch,
                yaw,
                data,
                velocity,
            }
        }
        0x0F => Clientbound::SpawnMob {
            entity: r.varint()?,
            kind: r.u8()?,
            pos: fixed_point(&mut r)?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            head_yaw: r.i8()?,
            velocity: r.triple(Reader::i16)?,
            metadata: r.metadata()?,
        },
        0x12 => Clientbound::EntityVelocity {
            entity: r.varint()?,
            velocity: r.triple(Reader::i16)?,
        },
        0x13 => Clientbound::DestroyEntities(r.list(Reader::varint)?),
        0x15 => Clientbound::EntityRelativeMove {
            entity: r.varint()?,
            delta: relative_move(&mut r)?,
            on_ground: r.bool()?,
        },
        0x16 => Clientbound::EntityLook {
            entity: r.varint()?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            on_ground: r.bool()?,
        },
        0x17 => Clientbound::EntityLookAndRelativeMove {
            entity: r.varint()?,
            delta: relative_move(&mut r)?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            on_ground: r.bool()?,
        },
        0x18 => Clientbound::EntityTeleport {
            entity: r.varint()?,
            pos: fixed_point(&mut r)?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            on_ground: r.bool()?,
        },
        0x19 => Clientbound::EntityHeadLook {
            entity: r.varint()?,
            head_yaw: r.i8()?,
        },
        0x1A => Clientbound::EntityStatus {
            entity: r.i32()?,
            status: r.i8()?,
        },
        0x1B => Clientbound::AttachEntity {
            entity: r.i32()?,
            vehicle: r.i32()?,
            leash: r.bool()?,
        },
        0x1C => Clientbound::EntityMetadata {
            entity: r.varint()?,
            metadata: r.metadata()?,
        },
        0x21 => Clientbound::ChunkData(r.chunk_data()?),
        0x22 => {
            let (chunk_x, chunk_z) = (r.i32()? << 4, r.i32()? << 4);
            Clientbound::MultiBlockChange(r.list(|r| {
                let (horizontal, y) = (r.u8()?, r.u8()?);
                let pos = [
                    chunk_x | i32::from(horizontal >> 4),
                    i32::from(y),
                    chunk_z | i32::from(horizontal & 15),
                ];
                Some((pos, block_state(r)?))
            })?)
        }
        0x23 => Clientbound::BlockChange {
            pos: r.position()?,
            state: block_state(&mut r)?,
        },
        0x24 => Clientbound::BlockAction {
            pos: r.position()?,
            action: r.u8()?,
            param: r.u8()?,
            block: r.varint()?,
        },
        0x25 => Clientbound::BlockBreakAnimation {
            breaker: r.varint()?,
            pos: r.position()?,
            stage: r.i8()?,
        },
        0x26 => Clientbound::MapChunkBulk(r.map_chunk_bulk()?),
        0x27 => {
            let (pos, radius) = (r.triple(Reader::f32)?, r.f32()?);
            let origin = pos.map(|axis| axis as i32);
            let len = usize::try_from(r.i32()?).ok()?;
            let blocks = r.array(len, |r| {
                let offset = r.triple(Reader::i8)?;
                Some(std::array::from_fn(|axis| {
                    origin[axis].wrapping_add(offset[axis].into())
                }))
            })?;
            Clientbound::Explosion {
                pos,
                radius,
                blocks,
                knockback: r.triple(Reader::f32)?,
            }
        }
        0x28 => Clientbound::Effect {
            id: r.i32()?,
            pos: r.position()?,
            data: r.i32()?,
            disable_relative_volume: r.bool()?,
        },
        0x29 => Clientbound::NamedSound {
            name: r.string()?,
            pos: r.triple(Reader::i32)?.map(|axis| f64::from(axis) / 8.0),
            volume: r.f32()?,
            pitch: r.u8()?,
        },
        0x2A => {
            let id = r.i32()?;
            let data_len = match id {
                36 => 2,
                37 | 38 => 1,
                _ => 0,
            };
            Clientbound::Particle {
                id,
                long_distance: r.bool()?,
                pos: r.triple(Reader::f32)?,
                offset: r.triple(Reader::f32)?,
                speed: r.f32()?,
                count: r.i32()?,
                data: r.array(data_len, Reader::varint)?,
            }
        }
        0x38 => {
            let action = r.varint()?;
            Clientbound::PlayerListItem(
                r.list(|r| Some((r.uuid()?, player_list_action(r, action)?)))?,
            )
        }
        0x3B => {
            let name = r.string()?;
            let removed = match r.i8()? {
                1 => true,
                0 | 2 => {
                    r.string()?;
                    r.string()?;
                    false
                }
                _ => return None,
            };
            Clientbound::ScoreboardObjective { name, removed }
        }
        0x3C => {
            let (entry, removed, objective) = (r.string()?, r.i8()? == 1, r.string()?);
            let value = if removed { None } else { Some(r.varint()?) };
            Clientbound::UpdateScore {
                entry,
                objective,
                value,
            }
        }
        0x3D => Clientbound::DisplayScoreboard {
            position: r.i8()?,
            objective: r.string()?,
        },
        0x3E => {
            let name = r.string()?;
            let action = match r.i8()? {
                0 => TeamAction::Create {
                    info: team_info(&mut r)?,
                    players: r.list(Reader::string)?,
                },
                1 => TeamAction::Remove,
                2 => TeamAction::Update(team_info(&mut r)?),
                3 => TeamAction::AddPlayers(r.list(Reader::string)?),
                4 => TeamAction::RemovePlayers(r.list(Reader::string)?),
                _ => return None,
            };
            Clientbound::Teams { name, action }
        }
        _ => return None,
    };
    r.is_empty().then_some(packet)
}

fn fixed_point(r: &mut Reader) -> Option<[f64; 3]> {
    Some(r.triple(Reader::i32)?.map(|axis| f64::from(axis) / 32.0))
}

fn relative_move(r: &mut Reader) -> Option<[f64; 3]> {
    Some(r.triple(Reader::i8)?.map(|axis| f64::from(axis) / 32.0))
}

fn block_state(r: &mut Reader) -> Option<u16> {
    u16::try_from(r.varint()?).ok()
}

fn player_list_action(r: &mut Reader, action: i32) -> Option<PlayerListAction> {
    Some(match action {
        0 => {
            let name = r.string()?;
            let properties = r.list(|r| {
                let (name, value) = (r.string()?, r.string()?);
                r.optional(Reader::string)?;
                Some((name, value))
            })?;
            PlayerListAction::Add {
                name,
                textures: properties
                    .iter()
                    .find(|(name, _)| name == "textures")
                    .and_then(|(_, value)| decode_textures(value)),
                gamemode: r.varint()?,
                ping: r.varint()?,
                display_name: r.optional(Reader::string)?,
            }
        }
        1 => PlayerListAction::UpdateGamemode(r.varint()?),
        2 => PlayerListAction::UpdateLatency(r.varint()?),
        3 => PlayerListAction::UpdateDisplayName(r.optional(Reader::string)?),
        4 => PlayerListAction::Remove,
        _ => return None,
    })
}

fn team_info(r: &mut Reader) -> Option<TeamInfo> {
    Some(TeamInfo {
        display_name: r.string()?,
        prefix: r.string()?,
        suffix: r.string()?,
        friendly_fire: r.i8()?,
        name_tag_visibility: r.string()?,
        color: r.i8()?,
    })
}
