use std::ops::Range;

use replay_assets::{Reader, write_list};

use crate::{Block, BlockModels, Face, Flags, Form, Quad, Rule};

const MAGIC: &[u8; 4] = b"UBM3";
const SCALE: f32 = 2048.0;

impl BlockModels {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for layer in [self.water, self.lava, self.item_frame]
            .iter()
            .chain(&self.destroy)
        {
            out.extend(layer.to_le_bytes());
        }
        write_list(&mut out, &self.blocks, |out, block| {
            out.extend(block.flags.0.to_le_bytes());
            out.push(block.rule as u8);
            out.extend((block.first_form as u32).to_le_bytes());
            out.extend(block.particle.to_le_bytes());
        });
        write_list(&mut out, &self.forms, |out, form| {
            write_range(out, &form.models);
            out.extend(form.tint.to_le_bytes());
        });
        write_list(&mut out, &self.models, write_range);
        write_list(&mut out, &self.quads, |out, quad| {
            for value in quad
                .corners
                .as_flattened()
                .iter()
                .chain(quad.uv.as_flattened())
            {
                out.extend(((value * SCALE).round() as i16).to_le_bytes());
            }
            out.extend(quad.layer.to_le_bytes());
            out.push(quad.face as u8);
            out.push(quad.cull.map_or(u8::MAX, |face| face as u8));
            out.push(quad.tinted as u8 | (quad.shade as u8) << 1 | (quad.smooth as u8) << 2);
        });
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<BlockModels> {
        let mut reader = Reader::new(bytes, MAGIC)?;
        let water = reader.u16()?;
        let lava = reader.u16()?;
        let item_frame = reader.u16()?;
        let mut destroy = [0; 10];
        for layer in &mut destroy {
            *layer = reader.u16()?;
        }
        let models = BlockModels {
            water,
            lava,
            item_frame,
            destroy,
            blocks: reader.list(|reader| {
                Some(Block {
                    flags: Flags(reader.u16()?),
                    rule: *Rule::ALL.get(usize::from(reader.u8()?))?,
                    first_form: reader.u32()? as usize,
                    particle: reader.u16()?,
                })
            })?,
            forms: reader.list(|reader| {
                Some(Form {
                    models: range(reader)?,
                    tint: reader.u32()?,
                })
            })?,
            models: reader.list(range)?,
            quads: reader.list(|reader| {
                let corners = [
                    scaled(reader)?,
                    scaled(reader)?,
                    scaled(reader)?,
                    scaled(reader)?,
                ];
                let uv = [
                    scaled(reader)?,
                    scaled(reader)?,
                    scaled(reader)?,
                    scaled(reader)?,
                ];
                let layer = reader.u16()?;
                let face = *Face::ALL.get(usize::from(reader.u8()?))?;
                let cull = Face::ALL.get(usize::from(reader.u8()?)).copied();
                let bits = reader.u8()?;
                Some(Quad {
                    corners,
                    uv,
                    layer,
                    face,
                    cull,
                    tinted: bits & 1 != 0,
                    shade: bits & 2 != 0,
                    smooth: bits & 4 != 0,
                })
            })?,
        };
        (reader.is_empty() && models.is_consistent()).then_some(models)
    }

    fn is_consistent(&self) -> bool {
        let within =
            |range: &Range<usize>, len: usize| range.start <= range.end && range.end <= len;
        !self.forms.is_empty()
            && self.blocks.iter().all(|block| {
                block
                    .first_form
                    .checked_add(block.rule.forms())
                    .is_some_and(|end| end <= self.forms.len())
            })
            && self
                .forms
                .iter()
                .all(|form| within(&form.models, self.models.len()))
            && self
                .models
                .iter()
                .all(|model| within(model, self.quads.len()))
    }
}

fn write_range(out: &mut Vec<u8>, range: &Range<usize>) {
    out.extend((range.start as u32).to_le_bytes());
    out.extend((range.end as u32).to_le_bytes());
}

fn range(reader: &mut Reader) -> Option<Range<usize>> {
    Some(reader.u32()? as usize..reader.u32()? as usize)
}

fn scaled<const N: usize>(reader: &mut Reader) -> Option<[f32; N]> {
    let mut values = [0.0; N];
    for value in &mut values {
        *value = f32::from(reader.i16()?) / SCALE;
    }
    Some(values)
}
