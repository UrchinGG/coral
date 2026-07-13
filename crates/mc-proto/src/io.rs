use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf, split};

use crate::codec::{PacketRead, frame_packet};
use crate::compression::CompressionHandler;
use crate::crypto::{Decryptor, Encryptor};
use crate::error::{ProtocolError, Result};
use crate::packets::RawPacket;

pub const MAX_PACKET_SIZE: usize = 2 * 1024 * 1024;
const READ_CHUNK: usize = 8 * 1024;

pub async fn read_packet<R: AsyncRead + Unpin>(reader: &mut R) -> Result<(i32, Bytes)> {
    let length = read_var_int_async(reader).await? as usize;
    if length > MAX_PACKET_SIZE {
        return Err(ProtocolError::PacketTooLarge(length));
    }
    let mut data = vec![0u8; length];
    reader.read_exact(&mut data).await?;
    let mut buf = Bytes::from(data);
    let id = buf.read_var_int()?;
    Ok((id, buf))
}

pub async fn write_packet<W: AsyncWrite + Unpin>(
    writer: &mut W,
    id: i32,
    payload: &[u8],
) -> Result<()> {
    writer.write_all(&frame_packet(id, payload)).await?;
    Ok(())
}

async fn read_var_int_async<R: AsyncRead + Unpin>(reader: &mut R) -> Result<i32> {
    let mut result = 0i32;
    let mut shift = 0u32;
    loop {
        let byte = reader.read_u8().await?;
        result |= ((byte & 0x7F) as i32) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 32 {
            return Err(ProtocolError::VarIntTooLarge);
        }
    }
}

pub struct Connection<S> {
    stream: S,
    read_buf: BytesMut,
    encryptor: Option<Encryptor>,
    decryptor: Option<Decryptor>,
    compression: Option<CompressionHandler>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Connection<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            read_buf: BytesMut::with_capacity(READ_CHUNK),
            encryptor: None,
            decryptor: None,
            compression: None,
        }
    }

    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        self.encryptor = Some(Encryptor::new(shared_secret));
        self.decryptor = Some(Decryptor::new(shared_secret));
    }

    pub fn enable_compression(&mut self, threshold: i32) {
        self.compression = Some(CompressionHandler::new(threshold));
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    pub async fn read_packet(&mut self) -> Result<RawPacket> {
        loop {
            if let Some(frame) = take_frame(&mut self.read_buf)? {
                return decode_frame(frame, &self.compression);
            }
            fill(&mut self.stream, &mut self.read_buf, &mut self.decryptor).await?;
        }
    }

    pub async fn write_packet(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let mut frame = encode_frame(id, payload, &self.compression)?;
        if let Some(encryptor) = &mut self.encryptor {
            encryptor.encrypt(&mut frame);
        }
        self.stream.write_all(&frame).await?;
        Ok(())
    }

    pub async fn write_raw(&mut self, packet: &RawPacket) -> Result<()> {
        self.write_packet(packet.id, &packet.data).await
    }

    pub fn split(
        self,
    ) -> (
        ConnectionReader<ReadHalf<S>>,
        ConnectionWriter<WriteHalf<S>>,
    ) {
        let (read_half, write_half) = split(self.stream);
        let threshold = self.compression.as_ref().map(CompressionHandler::threshold);
        (
            ConnectionReader {
                inner: read_half,
                read_buf: self.read_buf,
                decryptor: self.decryptor,
                compression: threshold.map(CompressionHandler::new),
            },
            ConnectionWriter {
                inner: write_half,
                encryptor: self.encryptor,
                compression: threshold.map(CompressionHandler::new),
            },
        )
    }
}

pub struct ConnectionReader<R> {
    inner: R,
    read_buf: BytesMut,
    decryptor: Option<Decryptor>,
    compression: Option<CompressionHandler>,
}

impl<R: AsyncRead + Unpin> ConnectionReader<R> {
    pub async fn read_packet(&mut self) -> Result<RawPacket> {
        loop {
            if let Some(frame) = take_frame(&mut self.read_buf)? {
                return decode_frame(frame, &self.compression);
            }
            fill(&mut self.inner, &mut self.read_buf, &mut self.decryptor).await?;
        }
    }
}

pub struct ConnectionWriter<W> {
    inner: W,
    encryptor: Option<Encryptor>,
    compression: Option<CompressionHandler>,
}

impl<W: AsyncWrite + Unpin> ConnectionWriter<W> {
    pub async fn write_packet(&mut self, id: i32, payload: &[u8]) -> Result<()> {
        let mut frame = encode_frame(id, payload, &self.compression)?;
        if let Some(encryptor) = &mut self.encryptor {
            encryptor.encrypt(&mut frame);
        }
        self.inner.write_all(&frame).await?;
        Ok(())
    }
}

async fn fill<R: AsyncRead + Unpin>(
    reader: &mut R,
    buf: &mut BytesMut,
    decryptor: &mut Option<Decryptor>,
) -> Result<()> {
    let mut chunk = [0u8; READ_CHUNK];
    let n = reader.read(&mut chunk).await?;
    if n == 0 {
        return Err(ProtocolError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "connection closed",
        )));
    }
    let bytes = &mut chunk[..n];
    if let Some(decryptor) = decryptor {
        decryptor.decrypt(bytes);
    }
    buf.extend_from_slice(bytes);
    Ok(())
}

fn encode_frame(
    id: i32,
    payload: &[u8],
    compression: &Option<CompressionHandler>,
) -> Result<BytesMut> {
    match compression {
        Some(handler) => handler.compress_packet(id, payload),
        None => Ok(frame_packet(id, payload)),
    }
}

fn decode_frame(frame: Bytes, compression: &Option<CompressionHandler>) -> Result<RawPacket> {
    let (id, data) = match compression {
        Some(handler) => handler.decompress_packet(frame)?,
        None => {
            let mut frame = frame;
            let id = frame.read_var_int()?;
            (id, frame)
        }
    };
    Ok(RawPacket { id, data })
}

fn take_frame(buf: &mut BytesMut) -> Result<Option<Bytes>> {
    let Some((len_size, frame_len)) = peek_var_int(buf) else {
        return Ok(None);
    };
    if frame_len > MAX_PACKET_SIZE {
        return Err(ProtocolError::PacketTooLarge(frame_len));
    }
    if buf.len() < len_size + frame_len {
        return Ok(None);
    }
    buf.advance(len_size);
    Ok(Some(buf.split_to(frame_len).freeze()))
}

fn peek_var_int(buf: &[u8]) -> Option<(usize, usize)> {
    let mut result = 0u32;
    let mut shift = 0u32;
    for (index, &byte) in buf.iter().enumerate() {
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Some((index + 1, result as usize));
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
    None
}
