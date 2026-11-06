//! What a running import tells the clients: where each of its items stands, as
//! `configuration_import_progress` broadcasts naming the import by the token the request carried.

use std::sync::Arc;

use crate::protocol::{Event, ImportFailure, ImportItem, ImportItemState};
use crate::state::AppState;

/// Reports the progress of one import; silent when the request named none.
pub(super) struct Progress {
    state: Arc<AppState>,
    import_id: Option<String>,
}

impl Progress {
    pub(super) fn new(state: &Arc<AppState>, import_id: Option<String>) -> Self {
        Self {
            state: state.clone(),
            import_id,
        }
    }

    pub(super) fn report(&self, item: ImportItem, state: ImportItemState) {
        if let Some(import_id) = &self.import_id {
            self.state.broadcast(Event::ConfigurationImportProgress {
                import_id: import_id.clone(),
                item,
                state,
            });
        }
    }

    pub(super) fn failed(&self, item: ImportItem, failure: &ImportFailure) {
        self.report(
            item,
            ImportItemState::Failed {
                error: failure.clone(),
            },
        );
    }

    pub(super) fn waiting(&self, item: ImportItem) {
        self.report(item, ImportItemState::Waiting);
    }
}
