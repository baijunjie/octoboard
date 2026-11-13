//! The executable the helpers of a launch run (hook, MCP server, status line) where the launch's
//! terminal runs in the daemon itself, with no holder (one could not be set up).
//!
//! A launch in a holder is given the holder's own copy of the build (`crate::holder::build`). The
//! daemon's own path is its running image's (`AppState::self_exe`), which an update replaces with
//! the new build while the daemon goes on running the old one until its restart: helpers started
//! from it would run the new build against the old daemon, those of a session launched before the
//! update included, whose hook script and MCP configuration name the path. So a daemon that updates
//! itself gives them a copy of the running build, made from the open image the way a holder's is,
//! in the same directory of copies, from the first launch on; so does any daemon once the path no
//! longer leads to the running build. A copy that cannot be made falls back to the path.

use std::path::Path;
use std::sync::OnceLock;

use crate::holder::build::Builds;
use crate::state::AppState;

/// The executable to give the helpers of a launch whose terminal runs in the daemon itself. Where a
/// holder runs the terminals it is `self_exe`, which the holder's own replaces.
pub(crate) async fn helper_exe(state: &AppState) -> String {
    let self_exe = state.self_exe.clone();
    if crate::holder::client::current().is_some() {
        return self_exe;
    }
    let updates = state.daemon_update.enabled();
    let exe = self_exe.clone();
    tokio::task::spawn_blocking(move || {
        local_helper_exe(
            &exe,
            updates,
            crate::platform::proc::names_running_image,
            builds(),
        )
    })
    .await
    .unwrap_or(self_exe)
}

/// The one directory of copies the helpers' copies are made in, so that two launches make and
/// hand out a copy one at a time (`Builds::run_from`).
fn builds() -> &'static Builds {
    static BUILDS: OnceLock<Builds> = OnceLock::new();
    BUILDS.get_or_init(|| Builds::new(crate::paths::builds_dir()))
}

/// `self_exe` where the daemon does not update itself (`updates`) and it leads to the running build
/// (`is_running_image`), else the copy of the running build in `builds`, made if there is none.
/// Blocking.
pub(super) fn local_helper_exe(
    self_exe: &str,
    updates: bool,
    is_running_image: fn(&Path) -> bool,
    builds: &Builds,
) -> String {
    if !updates && is_running_image(Path::new(self_exe)) {
        return self_exe.to_owned();
    }
    builds.run_from(
        crate::holder::build::build_id(),
        Path::new(self_exe),
        |exe| exe.to_string_lossy().into_owned(),
    )
}
