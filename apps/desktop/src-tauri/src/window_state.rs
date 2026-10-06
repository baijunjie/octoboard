//! Remembers the main window's size, position and maximized state across launches, in
//! `window-state.json` under the application's config directory.
//!
//! Every geometry value here is in logical points, in the desktop's one global coordinate space
//! (origin at the main display's top-left, y growing downwards): the space AppKit places windows
//! in, and the one `WebviewWindowBuilder::position` / `inner_size` take. A physical pixel means
//! something different on each display of a mixed-scale setup, so a frame stored in pixels on one
//! display cannot be read back correctly once the window opens on another; a point can.
//!
//! The initial frame is decided before the window exists (`initial_window`), so the window can be
//! created with it while still hidden. The normal frame is then followed from the window's own
//! move and resize events (`Tracker`) and written out on exit (`save`).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalRect, WebviewWindow, WindowEvent};

/// The window's size on a first launch, and when the saved frame has nowhere visible to be.
pub const DEFAULT_SIZE: (f64, f64) = (1200.0, 760.0);

/// The smallest inner size the window allows.
pub const MIN_SIZE: (f64, f64) = (1100.0, 600.0);

const FILE_NAME: &str = "window-state.json";

/// The band along the window's top edge that has to be on a display for a saved frame to be
/// restored: the UI's top bar, which is what the user grabs to move the window, is this tall.
const TOP_STRIP_HEIGHT: f64 = 40.0;

/// How much of `TOP_STRIP_HEIGHT` has to lie on one display, as (width, height): enough of the bar
/// to see it and drag the window back by it.
const MIN_VISIBLE_TOP_STRIP: (f64, f64) = (200.0, 20.0);

/// How long a frame has to stand before a maximize can no longer discard it; see `Tracker`.
const SETTLE: Duration = Duration::from_millis(500);

/// A rectangle in logical points; for the window, its outer top-left and its inner size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn is_well_formed(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }

    /// The width and height of the overlap with `other`, either of which is zero or negative when
    /// they do not overlap.
    fn overlap(&self, other: &Rect) -> (f64, f64) {
        let width = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let height = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        (width, height)
    }
}

/// What `window-state.json` holds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedState {
    /// The normal frame: where the window is, and how big, when it is neither maximized,
    /// fullscreen nor minimized — and so what it returns to after any of those.
    pub frame: Rect,
    pub maximized: bool,
}

/// What the window is created with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InitialWindow {
    /// `None` leaves the placement to the OS, which centres a new window on the main display.
    pub position: Option<(f64, f64)>,
    pub size: (f64, f64),
    pub maximized: bool,
}

impl InitialWindow {
    /// The normal frame decided for the window, when the placement is not left to the OS.
    fn frame(&self) -> Option<Rect> {
        self.position.map(|(x, y)| Rect {
            x,
            y,
            width: self.size.0,
            height: self.size.1,
        })
    }
}

/// Decides the frame the window opens with. `displays` are the work areas of the connected
/// displays (each without its menu bar and Dock); `main_display` is the main display's.
///
/// - No saved state, or one that is not a usable rectangle: the default size, placed by the OS.
/// - A saved frame whose top strip (`TOP_STRIP_HEIGHT`) has at least `MIN_VISIBLE_TOP_STRIP` on
///   some display is restored, held inside the bounding box of all the displays: its size first
///   (never below `MIN_SIZE`), then its position. A frame spanning displays that are all still
///   connected therefore comes back unchanged, while one that reached onto a display since
///   unplugged is pulled back onto the ones left. A frame straddling two displays passes on the
///   strength of either part.
/// - Otherwise, or when the frame held inside the box no longer has its top strip on a display (the
///   box also spans the gaps between displays): the default size, centred on the main display's
///   work area.
///
/// A saved maximized state is kept either way; the window maximizes on whichever display its
/// normal frame lands on.
pub fn initial_window(
    saved: Option<&SavedState>,
    displays: &[Rect],
    main_display: Option<Rect>,
) -> InitialWindow {
    let Some(saved) = saved.filter(|saved| saved.frame.is_well_formed()) else {
        return InitialWindow {
            position: None,
            size: DEFAULT_SIZE,
            maximized: false,
        };
    };

    let frame = saved.frame;
    if let Some(bounds) = bounding_box(displays) {
        if top_strip_reachable(frame.x, frame.y, frame.width.max(MIN_SIZE.0), displays) {
            let width = frame.width.min(bounds.width).max(MIN_SIZE.0);
            let height = frame.height.min(bounds.height).max(MIN_SIZE.1);
            let x = frame.x.min(bounds.x + bounds.width - width).max(bounds.x);
            let y = frame.y.min(bounds.y + bounds.height - height).max(bounds.y);
            if top_strip_reachable(x, y, width, displays) {
                return InitialWindow {
                    position: Some((x, y)),
                    size: (width, height),
                    maximized: saved.maximized,
                };
            }
        }
    }

    InitialWindow {
        position: main_display.map(|display| centred(DEFAULT_SIZE, display)),
        size: DEFAULT_SIZE,
        maximized: saved.maximized,
    }
}

/// Whether a window at `x`, `y`, `width` wide, has at least `MIN_VISIBLE_TOP_STRIP` of its top
/// strip (`TOP_STRIP_HEIGHT`) on one of `displays`.
fn top_strip_reachable(x: f64, y: f64, width: f64, displays: &[Rect]) -> bool {
    let top_strip = Rect {
        x,
        y,
        width,
        height: TOP_STRIP_HEIGHT,
    };
    displays.iter().any(|display| {
        let (width, height) = top_strip.overlap(display);
        width >= MIN_VISIBLE_TOP_STRIP.0 && height >= MIN_VISIBLE_TOP_STRIP.1
    })
}

/// The smallest rectangle holding every one of `displays`, or `None` when there are none.
fn bounding_box(displays: &[Rect]) -> Option<Rect> {
    let first = displays.first()?;
    let (mut left, mut top) = (first.x, first.y);
    let (mut right, mut bottom) = (first.x + first.width, first.y + first.height);
    for display in &displays[1..] {
        left = left.min(display.x);
        top = top.min(display.y);
        right = right.max(display.x + display.width);
        bottom = bottom.max(display.y + display.height);
    }
    Some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// The top-left that centres `size` in `area`, kept inside the area's top-left when it does not
/// fit, so the top bar stays reachable.
fn centred(size: (f64, f64), area: Rect) -> (f64, f64) {
    (
        (area.x + (area.width - size.0) / 2.0).max(area.x).round(),
        (area.y + (area.height - size.1) / 2.0).max(area.y).round(),
    )
}

/// Follows the window's normal frame and maximized state from its move and resize events.
///
/// AppKit animates a zoom, and can report the frames along the way while the window does not count
/// as maximized yet, so the latest normal-looking frame cannot simply be taken as the normal frame.
/// A frame becomes `settled` only once it has stood for `SETTLE` before the next event; a maximize
/// that arrives sooner discards the frames since, as the zoom's own. The trade-off is that a move
/// made less than `SETTLE` before maximizing is forgotten.
#[derive(Debug)]
pub struct Tracker {
    settled: Rect,
    recent: Option<(Rect, Instant)>,
    maximized: bool,
}

impl Tracker {
    pub fn new(frame: Rect, maximized: bool) -> Self {
        Self {
            settled: frame,
            recent: None,
            maximized,
        }
    }

    /// The window is in its normal state, with this frame.
    pub fn record_normal(&mut self, frame: Rect, now: Instant) {
        self.settle(now);
        self.recent = Some((frame, now));
        self.maximized = false;
    }

    /// The window is maximized.
    pub fn record_maximized(&mut self, now: Instant) {
        self.settle(now);
        self.recent = None;
        self.maximized = true;
    }

    fn settle(&mut self, now: Instant) {
        if let Some((frame, at)) = self.recent {
            if now.saturating_duration_since(at) >= SETTLE {
                self.settled = frame;
            }
        }
    }

    /// What to save now. A recent frame that no maximize has followed is a normal frame like any
    /// other, however briefly it stood.
    pub fn state(&self) -> SavedState {
        SavedState {
            frame: self.recent.map_or(self.settled, |(frame, _)| frame),
            maximized: self.maximized,
        }
    }
}

/// The managed state holding the main window's `Tracker`.
struct TrackedWindow(Mutex<Tracker>);

/// The saved state, or `None` when there is none or it cannot be read.
pub fn load(app: &AppHandle) -> Option<SavedState> {
    match state_path(app) {
        Ok(path) => read_state(&path),
        Err(err) => {
            eprintln!("octoboard: cannot locate the saved window state: {err}");
            None
        }
    }
}

fn read_state(path: &Path) -> Option<SavedState> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => {
            eprintln!("octoboard: cannot read {}: {err}", path.display());
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(state) => Some(state),
        Err(err) => {
            eprintln!("octoboard: ignoring {}: {err}", path.display());
            None
        }
    }
}

/// The connected displays' work areas and the main display's, in points.
pub fn displays(app: &AppHandle) -> (Vec<Rect>, Option<Rect>) {
    let displays = app
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| in_points(monitor.work_area(), monitor.scale_factor()))
                .collect()
        })
        .unwrap_or_else(|err| {
            eprintln!("octoboard: cannot list the displays: {err}");
            Vec::new()
        });
    let main_display = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| in_points(monitor.work_area(), monitor.scale_factor()));
    (displays, main_display)
}

/// A display's rectangle in points, from the pixels tao reports it in: tao scales each display's
/// points by that display's own scale factor, which is the `scale` to pass.
fn in_points(area: &PhysicalRect<i32, u32>, scale: f64) -> Rect {
    Rect {
        x: f64::from(area.position.x) / scale,
        y: f64::from(area.position.y) / scale,
        width: f64::from(area.size.width) / scale,
        height: f64::from(area.size.height) / scale,
    }
}

/// Starts following `window`, just created with `initial`. Best effort: a window whose frame cannot
/// be read is not followed, and its state is then not saved.
pub fn track(window: &WebviewWindow, initial: &InitialWindow) {
    // A position from the builder, and a requested maximize, are applied only after the window is
    // created, so its frame now may still be a provisional one on the main display: the decided
    // frame is the one to start from. When the OS placed the window, it did so while creating it,
    // so the window's own frame is already final.
    let frame = match initial.frame() {
        Some(frame) => frame,
        None => match frame_of(window) {
            Ok(frame) => frame,
            Err(err) => {
                eprintln!("octoboard: not remembering the window's state: {err}");
                return;
            }
        },
    };
    window.manage(TrackedWindow(Mutex::new(Tracker::new(
        frame,
        initial.maximized,
    ))));

    let observed = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
            if let Err(err) = observe(&observed) {
                eprintln!("octoboard: cannot read the window's state: {err}");
            }
        }
    });
}

/// Writes the main window's state out; meant for the moment the application exits.
pub fn save(app: &AppHandle) {
    let Some(tracked) = app.try_state::<TrackedWindow>() else {
        return;
    };
    // One last look, in case the final move or resize produced no event of its own; if the window
    // cannot be read any more, what the events recorded stands.
    if let Some(window) = app.get_webview_window("main") {
        let _ = observe(&window);
    }
    let state = tracked
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .state();
    let written = state_path(app)
        .map_err(anyhow::Error::from)
        .and_then(|path| write_state(&path, &state));
    if let Err(err) = written {
        eprintln!("octoboard: cannot save the window's state: {err}");
    }
}

fn observe(window: &WebviewWindow) -> tauri::Result<()> {
    // While the window is hidden, nothing but the shell's own placement moves it — the position and
    // the zoom applied after it is created, with the frames they pass through on the way — and the
    // user cannot move a window they cannot see, so none of those frames is anyone's choice. A
    // minimized window keeps its frame, and a fullscreen one is never restored as such. None of
    // these says anything about the normal frame or whether the window is maximized.
    if !window.is_visible()? || window.is_minimized()? || window.is_fullscreen()? {
        return Ok(());
    }
    let tracked = window.state::<TrackedWindow>();
    let now = Instant::now();
    if window.is_maximized()? {
        tracked
            .0
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .record_maximized(now);
    } else {
        let frame = frame_of(window)?;
        tracked
            .0
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .record_normal(frame, now);
    }
    Ok(())
}

/// The window's outer top-left and inner size in points, rounded to whole points so that a frame
/// carried through physical pixels and back does not drift from launch to launch. Tauri reports
/// both in pixels at the window's own scale factor, so dividing by that factor recovers points.
fn frame_of(window: &WebviewWindow) -> tauri::Result<Rect> {
    let scale = window.scale_factor()?;
    let position = window.outer_position()?.to_logical::<f64>(scale);
    let size = window.inner_size()?.to_logical::<f64>(scale);
    Ok(Rect {
        x: position.x.round(),
        y: position.y.round(),
        width: size.width.round(),
        height: size.height.round(),
    })
}

fn state_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(FILE_NAME))
}

/// Written to a sibling file and renamed over the old one, so a write cut short never leaves a
/// half-written file behind. The sibling is named after this process, since more than one build of
/// the application (a development one, an installed one) can share the directory.
fn write_state(path: &Path, state: &SavedState) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(format!(".{}.partial", std::process::id()));
    let partial = PathBuf::from(partial);
    let json = serde_json::to_vec_pretty(state)?;
    let written = fs::write(&partial, json).and_then(|()| fs::rename(&partial, path));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    Ok(written?)
}

#[cfg(test)]
mod tests {
    use tauri::{PhysicalPosition, PhysicalSize};

    use super::*;

    /// A 1512×982-point Retina laptop as the main display, its work area below a 33-point menu bar
    /// and beside nothing else.
    const LAPTOP: Rect = Rect {
        x: 0.0,
        y: 33.0,
        width: 1512.0,
        height: 949.0,
    };

    /// A 1920×1080 external display at 1× to the laptop's right, with no menu bar of its own.
    const EXTERNAL: Rect = Rect {
        x: 1512.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    /// A 1920×1080 Retina display above the laptop and reaching past its left edge, so both of its
    /// coordinates are negative.
    const ABOVE_LEFT: Rect = Rect {
        x: -829.0,
        y: -1080.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn saved(x: f64, y: f64, width: f64, height: f64, maximized: bool) -> SavedState {
        SavedState {
            frame: Rect {
                x,
                y,
                width,
                height,
            },
            maximized,
        }
    }

    fn restored(x: f64, y: f64, width: f64, height: f64, maximized: bool) -> InitialWindow {
        InitialWindow {
            position: Some((x, y)),
            size: (width, height),
            maximized,
        }
    }

    /// The default size, centred in `LAPTOP`'s work area.
    fn fallback(maximized: bool) -> InitialWindow {
        restored(156.0, 128.0, 1200.0, 760.0, maximized)
    }

    #[test]
    fn displays_of_different_scales_convert_to_one_space_of_points() {
        // The two displays above as tao reports them: the laptop at 2×, the external display at 1×.
        let laptop = PhysicalRect {
            position: PhysicalPosition::new(0, 66),
            size: PhysicalSize::new(3024, 1898),
        };
        let external = PhysicalRect {
            position: PhysicalPosition::new(1512, 0),
            size: PhysicalSize::new(1920, 1080),
        };
        let above_left = PhysicalRect {
            position: PhysicalPosition::new(-1658, -2160),
            size: PhysicalSize::new(3840, 2160),
        };
        assert_eq!(in_points(&laptop, 2.0), LAPTOP);
        assert_eq!(in_points(&external, 1.0), EXTERNAL);
        assert_eq!(in_points(&above_left, 2.0), ABOVE_LEFT);
    }

    #[test]
    fn first_launch_leaves_the_placement_to_the_os() {
        assert_eq!(
            initial_window(None, &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            InitialWindow {
                position: None,
                size: DEFAULT_SIZE,
                maximized: false,
            }
        );
    }

    #[test]
    fn unusable_saved_frame_counts_as_none() {
        for state in [
            saved(f64::NAN, 100.0, 1200.0, 760.0, false),
            saved(100.0, 100.0, 0.0, 760.0, false),
            saved(100.0, 100.0, 1200.0, f64::INFINITY, true),
        ] {
            assert_eq!(
                initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
                InitialWindow {
                    position: None,
                    size: DEFAULT_SIZE,
                    maximized: false,
                }
            );
        }
    }

    #[test]
    fn frame_on_a_display_above_and_left_of_the_main_one_is_restored() {
        let state = saved(-700.0, -1000.0, 1200.0, 760.0, true);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, ABOVE_LEFT], Some(LAPTOP)),
            restored(-700.0, -1000.0, 1200.0, 760.0, true)
        );
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            fallback(true)
        );
        // A frame pulled back onto the displays left keeps its maximized state too.
        let reaching = saved(-1800.0, 100.0, 2400.0, 760.0, true);
        assert_eq!(
            initial_window(Some(&reaching), &[LAPTOP], Some(LAPTOP)),
            restored(0.0, 100.0, 1512.0, 760.0, true)
        );
    }

    #[test]
    fn restored_frame_is_held_inside_the_displays() {
        // Saved reaching past the bottom right of the displays connected now.
        let state = saved(1600.0, 50.0, 2400.0, 1400.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(1032.0, 0.0, 2400.0, 1080.0, false)
        );
        // The minimum still wins over displays smaller than it.
        let tiny = Rect {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 500.0,
        };
        let state = saved(0.0, 0.0, 1200.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&state), &[tiny], Some(tiny)),
            restored(0.0, 0.0, 1100.0, 600.0, false)
        );
    }

    #[test]
    fn frame_reaching_onto_an_unplugged_display_is_pulled_onto_the_ones_left() {
        // It spanned an external display to the laptop's left and the laptop; only the laptop is
        // left, and the frame must end up wholly on it.
        let state = saved(-1800.0, 100.0, 2400.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            restored(0.0, 100.0, 1512.0, 760.0, false)
        );
    }

    #[test]
    fn frame_spanning_connected_displays_is_restored_unchanged() {
        let state = saved(800.0, 100.0, 1500.0, 800.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(800.0, 100.0, 1500.0, 800.0, false)
        );
    }

    #[test]
    fn absurd_saved_size_is_held_to_the_displays() {
        let state = saved(100.0, 100.0, 100_000.0, 100_000.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(0.0, 0.0, 3432.0, 1080.0, false)
        );
    }

    #[test]
    fn frame_held_inside_the_box_but_off_every_display_falls_back() {
        // A display at the top right and a small one at the bottom left leave the box's top-left
        // corner empty. A tall frame on the small display passes as saved, but held to the box's
        // height it is pushed up into that corner, where its top bar is on neither display.
        let top_right = Rect {
            x: 1000.0,
            y: 0.0,
            width: 2000.0,
            height: 1000.0,
        };
        let bottom_left = Rect {
            x: 0.0,
            y: 1100.0,
            width: 500.0,
            height: 1000.0,
        };
        let state = saved(0.0, 1600.0, 1100.0, 1500.0, false);
        assert_eq!(
            initial_window(Some(&state), &[top_right, bottom_left], Some(top_right)),
            restored(1400.0, 120.0, 1200.0, 760.0, false)
        );
    }

    #[test]
    fn frame_on_a_connected_display_of_another_scale_is_restored_in_points() {
        // Quit on the 1× external display while the Retina laptop is the main display: the frame
        // comes back exactly, not halved or doubled by either display's scale.
        let state = saved(1700.0, 100.0, 1200.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(1700.0, 100.0, 1200.0, 760.0, false)
        );
    }

    #[test]
    fn frame_on_a_disconnected_display_falls_back_to_the_main_display() {
        let state = saved(1700.0, 100.0, 1400.0, 900.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            fallback(false)
        );
    }

    #[test]
    fn fallback_without_a_main_display_leaves_the_placement_to_the_os() {
        let state = saved(1700.0, 100.0, 1400.0, 900.0, false);
        assert_eq!(
            initial_window(Some(&state), &[], None),
            InitialWindow {
                position: None,
                size: DEFAULT_SIZE,
                maximized: false,
            }
        );
    }

    #[test]
    fn frame_below_the_minimum_is_grown_to_it() {
        let state = saved(100.0, 100.0, 600.0, 380.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            restored(100.0, 100.0, 1100.0, 600.0, false)
        );
        let state = saved(100.0, 100.0, 1300.0, 380.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)).size,
            (1300.0, 600.0)
        );
    }

    #[test]
    fn frame_whose_top_strip_is_off_screen_falls_back_even_with_the_rest_on_screen() {
        // Most of the window is on the laptop, but its top bar is above every display.
        let above = saved(100.0, -30.0, 1200.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&above), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            fallback(false)
        );
        // Only the window's left corner of the top bar is on the laptop: too little to grab.
        let sideways = saved(1400.0, 3000.0, 1200.0, 760.0, false);
        let laptop_tall = Rect {
            height: 4000.0,
            ..LAPTOP
        };
        assert_eq!(
            initial_window(Some(&sideways), &[laptop_tall], Some(LAPTOP)),
            fallback(false)
        );
    }

    #[test]
    fn top_strip_partly_above_a_display_is_reachable_and_pulled_onto_it() {
        let state = saved(100.0, 20.0, 1200.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            restored(100.0, 33.0, 1200.0, 760.0, false)
        );
    }

    #[test]
    fn maximized_state_is_kept_with_its_normal_frame() {
        let state = saved(1700.0, 100.0, 1200.0, 760.0, true);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(1700.0, 100.0, 1200.0, 760.0, true)
        );
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            fallback(true)
        );
    }

    #[test]
    fn frame_straddling_two_displays_is_restored() {
        let state = saved(1000.0, 100.0, 1200.0, 760.0, false);
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP, EXTERNAL], Some(LAPTOP)),
            restored(1000.0, 100.0, 1200.0, 760.0, false)
        );
        // With the external display gone, the part left on the laptop still carries enough of
        // the top bar, and the frame is pulled back onto the laptop.
        assert_eq!(
            initial_window(Some(&state), &[LAPTOP], Some(LAPTOP)),
            restored(312.0, 100.0, 1200.0, 760.0, false)
        );
    }

    #[test]
    fn centring_keeps_an_oversized_window_inside_the_area() {
        let small = Rect {
            x: 0.0,
            y: 25.0,
            width: 1024.0,
            height: 700.0,
        };
        assert_eq!(centred(DEFAULT_SIZE, small), (0.0, 25.0));
    }

    fn frame(x: f64) -> Rect {
        Rect {
            x,
            y: 100.0,
            width: 1200.0,
            height: 760.0,
        }
    }

    #[test]
    fn tracker_saves_the_latest_normal_frame() {
        let start = Instant::now();
        let mut tracker = Tracker::new(frame(0.0), false);
        tracker.record_normal(frame(10.0), start);
        tracker.record_normal(frame(20.0), start + Duration::from_millis(16));
        assert_eq!(tracker.state(), saved(20.0, 100.0, 1200.0, 760.0, false));
    }

    #[test]
    fn tracker_drops_a_zoom_animations_frames_when_it_ends_maximized() {
        let start = Instant::now();
        let mut tracker = Tracker::new(frame(0.0), false);
        tracker.record_normal(frame(10.0), start);
        // The zoom starts well after the last move, so that move has settled.
        let zoom = start + Duration::from_secs(5);
        tracker.record_normal(frame(11.0), zoom);
        tracker.record_normal(frame(12.0), zoom + Duration::from_millis(16));
        tracker.record_maximized(zoom + Duration::from_millis(32));
        assert_eq!(tracker.state(), saved(10.0, 100.0, 1200.0, 760.0, true));
    }

    #[test]
    fn tracker_keeps_a_settled_frame_through_a_maximize_with_no_animation() {
        let start = Instant::now();
        let mut tracker = Tracker::new(frame(0.0), false);
        tracker.record_normal(frame(10.0), start);
        tracker.record_maximized(start + Duration::from_secs(5));
        assert_eq!(tracker.state(), saved(10.0, 100.0, 1200.0, 760.0, true));
    }

    #[test]
    fn tracker_starts_from_the_frame_the_window_was_created_with() {
        // Created maximized, then quit without ever leaving that state: the normal frame is the
        // one the window was created with.
        let start = Instant::now();
        let mut tracker = Tracker::new(frame(10.0), true);
        tracker.record_maximized(start);
        assert_eq!(tracker.state(), saved(10.0, 100.0, 1200.0, 760.0, true));
    }

    #[test]
    fn tracker_restores_the_normal_frame_after_unmaximizing() {
        let start = Instant::now();
        let mut tracker = Tracker::new(frame(10.0), true);
        tracker.record_normal(frame(300.0), start);
        tracker.record_normal(frame(10.0), start + Duration::from_millis(16));
        assert_eq!(tracker.state(), saved(10.0, 100.0, 1200.0, 760.0, false));
    }

    /// A fresh directory of this test's own, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "octoboard-window-state-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_state_file_reads_as_none() {
        let scratch = Scratch::new("missing");
        assert_eq!(read_state(&scratch.0.join(FILE_NAME)), None);
    }

    #[test]
    fn corrupt_state_file_reads_as_none() {
        let scratch = Scratch::new("corrupt");
        fs::create_dir_all(&scratch.0).unwrap();
        let path = scratch.0.join(FILE_NAME);
        fs::write(&path, b"{\"frame\": {\"x\": 1").unwrap();
        assert_eq!(read_state(&path), None);
        fs::write(&path, b"{\"x\": 1, \"y\": 2}").unwrap();
        assert_eq!(read_state(&path), None);
    }

    #[test]
    fn failed_write_leaves_no_partial_file_behind() {
        let scratch = Scratch::new("failed-write");
        // A directory where the file should be: writing the sibling succeeds, renaming a file over
        // a directory fails.
        let path = scratch.0.join(FILE_NAME);
        fs::create_dir_all(&path).unwrap();
        let state = saved(100.0, 100.0, 1200.0, 760.0, false);
        assert!(write_state(&path, &state).is_err());
        let partials: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".partial"))
            .collect();
        assert!(partials.is_empty(), "left behind: {partials:?}");
    }

    #[test]
    fn state_round_trips_through_a_directory_created_on_write() {
        let scratch = Scratch::new("round-trip");
        let path = scratch.0.join("nested").join(FILE_NAME);
        let first = saved(1700.0, 100.0, 1200.0, 760.0, true);
        write_state(&path, &first).unwrap();
        assert_eq!(read_state(&path), Some(first));

        // Overwriting goes through the same rename, and leaves nothing else behind.
        let second = saved(-700.0, -1000.0, 1300.0, 800.0, false);
        write_state(&path, &second).unwrap();
        assert_eq!(read_state(&path), Some(second));
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME]);
    }
}
