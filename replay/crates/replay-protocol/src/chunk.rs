use crate::Reader;

pub const BLOCK_BYTES: usize = 8192;
pub const LIGHT_BYTES: usize = 2048;
pub const BIOME_BYTES: usize = 256;

pub type Section = Box<[u16; 4096]>;

#[derive(Debug)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    pub full: bool,
    pub sections: [Option<Section>; 16],
}

impl Reader<'_> {
    pub(crate) fn chunk_data(&mut self) -> Option<Column> {
        let (x, z, full, mask) = (self.i32()?, self.i32()?, self.bool()?, self.u16()?);
        let size = self.varint_len()?;
        let data = self.bytes(size)?;
        let count = mask.count_ones() as usize;
        let biome_bytes = if full { BIOME_BYTES } else { 0 };
        let fits_light_layers =
            |layers| size == count * (BLOCK_BYTES + layers * LIGHT_BYTES) + biome_bytes;
        if mask != 0 && !fits_light_layers(1) && !fits_light_layers(2) {
            return None;
        }
        Some(Column {
            x,
            z,
            full,
            sections: sections(mask, data),
        })
    }

    pub(crate) fn map_chunk_bulk(&mut self) -> Option<Vec<Column>> {
        let sky_light = self.bool()?;
        let masks = self.list(|reader| Some((reader.i32()?, reader.i32()?, reader.u16()?)))?;
        let stride = BLOCK_BYTES + LIGHT_BYTES + if sky_light { LIGHT_BYTES } else { 0 };
        masks
            .into_iter()
            .map(|(x, z, mask)| {
                let data = self.bytes(mask.count_ones() as usize * stride + BIOME_BYTES)?;
                Some(Column {
                    x,
                    z,
                    full: true,
                    sections: sections(mask, data),
                })
            })
            .collect()
    }
}

fn sections(mask: u16, data: &[u8]) -> [Option<Section>; 16] {
    let mut blocks = data.chunks_exact(BLOCK_BYTES);
    std::array::from_fn(|y| {
        if mask & 1 << y == 0 {
            return None;
        }
        let mut states = Box::new([0; 4096]);
        for (state, bytes) in states.iter_mut().zip(blocks.next()?.chunks_exact(2)) {
            *state = u16::from_le_bytes([bytes[0], bytes[1]]);
        }
        Some(states)
    })
}
