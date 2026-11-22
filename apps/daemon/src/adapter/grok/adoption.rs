//! What a Grok launch records for the daemon that adopts it after the one that started it is gone:
//! its trust carrier, its writeback of the settings a session changed and its correction of the
//! plugin paths a session recorded are objects of that process, so the data they are made of is
//! kept with the launch and they are made again from it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::plugin_paths::PluginPaths;
use super::trust::CarriedTrust;
use super::writeback::SharedConfigWriteback;
use crate::adapter::RebuiltLaunch;

/// What a Grok launch keeps in its terminal's meta to rebuild its trust carrier and write-back at
/// adoption, with the types it embeds ([`CarriedTrust`], [`SharedConfigWriteback`]).
/// It crosses builds: a daemon of an older build in the holder's compatible range writes it and a
/// newer one reads it, so a field added later needs `#[serde(default)]`, and it is kept readable
/// by every build in that range: `crate::holder::compat::FLOOR` only moves by the
/// previous-stable release rule.
#[derive(Serialize)]
struct Record<'a> {
    carried_trust: &'a CarriedTrust,
    /// Whether the launch carries its trust record, which a console session's never does
    /// ([`record_without_carrier`]).
    carries_trust: bool,
    writeback: Option<&'a SharedConfigWriteback>,
    plugin_paths: Option<&'a PluginPaths>,
}

#[derive(Deserialize)]
struct Recorded {
    carried_trust: CarriedTrust,
    /// Absent in the record of a build that recorded a carrier for every launch, a console
    /// session's included.
    #[serde(default)]
    carries_trust: Option<bool>,
    writeback: Option<SharedConfigWriteback>,
    /// Absent in the record of a build that did not correct plugin paths.
    #[serde(default)]
    plugin_paths: Option<PluginPaths>,
}

/// Both pieces of work a Grok launch may leave for after its exit, in the order they run.
struct AfterGrokExit {
    writeback: Option<SharedConfigWriteback>,
    plugin_paths: PluginPaths,
}

impl crate::session::term::AfterExit for AfterGrokExit {
    fn run(&self) {
        if let Some(writeback) = &self.writeback {
            writeback.publish();
        }
        self.plugin_paths.publish();
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The work a launch with `writeback` and `plugin_paths` leaves for after its exit; the
/// write-back alone where there are no plugin paths to correct.
pub(super) fn after_exit(
    writeback: Option<SharedConfigWriteback>,
    plugin_paths: Option<PluginPaths>,
) -> Option<Box<dyn crate::session::term::AfterExit>> {
    match (writeback, plugin_paths) {
        (writeback, Some(plugin_paths)) => Some(Box::new(AfterGrokExit {
            writeback,
            plugin_paths,
        })),
        (Some(writeback), None) => Some(Box::new(writeback)),
        (None, None) => None,
    }
}

/// The record of a launch with `carried_trust` and, where its account shares its settings,
/// `writeback`, and, where its home reaches the user's plugin directory, `plugin_paths`.
pub(super) fn record(
    carried_trust: &CarriedTrust,
    writeback: Option<&SharedConfigWriteback>,
    plugin_paths: Option<&PluginPaths>,
) -> Option<serde_json::Value> {
    recorded(carried_trust, true, writeback, plugin_paths)
}

fn recorded(
    carried_trust: &CarriedTrust,
    carries_trust: bool,
    writeback: Option<&SharedConfigWriteback>,
    plugin_paths: Option<&PluginPaths>,
) -> Option<serde_json::Value> {
    serde_json::to_value(Record {
        carried_trust,
        carries_trust,
        writeback,
        plugin_paths,
    })
    .map_err(|err| tracing::warn!(%err, "the Grok launch's adoption record could not be made"))
    .ok()
}

/// The record of a launch that carries no trust, a console session's, with its `writeback` and
/// `plugin_paths`; none where it has neither. The record says that nothing is carried. An earlier
/// build in the compatible range does not read that and reads the record only with a carrier in
/// it, so one is recorded that carries nowhere: its user's store is the session's own copy, which
/// a build that carries it rewrites with what that copy already holds.
pub(super) fn record_without_carrier(
    session_store: &std::path::Path,
    folder: &str,
    writeback: Option<&SharedConfigWriteback>,
    plugin_paths: Option<&PluginPaths>,
) -> Option<serde_json::Value> {
    if writeback.is_none() && plugin_paths.is_none() {
        return None;
    }
    let nowhere = CarriedTrust {
        session_store: session_store.to_path_buf(),
        user_store: session_store.to_path_buf(),
        folder: folder.to_string(),
        record_wait: std::time::Duration::ZERO,
    };
    recorded(&nowhere, false, writeback, plugin_paths)
}

/// What a launch of this record had: nothing, when the record does not read. Whether the launch
/// carries its trust record is what its record says. An earlier build's record does not say, and
/// that build recorded a carrier into the user's store for a console session too, which Grok Build
/// no longer records anything for: no carrier is made of such a record whose folder lies under
/// `consoles`, the directory the consoles' own are kept in.
pub(super) fn rebuild(record: &serde_json::Value, consoles: &std::path::Path) -> RebuiltLaunch {
    match serde_json::from_value::<Recorded>(record.clone()) {
        Ok(recorded) => {
            let carries = recorded
                .carries_trust
                .unwrap_or_else(|| !lies_under(&recorded.carried_trust.folder, consoles));
            RebuiltLaunch {
                carried_trust: carries.then(|| {
                    Arc::new(recorded.carried_trust) as Arc<dyn crate::session::trust::TrustCarrier>
                }),
                after_exit: after_exit(recorded.writeback, recorded.plugin_paths),
                transport: None,
            }
        }
        Err(err) => {
            tracing::warn!(%err, "the Grok launch's adoption record does not read");
            RebuiltLaunch::default()
        }
    }
}

/// Whether `folder`, a launch's working directory, lies in `consoles`, judged where the two
/// really are ([`Place`](crate::hostfs::place::Place)) and never by their text. Where either
/// cannot be resolved it counts as lying there: taking a project's folder for a console's costs a
/// question asked again, and the opposite a write into the user's trust store.
fn lies_under(folder: &str, consoles: &std::path::Path) -> bool {
    use crate::hostfs::place::Place;

    match (Place::of(std::path::Path::new(folder)), Place::of(consoles)) {
        (Some(folder), Some(consoles)) => folder.lies_in(&consoles),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::*;

    fn carried() -> CarriedTrust {
        CarriedTrust {
            session_store: "/session/trusted_folders.toml".into(),
            user_store: "/user/trusted_folders.toml".into(),
            folder: "/work".to_string(),
            record_wait: Duration::from_secs(5),
        }
    }

    #[test]
    fn the_trust_carrier_is_rebuilt_from_its_record() {
        let record = record(&carried(), None, None).expect("a record");
        let rebuilt = rebuild(&record, Path::new("/octoboard/consoles"));
        let carrier = rebuilt.carried_trust.expect("the trust is carried");
        assert_eq!(
            carrier.user_store(),
            Path::new("/user/trusted_folders.toml")
        );
        assert!(rebuilt.after_exit.is_none());
    }

    #[test]
    fn a_record_that_does_not_read_rebuilds_nothing() {
        let rebuilt = rebuild(
            &serde_json::json!({"carried_trust": 3}),
            Path::new("/octoboard/consoles"),
        );
        assert!(rebuilt.carried_trust.is_none() && rebuilt.after_exit.is_none());
    }

    /// A record as an earlier build wrote it, which does not say whether the launch carries.
    fn of_an_earlier_build(carried: &CarriedTrust) -> serde_json::Value {
        let mut record = record(carried, None, None).expect("a record");
        record
            .as_object_mut()
            .expect("an object")
            .remove("carries_trust");
        record
    }

    /// A console session an earlier build launched recorded a carrier into the user's store. This
    /// build makes none of it, so answering that session's confirmation writes nothing there. The
    /// folder is judged where it really is: one that only begins with the consoles directory's
    /// name is a project's and keeps its carrier, and where the consoles directory cannot be
    /// resolved no carrier is made.
    #[test]
    fn an_earlier_builds_console_record_rebuilds_no_carrier() {
        let scratch = crate::test_support::ScratchDir::new("grok-adoption-consoles");
        let consoles = scratch.join("consoles");
        let in_console = consoles.join("c1");
        let beside = scratch.join("consoles-of-mine/project");
        for dir in [&in_console, &beside] {
            std::fs::create_dir_all(dir).expect("the folder");
        }
        let at = |folder: &Path| CarriedTrust {
            folder: folder.to_string_lossy().into_owned(),
            ..carried()
        };
        let rebuilt = |folder: &Path, consoles: &Path| {
            rebuild(&of_an_earlier_build(&at(folder)), consoles).carried_trust
        };
        assert!(rebuilt(&in_console, &consoles).is_none());
        assert!(rebuilt(&beside, &consoles).is_some());
        assert!(rebuilt(&beside, &scratch.join("gone")).is_none());
    }

    /// This build's record says whether the launch carries, and that decides: a project whose
    /// folder lies under the consoles directory keeps its carrier.
    #[test]
    fn a_record_that_says_the_launch_carries_keeps_its_carrier_wherever_the_folder_is() {
        let scratch = crate::test_support::ScratchDir::new("grok-adoption-project");
        let consoles = scratch.join("consoles");
        let project = consoles.join("c1/project");
        std::fs::create_dir_all(&project).expect("the folder");
        let in_consoles = CarriedTrust {
            folder: project.to_string_lossy().into_owned(),
            ..carried()
        };
        let record = record(&in_consoles, None, None).expect("a record");
        assert!(rebuild(&record, &consoles).carried_trust.is_some());
    }

    /// A launch that carries nothing and leaves nothing for after its exit has no record.
    #[test]
    fn a_launch_without_a_carrier_or_work_after_its_exit_has_no_record() {
        let store = Path::new("/session/trusted_folders.toml");
        assert!(record_without_carrier(store, "/work", None, None).is_none());
    }
}
