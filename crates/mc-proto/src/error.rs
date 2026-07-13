use crate::ConnectionState;

pub type Result<T> = std::result::Result<T, ProtocolError>;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("buffer underflow: needed {needed}, available {available}")]
    BufferUnderflow { needed: usize, available: usize },
    #[error("VarInt too large")]
    VarIntTooLarge,
    #[error("VarLong too large")]
    VarLongTooLarge,
    #[error("string too long: {0} chars exceeds max {1}")]
    StringTooLong(usize, usize),
    #[error("packet too large: {0} bytes")]
    PacketTooLarge(usize),
    #[error("invalid packet id {0:#x} for state {1:?}")]
    InvalidPacketId(i32, ConnectionState),
    #[error("invalid utf-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("encryption: {0}")]
    Encryption(String),
    #[error("compression: {0}")]
    Compression(String),
}
