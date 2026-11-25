//! Telling a value Octoboard set for one session from a value the user set themselves.
//!
//! A launch lays its own values over the shell snapshot: an account's configuration directory,
//! the files and switches a provider's plan names, Octoboard's own markers. A daemon started from
//! inside that session (a development daemon run from an agent's shell) inherits them, and a
//! variable alone does not say whose value it holds: a user who exports the same variable in their
//! shell profile means it for every session, while Octoboard meant its value for that one session
//! only. So the launch records every variable it changed in a marker variable of its own,
//! [`MARKER`], and an environment inherited under that marker has each recorded variable put back
//! as the snapshot had it, as long as it still holds the value the launch gave it.
//!
//! The marker never repeats a value the launch set, which can be a secret a plan carries (a
//! compatibility source's MCP header): it holds a digest of it, enough to tell that the variable
//! still holds it. The value the snapshot held before is the user's own, which every process they
//! start already sees, and is recorded as it is so that it can be put back.
//!
//! Nothing here names an agent or a variable of one: what is recorded is whatever the overlay
//! changed.

use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};

/// The variable a session carries naming what its launch changed from the shell snapshot: a JSON
/// object from each changed variable's name to `[digests, previous]`, where `digests` is the
/// [`digest`] of the value the launch set (one or more of them, [`DIGEST_LEN`] characters each:
/// see [`Change`]) and `previous` what the snapshot held before it (`null`
/// when it held none, or when it is not recorded: see [`MARKER_LIMIT`]). It is one of
/// Octoboard's own session markers, which a snapshot never passes on.
pub const MARKER: &str = "OCTOBOARD_SESSION_EXPORTS";

/// The most bytes the marker's value takes, as it is written. A host bounds one variable (32767
/// UTF-16 units on Windows, 128 KiB on Linux, and a UTF-8 byte count is never below the UTF-16
/// one), and a plan can replace a variable of any length (a whole configuration document the user
/// keeps in one), so the previous values are what gives way: they are recorded shortest first
/// while the marker stays within this, and one that does not fit is recorded as none. A daemon
/// that inherits the session then drops that variable rather than keeping the session's value,
/// and the user's value comes back only if their shell sets it again. The digests are never left
/// out, since they are what keeps a session's value from being taken for the user's; alone they
/// stay within this for some four hundred variables.
const MARKER_LIMIT: usize = 24 * 1024;

// With its name and the `=`, the marker is one variable within the tightest host's bound.
const _: () = assert!(MARKER.len() + 1 + MARKER_LIMIT < 32_767);

/// What a launch did to one variable: the digest of the value it set, and the value before. Where
/// the session's program is known to give its commands another value under the same name, that
/// value's digest follows the first with nothing between them ([`DIGEST_LEN`] characters each):
/// the variable is the session's while it holds either.
type Change = (String, Option<String>);

/// The length of one [`digest`].
const DIGEST_LEN: usize = 16;

/// Whether `digests`, one digest or several run together, has `digest` among them.
fn has_digest(digests: &str, digest: &str) -> bool {
    digests
        .as_bytes()
        .chunks(DIGEST_LEN)
        .any(|one| one == digest.as_bytes())
}

/// What stands for the value `value` of the variable `name` in the marker: the first 64 bits of
/// the SHA-256 of the two, in hexadecimal. Short, because the only question asked of it is
/// whether a variable still holds what the launch set, and one entry is recorded per variable.
fn digest(name: &str, value: &str) -> String {
    let hash = Sha256::new()
        .chain_update(name.as_bytes())
        .chain_update([0])
        .chain_update(value.as_bytes())
        .finalize();
    hash[..DIGEST_LEN / 2]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `value` as the marker's text.
fn written(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// [`session_environment_with`] for a session whose program sets nothing for its commands: what
/// the tests that do not exercise those call.
#[cfg(test)]
pub fn session_environment(
    shell_env: &HashMap<String, String>,
    overlay: &[(String, String)],
) -> HashMap<String, String> {
    session_environment_with(shell_env, overlay, &[])
}

/// The environment of a launch: `shell_env` with `overlay` laid over it, a later entry winning,
/// and [`MARKER`] recording every variable the overlay gave a value other than the snapshot's. A
/// variable the overlay left as the snapshot had it is not recorded: it is the user's, and a
/// daemon that inherits it keeps it. Everything a launch sets for its session goes through here.
/// An overlay can only set: a launch that ever has to remove a variable of the snapshot's must
/// have that recorded here too, or a daemon started inside the session would not get it back.
///
/// For a session whose program is known to set `command_environment` (each a name and the exact
/// value) for the commands it runs, nothing is set for these: each is
/// recorded as if the launch had set it, with the digest of the value the program will give it
/// and what this environment holds under the name, so that an environment inherited from one of
/// those commands is put back the same way. One this environment already holds at that very
/// value is the user's own and is not recorded. One the overlay sets too is recorded once, with
/// both digests and what the snapshot held: a command may hold the overlay's value or the
/// program's, and either is the session's.
pub fn session_environment_with(
    shell_env: &HashMap<String, String>,
    overlay: &[(String, String)],
    command_environment: &[(&str, &str)],
) -> HashMap<String, String> {
    let mut environment = shell_env.clone();
    environment.extend(overlay.iter().cloned());
    environment.remove(MARKER);
    // By name, so that one the overlay sets twice is recorded, and charged, once.
    let mut changes: BTreeMap<&str, Change> = BTreeMap::new();
    let mut previous: Vec<(&str, &String, usize)> = Vec::new();
    for (name, _) in overlay {
        let (Some(value), false) = (environment.get(name), changes.contains_key(name.as_str()))
        else {
            continue;
        };
        let before = shell_env.get(name);
        if before == Some(value) {
            continue;
        }
        changes.insert(name.as_str(), (digest(name, value), None));
        if let Some(before) = before {
            // What recording it adds to the marker: its text in place of `null`.
            previous.push((name.as_str(), before, written(before).len() - "null".len()));
        }
    }
    for (name, value) in command_environment {
        let declared = digest(name, value);
        if let Some((digests, _)) = changes.get_mut(*name) {
            if !has_digest(digests, &declared) {
                digests.push_str(&declared);
            }
            continue;
        }
        let before = environment.get(*name);
        if before.map(String::as_str) == Some(*value) {
            continue;
        }
        changes.insert(*name, (declared, None));
        if let Some(before) = before {
            previous.push((*name, before, written(before).len() - "null".len()));
        }
    }
    let mut size = written(&changes).len();
    previous.sort_by_key(|(name, _, cost)| (*cost, *name));
    for (name, before, cost) in previous {
        if size + cost > MARKER_LIMIT {
            tracing::warn!(
                variable = name,
                "the value this variable had before the launch is too long to record; a daemon \
                 started inside this session will not have the variable unless the shell sets it"
            );
            continue;
        }
        size += cost;
        if let Some(change) = changes.get_mut(name) {
            change.1 = Some(before.clone());
        }
    }
    if !changes.is_empty() {
        environment.insert(MARKER.to_string(), written(&changes));
    }
    environment
}

/// Undoes, in an environment inherited from a session, what that session's launch recorded in
/// [`MARKER`]: each recorded variable that still holds the value the launch set gets the value it
/// had before, or is removed when none is recorded. A variable that holds anything else was set
/// since (by the user inside the session, or by their shell profile when the snapshot shell ran)
/// and is left alone, as is everything when there is no marker or it cannot be read. The marker
/// itself is left for the caller's filter.
pub fn revert_inherited(env: &mut HashMap<String, String>) {
    let Some(changes) = env
        .get(MARKER)
        .and_then(|text| serde_json::from_str::<BTreeMap<String, Change>>(text).ok())
    else {
        return;
    };
    for (name, (set, previous)) in changes {
        if !env
            .get(&name)
            .is_some_and(|value| has_digest(&set, &digest(&name, value)))
        {
            continue;
        }
        match previous {
            Some(previous) => env.insert(name, previous),
            None => env.remove(&name),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VAR: &str = "AGENT_HOME";

    fn env(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn overlay(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// What a daemon started inside a session sees of `session`'s environment once reverted,
    /// without the marker, which its snapshot filters.
    fn inherited(session: &HashMap<String, String>) -> HashMap<String, String> {
        let mut env = session.clone();
        revert_inherited(&mut env);
        env.remove(MARKER);
        env
    }

    /// Every variable a launch sets keeps the launch's value in the session and is put back for a
    /// daemon that inherits it: removed where the snapshot had none, the user's value where the
    /// launch replaced one.
    #[test]
    fn every_variable_a_launch_sets_is_put_back_as_the_snapshot_had_it() {
        let shell = env(&[("PATH", "/bin"), (VAR, "/users/own")]);
        let laid = overlay(&[(VAR, "/accounts/work"), ("SWITCH", "1"), ("EMPTY", "")]);
        let session = session_environment(&shell, &laid);
        for (name, value) in &laid {
            assert_eq!(&session[name], value);
        }
        assert_eq!(inherited(&session), shell);
    }

    /// A variable the overlay leaves as the snapshot had it is the user's: nothing is recorded
    /// for it, and it is inherited unchanged. With nothing changed there is no marker at all.
    #[test]
    fn a_variable_the_launch_left_as_the_user_had_it_is_not_recorded() {
        let shell = env(&[(VAR, "/users/own")]);
        for laid in [overlay(&[]), overlay(&[(VAR, "/users/own")])] {
            assert_eq!(session_environment(&shell, &laid), shell);
        }
        let session = session_environment(&shell, &overlay(&[(VAR, "/users/own"), ("X", "1")]));
        assert!(!session[MARKER].contains(VAR));
        assert_eq!(inherited(&session), shell);
    }

    /// A recorded variable that no longer holds the value the launch set was set since, and stays
    /// as it is; one unset since stays unset.
    #[test]
    fn a_variable_changed_since_the_launch_is_kept() {
        let shell = env(&[(VAR, "/users/own")]);
        let mut session = session_environment(&shell, &overlay(&[(VAR, "/accounts/work")]));
        session.insert(VAR.to_string(), "/changed/inside".to_string());
        assert_eq!(inherited(&session), env(&[(VAR, "/changed/inside")]));
        session.remove(VAR);
        assert_eq!(inherited(&session), env(&[]));
    }

    /// The marker holds no value a launch set, whatever it is, only the user's own previous one.
    #[test]
    fn the_marker_repeats_no_value_the_launch_set() {
        let secret = "Bearer s3cr3t \"quoted\"\\,[x]=é\n";
        let shell = env(&[(VAR, "the user's own")]);
        let session = session_environment(&shell, &overlay(&[(VAR, secret), ("TOKEN", secret)]));
        let marker = &session[MARKER];
        assert!(!marker.contains("s3cr3t"), "{marker}");
        assert!(marker.contains("the user's own"), "{marker}");
        assert_eq!(inherited(&session), shell);
    }

    /// The same value under two names has two digests: an entry says nothing about another.
    #[test]
    fn a_digest_is_of_the_name_and_the_value() {
        assert_ne!(digest("A", "1"), digest("B", "1"));
        assert_ne!(digest("A", "1"), digest("A", "2"));
        assert_ne!(digest("AB", "C"), digest("A", "BC"));
        assert_eq!(digest("A", "1").len(), 16);
    }

    /// The marker stays within its limit as it is written, escapes included: previous values are
    /// recorded shortest first, and one that would take it over is recorded as none, so that the
    /// variable is dropped rather than left holding the session's value. A name the overlay sets
    /// twice is recorded once.
    #[test]
    fn the_marker_stays_within_its_limit_by_leaving_out_the_longest_previous_values() {
        // Each backslash is two bytes in the marker.
        let escaped = "\\".repeat(MARKER_LIMIT / 4);
        let long = "x".repeat(MARKER_LIMIT);
        let shell = env(&[("ESCAPED", &escaped), ("LONG", &long), ("SHORT", "mine")]);
        let laid = overlay(&[
            ("LONG", "theirs"),
            ("ESCAPED", "theirs"),
            ("SHORT", "first"),
            ("SHORT", "theirs"),
        ]);
        let session = session_environment(&shell, &laid);
        assert_eq!(session["SHORT"], "theirs");
        assert!(session[MARKER].len() <= MARKER_LIMIT);
        assert!(session[MARKER].len() > MARKER_LIMIT / 2);
        assert_eq!(session[MARKER].matches("SHORT").count(), 1);
        assert_eq!(
            inherited(&session),
            env(&[("ESCAPED", &escaped), ("SHORT", "mine")])
        );

        let both = "\\".repeat(MARKER_LIMIT / 4 + 100);
        let shell = env(&[("A", &both), ("B", &both)]);
        let session = session_environment(&shell, &overlay(&[("A", "theirs"), ("B", "theirs")]));
        assert!(session[MARKER].len() <= MARKER_LIMIT);
        assert_eq!(inherited(&session), env(&[("A", &both)]));
    }

    /// What the session's program sets for its commands is recorded without being set: an
    /// inherited variable holding exactly that value is put back as the launch environment had it
    /// (the user's value, or none), one the user holds at that very value is theirs and stays, and
    /// any other value is kept. The marker holds none of the program's values.
    #[test]
    fn what_the_program_sets_for_its_commands_is_put_back_by_value() {
        let shell = env(&[("PAGER", "less"), ("NO_COLOR", "1")]);
        let declared = [
            ("PAGER", "cat"),
            ("NO_COLOR", "1"),
            ("GIT_PAGER", "cat-for-git"),
        ];
        let session = session_environment_with(&shell, &overlay(&[]), &declared);
        let marker = session[MARKER].clone();
        assert!(
            !marker.contains("NO_COLOR") && !marker.contains("cat-for-git"),
            "{marker}"
        );
        let mut launched = session.clone();
        launched.remove(MARKER);
        assert_eq!(launched, shell);

        let mut command = session.clone();
        command.extend(env(&[("PAGER", "cat"), ("GIT_PAGER", "cat-for-git")]));
        assert_eq!(inherited(&command), shell);

        command.extend(env(&[("PAGER", "most"), ("GIT_PAGER", "delta")]));
        assert_eq!(
            inherited(&command),
            env(&[("PAGER", "most"), ("NO_COLOR", "1"), ("GIT_PAGER", "delta")])
        );
    }

    /// A name the launch sets and the program then sets again for its commands is the session's at
    /// either value, and is put back to what the snapshot held; at any other value it is kept.
    #[test]
    fn a_name_the_launch_and_the_program_both_set_is_the_sessions_at_either_value() {
        let shell = env(&[("PAGER", "less")]);
        let laid = overlay(&[("PAGER", "the launch's")]);
        let session = session_environment_with(&shell, &laid, &[("PAGER", "cat")]);
        assert_eq!(session["PAGER"], "the launch's");
        assert_eq!(inherited(&session), shell);
        for (held, expected) in [("cat", "less"), ("most", "most")] {
            let mut command = session.clone();
            command.insert("PAGER".to_string(), held.to_string());
            assert_eq!(inherited(&command), env(&[("PAGER", expected)]), "{held}");
        }
        // The same value from both is one digest.
        let same = session_environment_with(&shell, &laid, &[("PAGER", "the launch's")]);
        assert_eq!(same[MARKER].len(), session[MARKER].len() - DIGEST_LEN);
    }

    /// A marker inherited from an outer session is not carried into a launch's own, and one that
    /// is not what a launch writes undoes nothing.
    #[test]
    fn a_marker_that_is_not_this_launchs_own_is_not_followed() {
        let stale = env(&[(MARKER, "{}")]);
        assert_eq!(session_environment(&stale, &overlay(&[])), env(&[]));
        for marker in ["", "not json", "[]", "{\"AGENT_HOME\":\"/accounts/work\"}"] {
            let mut session = env(&[(VAR, "/accounts/work"), (MARKER, marker)]);
            revert_inherited(&mut session);
            assert_eq!(session[VAR], "/accounts/work", "{marker:?}");
        }
    }
}
