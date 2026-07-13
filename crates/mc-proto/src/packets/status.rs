use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::{Packet, PacketDirection};
use crate::ConnectionState;
use crate::codec::{PacketRead, PacketWrite};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct StatusRequest;

impl Packet for StatusRequest {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Status;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(_buf: &mut Bytes) -> Result<Self> {
        Ok(Self)
    }

    fn write(&self, _buf: &mut BytesMut) {}
}

#[derive(Debug, Clone)]
pub struct StatusResponse {
    pub json: String,
}

impl Packet for StatusResponse {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Status;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            json: buf.read_string()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.json);
    }
}

#[derive(Debug, Clone)]
pub struct Ping {
    pub payload: i64,
}

impl Packet for Ping {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Status;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            payload: buf.get_i64(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.put_i64(self.payload);
    }
}

#[derive(Debug, Clone)]
pub struct Pong {
    pub payload: i64,
}

impl Packet for Pong {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Status;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            payload: buf.get_i64(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.put_i64(self.payload);
    }
}
