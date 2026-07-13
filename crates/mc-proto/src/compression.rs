use std::io::{Read, Write};

use bytes::{BufMut, Bytes, BytesMut};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::codec::{PacketRead, PacketWrite, var_int_len};
use crate::error::{ProtocolError, Result};

const COMPRESSION_LEVEL: u32 = 1;

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 2),
        Compression::new(COMPRESSION_LEVEL),
    );
    encoder
        .write_all(data)
        .map_err(|e| ProtocolError::Compression(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| ProtocolError::Compression(e.to_string()))
}

pub fn decompress(data: &[u8], max_size: usize) -> Result<Vec<u8>> {
    let mut decompressed = Vec::new();
    ZlibDecoder::new(data)
        .take(max_size as u64)
        .read_to_end(&mut decompressed)
        .map_err(|e| ProtocolError::Compression(e.to_string()))?;
    Ok(decompressed)
}

pub struct CompressionHandler {
    threshold: i32,
}

impl CompressionHandler {
    pub fn new(threshold: i32) -> Self {
        Self { threshold }
    }

    pub fn threshold(&self) -> i32 {
        self.threshold
    }

    pub fn compress_packet(&self, packet_id: i32, data: &[u8]) -> Result<BytesMut> {
        let body_len = var_int_len(packet_id) + data.len();
        if (body_len as i32) < self.threshold {
            self.build_uncompressed(packet_id, data, body_len)
        } else {
            self.build_compressed(packet_id, data, body_len)
        }
    }

    pub fn decompress_packet(&self, mut data: Bytes) -> Result<(i32, Bytes)> {
        let uncompressed_size = data.read_var_int()?;
        if uncompressed_size == 0 {
            let packet_id = data.read_var_int()?;
            Ok((packet_id, data))
        } else {
            let mut body = Bytes::from(decompress(&data, uncompressed_size as usize)?);
            let packet_id = body.read_var_int()?;
            Ok((packet_id, body))
        }
    }

    fn build_uncompressed(&self, packet_id: i32, data: &[u8], body_len: usize) -> Result<BytesMut> {
        let packet_len = 1 + body_len;
        let mut buf = BytesMut::with_capacity(var_int_len(packet_len as i32) + packet_len);
        buf.write_var_int(packet_len as i32);
        buf.write_var_int(0);
        buf.write_var_int(packet_id);
        buf.put_slice(data);
        Ok(buf)
    }

    fn build_compressed(&self, packet_id: i32, data: &[u8], body_len: usize) -> Result<BytesMut> {
        let mut body = BytesMut::with_capacity(body_len);
        body.write_var_int(packet_id);
        body.put_slice(data);

        let compressed = compress(&body)?;
        let packet_len = var_int_len(body_len as i32) + compressed.len();

        let mut buf = BytesMut::with_capacity(var_int_len(packet_len as i32) + packet_len);
        buf.write_var_int(packet_len as i32);
        buf.write_var_int(body_len as i32);
        buf.put_slice(&compressed);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_roundtrip() {
        let original = b"Hello, Minecraft! This is some test data that should compress well.";
        let compressed = compress(original).unwrap();
        assert_eq!(decompress(&compressed, 1024).unwrap(), original);
    }

    #[test]
    fn handler_below_threshold() {
        let handler = CompressionHandler::new(256);
        let framed = handler.compress_packet(0x00, b"small").unwrap();
        let mut buf = framed.freeze();
        let _packet_length = buf.read_var_int().unwrap();
        assert_eq!(
            handler.decompress_packet(buf).unwrap(),
            (0x00, Bytes::from_static(b"small"))
        );
    }

    #[test]
    fn handler_above_threshold() {
        let handler = CompressionHandler::new(10);
        let data = b"This is a longer message that should be compressed!";
        let framed = handler.compress_packet(0x00, data).unwrap();
        let mut buf = framed.freeze();
        let _packet_length = buf.read_var_int().unwrap();
        let (id, payload) = handler.decompress_packet(buf).unwrap();
        assert_eq!(id, 0x00);
        assert_eq!(&payload[..], data);
    }
}
