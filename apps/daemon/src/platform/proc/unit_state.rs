//! What systemd says of a user unit (`systemctl --user show`), read from its output: the question
//! itself is asked in `systemd.rs`, since only Linux has a systemd to ask. A command following a
//! daemon through a restart by its service manager uses the answer to know whether one is coming
//! instead of waiting a fixed time.

/// What systemd says of a unit, as far as whether it may yet bring up a daemon goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitState {
    /// Activating or deactivating: starting, or stopping on the way to a restart (systemd passes
    /// through deactivating, then waits `RestartSec=` as activating) or to inactive.
    Changing,
    /// Active, with this main process (0 for none).
    Active { main_pid: u32 },
    /// Inactive or failed: it brings up nothing unless started again.
    Down,
}

/// What `systemctl show` wrote, read as a [`UnitState`]: `LoadState=loaded`, and an `ActiveState`
/// it knows.
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
