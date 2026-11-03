//! The accounts a session was moved off for a usage limit, so that it is not moved straight back
//! to one.

use std::time::Duration;

use super::AppState;
use crate::time::now_millis;

/// milliseconds since the epoch.
pub(super) type AccountLeft = (Option<String>, i64);

impl AppState {
    /// Records that Octoboard moved the session off `account` (`None` the default account)
    /// because a turn stopped on its usage limit.
    pub fn note_left_for_limit(&self, id: &str, account: Option<&str>) {
        self.left_for_limit
            .lock()
            .expect("left-for-limit lock poisoned")
            .entry(id.to_string())
            .or_default()
            .push((account.map(str::to_string), now_millis()));
    }

    /// Drops what is held of the accounts the session was moved off for a limit, for a session
    /// archived or deleted.
    pub fn forget_left_for_limit(&self, id: &str) {
        self.cancel_recoveries(id);
        self.left_for_limit
            .lock()
            .expect("left-for-limit lock poisoned")
            .remove(id);
    }

    /// The accounts the session was moved off for a usage limit less than `within` ago, on the wall
    /// clock, which a machine that sleeps does not stop.
    pub fn recently_left_for_limit(&self, id: &str, within: Duration) -> Vec<Option<String>> {
        let mut left = self
            .left_for_limit
            .lock()
            .expect("left-for-limit lock poisoned");
        let now = now_millis();
        let Some(accounts) = left.get_mut(id) else {
            return Vec::new();
        };
        accounts.retain(|(_, at)| now - *at < within.as_millis() as i64);
        let recent = accounts
            .iter()
            .map(|(account, _)| account.clone())
            .collect();
        if accounts.is_empty() {
            left.remove(id);
        }
        recent
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::app_state;
    use std::time::Duration;

    /// A session moved off an account for its limit is remembered for a while, per account, and
    /// then forgotten.
    #[test]
    fn an_account_left_for_a_limit_is_remembered_for_a_while() {
        let (state, _dir) = app_state("state-left-for-limit");
        state.note_left_for_limit("s", None);
        state.note_left_for_limit("s", Some("claude-a"));
        state.note_left_for_limit("other", Some("claude-b"));

        let mut left = state.recently_left_for_limit("s", Duration::from_secs(60));
        left.sort();
        assert_eq!(left, [None, Some("claude-a".to_string())]);
        assert!(state
            .recently_left_for_limit("s", Duration::ZERO)
            .is_empty());
        assert!(state
            .recently_left_for_limit("s", Duration::from_secs(60))
            .is_empty());
        assert_eq!(
            state.recently_left_for_limit("other", Duration::from_secs(60)),
            [Some("claude-b".to_string())]
        );
    }
}
