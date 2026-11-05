//! The platform's part in the application's life around the shared exit flow (`exit.rs`) and
//! running in the background (`background.rs`): the system's own ways in and out of the app, and
//! what hiding and showing the window takes beyond the window itself. The decisions stay in those
//! two modules; these are the hooks they and `run` call.
//!
//! - `register_first`: the plugins that have to come before every other one.
//! - `install`: from `setup`, once the event loop is up.
//! - `on_run_event`: a `RunEvent` neither the exit flow nor the window state answers.
//! - `hide_after_leaving_fullscreen`: whether the window has to leave fullscreen before it hides.
//! - `window_hidden` and `before_show`: around the close button's hide and every show of the
//!   window.
//! - `LifecycleState`: what these keep between calls.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

pub use platform::{
    before_show, hide_after_leaving_fullscreen, install, on_run_event, register_first,
    window_hidden, LifecycleState,
};
