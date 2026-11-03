//! How the lower layers tell the automatic account switch (`crate::coordinator::auto_switch`) that
//! a quota reading was recorded or a turn failed, without naming it.

use std::sync::Arc;

use super::AppState;
use crate::protocol::Agent;

/// The two entry points of the automatic account switch, installed once where the daemon's
/// `AppState` is built.
pub(crate) struct AutoSwitchHooks {
    /// See [`AppState::after_quota_refresh`].
    pub after_quota_refresh: fn(
        &Arc<AppState>,
        Agent,
        Option<String>,
        Option<String>,
        i64,
    ) -> Option<tokio::task::JoinHandle<()>>,
    /// See [`AppState::after_failed_turn`].
    pub after_failed_turn: fn(&Arc<AppState>, &str) -> Option<tokio::task::JoinHandle<bool>>,
}

impl AppState {
    /// Installs the automatic switch, once. A second call changes nothing.
    pub(crate) fn install_auto_switch(&self, hooks: AutoSwitchHooks) {
        let _ = self.auto_switch.set(hooks);
    }

    /// The installed hooks. Where none is installed the daemon was assembled without the switch, so
    /// that is logged, once, and the callers go on without it.
    fn installed_auto_switch(&self) -> Option<&AutoSwitchHooks> {
        static WARNED: std::sync::Once = std::sync::Once::new();
        let hooks = self.auto_switch.get();
        if hooks.is_none() {
            WARNED.call_once(|| {
                tracing::warn!("no automatic account switch is installed, so none will run");
            });
        }
        hooks
    }

    /// Has the automatic switch reconsider its waits after a newly recorded, successful probe of
    /// the account. `None` when nothing was scheduled, and always where no switch is installed.
    pub(crate) fn after_quota_refresh(
        self: &Arc<Self>,
        agent: Agent,
        account: Option<String>,
        config_dir: Option<String>,
        started_at: i64,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let hooks = self.installed_auto_switch()?;
        (hooks.after_quota_refresh)(self, agent, account, config_dir, started_at)
    }

    /// Has the automatic switch judge the turn of the session `id` that has just failed. `None`
    /// when nothing was scheduled, and always where no switch is installed.
    pub(crate) fn after_failed_turn(
        self: &Arc<Self>,
        id: &str,
    ) -> Option<tokio::task::JoinHandle<bool>> {
        let hooks = self.installed_auto_switch()?;
        (hooks.after_failed_turn)(self, id)
    }
}
