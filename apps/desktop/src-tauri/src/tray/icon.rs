//! Template canvases for the menu bar icon, 36 pixels square: the status item shows them 18 points
//! tall, so one canvas pixel is one pixel on a Retina display. A quota keeps the canvas size, with
//! a full-size octopus behind an outlined capsule and an orange attention dot at the upper right.
//! An account with a 5-hour and a weekly window gets a slimmer outlined capsule for the 5-hour
//! window with a thinner outlined one beneath it for the weekly one. While the computer is kept
//! awake the octopus's eye sockets are cleared, for red eyes to be laid in them. Windows has its
//! own coloured picture instead (`app_icon`).

use image::{ImageFormat, Rgba, RgbaImage};
use tauri::image::Image;

const SIDE: u32 = 36;
const PILL_X: f64 = 0.0;
/// The single capsule: its outline is a quarter of its height, as in
/// `docs/product/menu-bar-icon.md`, "The icon's look".
const PILL_Y: f64 = 28.0;
const PILL_WIDTH: f64 = 36.0;
const PILL_HEIGHT: f64 = 8.0;
const GAP: u32 = 2;
/// The two-layer look: the capsule for the main window, then the thinner one for the other, two
/// pixels apart so the gap stays distinct at the menu bar's size; together they clear seven more
/// pixel rows of tentacles than the single capsule does. Main capsule, gap and thinner capsule
/// stand 7 : 2 : 6, close to the two-layer proportion in `docs/product/menu-bar-icon.md`, "The
/// icon's look", on whole pixel rows.
const MAIN_Y: f64 = 21.0;
const MAIN_HEIGHT: f64 = 7.0;
const THIN_Y: f64 = 30.0;
const THIN_HEIGHT: f64 = 6.0;
const DOT_X: f64 = 29.5;
const DOT_Y: f64 = 6.5;
const DOT_RADIUS: f64 = 5.5;
const DOT_CLEARANCE: f64 = 1.0;
const BORDER: f64 = 2.0;
/// The octopus's eye socket as the app icon draws it: a slanted, winged shape, high at the outer
/// side with its brow edge falling toward the middle and a point at the lower inner corner. One
/// row per string, `#` for a pixel of the socket, starting at (`SOCKET_X`, `SOCKET_Y`) for the left
/// eye; the right eye is its mirror image. The artwork has each socket as one flat partial alpha,
/// and while the computer is kept awake the whole socket is cleared and drawn red instead. The
/// sockets sit high enough for the cleared bands above the attention dot and the quota bars to
/// stay clear of them.
const SOCKET: [&str; 5] = ["##....", "####..", "######", ".#####", "...###"];
const SOCKET_X: u32 = 10;
const SOCKET_Y: u32 = 13;
/// The same eyes on the app icon's 32 pixel picture, which has red ones of its own there; they are
/// drawn larger and brighter, with no ring, as the picture's body is not a flat silhouette.
const APP_EYES: [(f64, f64); 2] = [(12.5, 15.0), (19.5, 15.0)];
const APP_EYE_RADIUS: f64 = 2.4;
/// The attention dot's colour on the coloured picture, the system orange the macOS dot is tinted with.
const DOT_COLOUR: [u8; 3] = [255, 149, 0];
/// The eyes' colour on the coloured picture, the system red the macOS eyes are tinted with.
const EYE_COLOUR: [u8; 3] = [255, 59, 48];

/// What the quota bars show, in percent remaining: the main window's, and the weekly window's
/// beneath it when the account has both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Quota {
    pub main: u8,
    pub thin: Option<u8>,
}

/// Keep the original octopus scale, clearing space only for visible overlays. `awake` cuts the
/// eyes out for `eyes_png` to fill.
pub(super) fn octopus(attention: bool, awake: bool, quota: Option<Quota>) -> Image<'static> {
    let original = tauri::include_image!("icons/tray-template.png");
    if !attention && !awake && quota.is_none() {
        return original;
    }
    let mut canvas = pixels(&original);
    if attention {
        clear_dot(&mut canvas);
    }
    if awake {
        clear_eyes(&mut canvas);
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

/// The notification area's picture: the app icon in its own colours, since a template octopus would
/// vanish on a dark taskbar. Attention swaps in a copy with the dot at its upper right, laid out as
/// in the template canvas (scaled to the picture) and drawn the same way, a ring of the icon cleared
/// around it. While the computer is kept awake its eyes, red already, are drawn larger over the
/// picture's own. A quota is not drawn here.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(super) fn app_icon(attention: bool, awake: bool) -> Image<'static> {
    let original = tauri::include_image!("icons/32x32.png");
    if !attention && !awake {
        return original;
    }
    let mut canvas = pixels(&original);
    let scale = f64::from(canvas.width()) / f64::from(SIDE);
    if attention {
        let (centre_x, centre_y) = (DOT_X * scale, DOT_Y * scale);
        let (radius, clearance) = (DOT_RADIUS * scale, DOT_CLEARANCE * scale);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let distance = (f64::from(x) + 0.5 - centre_x).hypot(f64::from(y) + 0.5 - centre_y);
                if distance > radius + clearance + 0.5 {
                    continue;
                }
                let alpha = ((radius + 0.5 - distance).clamp(0.0, 1.0) * 255.0) as u8;
                let [red, green, blue] = DOT_COLOUR;
                canvas.put_pixel(x, y, Rgba([red, green, blue, alpha]));
            }
        }
    }
    if awake {
        let scale = f64::from(canvas.width()) / 32.0;
        for (x, y) in APP_EYES {
            draw_disc(&mut canvas, x * scale, y * scale, APP_EYE_RADIUS * scale);
        }
    }
    Image::new_owned(canvas.into_raw(), original.width(), original.height())
}

/// Lays an `EYE_COLOUR` disc over what is there, its edge one pixel of blend.
fn draw_disc(canvas: &mut RgbaImage, centre_x: f64, centre_y: f64, radius: f64) {
    let [red, green, blue] = EYE_COLOUR;
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let distance = (f64::from(x) + 0.5 - centre_x).hypot(f64::from(y) + 0.5 - centre_y);
            let cover = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if cover == 0.0 {
                continue;
            }
            let Rgba([r, g, b, a]) = *canvas.get_pixel(x, y);
            let mix = |over: u8, under: u8| {
                (f64::from(over) * cover + f64::from(under) * (1.0 - cover)) as u8
            };
            let alpha = (255.0 * cover + f64::from(a) * (1.0 - cover)) as u8;
            canvas.put_pixel(
                x,
                y,
                Rgba([mix(red, r), mix(green, g), mix(blue, b), alpha]),
            );
        }
    }
}

/// AppKit tints this independent template layer orange, including without a readable quota.
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

/// AppKit tints this independent template layer red, over the sockets `octopus` clears while the
/// computer is kept awake: exactly the socket pixels, solid.
pub(super) fn eyes_png() -> Option<Vec<u8>> {
    let mut canvas = RgbaImage::new(SIDE, SIDE);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let alpha = if in_socket(x, y) { 255 } else { 0 };
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
    .expect("embedded tray picture has one RGBA pixel per position")
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

/// Whether a canvas pixel is in one of the two eye sockets.
fn in_socket(x: u32, y: u32) -> bool {
    if x >= SIDE {
        return false;
    }
    let x = if x < SIDE / 2 { x } else { SIDE - 1 - x };
    x.checked_sub(SOCKET_X)
        .zip(y.checked_sub(SOCKET_Y))
        .and_then(|(column, row)| SOCKET.get(row as usize)?.as_bytes().get(column as usize))
        == Some(&b'#')
}

/// The sockets are cleared of the octopus for the red eyes to fill, so they stand on a clear
/// ground of their own at the menu bar's size.
fn clear_eyes(canvas: &mut RgbaImage) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            if in_socket(x, y) {
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
    fn the_coloured_icon_gains_an_orange_dot_in_a_cleared_corner() {
        let plain = app_icon(false, false);
        let attention = app_icon(true, false);
        assert_eq!((attention.width(), attention.height()), (32, 32));
        assert_ne!(plain.rgba(), attention.rgba());
        let pixel = |image: &Image<'_>, x: u32, y: u32| {
            let at = ((y * image.width() + x) * 4) as usize;
            image.rgba()[at..at + 4].to_vec()
        };
        // The dot's centre is solid orange.
        assert_eq!(pixel(&attention, 25, 6), [255, 149, 0, 255]);
        // Only the corner around the dot changes, and the icon has colour of its own.
        let reach = (DOT_RADIUS + DOT_CLEARANCE) * 32.0 / f64::from(SIDE) + 0.5;
        for y in 0..32 {
            for x in 0..32 {
                let distance = (f64::from(x) + 0.5 - DOT_X * 32.0 / f64::from(SIDE))
                    .hypot(f64::from(y) + 0.5 - DOT_Y * 32.0 / f64::from(SIDE));
                if distance > reach {
                    assert_eq!(pixel(&attention, x, y), pixel(&plain, x, y));
                }
            }
        }
        assert!(plain
            .rgba()
            .chunks(4)
            .any(|p| p[3] > 0 && p[..3] != [0, 0, 0]));
    }

    #[test]
    fn quota_endpoints_keep_the_outline_and_fill_only_the_remainder() {
        let empty = octopus(false, false, single(0));
        let full = octopus(false, false, single(100));
        let half = octopus(false, false, single(50));
        for icon in [&empty, &half, &full] {
            assert_eq!((icon.width(), icon.height()), (SIDE, SIDE));
            assert!(icon.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
            assert_eq!(alpha(icon, 18, 28), 255);
            assert!((0..SIDE).all(|x| alpha(icon, x, 26) == 0 && alpha(icon, x, 27) == 0));
        }
        let original = octopus(false, false, None);
        for y in 0..26 {
            for x in 0..SIDE {
                assert_eq!(alpha(&empty, x, y), alpha(&original, x, y));
            }
        }
        assert!(alpha(&empty, 0, 32) > 0 && alpha(&empty, 35, 32) > 0);
        assert_eq!(alpha(&empty, 18, 32), 0);
        assert_eq!(alpha(&full, 18, 32), 255);
        assert_eq!(alpha(&half, 13, 32), 255);
        assert_eq!(alpha(&half, 23, 32), 0);
        assert_eq!(octopus(false, false, single(255)).rgba(), full.rgba());
    }

    #[test]
    fn two_layers_draw_the_main_capsule_over_a_thinner_one() {
        let original = octopus(false, false, None);
        let icon = octopus(false, false, layered(50, 25));
        assert_eq!((icon.width(), icon.height()), (SIDE, SIDE));
        assert!(icon.rgba().chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        for y in 0..19 {
            for x in 0..SIDE {
                assert_eq!(alpha(&icon, x, y), alpha(&original, x, y));
            }
        }
        // The band above the main capsule is cleared.
        assert!((0..SIDE).all(|x| alpha(&icon, x, 19) == 0 && alpha(&icon, x, 20) == 0));
        // The main capsule: outlined, filled to half inside.
        assert_eq!(alpha(&icon, 18, 21), 255);
        assert_eq!(alpha(&icon, 18, 27), 255);
        assert_eq!(alpha(&icon, 10, 24), 255);
        assert_eq!(alpha(&icon, 26, 24), 0);
        // Nothing in the two pixels between the two.
        assert!((0..SIDE).all(|x| alpha(&icon, x, 28) == 0 && alpha(&icon, x, 29) == 0));
        // The thinner capsule: outlined too, filled to a quarter inside.
        assert_eq!(alpha(&icon, 18, 30), 255);
        assert_eq!(alpha(&icon, 18, 35), 255);
        assert_eq!(alpha(&icon, 5, 32), 255);
        assert_eq!(alpha(&icon, 24, 32), 0);
        // The main bar at 0 leaves its capsule's interior empty; the other at 100 fills its own.
        let drained = octopus(false, false, layered(0, 100));
        assert_eq!(alpha(&drained, 18, 24), 0);
        assert_eq!(alpha(&drained, 24, 32), 255);
        assert_ne!(octopus(false, false, layered(50, 100)).rgba(), icon.rgba());
        assert_eq!(
            octopus(false, false, layered(255, 255)).rgba(),
            octopus(false, false, layered(100, 100)).rgba()
        );
    }

    #[test]
    fn attention_dot_stays_upper_right_and_clear_of_the_quota() {
        let dot = Image::from_bytes(&dot_png().expect("dot PNG")).expect("PNG");
        assert_eq!(alpha(&dot, 30, 5), 255);
        assert!((0..SIDE).all(|y| (0..24).all(|x| alpha(&dot, x, y) == 0)));
        assert!((13..SIDE).all(|y| (0..SIDE).all(|x| alpha(&dot, x, y) == 0)));
        for quota in [None, single(0), single(50), single(100), layered(50, 20)] {
            let normal = octopus(false, false, quota);
            let attention = octopus(true, false, quota);
            assert_ne!(normal.rgba(), attention.rgba());
            assert!(dot
                .rgba()
                .chunks(4)
                .zip(attention.rgba().chunks(4))
                .all(|(dot, base)| dot[3] == 0 || base[3] == 0));
            assert_eq!(
                &normal.rgba()[16 * SIDE as usize * 4..],
                &attention.rgba()[16 * SIDE as usize * 4..]
            );
        }
    }

    fn near_a_socket(x: u32, y: u32) -> bool {
        (x.saturating_sub(1)..=x + 1)
            .any(|near_x| (y.saturating_sub(1)..=y + 1).any(|near_y| in_socket(near_x, near_y)))
    }

    fn near_an_eye(x: u32, y: u32, scale: f64, eyes: &[(f64, f64)], reach: f64) -> bool {
        eyes.iter().any(|&(eye_x, eye_y)| {
            (f64::from(x) + 0.5 - eye_x * scale).hypot(f64::from(y) + 0.5 - eye_y * scale)
                <= reach * scale
        })
    }

    #[test]
    fn the_hold_cuts_the_eyes_out_for_the_red_layer_and_changes_nothing_else() {
        let eyes = Image::from_bytes(&eyes_png().expect("eyes PNG")).expect("PNG");
        for attention in [false, true] {
            for quota in [None, single(50), layered(50, 20)] {
                let plain = octopus(attention, false, quota);
                let awake = octopus(attention, true, quota);
                assert_ne!(plain.rgba(), awake.rgba());
                for y in 0..SIDE {
                    for x in 0..SIDE {
                        if !near_a_socket(x, y) {
                            assert_eq!(alpha(&plain, x, y), alpha(&awake, x, y));
                        }
                    }
                }
                // The red never lands on the octopus, and its middle is in a cut-out.
                assert!(eyes
                    .rgba()
                    .chunks(4)
                    .zip(awake.rgba().chunks(4))
                    .all(|(eye, base)| eye[3] == 0 || base[3] == 0));
                // The red covers exactly the sockets, which are cleared.
                for y in 0..SIDE {
                    for x in 0..SIDE {
                        let socket = in_socket(x, y);
                        assert_eq!(alpha(&eyes, x, y), if socket { 255 } else { 0 });
                        assert_eq!(alpha(&awake, x, y) == 0, socket || alpha(&plain, x, y) == 0);
                    }
                }
            }
        }
    }

    #[test]
    fn no_overlay_clears_into_the_eyes() {
        for awake in [false, true] {
            let bare = octopus(false, awake, None);
            for attention in [false, true] {
                for quota in [None, single(0), single(100), layered(50, 20)] {
                    let icon = octopus(attention, awake, quota);
                    for y in 0..SIDE {
                        for x in 0..SIDE {
                            if in_socket(x, y) {
                                assert_eq!(
                                    alpha(&icon, x, y),
                                    alpha(&bare, x, y),
                                    "({x}, {y}) {attention} {awake} {quota:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_coloured_icon_draws_larger_red_eyes_beside_the_dot() {
        let pixel = |image: &Image<'_>, x: u32, y: u32| {
            let at = ((y * image.width() + x) * 4) as usize;
            image.rgba()[at..at + 4].to_vec()
        };
        let plain = app_icon(false, false);
        let awake = app_icon(false, true);
        assert_ne!(plain.rgba(), awake.rgba());
        for (x, y) in [(12, 14), (19, 14)] {
            assert_eq!(pixel(&awake, x, y), [255, 59, 48, 255]);
        }
        // Larger than the picture's own eyes: a pixel beside them is red now.
        let beside = pixel(&awake, 14, 14);
        assert!(beside[0] > 200 && beside[1] < 100 && beside[2] < 100);
        for y in 0..32 {
            for x in 0..32 {
                if !near_an_eye(x, y, 1.0, &APP_EYES, APP_EYE_RADIUS + 1.0) {
                    assert_eq!(pixel(&awake, x, y), pixel(&plain, x, y));
                }
            }
        }
        let both = app_icon(true, true);
        assert_eq!(pixel(&both, 25, 6), [255, 149, 0, 255]);
        assert_eq!(pixel(&both, 12, 14), [255, 59, 48, 255]);
    }

    #[test]
    fn the_plain_eyes_are_slanted_sockets_of_one_flat_partial_alpha() {
        let plain = octopus(false, false, None);
        for y in 0..SIDE {
            for x in 0..SIDE {
                if in_socket(x, y) {
                    assert_eq!(alpha(&plain, x, y), 166, "({x}, {y})");
                }
            }
        }
        // The sockets are mirror images, high at the outer side with the brow falling toward the
        // middle, and the body between them is solid.
        assert!(in_socket(10, 13) && in_socket(25, 13) && !in_socket(12, 13) && !in_socket(23, 13));
        assert!(in_socket(15, 15) && in_socket(20, 15) && !in_socket(10, 16));
        assert!(in_socket(15, 17) && in_socket(20, 17) && !in_socket(10, 17));
        // A nose ridge of solid body runs between them.
        for y in 12..19 {
            for x in 16..20 {
                assert_eq!(alpha(&plain, x, y), 255, "({x}, {y})");
            }
        }
        // Every socket pixel has solid head body all round it: the sockets never reach the outline
        // or the tentacles.
        for y in 0..SIDE {
            for x in 0..SIDE {
                if in_socket(x, y) {
                    for (near_x, near_y) in [
                        (x - 1, y - 1),
                        (x, y - 1),
                        (x + 1, y - 1),
                        (x - 1, y),
                        (x + 1, y),
                        (x - 1, y + 1),
                        (x, y + 1),
                        (x + 1, y + 1),
                    ] {
                        assert!(
                            in_socket(near_x, near_y) || alpha(&plain, near_x, near_y) == 255,
                            "({x}, {y}) beside ({near_x}, {near_y})"
                        );
                    }
                }
            }
        }
    }
}
