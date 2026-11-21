use crate::{Nbt, Reader};

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub id: i16,
    pub count: i8,
    pub damage: i16,
    pub nbt: Option<Nbt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    Byte(u8),
    Short(i16),
    Int(i32),
    Float(f32),
    String(String),
    Slot(Option<Slot>),
    Position([i32; 3]),
    Rotation([f32; 3]),
}

pub type Metadata = Vec<(u8, MetadataValue)>;

impl Reader<'_> {
    pub fn slot(&mut self) -> Option<Option<Slot>> {
        let id = self.i16()?;
        if id < 0 {
            return Some(None);
        }
        Some(Some(Slot {
            id,
            count: self.i8()?,
            damage: self.i16()?,
            nbt: self.nbt()?,
        }))
    }

    pub fn metadata(&mut self) -> Option<Metadata> {
        let mut entries = Vec::new();
        loop {
            let key = self.u8()?;
            if key == 0x7F {
                return Some(entries);
            }
            let value = match key >> 5 {
                0 => MetadataValue::Byte(self.u8()?),
                1 => MetadataValue::Short(self.i16()?),
                2 => MetadataValue::Int(self.i32()?),
                3 => MetadataValue::Float(self.f32()?),
                4 => MetadataValue::String(self.string()?),
                5 => MetadataValue::Slot(self.slot()?),
                6 => MetadataValue::Position(self.triple(Self::i32)?),
                _ => MetadataValue::Rotation(self.triple(Self::f32)?),
            };
            entries.push((key & 0x1F, value));
        }
    }
}
