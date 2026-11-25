//! Whether the host's credential store holds an item, without inspecting its secret or asking
//! the user to unlock it. Providers name the item and the host chooses how to look it up.
//!
//! macOS uses the exit status of `security find-generic-password`: 0 for present, 44 for absent.
//! Windows uses the result of an exact generic credential lookup. Linux uses Secret Service's
//! `SearchItems` reply, which lists matches in both unlocked and locked collections.

use std::ffi::OsString;
use std::process::Command;
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

use super::HostOs;

/// Whether the credential store holds an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Absent,
    /// The host has no query, or the query did not answer either way.
    CannotTell,
}

/// A credential lookup selected for the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceQuery {
    Command(CommandQuery),
    Target(String),
    Attributes { service: String, account: String },
}

/// The command that asks the host's credential store whether it holds one item, and the exit
/// status that means it does not. Plain data: it says what to run, and its caller runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandQuery {
    program: OsString,
    args: Vec<OsString>,
    /// The exit status that says the store has no such item; 0 says it has.
    absent_status: i32,
}

impl CommandQuery {
    /// The command to run. What it prints is the item's attributes at most, which its caller has
    /// no use for.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }

    /// What the command's exit `status` says; `None` is a command ended by a signal.
    pub fn read(&self, status: Option<i32>) -> Presence {
        match status {
            Some(0) => Presence::Present,
            Some(status) if status == self.absent_status => Presence::Absent,
            _ => Presence::CannotTell,
        }
    }
}

impl PresenceQuery {
    /// A command query backed by a stand-in store.
    #[cfg(test)]
    pub(crate) fn stand_in(
        program: impl Into<OsString>,
        args: &[&str],
        absent_status: i32,
    ) -> Self {
        Self::Command(CommandQuery {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            absent_status,
        })
    }

    /// Run a native lookup within the caller's deadline. Command queries use the caller's
    /// subprocess runner.
    pub fn native(self, timeout: Duration) -> Presence {
        match self {
            #[cfg(windows)]
            Self::Target(target) => bounded(timeout, move || windows::lookup(&target)),
            #[cfg(target_os = "linux")]
            Self::Attributes { service, account } => {
                bounded(timeout, move || linux::lookup(&service, &account, timeout))
            }
            _ => {
                let _ = timeout;
                Presence::CannotTell
            }
        }
    }
}

/// The caller's wait covers connection setup as well as the lookup. A native call can outlive
/// that wait, so it keeps its slot until it returns and repeated polls cannot strand more threads.
#[cfg(any(windows, target_os = "linux"))]
fn bounded(timeout: Duration, lookup: impl FnOnce() -> Presence + Send + 'static) -> Presence {
    use std::sync::mpsc;
    use std::thread;
    use tokio::sync::Semaphore;

    static SLOTS: Semaphore = Semaphore::const_new(8);
    let Ok(slot) = SLOTS.try_acquire() else {
        return Presence::CannotTell;
    };
    let (sender, receiver) = mpsc::channel();
    if thread::Builder::new()
        .spawn(move || {
            let answer = lookup();
            drop(slot);
            let _ = sender.send(answer);
        })
        .is_err()
    {
        return Presence::CannotTell;
    }
    receiver
        .recv_timeout(timeout)
        .unwrap_or(Presence::CannotTell)
}

/// The query for the provider's service and account, or its explicit Credential Manager target.
pub fn presence_query(service: &str, account: &str, target: &str) -> Option<PresenceQuery> {
    presence_query_on(HostOs::current(), service, account, target)
}

/// [`presence_query`] for the host `os`.
pub(crate) fn presence_query_on(
    os: HostOs,
    service: &str,
    account: &str,
    target: &str,
) -> Option<PresenceQuery> {
    match os {
        HostOs::Macos if names_an_item(service) && names_an_item(account) => {
            Some(PresenceQuery::Command(CommandQuery {
                program: "/usr/bin/security".into(),
                args: ["find-generic-password", "-s", service, "-a", account]
                    .map(OsString::from)
                    .to_vec(),
                absent_status: 44,
            }))
        }
        HostOs::Windows if names_an_item(target) => Some(PresenceQuery::Target(target.into())),
        HostOs::Linux if names_an_item(service) && names_an_item(account) => {
            Some(PresenceQuery::Attributes {
                service: service.into(),
                account: account.into(),
            })
        }
        _ => None,
    }
}

/// Whether `name` can identify an item: not empty, which a store may
/// read as "any", and with no control character.
fn names_an_item(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn macos_asks_security_for_the_item_and_never_for_its_content() {
        let Some(PresenceQuery::Command(query)) =
            presence_query_on(HostOs::Macos, "some-service", "me@example.com", "target")
        else {
            panic!("macOS has a command query");
        };
        let command = query.command();
        assert_eq!(command.get_program(), "/usr/bin/security");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "find-generic-password",
                "-s",
                "some-service",
                "-a",
                "me@example.com"
            ]
            .map(OsStr::new)
        );
        // The flags that print an item's content.
        for reveals in ["-w", "-g"] {
            assert!(!command.get_args().any(|arg| arg == reveals), "{reveals}");
        }
        assert_eq!(query.read(Some(0)), Presence::Present);
        assert_eq!(query.read(Some(44)), Presence::Absent);
    }

    #[test]
    fn any_other_end_of_the_query_cannot_tell() {
        let Some(PresenceQuery::Command(query)) =
            presence_query_on(HostOs::Macos, "s", "a", "target")
        else {
            panic!("macOS has a command query");
        };
        // `None` is a query ended by a signal, as one killed at its deadline is.
        for status in [Some(1), Some(43), Some(45), Some(-1), None] {
            assert_eq!(query.read(status), Presence::CannotTell, "{status:?}");
        }
    }

    #[test]
    fn windows_uses_the_declared_target_and_linux_uses_the_attributes() {
        assert_eq!(
            presence_query_on(HostOs::Windows, "some-service", "me", "exact-target"),
            Some(PresenceQuery::Target("exact-target".into()))
        );
        assert_eq!(
            presence_query_on(HostOs::Linux, "some-service", "me", "exact-target"),
            Some(PresenceQuery::Attributes {
                service: "some-service".into(),
                account: "me".into(),
            })
        );
    }

    #[test]
    fn a_name_no_item_can_have_is_not_asked_about() {
        for (service, account) in [
            ("", "me"),
            ("service", ""),
            ("ser\nvice", "me"),
            ("s", "\0"),
        ] {
            assert_eq!(
                presence_query_on(HostOs::Macos, service, account, "target"),
                None,
                "{service:?} {account:?}"
            );
        }
    }

    #[test]
    fn this_hosts_query_is_the_one_of_its_name() {
        assert_eq!(
            presence_query("some-service", "me", "target"),
            presence_query_on(HostOs::current(), "some-service", "me", "target")
        );
    }
}
