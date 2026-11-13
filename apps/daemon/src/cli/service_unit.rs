//! Asking systemd about the user unit a service-managed daemon recorded it runs as
//! (`systemctl --user show`), so that a command following a daemon through a restart by its
//! service manager knows whether one is coming instead of waiting a fixed time.

use std::time::Duration;

/// What systemd says of a unit, as far as whether it may yet bring up a daemon goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnitState {
    /// Activating or deactivating: starting, or stopping on the way to a restart (systemd passes
    /// through deactivating, then waits `RestartSec=` as activating) or to inactive.
    Changing,
    /// Active, with this main process (0 for none).
    Active { main_pid: u32 },
    /// Inactive or failed: it brings up nothing unless started again.
    Down,
}

/// How long `systemctl` has to answer before systemd counts as not to be asked.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const ANSWER_WAIT: Duration = Duration::from_secs(2);

/// What systemd says of the user unit `unit`; `None` where it cannot be asked: `systemctl` is
/// missing, does not reach the user's manager (no `XDG_RUNTIME_DIR`, say), takes too long, or does
/// not know the unit, and on every host but Linux. A `systemctl` that could not be started or did
/// not answer in time is not asked again by this command, which polls. An inactive unit is asked
/// again after [`DOWN_RECHECK`] before it counts as down: systemd before 254 lets a unit restarting
/// pass through inactive while its restart job turns from a stop into a start.
pub(super) fn state(unit: &str) -> Option<UnitState> {
    #[cfg(target_os = "linux")]
    {
        match ask(unit)? {
            UnitState::Down => {
                std::thread::sleep(DOWN_RECHECK);
                ask(unit)
            }
            state => Some(state),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = unit;
        None
    }
}

/// How long [`state`] waits before it asks again about a unit found inactive or failed.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const DOWN_RECHECK: Duration = Duration::from_millis(100);

/// Set once `systemctl` could not be started or did not answer in time.
#[cfg(target_os = "linux")]
static UNANSWERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One `systemctl --user show` of `unit`.
#[cfg(target_os = "linux")]
fn ask(unit: &str) -> Option<UnitState> {
    use std::sync::atomic::Ordering;

    if UNANSWERED.load(Ordering::Relaxed) {
        return None;
    }
    let output = run_bounded(
        std::process::Command::new("systemctl").args([
            "--user",
            "show",
            "--property=LoadState,ActiveState,MainPID",
            "--",
            unit,
        ]),
        ANSWER_WAIT,
    );
    match output {
        Ran::Answered(output) => parse_show(&output),
        Ran::Failed => None,
        Ran::Unanswered => {
            UNANSWERED.store(true, Ordering::Relaxed);
            None
        }
    }
}

/// How [`run_bounded`] went.
#[cfg(target_os = "linux")]
enum Ran {
    /// It exited successfully, having written this.
    Answered(String),
    /// It exited unsuccessfully.
    Failed,
    /// It could not be started, or did not exit in time and was killed.
    Unanswered,
}

/// Runs `command` with its stdout piped and nothing else, and gives what it wrote there if it
/// exits successfully within `wait`; kills it if it does not exit in time.
#[cfg(target_os = "linux")]
fn run_bounded(command: &mut std::process::Command, wait: Duration) -> Ran {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;

    let Ok(mut child) = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Ran::Unanswered;
    };
    let deadline = Instant::now() + wait;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                child.kill().ok();
                child.wait().ok();
                return Ran::Unanswered;
            }
        }
    };
    // What `show` writes is a few lines, well within what the pipe holds before it exits.
    let mut output = String::new();
    let read = child
        .stdout
        .take()
        .is_some_and(|mut stdout| stdout.read_to_string(&mut output).is_ok());
    if read && status.success() {
        Ran::Answered(output)
    } else {
        Ran::Failed
    }
}

/// [`state`] over what `systemctl show` wrote: `LoadState=loaded`, and an `ActiveState` it knows.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn parse_show(output: &str) -> Option<UnitState> {
    let property = |name: &str| {
        output.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
                .map(str::trim)
        })
    };
    if property("LoadState")? != "loaded" {
        return None;
    }
    match property("ActiveState")? {
        "activating" | "deactivating" => Some(UnitState::Changing),
        "active" | "reloading" | "refreshing" => Some(UnitState::Active {
            main_pid: property("MainPID")?.parse().ok()?,
        }),
        "inactive" | "failed" => Some(UnitState::Down),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `systemctl --user show --property=LoadState,ActiveState,MainPID` writes, in the order
    /// systemd lists the properties.
    fn shown(load: &str, active: &str, main_pid: &str) -> String {
        format!("MainPID={main_pid}\nLoadState={load}\nActiveState={active}\n")
    }

    #[test]
    fn a_unit_that_is_starting_or_stopping_is_changing() {
        for active in ["activating", "deactivating"] {
            assert_eq!(
                parse_show(&shown("loaded", active, "0")),
                Some(UnitState::Changing),
                "{active}"
            );
        }
    }

    #[test]
    fn a_unit_that_is_active_carries_its_main_process() {
        for active in ["active", "reloading", "refreshing"] {
            assert_eq!(
                parse_show(&shown("loaded", active, "4242")),
                Some(UnitState::Active { main_pid: 4242 }),
                "{active}"
            );
        }
        assert_eq!(
            parse_show(&shown("loaded", "active", "0")),
            Some(UnitState::Active { main_pid: 0 })
        );
    }

    #[test]
    fn a_main_process_that_is_no_number_is_no_answer() {
        for main_pid in ["", "abc", "-1", "4242x"] {
            assert_eq!(
                parse_show(&shown("loaded", "active", main_pid)),
                None,
                "{main_pid:?}"
            );
        }
    }

    #[test]
    fn a_unit_that_is_inactive_or_failed_is_down() {
        for active in ["inactive", "failed"] {
            assert_eq!(
                parse_show(&shown("loaded", active, "0")),
                Some(UnitState::Down),
                "{active}"
            );
        }
    }

    #[test]
    fn a_state_systemd_does_not_have_is_no_answer() {
        for active in ["", "maintenance", "ACTIVE", "unknown"] {
            assert_eq!(
                parse_show(&shown("loaded", active, "1")),
                None,
                "{active:?}"
            );
        }
    }

    /// A unit systemd has not loaded (not found, say) answers `inactive`, which is no answer about a
    /// unit of ours.
    #[test]
    fn a_unit_that_is_not_loaded_is_no_answer() {
        for load in ["not-found", "masked", "error", "bad-setting", ""] {
            for active in ["active", "inactive", "failed", "activating"] {
                assert_eq!(
                    parse_show(&shown(load, active, "4242")),
                    None,
                    "{load} {active}"
                );
            }
        }
    }

    #[test]
    fn a_missing_property_is_no_answer() {
        for output in [
            "",
            "\n",
            "ActiveState=active\nMainPID=4242\n",
            "LoadState=loaded\nMainPID=4242\n",
            "LoadState=loaded\nActiveState=active\n",
        ] {
            assert_eq!(parse_show(output), None, "{output:?}");
        }
    }

    #[test]
    fn the_order_of_the_properties_does_not_matter() {
        let lines = ["LoadState=loaded", "ActiveState=active", "MainPID=4242"];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let output = order.map(|index| lines[index]).join("\n");
            assert_eq!(
                parse_show(&output),
                Some(UnitState::Active { main_pid: 4242 }),
                "{output:?}"
            );
        }
    }
}
