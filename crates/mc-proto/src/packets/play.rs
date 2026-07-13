use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::{Packet, PacketDirection};
use crate::ConnectionState;
use crate::codec::{PacketRead, PacketWrite};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct JoinGame {
    pub entity_id: i32,
    pub gamemode: u8,
    pub dimension: i8,
    pub difficulty: u8,
    pub max_players: u8,
    pub level_type: String,
    pub reduced_debug_info: bool,
}

impl Packet for JoinGame {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            entity_id: buf.get_i32(),
            gamemode: buf.get_u8(),
            dimension: buf.get_i8(),
            difficulty: buf.get_u8(),
            max_players: buf.get_u8(),
            level_type: buf.read_string()?,
            reduced_debug_info: buf.get_u8() != 0,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.put_i32(self.entity_id);
        buf.put_u8(self.gamemode);
        buf.put_i8(self.dimension);
        buf.put_u8(self.difficulty);
        buf.put_u8(self.max_players);
        buf.write_string(&self.level_type);
        buf.put_u8(self.reduced_debug_info as u8);
    }
}

#[derive(Debug, Clone)]
pub struct ClientKeepAlive {
    pub id: i32,
}

impl Packet for ClientKeepAlive {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            id: buf.read_var_int()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_var_int(self.id);
    }
}

#[derive(Debug, Clone)]
pub struct ServerChat {
    pub json: String,
    pub position: u8,
}

impl Packet for ServerChat {
    const ID: i32 = 0x02;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            json: buf.read_string()?,
            position: buf.get_u8(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.json);
        buf.put_u8(self.position);
    }
}

#[derive(Debug, Clone)]
pub struct Disconnect {
    pub reason: String,
}

impl Packet for Disconnect {
    const ID: i32 = 0x40;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            reason: buf.read_string()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.reason);
    }
}

#[derive(Debug, Clone)]
pub struct ServerKeepAlive {
    pub id: i32,
}

impl Packet for ServerKeepAlive {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            id: buf.read_var_int()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_var_int(self.id);
    }
}

#[derive(Debug, Clone)]
pub struct ClientChat {
    pub message: String,
}

impl Packet for ClientChat {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            message: buf.read_string()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.message);
    }
}

#[derive(Debug, Clone)]
pub struct PlayerPositionLook {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl Packet for PlayerPositionLook {
    const ID: i32 = 0x06;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            x: buf.get_f64(),
            y: buf.get_f64(),
            z: buf.get_f64(),
            yaw: buf.get_f32(),
            pitch: buf.get_f32(),
            on_ground: buf.get_u8() != 0,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.put_f64(self.x);
        buf.put_f64(self.y);
        buf.put_f64(self.z);
        buf.put_f32(self.yaw);
        buf.put_f32(self.pitch);
        buf.put_u8(self.on_ground as u8);
    }
}

#[derive(Debug, Clone)]
pub struct HeldItemChange {
    pub slot: i16,
}

impl Packet for HeldItemChange {
    const ID: i32 = 0x09;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            slot: buf.get_i16(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.put_i16(self.slot);
    }
}

#[derive(Debug, Clone)]
pub struct ArmAnimation;

impl Packet for ArmAnimation {
    const ID: i32 = 0x0A;
    const STATE: ConnectionState = ConnectionState::Play;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(_buf: &mut Bytes) -> Result<Self> {
        Ok(Self)
    }

    fn write(&self, _buf: &mut BytesMut) {}
}
