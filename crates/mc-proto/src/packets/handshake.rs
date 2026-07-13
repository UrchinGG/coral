use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::{Packet, PacketDirection};
use crate::ConnectionState;
use crate::codec::{PacketRead, PacketWrite};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Handshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl Packet for Handshake {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Handshake;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            protocol_version: buf.read_var_int()?,
            server_address: buf.read_string()?,
            server_port: buf.get_u16(),
            next_state: buf.read_var_int()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_var_int(self.protocol_version);
        buf.write_string(&self.server_address);
        buf.put_u16(self.server_port);
        buf.write_var_int(self.next_state);
    }
}
