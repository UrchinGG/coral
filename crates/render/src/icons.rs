use std::sync::OnceLock;

use image::{DynamicImage, RgbaImage, imageops::FilterType};

const URCHIN_PNG: &[u8] = include_bytes!("../assets/urchin.png");
const ANTISNIPER_PNG: &[u8] = include_bytes!("../assets/antisniper.png");
const MDI_ALERT_OCTAGRAM: &[u8] = include_bytes!("../assets/mdi_alert_octagram.png");
const MDI_ALERT_OCTAGRAM_OUTLINE: &[u8] =
    include_bytes!("../assets/mdi_alert_octagram_outline.png");
const MDI_TARGET_VARIANT: &[u8] = include_bytes!("../assets/mdi_target_variant.png");
const MDI_ALERT_RHOMBUS_OUTLINE: &[u8] = include_bytes!("../assets/mdi_alert_rhombus_outline.png");
const MDI_ACCOUNT_ALERT: &[u8] = include_bytes!("../assets/mdi_account_alert.png");
const MDI_INFORMATION_OUTLINE: &[u8] = include_bytes!("../assets/mdi_information_outline.png");

static URCHIN_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static ANTISNIPER_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_ALERT_OCTAGRAM_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_ALERT_OCTAGRAM_OUTLINE_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_TARGET_VARIANT_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_ALERT_RHOMBUS_OUTLINE_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_ACCOUNT_ALERT_DECODED: OnceLock<RgbaImage> = OnceLock::new();
static MDI_INFORMATION_OUTLINE_DECODED: OnceLock<RgbaImage> = OnceLock::new();

fn decode_once<'a>(lock: &'a OnceLock<RgbaImage>, bytes: &[u8]) -> &'a RgbaImage {
    lock.get_or_init(|| {
        image::load_from_memory(bytes)
            .expect("embedded icon is valid")
            .to_rgba8()
    })
}

pub fn urchin(size: u32, corner_radius: u32) -> DynamicImage {
    resize_and_round(
        decode_once(&URCHIN_DECODED, URCHIN_PNG),
        size,
        corner_radius,
    )
}

pub fn antisniper(size: u32, corner_radius: u32) -> DynamicImage {
    resize_and_round(
        decode_once(&ANTISNIPER_DECODED, ANTISNIPER_PNG),
        size,
        corner_radius,
    )
}

fn resize_and_round(rgba: &RgbaImage, size: u32, corner_radius: u32) -> DynamicImage {
    let mut resized = image::imageops::resize(rgba, size, size, FilterType::Triangle);
    let r = (corner_radius as f32).min(size as f32 / 2.0);
    for py in 0..size {
        for px in 0..size {
            let (cx, cy) = corner_center(px, py, size, size, r);
            if let Some((cx, cy)) = cx.zip(cy) {
                let dx = px as f32 - cx;
                let dy = py as f32 - cy;
                if dx * dx + dy * dy > r * r {
                    resized.get_pixel_mut(px, py)[3] = 0;
                }
            }
        }
    }
    DynamicImage::ImageRgba8(resized)
}

fn corner_center(px: u32, py: u32, w: u32, h: u32, r: f32) -> (Option<f32>, Option<f32>) {
    let cx = if (px as f32) < r {
        Some(r - 0.5)
    } else if px as f32 >= w as f32 - r {
        Some(w as f32 - r - 0.5)
    } else {
        None
    };
    let cy = if (py as f32) < r {
        Some(r - 0.5)
    } else if py as f32 >= h as f32 - r {
        Some(h as f32 - r - 0.5)
    } else {
        None
    };
    (cx, cy)
}

pub fn tag_icon(mdi_name: &str, size: u32, color: u32) -> Option<DynamicImage> {
    let decoded = match mdi_name {
        "mdi-alert-octagram" => decode_once(&MDI_ALERT_OCTAGRAM_DECODED, MDI_ALERT_OCTAGRAM),
        "mdi-alert-octagram-outline" => decode_once(
            &MDI_ALERT_OCTAGRAM_OUTLINE_DECODED,
            MDI_ALERT_OCTAGRAM_OUTLINE,
        ),
        "mdi-target-variant" => decode_once(&MDI_TARGET_VARIANT_DECODED, MDI_TARGET_VARIANT),
        "mdi-alert-rhombus-outline" => decode_once(
            &MDI_ALERT_RHOMBUS_OUTLINE_DECODED,
            MDI_ALERT_RHOMBUS_OUTLINE,
        ),
        "mdi-account-alert" => decode_once(&MDI_ACCOUNT_ALERT_DECODED, MDI_ACCOUNT_ALERT),
        "mdi-information-outline" => {
            decode_once(&MDI_INFORMATION_OUTLINE_DECODED, MDI_INFORMATION_OUTLINE)
        }
        _ => return None,
    };

    let mut resized = image::imageops::resize(decoded, size, size, FilterType::Triangle);
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    for pixel in resized.pixels_mut() {
        if pixel[3] > 0 {
            pixel[0] = r;
            pixel[1] = g;
            pixel[2] = b;
        }
    }
    Some(DynamicImage::ImageRgba8(resized))
}
