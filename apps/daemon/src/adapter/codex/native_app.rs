//! Opening a Codex conversation in the Codex application, which is the ChatGPT application: it
//! registers `codex:` and shows the thread named by `codex://threads/<id>`, where the id is the
//! conversation's own.

use crate::adapter::{native_app_unavailable, NativeApp, NativeAppOpen};
use crate::host_open::native_app::open_url;

pub(super) struct CodexApp;

impl NativeApp for CodexApp {
    fn open(&self, open: &NativeAppOpen<'_>) -> anyhow::Result<()> {
        open_url(&format!("codex://threads/{}", open.conversation))
            .map_err(|detail| native_app_unavailable(&detail))
    }
}
