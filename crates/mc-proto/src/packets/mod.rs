use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::ConnectionState;
use crate::codec::{PacketRead, PacketWrite, var_int_len};
use crate::error::{ProtocolError, Result};

pub mod handshake;
pub mod login;
pub mod play;
pub mod status;

pub use handshake::Handshake;
pub use login::{
    EncryptionRequest, EncryptionResponse, LoginDisconnect, LoginStart, LoginSuccess,
    SetCompression,
};
pub use status::{Ping, Pong, StatusRequest, StatusResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    Serverbound,
    Clientbound,
}

pub trait Packet: Sized {
    const ID: i32;
    const STATE: ConnectionState;
    const DIRECTION: PacketDirection;

    fn read(buf: &mut Bytes) -> Result<Self>;
    fn write(&self, buf: &mut BytesMut);

    fn frame(&self) -> BytesMut {
        let mut payload = BytesMut::new();
        payload.write_var_int(Self::ID);
        self.write(&mut payload);

        let mut buf = BytesMut::with_capacity(var_int_len(payload.len() as i32) + payload.len());
        buf.write_var_int(payload.len() as i32);
        buf.put_slice(&payload);
        buf
    }
}

#[derive(Debug, Clone)]
pub struct RawPacket {
    pub id: i32,
    pub data: Bytes,
}

impl RawPacket {
    pub fn new(id: i32, data: Bytes) -> Self {
        Self { id, data }
    }

    pub fn parse<P: Packet>(&self) -> Result<P> {
        if self.id != P::ID {
            return Err(ProtocolError::InvalidPacketId(self.id, P::STATE));
        }
        P::read(&mut self.data.clone())
    }

    pub fn frame(&self) -> BytesMut {
        let payload_len = var_int_len(self.id) + self.data.len();
        let mut buf = BytesMut::with_capacity(var_int_len(payload_len as i32) + payload_len);
        buf.write_var_int(payload_len as i32);
        buf.write_var_int(self.id);
        buf.put_slice(&self.data);
        buf
    }

    pub fn read_framed(buf: &mut BytesMut) -> Result<Option<Self>> {
        let mut peek = Bytes::copy_from_slice(&buf[..]);
        let frame_len = match peek.read_var_int() {
            Ok(len) => len as usize,
            Err(ProtocolError::BufferUnderflow { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let len_size = buf.len() - peek.remaining();

        if buf.len() < len_size + frame_len {
            return Ok(None);
        }

        buf.advance(len_size);
        let mut frame = buf.split_to(frame_len).freeze();
        let id = frame.read_var_int()?;
        Ok(Some(Self { id, data: frame }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_framing_roundtrip() {
        let packet = RawPacket::new(0x02, Bytes::from_static(b"test data"));
        let mut buf = BytesMut::from(&packet.frame()[..]);
        let parsed = RawPacket::read_framed(&mut buf).unwrap().unwrap();
        assert_eq!(parsed.id, 0x02);
        assert_eq!(&parsed.data[..], b"test data");
        assert!(buf.is_empty());
    }

    #[test]
    fn partial_frame_returns_none() {
        let packet = RawPacket::new(0x02, Bytes::from_static(b"test data"));
        let mut buf = BytesMut::from(&packet.frame()[..5]);
        assert!(RawPacket::read_framed(&mut buf).unwrap().is_none());
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn back_to_back_frames() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&RawPacket::new(0x01, Bytes::from_static(b"first")).frame());
        buf.extend_from_slice(&RawPacket::new(0x02, Bytes::from_static(b"second")).frame());

        let first = RawPacket::read_framed(&mut buf).unwrap().unwrap();
        assert_eq!((first.id, &first.data[..]), (0x01, &b"first"[..]));
        let second = RawPacket::read_framed(&mut buf).unwrap().unwrap();
        assert_eq!((second.id, &second.data[..]), (0x02, &b"second"[..]));
        assert!(buf.is_empty());
    }

    #[test]
    fn typed_roundtrip_through_raw() {
        let original = LoginStart {
            name: "Notch".to_string(),
        };
        let mut framed = original.frame();
        let raw = RawPacket::read_framed(&mut framed).unwrap().unwrap();
        let parsed: LoginStart = raw.parse().unwrap();
        assert_eq!(parsed.name, "Notch");
    }
}
