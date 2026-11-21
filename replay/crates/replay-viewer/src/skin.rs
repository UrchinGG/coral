use replay_render::SKIN_SIZE;

const SIZE: usize = SKIN_SIZE as usize;
const BYTES: usize = SIZE * SIZE * 4;
const LEGACY_LIMBS: [[usize; 5]; 12] = [
    [20, 48, 4, 16, 4],
    [24, 48, 8, 16, 4],
    [16, 52, 8, 20, 12],
    [20, 52, 4, 20, 12],
    [24, 52, 0, 20, 12],
    [28, 52, 12, 20, 12],
    [36, 48, 44, 16, 4],
    [40, 48, 48, 16, 4],
    [32, 52, 48, 20, 12],
    [36, 52, 44, 20, 12],
    [40, 52, 40, 20, 12],
    [44, 52, 52, 20, 12],
];
const OPAQUE: [[usize; 4]; 3] = [[0, 0, 32, 16], [0, 16, 64, 32], [16, 48, 48, 64]];
const CLEARABLE: [[usize; 4]; 6] = [
    [32, 0, 64, 32],
    [0, 32, 16, 48],
    [16, 32, 40, 48],
    [40, 32, 56, 48],
    [0, 48, 16, 64],
    [48, 48, 64, 64],
];

const FALLBACK_SKIN: [u8; 3] = [0xB9, 0x86, 0x68];
const FALLBACK_SHADOW: [u8; 3] = [0x83, 0x5A, 0x49];
const DEPTH: usize = 4;

pub fn normalize(rgba: &[u8]) -> Option<Vec<u8>> {
    let mut skin = match rgba.len() {
        BYTES => rgba.to_vec(),
        length if length == BYTES / 2 => legacy(rgba),
        _ => return None,
    };
    for area in OPAQUE {
        for texel in texels(area) {
            skin[texel + 3] = u8::MAX;
        }
    }
    for area in CLEARABLE {
        if texels(area).all(|texel| skin[texel + 3] >= 128) {
            texels(area).for_each(|texel| skin[texel + 3] = 0);
        }
    }
    Some(skin)
}

pub fn fallback(name: &str) -> Vec<u8> {
    let hash = name.chars().fold(0x811C_9DC5_u32, |hash, c| {
        (hash ^ u32::from(c.encode_utf16(&mut [0; 2])[0])).wrapping_mul(0x0100_0193)
    });
    let hue = (hash % 360) as f32;
    let shirt = hsl(hue, 0.55, 0.42);
    let trousers = hsl((hue + 190.0) % 360.0, 0.35, 0.24);
    let hair = hsl((hue + 20.0) % 360.0, 0.3, 0.17);
    let limb = [FALLBACK_SKIN, FALLBACK_SHADOW, shirt, FALLBACK_SKIN];
    let parts = [
        ([0, 0, 8, 8], [FALLBACK_SKIN, FALLBACK_SHADOW, hair, hair]),
        ([16, 16, 8, 12], [shirt; 4]),
        ([40, 16, 4, 12], limb),
        ([32, 48, 4, 12], limb),
        ([0, 16, 4, 12], [trousers; 4]),
        ([16, 48, 4, 12], [trousers; 4]),
    ];
    let mut skin = vec![0; BYTES];
    for ([x, y, width, height], [front, side, top, back]) in parts {
        let faces = [
            ([x, y + DEPTH, x + DEPTH, y + DEPTH + height], side),
            (
                [
                    x + DEPTH + width,
                    y + DEPTH,
                    x + 2 * DEPTH + width,
                    y + DEPTH + height,
                ],
                side,
            ),
            ([x + DEPTH, y, x + DEPTH + width, y + DEPTH], top),
            (
                [x + DEPTH + width, y, x + DEPTH + 2 * width, y + DEPTH],
                back,
            ),
            (
                [
                    x + 2 * DEPTH + width,
                    y + DEPTH,
                    x + 2 * (DEPTH + width),
                    y + DEPTH + height,
                ],
                back,
            ),
            (
                [x + DEPTH, y + DEPTH, x + DEPTH + width, y + DEPTH + height],
                front,
            ),
        ];
        for (area, [red, green, blue]) in faces {
            for texel in texels(area) {
                skin[texel..texel + 4].copy_from_slice(&[red, green, blue, u8::MAX]);
            }
        }
    }
    skin
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> [u8; 3] {
    let chroma = saturation * lightness.min(1.0 - lightness);
    [0.0, 8.0, 4.0].map(|offset: f32| {
        let k = (offset + hue / 30.0) % 12.0;
        let value = lightness - chroma * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (value * 255.0).round() as u8
    })
}

fn legacy(rgba: &[u8]) -> Vec<u8> {
    let mut skin = rgba.to_vec();
    skin.resize(BYTES, 0);
    for [to_x, to_y, from_x, from_y, height] in LEGACY_LIMBS {
        for row in 0..height {
            for column in 0..4 {
                let from = texel(from_x + column, from_y + row);
                let to = texel(to_x + 3 - column, to_y + row);
                skin.copy_within(from..from + 4, to);
            }
        }
    }
    skin
}

fn texels([left, top, right, bottom]: [usize; 4]) -> impl Iterator<Item = usize> {
    (top..bottom).flat_map(move |y| (left..right).map(move |x| texel(x, y)))
}

fn texel(x: usize, y: usize) -> usize {
    (y * SIZE + x) * 4
}
