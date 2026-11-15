//! The flyout the notification-area icon opens on Windows, in place of the system's popup menu,
//! which can hold neither tabs nor the quota bars: a frameless window of the application's own
//! (labelled `FLYOUT_LABEL`), made hidden at launch, whose page (`TrayFlyout` in `packages/ui`)
//! draws the model this module hands it (`flyout_model::FlyoutModel`), the same lines the macOS
//! menu is built from. A click on the icon shows it against the icon and the edge the taskbar sits
//! on, inside the monitor's work area (`place`); it hides on a choice, on Esc, and when it loses
//! the focus, which a click outside it or on the icon does. While it is open the model is held, as
//! the macOS menu is (`MenuTracking`), and the page is told when it opens so it chooses the tab of
//! the window focused last and every daemon's window asks for fresh quotas.
//!
//! The page reports the size of what it draws (`tray_flyout_size`), in CSS pixels; the window takes
//! that size, capped to the work area, and is placed again.

// Only Windows makes the flyout; the commands are registered everywhere, and find no window
// elsewhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};

use super::flyout_model::FlyoutModel;
use super::menu_tracking::tracking;
use super::{build, chosen_on_open, menu_opened, TrayState};
use crate::diagnostics::diagnostic;

/// The flyout window's label; `capabilities/flyout.json` names it.
pub const FLYOUT_LABEL: &str = "tray-flyout";

/// Makes the flyout, hidden, where the icon opens one (`super::FLYOUT`): a frameless window that
/// stays above the others and out of the taskbar, on the page the shell gives the flyout
/// (`flyout=1`, with the host's facts and the shell's settings as every window has them). Nothing
/// elsewhere. On the main thread.
pub fn make(app: &AppHandle) {
    if !super::FLYOUT {
        return;
    }
    let handle = app.clone();
    crate::page_isolation::prepare(move |configuration| {
        let query = crate::window_factory::window_query(
            &handle,
            "flyout=1",
            &crate::host_facts::current_json(),
        );
        let url = tauri::WebviewUrl::App(format!("index.html?{query}").into());
        let builder = tauri::WebviewWindowBuilder::new(&handle, FLYOUT_LABEL, url)
            .title(crate::menu::APP_NAME)
            .inner_size(FIRST_SIZE.0, FIRST_SIZE.1)
            .decorations(false)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .shadow(true)
            .visible(false);
        let builder = crate::page_isolation::apply(builder, configuration);
        match builder.build() {
            Ok(window) => crate::page_isolation::install(&window),
            Err(err) => diagnostic!("cannot make the notification-area icon's flyout: {err}"),
        }
    });
}

/// What the page is sent when a newer model is built, while the flyout is hidden.
const MODEL_EVENT: &str = "tray-flyout-model";
/// What the page is sent as the flyout opens, with the tab to choose.
const OPENED_EVENT: &str = "tray-flyout-opened";

/// The space between the flyout and the taskbar or the work area's edges, in logical pixels.
const GAP: f64 = 12.0;
/// The size the flyout takes before its page has reported one, in logical pixels.
const FIRST_SIZE: (f64, f64) = (340.0, 420.0);
/// How soon after the flyout hid for losing the focus a click on the icon is the click that took
/// the focus away, which closes the flyout rather than opening it again.
const REOPEN_HOLD: Duration = Duration::from_millis(400);

/// The flyout's state.
#[derive(Default)]
pub struct FlyoutState {
    model: Option<FlyoutModel>,
    open: bool,
    /// The icon's rectangle at the click that opened it, in physical pixels.
    anchor: Option<Rect>,
    /// The size the page last reported, in logical pixels.
    size: Option<(f64, f64)>,
    hidden_at: Option<Instant>,
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, FlyoutState> {
    app.state::<TrayState>()
        .inner()
        .flyout
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Keeps `model` as the one the page draws, and hands it to the page.
pub(super) fn set_model(app: &AppHandle, model: FlyoutModel) {
    state(app).model = Some(model.clone());
    let _ = app.emit_to(FLYOUT_LABEL, MODEL_EVENT, model);
}

/// Answers a click on the icon, whose rectangle is `icon`: opens the flyout, or closes it when it
/// is open or has just closed for the focus this click took from it.
pub(super) fn toggle(app: &AppHandle, icon: Rect) {
    let (open, just_hidden) = {
        let flyout = state(app);
        (
            flyout.open,
            flyout
                .hidden_at
                .is_some_and(|at| at.elapsed() < REOPEN_HOLD),
        )
    };
    if open {
        hide(app);
    } else if !just_hidden {
        show(app, icon);
    }
}

fn show(app: &AppHandle, icon: Rect) {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        return;
    };
    tracking(app).opened();
    let size = {
        let mut flyout = state(app);
        flyout.open = true;
        flyout.anchor = Some(icon);
        flyout.size.unwrap_or(FIRST_SIZE)
    };
    place_window(app, &window, icon, size);
    let _ = window.show();
    // Placed again once shown: before its first showing, Windows sizes the frameless window's frame
    // as if it had a caption, and the flyout would come out that much lower, over the taskbar.
    place_window(app, &window, icon, size);
    let _ = window.set_focus();
    let _ = app.emit_to(
        FLYOUT_LABEL,
        OPENED_EVENT,
        Opened {
            chosen: chosen_on_open(app),
        },
    );
    menu_opened(app);
}

#[derive(Clone, Serialize)]
struct Opened {
    chosen: usize,
}

/// Hides the flyout, and builds the model held while it was open.
pub fn hide(app: &AppHandle) {
    {
        let mut flyout = state(app);
        if !flyout.open {
            return;
        }
        flyout.open = false;
        flyout.hidden_at = Some(Instant::now());
    }
    if let Some(window) = app.get_webview_window(FLYOUT_LABEL) {
        let _ = window.hide();
    }
    if tracking(app).closed() && tracking(app).due() {
        build(app);
    }
}

/// The flyout window lost the focus: a click outside it, on the icon or elsewhere, or another
/// window taking it.
pub fn focus_lost(app: &AppHandle) {
    hide(app);
}

/// Sizes the window to `size` (logical pixels), capped to the work area of the monitor the icon is
/// on, and places it against the icon.
fn place_window(app: &AppHandle, window: &tauri::WebviewWindow, icon: Rect, size: (f64, f64)) {
    let centre = (
        f64::from(icon.x) + f64::from(icon.width) / 2.0,
        f64::from(icon.y) + f64::from(icon.height) / 2.0,
    );
    let monitor = app
        .monitor_from_point(centre.0, centre.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = window.set_size(LogicalSize::new(size.0, size.1));
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let work = Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width as i32,
        height: area.size.height as i32,
    };
    let physical = (
        (size.0 * scale).ceil() as i32,
        (size.1 * scale).ceil() as i32,
    );
    let placed = place(icon, work, physical, (GAP * scale).round() as i32);
    let _ = window.set_size(PhysicalSize::new(
        placed.width.max(1) as u32,
        placed.height.max(1) as u32,
    ));
    let _ = window.set_position(PhysicalPosition::new(placed.x, placed.y));
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn right(&self) -> i32 {
        self.x + self.width
    }

    fn bottom(&self) -> i32 {
        self.y + self.height
    }

    fn centre(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

/// The edge of the work area the taskbar, and so the icon, sits at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

/// Where the taskbar sits, from the icon's centre against the work area: outside it, on the side it
/// is outside; inside it (an icon in the overflow area, or an auto-hidden taskbar), the nearest
/// edge.
fn edge(icon: Rect, work: Rect) -> Edge {
    let (x, y) = icon.centre();
    if y >= work.bottom() {
        return Edge::Bottom;
    }
    if y < work.y {
        return Edge::Top;
    }
    if x < work.x {
        return Edge::Left;
    }
    if x >= work.right() {
        return Edge::Right;
    }
    let distances = [
        (work.bottom() - y, Edge::Bottom),
        (y - work.y, Edge::Top),
        (x - work.x, Edge::Left),
        (work.right() - x, Edge::Right),
    ];
    distances
        .into_iter()
        .min_by_key(|(distance, _)| *distance)
        .map_or(Edge::Bottom, |(_, edge)| edge)
}

/// The flyout's rectangle: `size`, capped to the work area less `gap` on every side, against the
/// edge the taskbar sits at (`gap` from it), centred on the icon along that edge, and kept `gap`
/// inside the work area.
fn place(icon: Rect, work: Rect, size: (i32, i32), gap: i32) -> Rect {
    let width = size.0.min(work.width - 2 * gap).max(1);
    let height = size.1.min(work.height - 2 * gap).max(1);
    let (cx, cy) = icon.centre();
    let clamp = |value: i32, low: i32, high: i32| value.min(high).max(low);
    let along_x = clamp(cx - width / 2, work.x + gap, work.right() - gap - width);
    let along_y = clamp(cy - height / 2, work.y + gap, work.bottom() - gap - height);
    let (x, y) = match edge(icon, work) {
        Edge::Bottom => (along_x, work.bottom() - gap - height),
        Edge::Top => (along_x, work.y + gap),
        Edge::Left => (work.x + gap, along_y),
        Edge::Right => (work.right() - gap - width, along_y),
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The model the page draws now, which it asks for as it loads.
#[tauri::command]
pub fn tray_flyout_model(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
) -> Option<FlyoutModel> {
    if webview_window.label() != FLYOUT_LABEL {
        return None;
    }
    state(&app).model.clone()
}

/// The page reports the size of what it draws, in CSS pixels: the window takes it, and is placed
/// again while it is open.
#[tauri::command]
pub fn tray_flyout_size(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    width: f64,
    height: f64,
) {
    if webview_window.label() != FLYOUT_LABEL || !(width > 0.0 && height > 0.0) {
        return;
    }
    let anchor = {
        let mut flyout = state(&app);
        flyout.size = Some((width, height));
        flyout.anchor.filter(|_| flyout.open)
    };
    if let Some(icon) = anchor {
        place_window(&app, &webview_window, icon, (width, height));
    }
}

/// The user chose a line of the flyout: it hides, and the choice is answered as the macOS menu's
/// is (`remote::handle_menu_item` for opening a daemon held for the browser, `handle_menu_event`
/// for the rest).
#[tauri::command]
pub fn tray_flyout_choose(app: AppHandle, webview_window: tauri::WebviewWindow, id: String) {
    if webview_window.label() != FLYOUT_LABEL {
        return;
    }
    hide(&app);
    if !crate::remote::handle_menu_item(&app, &id) {
        super::handle_menu_event(&app, &id);
    }
}

/// Esc in the flyout.
#[tauri::command]
pub fn tray_flyout_close(app: AppHandle, webview_window: tauri::WebviewWindow) {
    if webview_window.label() == FLYOUT_LABEL {
        hide(&app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920 × 1080 monitor whose work area leaves a 48 px taskbar at `edge`.
    fn work(edge: Edge) -> Rect {
        match edge {
            Edge::Bottom => Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1032,
            },
            Edge::Top => Rect {
                x: 0,
                y: 48,
                width: 1920,
                height: 1032,
            },
            Edge::Left => Rect {
                x: 48,
                y: 0,
                width: 1872,
                height: 1080,
            },
            Edge::Right => Rect {
                x: 0,
                y: 0,
                width: 1872,
                height: 1080,
            },
        }
    }

    fn icon(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 24,
            height: 24,
        }
    }

    #[test]
    fn the_taskbar_s_edge_is_the_side_the_icon_lies_outside_the_work_area() {
        assert_eq!(edge(icon(1700, 1044), work(Edge::Bottom)), Edge::Bottom);
        assert_eq!(edge(icon(1700, 12), work(Edge::Top)), Edge::Top);
        assert_eq!(edge(icon(12, 900), work(Edge::Left)), Edge::Left);
        assert_eq!(edge(icon(1884, 900), work(Edge::Right)), Edge::Right);
        // An icon inside the work area (the overflow area) goes by the nearest edge.
        assert_eq!(edge(icon(1700, 990), work(Edge::Bottom)), Edge::Bottom);
    }

    #[test]
    fn above_a_bottom_taskbar_centred_on_the_icon() {
        let placed = place(icon(900, 1044), work(Edge::Bottom), (340, 400), 12);
        assert_eq!(
            placed,
            Rect {
                x: 742,
                y: 620,
                width: 340,
                height: 400
            }
        );
    }

    #[test]
    fn near_the_clock_it_is_kept_inside_the_work_area() {
        let placed = place(icon(1880, 1044), work(Edge::Bottom), (340, 400), 12);
        assert_eq!(placed.x + placed.width, 1920 - 12);
        assert_eq!(placed.y + placed.height, 1032 - 12);
    }

    #[test]
    fn against_a_top_left_or_right_taskbar() {
        let top = place(icon(1700, 12), work(Edge::Top), (340, 400), 12);
        assert_eq!((top.y, top.x + top.width), (60, 1882));
        let left = place(icon(12, 900), work(Edge::Left), (340, 400), 12);
        assert_eq!((left.x, left.y + left.height), (60, 1068));
        let right = place(icon(1884, 100), work(Edge::Right), (340, 400), 12);
        assert_eq!((right.x + right.width, right.y), (1860, 12));
    }

    #[test]
    fn a_flyout_taller_than_the_screen_is_cut_to_the_work_area() {
        let placed = place(icon(900, 1044), work(Edge::Bottom), (340, 5000), 12);
        assert_eq!((placed.y, placed.height), (12, 1032 - 24));
    }

    #[test]
    fn on_a_second_monitor_the_flyout_stays_inside_that_monitor_s_work_area() {
        // A monitor to the right of the main one, and another to its left (negative x).
        for origin in [1920, -1920] {
            let work = Rect {
                x: origin,
                y: 0,
                width: 1920,
                height: 1032,
            };
            let at_clock = icon(origin + 1880, 1044);
            let placed = place(at_clock, work, (340, 400), 12);
            assert_eq!(placed.x + placed.width, origin + 1920 - 12);
            assert_eq!(placed.y + placed.height, 1032 - 12);
            let at_start = icon(origin + 20, 1044);
            let placed = place(at_start, work, (340, 400), 12);
            assert_eq!(placed.x, origin + 12);
        }
    }

    #[test]
    fn a_flyout_wider_than_the_work_area_is_cut_to_it() {
        let placed = place(icon(900, 1044), work(Edge::Bottom), (5000, 400), 12);
        assert_eq!((placed.x, placed.width), (12, 1920 - 24));
    }

    #[test]
    fn the_flyout_never_covers_the_taskbar_whatever_edge_it_sits_at() {
        for edge in [Edge::Bottom, Edge::Top, Edge::Left, Edge::Right] {
            let work = work(edge);
            let icon = match edge {
                Edge::Bottom => icon(960, 1044),
                Edge::Top => icon(960, 12),
                Edge::Left => icon(12, 540),
                Edge::Right => icon(1884, 540),
            };
            for size in [(340, 400), (340, 5000), (5000, 400)] {
                let placed = place(icon, work, size, 12);
                assert!(
                    placed.x >= work.x && placed.y >= work.y,
                    "{edge:?} {size:?}"
                );
                assert!(
                    placed.right() <= work.right() && placed.bottom() <= work.bottom(),
                    "{edge:?} {size:?}"
                );
                assert!(placed.width >= 1 && placed.height >= 1);
            }
        }
    }

    #[test]
    fn a_work_area_smaller_than_its_gaps_still_gives_a_flyout_with_a_size() {
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let placed = place(icon(5, 20), tiny, (340, 400), 12);
        assert!(placed.width >= 1 && placed.height >= 1);
    }
}
