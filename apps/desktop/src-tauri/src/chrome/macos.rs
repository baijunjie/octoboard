//! The macOS window frame: an overlay titlebar with the traffic lights floating over the UI's own
//! top bar, and a translucent window.

use tauri::{
    window::{Effect, EffectsBuilder},
    LogicalPosition, TitleBarStyle,
};

use super::SystemFrame;
use crate::MainWindowBuilder;

/// The traffic lights' inset from the window's top-left, in points, as tao applies it to the close
/// button. `Y` was tuned by eye so the buttons sit centred in the UI's 40 pt top bar.
const TRAFFIC_LIGHT_X: f64 = 16.0;
const TRAFFIC_LIGHT_Y: f64 = 21.0;

/// The overlay titlebar draws over the page, so the window's outer size is its inner size, and
/// AppKit fits a new titled window onto its screen itself (`constrainFrameRect:toScreen:`).
pub fn system_frame() -> SystemFrame {
    SystemFrame {
        extra: (0.0, 0.0),
        fits_new_window: true,
    }
}

/// WKWebView has no browser keys of its own: the app menu holds every command's key.
pub fn install(_window: &tauri::WebviewWindow) {}

pub fn decorate(builder: MainWindowBuilder<'_>) -> MainWindowBuilder<'_> {
    // The UI draws a top bar across the whole window (`TitleBar` in `packages/ui`), so the native
    // titlebar's background and text go and the traffic lights float over the page.
    // `TRAFFIC_LIGHT_X` / `TRAFFIC_LIGHT_Y` centre them in the bar's `--title-bar-height`, and
    // `TRAFFIC_LIGHT_INSET` in `host_facts.rs` is the width the bar keeps clear for them, which the
    // window's query hands the UI; the three move together. The window keeps its title for the Dock and Mission
    // Control.
    //
    // The window is also translucent: a sidebar-material `NSVisualEffectView` behind a transparent
    // webview, which is why the UI paints nothing of its own behind the top bar and the left rail
    // (the window chrome) and paints the content panel opaquely (`--window-background` in
    // `packages/ui/src/style.css`). The material follows the window's appearance, which the UI
    // pushes before it reveals the window (`packages/ui/src/main.tsx`), and the window stays hidden
    // until then, so neither the material nor the transparent webview is ever seen in the wrong
    // appearance. Transparency needs `macos-private-api` (`Cargo.toml`, `tauri.conf.json`).
    builder
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y))
        .transparent(true)
        .effects(EffectsBuilder::new().effect(Effect::Sidebar).build())
}
