//! Whether the host's credential store holds an item an agent keeps there, for a provider whose
//! sign-in leaves nothing else to read (`AccountOps::sign_in_state`): the host's query
//! (`platform::credential_store`) run within bounds. The query uses only the item's name.
//! Command output is ignored, and native lookups return only presence.
//!
//! Every way of not getting an answer reads as [`Presence::CannotTell`]: a host with no query, a
//! query that cannot be started, one that does not end in time, and one that ends in a way that
//! is neither "present" nor "absent". On macOS a locked login keychain was not measured and falls
//! under one of those.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::platform::credential_store::{self, Presence, PresenceQuery};
use crate::platform::proc::Reach;
use crate::processes::subprocess::{run_bounded, Bounds};

/// How long the store's query is given. It was measured to answer in under 0.3 s; one that takes
/// longer waits on something, a person's consent perhaps, that the daemon must not wait for.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// The most the query may print: an item's attributes are a few hundred bytes.
const QUERY_OUTPUT_LIMIT: usize = 64 * 1024;

/// The bounds of the store's query: it is no agent CLI, so a stop reaches its group, and it has
/// nothing of its own to finish, so it is killed at once.
fn bounds(timeout: Duration) -> Bounds {
    Bounds {
        timeout,
        stdout_limit: QUERY_OUTPUT_LIMIT,
        stderr_limit: QUERY_OUTPUT_LIMIT,
        reach: Reach::Group,
        term_grace: Duration::ZERO,
    }
}

/// Whether the credential store holds the declared service and account or the declared target.
/// **Blocks**, for
/// [`QUERY_TIMEOUT`] at most; a caller on the runtime runs it on a blocking thread, as the account
/// poll runs a provider's sign-in reading.
pub(crate) fn item_present(service: &str, account: &str, target: &str) -> Presence {
    ask(credential_store::presence_query(service, account, target))
}

/// What `query` answers, run within the bounds of [`item_present`]; a host with no query (`None`)
/// cannot tell. **Blocks.**
pub(crate) fn ask(query: Option<PresenceQuery>) -> Presence {
    ask_within(query, QUERY_TIMEOUT)
}

fn ask_within(query: Option<PresenceQuery>, timeout: Duration) -> Presence {
    let query = match query {
        Some(PresenceQuery::Command(query)) => query,
        Some(native) => return native.native(timeout),
        None => return Presence::CannotTell,
    };
    let never = AtomicBool::new(false);
    match run_bounded(&mut query.command(), &bounds(timeout), &never) {
        Ok(output) => query.read(output.status.code()),
        Err(failure) => {
            tracing::debug!(
                failure = ?failure,
                "the credential store's presence query gave no answer"
            );
            Presence::CannotTell
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::test_support::{bash, PATIENCE};

    /// A stand-in store's query: a shell script, with 44 for an item that is not there, as the
    /// one measured store answers.
    fn script(script: &str) -> Option<PresenceQuery> {
        Some(PresenceQuery::stand_in(bash(), &["-c", script], 44))
    }

    #[test]
    fn the_stores_answer_is_read_from_how_its_query_ends() {
        let cases = [
            ("the query ends well", "exit 0", Presence::Present),
            (
                "it ends as an absent item does",
                "exit 44",
                Presence::Absent,
            ),
            ("it ends any other way", "exit 1", Presence::CannotTell),
            (
                "what it prints changes nothing",
                "echo 'password: nothing of the kind'; exit 44",
                Presence::Absent,
            ),
        ];
        for (name, text, expected) in cases {
            assert_eq!(ask(script(text)), expected, "{name}");
        }
    }

    /// A name no item can have is not asked about on any host, so the host's own store is never
    /// reached by this.
    #[test]
    fn an_item_with_a_name_that_is_not_asked_about_cannot_be_told() {
        assert_eq!(item_present("", "me@example.com", ""), Presence::CannotTell);
        assert_eq!(item_present("some-service", "", ""), Presence::CannotTell);
    }

    #[test]
    fn a_host_with_no_query_cannot_tell() {
        assert_eq!(ask(None), Presence::CannotTell);
    }

    #[test]
    fn a_query_that_cannot_be_started_cannot_tell() {
        let missing = PresenceQuery::stand_in("/no/such/credential-store-tool", &[], 44);
        assert_eq!(ask(Some(missing)), Presence::CannotTell);
    }

    #[test]
    fn a_query_that_does_not_end_in_time_cannot_tell_and_is_not_waited_for() {
        let started = Instant::now();
        let answer = ask_within(script("sleep 300"), Duration::from_millis(300));
        assert_eq!(answer, Presence::CannotTell);
        assert!(
            started.elapsed() < PATIENCE,
            "the query was waited for past its deadline"
        );
    }

    #[test]
    fn a_query_that_prints_past_its_limit_cannot_tell() {
        let flood = format!("head -c {} /dev/zero; exit 0", QUERY_OUTPUT_LIMIT + 1);
        assert_eq!(ask(script(&flood)), Presence::CannotTell);
    }
}
