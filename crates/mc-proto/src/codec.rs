use bytes::{Buf, BufMut, Bytes};

use crate::error::{ProtocolError, Result};

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;
const BITS_PER_SEGMENT: u32 = 7;

const UUID_BYTE_LEN: usize = 16;
const POSITION_BYTE_LEN: usize = 8;

const STRING_MAX_CHARS: usize = 32767;
const STRING_MAX_BYTES_PER_CHAR: usize = 4;

const POSITION_X_BITS: i32 = 38;
const POSITION_Y_BITS: i32 = 26;
const POSITION_Y_MASK: i64 = 0xFFF;
const POSITION_Z_MASK: i64 = 0x3FFFFFF;
const POSITION_X_MASK: i64 = 0x3FFFFFF;

pub trait PacketRead: Buf {
    #[inline]
    fn read_var_int(&mut self) -> Result<i32> {
        let mut result: i32 = 0;
        let mut bit_offset: u32 = 0;

        loop {
            if !self.has_remaining() {
                return Err(ProtocolError::BufferUnderflow {
                    needed: 1,
                    available: 0,
                });
            }

            let byte = self.get_u8();
            result |= ((byte & SEGMENT_BITS) as i32) << bit_offset;

            if byte & CONTINUE_BIT == 0 {
                break;
            }

            bit_offset += BITS_PER_SEGMENT;
            if bit_offset >= 32 {
                return Err(ProtocolError::VarIntTooLarge);
            }
        }

        Ok(result)
    }

    #[inline]
    fn read_var_long(&mut self) -> Result<i64> {
        let mut result: i64 = 0;
        let mut bit_offset: u32 = 0;

        loop {
            if !self.has_remaining() {
                return Err(ProtocolError::BufferUnderflow {
                    needed: 1,
                    available: 0,
                });
            }

            let byte = self.get_u8();
            result |= ((byte & SEGMENT_BITS) as i64) << bit_offset;

            if byte & CONTINUE_BIT == 0 {
                break;
            }

            bit_offset += BITS_PER_SEGMENT;
            if bit_offset >= 64 {
                return Err(ProtocolError::VarLongTooLarge);
            }
        }

        Ok(result)
    }

    #[inline]
    fn read_string(&mut self) -> Result<String> {
        self.read_string_max(STRING_MAX_CHARS)
    }

    #[inline]
    fn read_string_max(&mut self, max_len: usize) -> Result<String> {
        let byte_len = self.read_var_int()? as usize;
        let max_byte_len = max_len * STRING_MAX_BYTES_PER_CHAR;

        if byte_len > max_byte_len {
            return Err(ProtocolError::StringTooLong(byte_len, max_byte_len));
        }

        if self.remaining() < byte_len {
            return Err(ProtocolError::BufferUnderflow {
                needed: byte_len,
                available: self.remaining(),
            });
        }

        let bytes = self.copy_to_bytes(byte_len);
        let string = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8(bytes.to_vec())?,
        };

        let char_count = string.chars().count();
        if char_count > max_len {
            return Err(ProtocolError::StringTooLong(char_count, max_len));
        }

        Ok(string)
    }

    #[inline]
    fn read_uuid(&mut self) -> Result<uuid::Uuid> {
        if self.remaining() < UUID_BYTE_LEN {
            return Err(ProtocolError::BufferUnderflow {
                needed: UUID_BYTE_LEN,
                available: self.remaining(),
            });
        }

        let mut bytes = [0u8; UUID_BYTE_LEN];
        self.copy_to_slice(&mut bytes);
        Ok(uuid::Uuid::from_bytes(bytes))
    }

    #[inline]
    fn read_position(&mut self) -> Result<(i32, i16, i32)> {
        if self.remaining() < POSITION_BYTE_LEN {
            return Err(ProtocolError::BufferUnderflow {
                needed: POSITION_BYTE_LEN,
                available: self.remaining(),
            });
        }

        let packed = self.get_i64();
        let x = (packed >> POSITION_X_BITS) as i32;
        let y = ((packed >> POSITION_Y_BITS) & POSITION_Y_MASK) as i16;
        let z = (packed << POSITION_X_BITS >> POSITION_X_BITS) as i32;

        Ok((x, y, z))
    }

    #[inline]
    fn read_byte_array(&mut self) -> Result<Bytes> {
        let len = self.read_var_int()? as usize;
        if self.remaining() < len {
            return Err(ProtocolError::BufferUnderflow {
                needed: len,
                available: self.remaining(),
            });
        }
        Ok(self.copy_to_bytes(len))
    }

    #[inline]
    fn read_remaining(&mut self) -> Bytes {
        self.copy_to_bytes(self.remaining())
    }

    #[inline]
    fn read_fixed_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.remaining() < N {
            return Err(ProtocolError::BufferUnderflow {
                needed: N,
                available: self.remaining(),
            });
        }
        let mut arr = [0u8; N];
        self.copy_to_slice(&mut arr);
        Ok(arr)
    }
}

pub trait PacketWrite: BufMut {
    #[inline]
    fn write_var_int(&mut self, mut value: i32) {
        loop {
            if (value & !(SEGMENT_BITS as i32)) == 0 {
                self.put_u8(value as u8);
                return;
            }

            self.put_u8(((value & SEGMENT_BITS as i32) | CONTINUE_BIT as i32) as u8);
            value = ((value as u32) >> BITS_PER_SEGMENT) as i32;
        }
    }

    #[inline]
    fn write_var_long(&mut self, mut value: i64) {
        loop {
            if (value & !(SEGMENT_BITS as i64)) == 0 {
                self.put_u8(value as u8);
                return;
            }

            self.put_u8(((value & SEGMENT_BITS as i64) | CONTINUE_BIT as i64) as u8);
            value = ((value as u64) >> BITS_PER_SEGMENT) as i64;
        }
    }

    #[inline]
    fn write_string(&mut self, s: &str) {
        self.write_var_int(s.len() as i32);
        self.put_slice(s.as_bytes());
    }

    #[inline]
    fn write_uuid(&mut self, uuid: uuid::Uuid) {
        self.put_slice(uuid.as_bytes());
    }

    #[inline]
    fn write_position(&mut self, x: i32, y: i16, z: i32) {
        let packed = ((x as i64 & POSITION_X_MASK) << POSITION_X_BITS)
            | ((y as i64 & POSITION_Y_MASK) << POSITION_Y_BITS)
            | (z as i64 & POSITION_Z_MASK);
        self.put_i64(packed);
    }

    #[inline]
    fn write_byte_array(&mut self, data: &[u8]) {
        self.write_var_int(data.len() as i32);
        self.put_slice(data);
    }
}

impl<T: Buf> PacketRead for T {}
impl<T: BufMut> PacketWrite for T {}

#[inline]
pub fn var_int_len(value: i32) -> usize {
    let mut u_value = value as u32;
    let mut byte_count = 0;

    loop {
        byte_count += 1;
        u_value >>= BITS_PER_SEGMENT;
        if u_value == 0 {
            break;
        }
    }

    byte_count
}

#[inline]
pub fn frame_packet(packet_id: i32, data: &[u8]) -> bytes::BytesMut {
    let body_len = var_int_len(packet_id) + data.len();

    let mut buf = bytes::BytesMut::with_capacity(var_int_len(body_len as i32) + body_len);
    buf.write_var_int(body_len as i32);
    buf.write_var_int(packet_id);
    buf.put_slice(data);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn var_int_roundtrip() {
        for value in [
            0,
            1,
            127,
            128,
            255,
            25565,
            2097151,
            2147483647,
            -1,
            -2147483648,
        ] {
            let mut buf = BytesMut::new();
            buf.write_var_int(value);
            let mut reader = buf.freeze();
            assert_eq!(value, reader.read_var_int().unwrap());
        }
    }

    #[test]
    fn var_long_roundtrip() {
        for value in [0i64, 1, 127, 128, 2147483647, i64::MAX, -1, i64::MIN] {
            let mut buf = BytesMut::new();
            buf.write_var_long(value);
            let mut reader = buf.freeze();
            assert_eq!(value, reader.read_var_long().unwrap());
        }
    }

    #[test]
    fn string_roundtrip() {
        for s in ["", "Hello", "Minecraft", "Hello, World!"] {
            let mut buf = BytesMut::new();
            buf.write_string(s);
            let mut reader = buf.freeze();
            assert_eq!(s, reader.read_string().unwrap());
        }
    }

    #[test]
    fn position_roundtrip() {
        let cases: [(i32, i16, i32); 5] = [
            (0, 0, 0),
            (1, 2, 3),
            (-1, 64, -1),
            (33554431, 4095, 33554431),
            (-33554432, 0, -33554432),
        ];
        for (x, y, z) in cases {
            let mut buf = BytesMut::new();
            buf.write_position(x, y, z);
            let mut reader = buf.freeze();
            assert_eq!((x, y, z), reader.read_position().unwrap());
        }
    }

    #[test]
    fn var_int_len_matches() {
        assert_eq!(var_int_len(0), 1);
        assert_eq!(var_int_len(127), 1);
        assert_eq!(var_int_len(128), 2);
        assert_eq!(var_int_len(2097151), 3);
        assert_eq!(var_int_len(2097152), 4);
        assert_eq!(var_int_len(-1), 5);
    }

    #[test]
    fn string_max_length_enforced() {
        let mut buf = BytesMut::new();
        buf.write_string(&"a".repeat(40000));
        let mut reader = buf.freeze();
        assert!(reader.read_string_max(100).is_err());
    }
}
