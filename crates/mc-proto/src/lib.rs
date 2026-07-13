pub mod codec;
pub mod compression;
pub mod crypto;
pub mod error;
pub mod io;
pub mod packets;

pub use codec::{PacketRead, PacketWrite};
pub use error::{ProtocolError, Result};
pub use io::Connection;
pub use packets::{Packet, PacketDirection, RawPacket};

pub const PROTOCOL_1_8: i32 = 47;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
    Play,
}

impl ConnectionState {
    pub fn from_handshake_next_state(next: i32) -> Option<Self> {
        match next {
            1 => Some(Self::Status),
            2 => Some(Self::Login),
            _ => None,
        }
    }
}
