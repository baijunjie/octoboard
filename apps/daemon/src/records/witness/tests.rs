use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use super::*;
use crate::protocol::AgentId;
use crate::records::fixtures::{session, touch, CLAUDE_ID};
use crate::records::resumability::{judge, presence, Resumability};
use crate::records::Hints;

const RUNNING: AgentVersion = (2, 0, 0);

/// Has [`Watch::observe`] on this thread leave `every` between looks until the guard drops: zero
/// looks on every call, an hour looks only once, whatever the machine's speed.
fn looking_every(every: Duration) -> impl Drop {
    struct Restore(Option<Duration>);
    impl Drop for Restore {
        fn drop(&mut self) {
            LOOK_EVERY_OVERRIDE.set(self.0);
        }
    }
    Restore(LOOK_EVERY_OVERRIDE.replace(Some(every)))
}

fn claude_record(account: &std::path::Path) -> PathBuf {
    account
        .join("projects/work")
        .join(format!("{CLAUDE_ID}.jsonl"))
}

fn written_later(path: &std::path::Path, by: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(by))
        .unwrap();
}

/// A record missing from an account searched whole proves the conversation gone only where a
/// record written by the version installed for the account was seen recognised: not without
/// witnesses, not for a version that cannot be read, and not for another version than the one
/// witnessed. The same holds for the records a deletion would remove.
#[test]
fn a_missing_record_is_proof_only_with_a_witness_for_the_installed_version() {
    use crate::protocol::SessionRecordPresence::{Absent, Unknown};
    let dir = crate::test_support::ScratchDir::new("agent-records-witness");
    let account = dir.join("account");
    std::fs::create_dir_all(account.join("projects")).unwrap();
    let own = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
    let witnesses = Witnesses::default();
    let shell = HashMap::from([(
        "HOME".to_owned(),
        dir.join("home").to_string_lossy().into_owned(),
    )]);
    let verdict = |witnesses: Option<&Witnesses>| {
        let mut locator = Locator::with_shell(shell.clone());
        if let Some(witnesses) = witnesses {
            locator = locator.with_witnesses(witnesses);
        }
        let judgement = judge(&locator, &own);
        let present = presence(&Hints::default(), &mut locator, &[], &own);
        (judgement.resumability, judgement.record_not_found, present)
    };
    let unproven = (Resumability::Unknown, true, Unknown);

    assert_eq!(verdict(None), unproven);
    // No version can be read for the account.
    assert_eq!(verdict(Some(&witnesses)), unproven);
    // The installed version was never seen writing a record.
    witnesses.install(AgentId::CLAUDE, RUNNING);
    assert_eq!(verdict(Some(&witnesses)), unproven);
    // Another version was.
    witnesses.note(AgentId::CLAUDE, (1, 9, 9));
    assert_eq!(verdict(Some(&witnesses)), unproven);
    witnesses.note(AgentId::CLAUDE, RUNNING);
    assert_eq!(
        verdict(Some(&witnesses)),
        (Resumability::Unresumable, false, Absent)
    );
    // A witness of another agent proves nothing for this one.
    let others = Witnesses::default();
    others.witness_installed(AgentId::CODEX, RUNNING);
    others.install(AgentId::CLAUDE, RUNNING);
    assert_eq!(verdict(Some(&others)), unproven);

    // A record that is there is resumable either way, and no notice is owed.
    touch(&claude_record(&account));
    assert_eq!(verdict(None).0, Resumability::Resumable);
    assert!(!verdict(None).1);
}

/// What could not be searched whole stays silent: a linked records directory is `Unknown` with no
/// notice owed, witnessed or not.
#[cfg(unix)]
#[test]
fn a_record_not_searched_whole_owes_no_notice() {
    let dir = crate::test_support::ScratchDir::new("agent-records-witness-link");
    let account = dir.join("account");
    std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
    std::fs::create_dir_all(&account).unwrap();
    crate::test_support::symlink(dir.join("elsewhere"), account.join("projects")).unwrap();
    let own = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
    let judgement = judge(&Locator::with_shell(HashMap::new()), &own);
    assert_eq!(judgement.resumability, Resumability::Unknown);
    assert!(!judgement.record_not_found);
}

/// A running session witnesses the version its launch started once the search a resume makes
/// finds its record written during the launch: a record already there at the first look counts
/// only once it is written again, one that appears afterwards counts at once, and a new launch
/// starts over.
#[test]
fn a_running_session_witnesses_its_version_once_its_record_is_written() {
    let dir = crate::test_support::ScratchDir::new("agent-records-witness-watch");
    let account = dir.join("account");
    let own = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
    let locator = Locator::with_shell(HashMap::new());
    let record = claude_record(&account);
    let _every_call = looking_every(Duration::ZERO);

    // A record as an earlier release left it: found, but not written by this launch.
    touch(&record);
    let witnesses = Witnesses::default();
    let mut watch = Watch::default();
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    assert!(!witnesses.holds(AgentId::CLAUDE, RUNNING));
    written_later(&record, 5);
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    assert!(witnesses.holds(AgentId::CLAUDE, RUNNING));
    // Only the version the launch started is witnessed.
    assert!(!witnesses.holds(AgentId::CLAUDE, (2, 0, 1)));

    // A record that appears after the first look was written by this launch.
    std::fs::remove_file(&record).unwrap();
    let witnesses = Witnesses::default();
    let mut watch = Watch::default();
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    touch(&record);
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    assert!(witnesses.holds(AgentId::CLAUDE, RUNNING));

    // A new launch takes its own first look, so an earlier launch's write does not count for it.
    let witnesses = Witnesses::default();
    let mut watch = Watch::default();
    watch.observe(&witnesses, &locator, &own, "launch-1", RUNNING);
    written_later(&record, 10);
    watch.observe(&witnesses, &locator, &own, "launch-2", RUNNING);
    watch.observe(&witnesses, &locator, &own, "launch-2", RUNNING);
    assert!(!witnesses.holds(AgentId::CLAUDE, RUNNING));
}

/// Looks are spaced: a write is not seen before the next look is due, however soon it comes.
#[test]
fn a_write_is_not_seen_before_the_next_look_is_due() {
    let dir = crate::test_support::ScratchDir::new("agent-records-witness-spaced");
    let account = dir.join("account");
    let own = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
    let locator = Locator::with_shell(HashMap::new());
    let record = claude_record(&account);
    touch(&record);
    let witnesses = Witnesses::default();
    let mut watch = Watch::default();
    {
        let _hourly = looking_every(Duration::from_secs(3600));
        watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
        written_later(&record, 5);
        watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
        assert!(!witnesses.holds(AgentId::CLAUDE, RUNNING));
    }
    // Once a look is due, the write is seen.
    let _every_call = looking_every(Duration::ZERO);
    watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
    assert!(witnesses.holds(AgentId::CLAUDE, RUNNING));
}

/// A record the search does not recognise, as a release that renamed its records writes, never
/// witnesses its version, however often it is written.
#[test]
fn a_record_the_search_does_not_recognise_witnesses_nothing() {
    let dir = crate::test_support::ScratchDir::new("agent-records-witness-renamed");
    let account = dir.join("account");
    let own = session(AgentId::CLAUDE, CLAUDE_ID, Some(&account));
    let locator = Locator::with_shell(HashMap::new());
    let renamed = account
        .join("conversations")
        .join(format!("{CLAUDE_ID}.json"));
    let _every_call = looking_every(Duration::ZERO);
    let witnesses = Witnesses::default();
    let mut watch = Watch::default();
    watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
    touch(&renamed);
    watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
    written_later(&renamed, 5);
    watch.observe(&witnesses, &locator, &own, "launch", RUNNING);
    assert!(!witnesses.holds(AgentId::CLAUDE, RUNNING));
}
