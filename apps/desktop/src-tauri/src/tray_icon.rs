//! Template canvases for the menu bar icon. A quota keeps the original canvas size, with a
//! full-size octopus behind an outlined capsule and an orange attention dot at the upper right.
//! An account with a 5-hour and a weekly window gets a slimmer outlined capsule for the 5-hour
//! window with a thinner outlined one beneath it for the weekly one.

use image::{ImageFormat, Rgba, RgbaImage};
use tauri::image::Image;

const SIDE: u32 = 44;
const PILL_X: f64 = 0.0;
const PILL_Y: f64 = 32.0;
const PILL_WIDTH: f64 = 44.0;
const PILL_HEIGHT: f64 = 12.0;
const GAP: u32 = 2;
/// The two-layer look: the capsule for the main window, then the thinner one for the other, two
/// pixels apart so the gap stays distinct at the menu bar's size; together they clear one more
/// row of tentacles than the single capsule does.
const MAIN_Y: f64 = 29.0;
const MAIN_HEIGHT: f64 = 7.0;
const THIN_Y: f64 = 38.0;
const THIN_HEIGHT: f64 = 6.0;
const DOT_X: f64 = 35.0;
const DOT_Y: f64 = 9.0;
const DOT_RADIUS: f64 = 8.0;
const DOT_CLEARANCE: f64 = 2.0;
const BORDER: f64 = 2.0;

/// What the quota bars show, in percent remaining: the main window's, and the weekly window's
/// beneath it when the account has both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Quota {
    pub main: u8,
    pub thin: Option<u8>,
}

/// Keep the original octopus scale, clearing space only for visible overlays.
pub(super) fn octopus(attention: bool, quota: Option<Quota>) -> Image<'static> {
    let original = tauri::include_image!("icons/tray-template.png");
    if !attention && quota.is_none() {
        return original;
    }
    let mut canvas = pixels(&original);
    if attention {
        clear_dot(&mut canvas);
    }
    if let Some(quota) = quota {
        // The bars cover the lower tentacles, keeping the octopus at its original scale.
        let top = if quota.thin.is_some() { MAIN_Y } else { PILL_Y };
        for y in top as u32 - GAP..SIDE {
            for x in 0..SIDE {
                canvas.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        match quota.thin {
            Some(thin) => {
                draw_pill(&mut canvas, MAIN_Y, MAIN_HEIGHT, quota.main.min(100));
                draw_pill(&mut canvas, THIN_Y, THIN_HEIGHT, thin.min(100));
            }
            None => draw_pill(&mut canvas, PILL_Y, PILL_HEIGHT, quota.main.min(100)),
        }
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
fn capsule_distance(x: f64, y: f64, top: f64, height: f64) -> f64 {
    let radius = height / 2.0;
    let nearest_x = x.clamp(PILL_X + radius, PILL_X + PILL_WIDTH - radius);
    (x - nearest_x).hypot(y - (top + radius)) - radius
}

/// Draws an outlined capsule at `top`, filled inside the outline to `remaining` percent.
fn draw_pill(canvas: &mut RgbaImage, top: f64, height: f64, remaining: u8) {
    let end = PILL_X + BORDER + (PILL_WIDTH - BORDER * 2.0) * f64::from(remaining) / 100.0;
    // Four samples per axis keep the two rounded ends smooth at the menu bar's small size.
    const SAMPLES: u32 = 4;
    for y in top as u32..(top + height) as u32 {
        for x in PILL_X as u32..(PILL_X + PILL_WIDTH) as u32 {
            let mut covered = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SAMPLES);
                    let py = f64::from(y) + (f64::from(sy) + 0.5) / f64::from(SAMPLES);
                    let distance = capsule_distance(px, py, top, height);
                    if distance <= 0.0 && (distance >= -BORDER || px <= end) {
                        covered += 255;
                    }
                }
            }
            canvas.put_pixel(x, y, Rgba([0, 0, 0, (covered / (SAMPLES * SAMPLES)) as u8]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(image: &Image<'_>, x: u32, y: u32) -> u8 {
        image.rgba()[((y * image.width() + x) * 4 + 3) as usize]
    }

    fn single(main: u8) -> Option<Quota> {
        Some(Quota { main, thin: None })
    }

    fn layered(main: u8, thin: u8) -> Option<Quota> {
        Some(Quota {
            main,
            thin: Some(thin),
        })
    }

    #[test]
    fn quota_endpoints_keep_the_outline_and_fill_only_the_remainder() {
        let empty = octopus(false, single(0));
        let full = octopus(false, single(100));
        let half = octopus(false, single(50));
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
        assert_eq!(octopus(false, single(255)).rgba(), full.rgba());
    }

    #[test]
    fn two_layers_draw_the_main_capsule_over_a_thinner_one() {
        let original = octopus(false, None);
        let icon = octopus(false, layered(50, 25));
        assert_eq!((icon.width(), icon.height()), (SIDE, SIDE));
        assert!(icon.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        for y in 0..27 {
            for x in 0..SIDE {
                assert_eq!(alpha(&icon, x, y), alpha(&original, x, y));
            }
        }
        // The main capsule: outlined, filled to half inside.
        assert_eq!(alpha(&icon, 22, 29), 255);
        assert_eq!(alpha(&icon, 22, 35), 255);
        assert_eq!(alpha(&icon, 12, 32), 255);
        assert_eq!(alpha(&icon, 32, 32), 0);
        // Nothing in the two pixels between the two.
        assert!((0..SIDE).all(|x| alpha(&icon, x, 36) == 0 && alpha(&icon, x, 37) == 0));
        // The thinner capsule: outlined too, filled to a quarter inside.
        assert_eq!(alpha(&icon, 22, 38), 255);
        assert_eq!(alpha(&icon, 22, 43), 255);
        assert_eq!(alpha(&icon, 6, 40), 255);
        assert_eq!(alpha(&icon, 30, 40), 0);
        // The main bar at 0 leaves its capsule's interior empty; the other at 100 fills its own.
        let drained = octopus(false, layered(0, 100));
        assert_eq!(alpha(&drained, 22, 32), 0);
        assert_eq!(alpha(&drained, 30, 40), 255);
        assert_ne!(octopus(false, layered(50, 100)).rgba(), icon.rgba());
        assert_eq!(
            octopus(false, layered(255, 255)).rgba(),
            octopus(false, layered(100, 100)).rgba()
        );
    }

    #[test]
    fn attention_dot_stays_upper_right_and_clear_of_the_quota() {
        let dot = Image::from_bytes(&dot_png().expect("dot PNG")).expect("PNG");
        assert_eq!(alpha(&dot, 35, 9), 255);
        assert!((0..SIDE).all(|y| (0..27).all(|x| alpha(&dot, x, y) == 0)));
        assert!((18..SIDE).all(|y| (0..SIDE).all(|x| alpha(&dot, x, y) == 0)));
        for quota in [None, single(0), single(50), single(100), layered(50, 20)] {
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
