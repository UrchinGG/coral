use std::collections::HashMap;
use std::io::Read;

use replay_protocol::{BIOME_BYTES, BLOCK_BYTES, LIGHT_BYTES, Reader};
use serde::Deserialize;

use crate::{Error, Metadata};

const CHUNK: i32 = 0x21;
const CHUNK_BULK: i32 = 0x26;
const VERBATIM: u8 = 0;
const SINGLE: u8 = 1;
const BULK: u8 = 2;
const HASH_BYTES: usize = 16;
const LAYERS: [std::ops::Range<usize>; 3] = [
    0..BLOCK_BYTES,
    BLOCK_BYTES..BLOCK_BYTES + LIGHT_BYTES,
    BLOCK_BYTES + LIGHT_BYTES..BLOCK_BYTES + 2 * LIGHT_BYTES,
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    format: String,
    version: u32,
    packet_count: usize,
    duration_ms: u32,
    #[serde(default)]
    meta: Meta,
    streams: Vec<StreamEntry>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Meta {
    server_name: Option<String>,
    game: Option<String>,
    mode: Option<String>,
    map: Option<String>,
}

#[derive(Deserialize)]
struct StreamEntry {
    name: String,
    offset: usize,
    length: usize,
}

pub fn read(bytes: &[u8]) -> Result<(Vec<u8>, Metadata), Error> {
    let (header_length, rest) = bytes[4..].split_first_chunk().ok_or(Error::Corrupt)?;
    let (json, payload) = rest
        .split_at_checked(u32::from_be_bytes(*header_length) as usize)
        .ok_or(Error::Corrupt)?;
    let header: Header = serde_json::from_slice(json)?;
    if header.format != "urchin-replay-packets" || header.version > 1 {
        return Err(Error::UnsupportedArchive);
    }

    let streams = header
        .streams
        .iter()
        .map(|entry| {
            let compressed = payload
                .get(entry.offset..)
                .and_then(|rest| rest.get(..entry.length))
                .ok_or(Error::Corrupt)?;
            let mut data = Vec::new();
            brotli_decompressor::Decompressor::new(compressed, 4096).read_to_end(&mut data)?;
            Ok((entry.name.as_str(), data))
        })
        .collect::<Result<HashMap<_, _>, Error>>()?;
    let stream = |name| Reader::new(streams.get(name).map_or(&[][..], Vec::as_slice));

    let section_count = streams.get("sectionTable").map_or(0, Vec::len) / HASH_BYTES;
    let sections = match streams.get("sections") {
        Some(data) => {
            let mut reader = Reader::new(data);
            (0..section_count)
                .map(|_| prefixed(&mut reader))
                .collect::<Option<_>>()
                .ok_or(Error::Corrupt)?
        }
        None if section_count == 0 => Vec::new(),
        None => return Err(Error::NeedsWorldStore),
    };

    let columns = Columns {
        times: stream("times"),
        ids: stream("ids"),
        world: stream("world"),
        biomes: stream("biomes"),
        raw: stream("body.raw"),
        bodies: streams
            .iter()
            .filter_map(|(name, data)| {
                let id = i32::from_str_radix(name.strip_prefix("body.")?, 16).ok()?;
                Some((id, Reader::new(data)))
            })
            .collect(),
        sections,
    };
    let canonical = columns.rebuild(header.packet_count).ok_or(Error::Corrupt)?;

    let metadata = Metadata {
        duration_ms: header.duration_ms,
        server_name: header.meta.server_name,
        game: header.meta.game,
        mode: header.meta.mode,
        map: header.meta.map,
    };
    Ok((canonical, metadata))
}

struct Columns<'a> {
    times: Reader<'a>,
    ids: Reader<'a>,
    world: Reader<'a>,
    biomes: Reader<'a>,
    raw: Reader<'a>,
    bodies: HashMap<i32, Reader<'a>>,
    sections: Vec<&'a [u8]>,
}

struct Column<'a> {
    x: i32,
    z: i32,
    mask: u16,
    sections: Vec<&'a [u8]>,
    biomes: &'a [u8],
}

impl<'a> Columns<'a> {
    fn rebuild(mut self, packet_count: usize) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let mut time = 0i32;
        for _ in 0..packet_count {
            time = time.wrapping_add(zigzag(&mut self.times)?);
            out.extend_from_slice(&time.to_be_bytes());
            let length_at = out.len();
            out.extend_from_slice(&[0; 4]);
            match self.ids.varint()? {
                0 => out.extend_from_slice(prefixed(&mut self.raw)?),
                id @ 1.. => {
                    let id = id - 1;
                    write_varint(&mut out, id as u32);
                    match id {
                        CHUNK | CHUNK_BULK => self.write_world_entry(&mut out, id)?,
                        _ => out.extend_from_slice(prefixed(self.bodies.get_mut(&id)?)?),
                    }
                }
                _ => return None,
            }
            let length = (out.len() - length_at - 4) as u32;
            out[length_at..length_at + 4].copy_from_slice(&length.to_be_bytes());
        }
        Some(out)
    }

    fn write_world_entry(&mut self, out: &mut Vec<u8>, id: i32) -> Option<()> {
        match (self.world.u8()?, id) {
            (VERBATIM, _) => out.extend_from_slice(prefixed(&mut self.world)?),
            (SINGLE, CHUNK) => {
                let x = zigzag(&mut self.world)?;
                let z = zigzag(&mut self.world)?;
                let ground_up = self.world.u8()?;
                let mask = u16::try_from(self.world.varint()?).ok()?;
                let sky = self.world.u8()? != 0;
                let sections = self.read_sections(mask)?;
                let biomes = if ground_up != 0 {
                    self.biomes.bytes(BIOME_BYTES)?
                } else {
                    &[]
                };
                out.extend_from_slice(&x.to_be_bytes());
                out.extend_from_slice(&z.to_be_bytes());
                out.push(ground_up);
                out.extend_from_slice(&mask.to_be_bytes());
                let section_bytes = BLOCK_BYTES + LIGHT_BYTES + if sky { LIGHT_BYTES } else { 0 };
                write_varint(out, (sections.len() * section_bytes + biomes.len()) as u32);
                write_sections(out, &sections, sky)?;
                out.extend_from_slice(biomes);
            }
            (BULK, CHUNK_BULK) => {
                let sky = self.world.u8()?;
                let count = self.world.varint_len()?;
                let columns = (0..count)
                    .map(|_| {
                        let x = zigzag(&mut self.world)?;
                        let z = zigzag(&mut self.world)?;
                        let mask = u16::try_from(self.world.varint()?).ok()?;
                        let sections = self.read_sections(mask)?;
                        let biomes = self.biomes.bytes(BIOME_BYTES)?;
                        Some(Column {
                            x,
                            z,
                            mask,
                            sections,
                            biomes,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                out.push(sky);
                write_varint(out, count as u32);
                for column in &columns {
                    out.extend_from_slice(&column.x.to_be_bytes());
                    out.extend_from_slice(&column.z.to_be_bytes());
                    out.extend_from_slice(&column.mask.to_be_bytes());
                }
                for column in &columns {
                    write_sections(out, &column.sections, sky != 0)?;
                    out.extend_from_slice(column.biomes);
                }
            }
            _ => return None,
        }
        Some(())
    }

    fn read_sections(&mut self, mask: u16) -> Option<Vec<&'a [u8]>> {
        (0..mask.count_ones())
            .map(|_| self.sections.get(self.world.varint_len()?).copied())
            .collect()
    }
}

fn write_sections(out: &mut Vec<u8>, sections: &[&[u8]], sky: bool) -> Option<()> {
    for layer in &LAYERS[..if sky { 3 } else { 2 }] {
        for section in sections {
            out.extend_from_slice(section.get(layer.clone())?);
        }
    }
    Some(())
}

fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn prefixed<'a>(reader: &mut Reader<'a>) -> Option<&'a [u8]> {
    let length = reader.varint_len()?;
    reader.bytes(length)
}

fn zigzag(reader: &mut Reader) -> Option<i32> {
    let value = reader.varint()? as u32;
    Some((value >> 1) as i32 ^ -((value & 1) as i32))
}
