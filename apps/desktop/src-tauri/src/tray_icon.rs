//! Template canvases for the menu bar icon. A quota keeps the original canvas size, with a
//! full-size octopus behind an outlined capsule and an orange attention dot at the upper right.

use image::{ImageFormat, Rgba, RgbaImage};
use tauri::image::Image;

const SIDE: u32 = 44;
const PILL_X: f64 = 0.0;
const PILL_Y: f64 = 32.0;
const PILL_WIDTH: f64 = 44.0;
const PILL_HEIGHT: f64 = 12.0;
const GAP: u32 = 2;
const DOT_X: f64 = 35.0;
const DOT_Y: f64 = 9.0;
const DOT_RADIUS: f64 = 8.0;
const DOT_CLEARANCE: f64 = 2.0;
const BORDER: f64 = 2.0;

/// Keep the original octopus scale, clearing space only for visible overlays.
pub(super) fn octopus(attention: bool, quota: Option<u8>) -> Image<'static> {
    let original = tauri::include_image!("icons/tray-template.png");
    if !attention && quota.is_none() {
        return original;
    }
    let mut canvas = pixels(&original);
    if attention {
        clear_dot(&mut canvas);
    }
    if let Some(remaining) = quota {
        // The pill covers the lower tentacles, keeping the octopus at its original scale.
        for y in PILL_Y as u32 - GAP..SIDE {
            for x in 0..SIDE {
                canvas.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        draw_pill(&mut canvas, remaining.min(100));
    }
    Image::new_owned(canvas.into_raw(), SIDE, SIDE)
}

/// AppKit tints this independent template layer orange, including without a readable quota.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn dot_png() -> Option<Vec<u8>> {
    let mut canvas = RgbaImage::new(SIDE, SIDE);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let distance = (f64::from(x) + 0.5 - DOT_X).hypot(f64::from(y) + 0.5 - DOT_Y);
            let alpha = ((DOT_RADIUS + 0.5 - distance).clamp(0.0, 1.0) * 255.0) as u8;
            canvas.put_pixel(x, y, Rgba([0, 0, 0, alpha]));
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    canvas.write_to(&mut bytes, ImageFormat::Png).ok()?;
    Some(bytes.into_inner())
}

fn pixels(original: &Image<'_>) -> RgbaImage {
    RgbaImage::from_raw(
        original.width(),
        original.height(),
        original.rgba().to_vec(),
    )
    .expect("embedded tray template has one RGBA pixel per position")
}

/// A transparent ring separates the attention dot from the octopus below it.
fn clear_dot(canvas: &mut RgbaImage) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            let distance = (f64::from(x) + 0.5 - DOT_X).hypot(f64::from(y) + 0.5 - DOT_Y);
            if distance <= DOT_RADIUS + DOT_CLEARANCE + 0.5 {
                canvas.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
}

/// The capsule's signed distance; negative inside, positive outside. Its straight portion runs
/// horizontally between the two semicircle centres.
fn capsule_distance(x: f64, y: f64) -> f64 {
    let radius = PILL_HEIGHT / 2.0;
    let nearest_x = x.clamp(PILL_X + radius, PILL_X + PILL_WIDTH - radius);
    (x - nearest_x).hypot(y - (PILL_Y + radius)) - radius
}

fn draw_pill(canvas: &mut RgbaImage, remaining: u8) {
    let end = PILL_X + BORDER + (PILL_WIDTH - BORDER * 2.0) * f64::from(remaining) / 100.0;
    // Four samples per axis keep the two rounded ends smooth at the menu bar's small size.
    const SAMPLES: u32 = 4;
    for y in PILL_Y as u32..SIDE {
        for x in PILL_X as u32..(PILL_X + PILL_WIDTH) as u32 {
            let mut covered = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SAMPLES);
                    let py = f64::from(y) + (f64::from(sy) + 0.5) / f64::from(SAMPLES);
                    let distance = capsule_distance(px, py);
                    if distance <= 0.0 && (distance >= -BORDER || px <= end) {
                        covered += 1;
                    }
                }
            }
            canvas.put_pixel(
                x,
                y,
                Rgba([0, 0, 0, (covered * 255 / (SAMPLES * SAMPLES)) as u8]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(image: &Image<'_>, x: u32, y: u32) -> u8 {
        image.rgba()[((y * image.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn quota_endpoints_keep_the_outline_and_fill_only_the_remainder() {
        let empty = octopus(false, Some(0));
        let full = octopus(false, Some(100));
        let half = octopus(false, Some(50));
        for icon in [&empty, &half, &full] {
            assert_eq!((icon.width(), icon.height()), (SIDE, SIDE));
            assert!(icon.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
            assert_eq!(alpha(icon, 22, 32), 255);
            assert!((0..SIDE).all(|x| alpha(icon, x, 30) == 0 && alpha(icon, x, 31) == 0));
        }
        let original = octopus(false, None);
        for y in 0..30 {
            for x in 0..SIDE {
                assert_eq!(alpha(&empty, x, y), alpha(&original, x, y));
            }
        }
        assert!(alpha(&empty, 0, 38) > 0 && alpha(&empty, 43, 38) > 0);
        assert_eq!(alpha(&empty, 22, 38), 0);
        assert_eq!(alpha(&full, 22, 38), 255);
        assert_eq!(alpha(&half, 16, 38), 255);
        assert_eq!(alpha(&half, 28, 38), 0);
        assert_eq!(octopus(false, Some(255)).rgba(), full.rgba());
    }

    #[test]
    fn attention_dot_stays_upper_right_and_clear_of_the_quota() {
        let dot = Image::from_bytes(&dot_png().expect("dot PNG")).expect("PNG");
        assert_eq!(alpha(&dot, 35, 9), 255);
        assert!((0..SIDE).all(|y| (0..27).all(|x| alpha(&dot, x, y) == 0)));
        assert!((18..SIDE).all(|y| (0..SIDE).all(|x| alpha(&dot, x, y) == 0)));
        for quota in [None, Some(0), Some(50), Some(100)] {
            let normal = octopus(false, quota);
            let attention = octopus(true, quota);
            assert_ne!(normal.rgba(), attention.rgba());
            assert!(dot
                .rgba()
                .chunks(4)
                .zip(attention.rgba().chunks(4))
                .all(|(dot, base)| dot[3] == 0 || base[3] == 0));
            assert_eq!(
                &normal.rgba()[32 * SIDE as usize * 4..],
                &attention.rgba()[32 * SIDE as usize * 4..]
            );
        }
    }
}
