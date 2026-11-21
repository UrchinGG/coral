use glam::{Vec2, Vec3, vec2};
use replay_assets::{GLYPH_HEIGHT, TEXELS_PER_PIXEL};
use replay_render::{MeshVertex, PLAIN, SHEET, quad};

use crate::draw::Draw;

const SCALE: f32 = 1.6 / 60.0;
const RANGE: f32 = 64.0;
const SNEAKING_RANGE: f32 = 32.0;
const SNEAKING_DROP: f32 = 9.374999;
const LINE: f32 = 8.0;
const BACKGROUND: u32 = 0x0000_0040;
const FAINT: u32 = 0x20;
const OPAQUE: u32 = 0xFF;

pub fn name_tag(draw: &mut Draw, anchor: Vec3, text: &str, color: u32, sneaking: bool) {
    let range = if sneaking { SNEAKING_RANGE } else { RANGE };
    if text.is_empty() || anchor.distance_squared(draw.camera.position) > range * range {
        return;
    }
    let billboard = draw.camera.billboard();
    let (right, down) = (billboard.x_axis * SCALE, billboard.y_axis * -SCALE);
    let drop = if sneaking { SNEAKING_DROP } else { 0.0 };
    let at = |x: f32, y: f32| anchor + right * x + down * (y + drop);
    let half = (draw.assets.glyphs.width(text) / 2) as f32;
    let background = quad(
        [
            at(-half - 1.0, -1.0),
            at(half + 1.0, -1.0),
            at(half + 1.0, LINE),
            at(-half - 1.0, LINE),
        ],
        [Vec2::ZERO; 4],
        PLAIN,
        BACKGROUND,
    );
    let scene = &mut *draw.scene;
    let (faint, solid) = if sneaking {
        (&mut scene.blended, None)
    } else {
        (&mut scene.xray, Some(&mut scene.blended))
    };
    faint.extend(background);
    let first = faint.len();

    let mut x = -half;
    for segment in replay_protocol::legacy_segments(text, color) {
        let style = segment.style;
        for c in segment.text.chars() {
            let Some(glyph) = draw.assets.glyphs.get(c, style.bold) else {
                continue;
            };
            let width = f32::from(glyph.width) / TEXELS_PER_PIXEL as f32;
            let corners = [
                at(x, 0.0),
                at(x + width, 0.0),
                at(x + width, LINE),
                at(x, LINE),
            ];
            let [u, v] = [glyph.x, glyph.y].map(f32::from);
            let [u1, v1] = [u + f32::from(glyph.width), v + GLYPH_HEIGHT as f32];
            let uv = [vec2(u, v), vec2(u1, v), vec2(u1, v1), vec2(u, v1)];
            faint.extend(quad(corners, uv, SHEET, style.color << 8 | FAINT));
            x += f32::from(glyph.advance) / TEXELS_PER_PIXEL as f32;
        }
    }
    if let Some(solid) = solid {
        solid.extend(faint[first..].iter().map(|vertex| MeshVertex {
            tint: vertex.tint | OPAQUE,
            ..*vertex
        }));
    }
}
