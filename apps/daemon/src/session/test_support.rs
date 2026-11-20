//! A running session with a scratch directory, for the tests of what Octoboard keeps there.

use std::path::PathBuf;
use std::sync::Arc;

use crate::protocol::{AgentId, SessionStatus};
use crate::state::test_support::insert_record;
use crate::state::AppState;
use crate::test_support::{app_state, fake_live_session_in, ScratchDir};

/// The first bytes of a PNG file, and some more.
pub(crate) const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest of the image";

pub(crate) struct Fixture {
    pub(crate) state: Arc<AppState>,
    pub(crate) id: String,
    /// The stand-in process, which ends before the directory its state lives in goes.
    pub(crate) live: Option<Box<dyn std::any::Any>>,
    _dir: ScratchDir,
    /// The directory of the sessions' scratch directories.
    pub(crate) run: ScratchDir,
}

/// A state with a stored session record, running when `running`, its scratch directory under
/// `run`.
pub(crate) fn fixture(running: bool) -> Fixture {
    let id = "scratch-session".to_string();
    let (state, dir) = app_state("scratch");
    insert_record(&state, &dir, &id, SessionStatus::Working);
    let run = ScratchDir::new("scratch-run");
    let live = running.then(|| {
        let scratch = run.join(&id);
        std::fs::create_dir_all(&scratch).unwrap();
        let live = fake_live_session_in(&id, AgentId::CLAUDE, "sleep 30", scratch);
        state.register_live(live.clone());
        Box::new(live) as Box<dyn std::any::Any>
    });
    Fixture {
        state,
        id,
        live,
        _dir: dir,
        run,
    }
}

/// Where the pasted images of the session `id` go.
pub(crate) fn pasted_directory(f: &Fixture, id: &str) -> PathBuf {
    f.run.join(id).join("pasted")
}
