use std::collections::BTreeMap;

use crate::{Reader, write_list};

const MAGIC: &[u8; 4] = b"UIT1";
const POTION: i16 = 373;
const SPLASH: i16 = 0x4000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Items {
    pub looks: BTreeMap<(i16, i16), ItemLook>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemLook {
    Block(u16),
    Sprite(u16, Hold),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Generated,
    Handheld,
    Sword,
    Bow,
}

impl Items {
    pub fn look(&self, id: i16, damage: i16) -> Option<ItemLook> {
        let damage = if id == POTION {
            damage & SPLASH
        } else {
            damage
        };
        self.looks
            .get(&(id, damage))
            .or_else(|| self.looks.get(&(id, 0)))
            .copied()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        write_list(&mut out, &self.looks, |out, (&(id, damage), look)| {
            let (kind, value) = match *look {
                ItemLook::Block(state) => (0, state),
                ItemLook::Sprite(layer, hold) => (hold as u8 + 1, layer),
            };
            out.extend(id.to_le_bytes());
            out.extend(damage.to_le_bytes());
            out.push(kind);
            out.extend(value.to_le_bytes());
        });
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Items> {
        let mut reader = Reader::new(bytes, MAGIC)?;
        let looks = reader.list(|reader| {
            let key = (reader.i16()?, reader.i16()?);
            let kind = reader.u8()?;
            let value = reader.u16()?;
            let look = match kind.checked_sub(1) {
                None => ItemLook::Block(value),
                Some(hold) => ItemLook::Sprite(value, *Hold::ALL.get(usize::from(hold))?),
            };
            Some((key, look))
        })?;
        reader.is_empty().then(|| Items {
            looks: looks.into_iter().collect(),
        })
    }
}

impl Hold {
    const ALL: [Hold; 4] = [Hold::Generated, Hold::Handheld, Hold::Sword, Hold::Bow];
}
