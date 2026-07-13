use bytes::{Bytes, BytesMut};

use super::{Packet, PacketDirection};
use crate::ConnectionState;
use crate::codec::{PacketRead, PacketWrite};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct LoginStart {
    pub name: String,
}

impl Packet for LoginStart {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Login;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            name: buf.read_string()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.name);
    }
}

#[derive(Debug, Clone)]
pub struct EncryptionRequest {
    pub server_id: String,
    pub public_key: Vec<u8>,
    pub verify_token: Vec<u8>,
}

impl Packet for EncryptionRequest {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Login;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            server_id: buf.read_string()?,
            public_key: buf.read_byte_array()?.to_vec(),
            verify_token: buf.read_byte_array()?.to_vec(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.server_id);
        buf.write_byte_array(&self.public_key);
        buf.write_byte_array(&self.verify_token);
    }
}

#[derive(Debug, Clone)]
pub struct EncryptionResponse {
    pub shared_secret: Vec<u8>,
    pub verify_token: Vec<u8>,
}

impl Packet for EncryptionResponse {
    const ID: i32 = 0x01;
    const STATE: ConnectionState = ConnectionState::Login;
    const DIRECTION: PacketDirection = PacketDirection::Serverbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            shared_secret: buf.read_byte_array()?.to_vec(),
            verify_token: buf.read_byte_array()?.to_vec(),
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_byte_array(&self.shared_secret);
        buf.write_byte_array(&self.verify_token);
    }
}

#[derive(Debug, Clone)]
pub struct LoginSuccess {
    pub uuid: String,
    pub username: String,
}

impl Packet for LoginSuccess {
    const ID: i32 = 0x02;
    const STATE: ConnectionState = ConnectionState::Login;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            uuid: buf.read_string()?,
            username: buf.read_string()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_string(&self.uuid);
        buf.write_string(&self.username);
    }
}

#[derive(Debug, Clone)]
pub struct SetCompression {
    pub threshold: i32,
}

impl Packet for SetCompression {
    const ID: i32 = 0x03;
    const STATE: ConnectionState = ConnectionState::Login;
    const DIRECTION: PacketDirection = PacketDirection::Clientbound;

    fn read(buf: &mut Bytes) -> Result<Self> {
        Ok(Self {
            threshold: buf.read_var_int()?,
        })
    }

    fn write(&self, buf: &mut BytesMut) {
        buf.write_var_int(self.threshold);
    }
}

#[derive(Debug, Clone)]
pub struct LoginDisconnect {
    pub reason: String,
}

impl Packet for LoginDisconnect {
    const ID: i32 = 0x00;
    const STATE: ConnectionState = ConnectionState::Login;
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
