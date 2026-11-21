use std::collections::HashMap;

use crate::Reader;

const MAX_DEPTH: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Nbt>),
    Compound(HashMap<String, Nbt>),
    IntArray(Vec<i32>),
}

impl Nbt {
    pub fn get(&self, key: &str) -> Option<&Nbt> {
        match self {
            Nbt::Compound(fields) => fields.get(key),
            _ => None,
        }
    }
}

impl Reader<'_> {
    pub fn nbt(&mut self) -> Option<Option<Nbt>> {
        match self.u8()? {
            0 => Some(None),
            10 => {
                self.nbt_string()?;
                self.nbt_payload(10, 0).map(Some)
            }
            _ => None,
        }
    }

    fn nbt_string(&mut self) -> Option<String> {
        let len = usize::from(self.u16()?);
        Some(String::from_utf8_lossy(self.bytes(len)?).into_owned())
    }

    fn nbt_len(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }

    fn nbt_payload(&mut self, kind: u8, depth: usize) -> Option<Nbt> {
        if depth > MAX_DEPTH {
            return None;
        }
        Some(match kind {
            1 => Nbt::Byte(self.i8()?),
            2 => Nbt::Short(self.i16()?),
            3 => Nbt::Int(self.i32()?),
            4 => Nbt::Long(self.i64()?),
            5 => Nbt::Float(self.f32()?),
            6 => Nbt::Double(self.f64()?),
            7 => {
                let len = self.nbt_len()?;
                Nbt::ByteArray(self.bytes(len)?.to_vec())
            }
            8 => Nbt::String(self.nbt_string()?),
            9 => {
                let element = self.u8()?;
                let len = self.nbt_len()?;
                Nbt::List(self.array(len, |reader| reader.nbt_payload(element, depth + 1))?)
            }
            10 => {
                let mut fields = HashMap::new();
                loop {
                    let field = self.u8()?;
                    if field == 0 {
                        break Nbt::Compound(fields);
                    }
                    let name = self.nbt_string()?;
                    fields.insert(name, self.nbt_payload(field, depth + 1)?);
                }
            }
            11 => {
                let len = self.nbt_len()?;
                Nbt::IntArray(self.array(len, Self::i32)?)
            }
            _ => return None,
        })
    }
}
