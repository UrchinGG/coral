use image::Rgba;

pub const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);
pub const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);
pub const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
pub const CANVAS_BACKGROUND: Rgba<u8> = Rgba([0, 0, 0, 0]);
pub const BOX_BACKGROUND: Rgba<u8> = Rgba([20, 20, 20, 150]);

pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba<u8> {
    Rgba([r, g, b, a])
}

pub fn rgb(r: u8, g: u8, b: u8) -> Rgba<u8> {
    Rgba([r, g, b, 255])
}

pub fn blend(base: Rgba<u8>, overlay: Rgba<u8>) -> Rgba<u8> {
    let sa = overlay[3] as u32;
    let da = base[3] as u32;
    let inv_sa = 255 - sa;
    let out_a = sa + (da * inv_sa + 127) / 255;
    if out_a == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let blend = |s: u8, d: u8| -> u8 {
        ((s as u32 * sa + d as u32 * da * inv_sa / 255 + out_a / 2) / out_a) as u8
    };
    Rgba([
        blend(overlay[0], base[0]),
        blend(overlay[1], base[1]),
        blend(overlay[2], base[2]),
        out_a as u8,
    ])
}
