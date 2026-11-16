//! The names and addresses another computer can reach this one by: what Settings offers for the
//! `<server>` of the SSH forward. A backend provides `host_name() -> Option<String>`, the name this
//! computer announces on its network, and `interface_addresses() -> Vec<IpAddr>`, the addresses
//! of its active network interfaces as the system lists them, less those the system itself marks as not to be used:
//! deprecated, tentative or failed ones where it says so (Linux, Windows), and, on Linux only, temporary IPv6 ones (on
//! macOS and Windows temporary addresses are kept, as Windows gives stable ones the same mark). What every system
//! lists is then filtered here: loopback, unspecified, multicast and link-local addresses and `localhost` names are
//! left out.
//!
//! The name is each system's own: on macOS the Bonjour local host name with `.local` (what
//! System Settings > Sharing shows, answered by mDNS on every Mac); on Linux the host name as
//! `gethostname` reports it and on Windows the computer name, both as they are, since `.local` is
//! an mDNS name that neither system is sure to answer to (a Linux computer needs avahi, a Windows
//! one answers it only for its own mDNS name), while the plain name is what the network's DHCP
//! and DNS, NetBIOS or LLMNR resolve on a local network.

use std::net::{IpAddr, Ipv6Addr};

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub(crate) use unix::host_name;
#[cfg(unix)]
use unix::interface_addresses;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::host_name;
#[cfg(windows)]
use windows::interface_addresses;

/// What another computer may reach this one by, as text for `user@<host>` of `ssh` (which takes an
/// IPv6 address bare there, without brackets): the computer's name first, then its IPv4 addresses
/// and then its IPv6 ones.
pub(crate) fn server_addresses() -> Vec<String> {
    candidates(host_name(), interface_addresses())
}

/// The candidates for `name` and `addresses`: the name where there is one and it is not `localhost`, which names
/// every computer to itself and no other computer to this one, then the addresses that [`reachable`] keeps, IPv4
/// before IPv6, each once, in the order the system listed them.
fn candidates(name: Option<String>, addresses: Vec<IpAddr>) -> Vec<String> {
    let mut kept: Vec<IpAddr> = Vec::new();
    for address in addresses {
        if reachable(address) && !kept.contains(&address) {
            kept.push(address);
        }
    }
    kept.sort_by_key(IpAddr::is_ipv6);
    name.into_iter()
        .filter(|name| !name.is_empty() && !is_localhost(name))
        .chain(kept.iter().map(IpAddr::to_string))
        .collect()
}

/// Whether another computer could use `address` to reach this one: not loopback, unspecified,
/// multicast or link-local (`169.254.0.0/16`, `fe80::/10`, which are good on one link only and
/// need a scope in ssh).
fn reachable(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            !(v4.is_loopback() || v4.is_unspecified() || v4.is_multicast() || v4.is_link_local())
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || is_link_local_v6(&v6))
        }
    }
}

fn is_localhost(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost" || name.ends_with(".localhost")
}

fn is_link_local_v6(address: &Ipv6Addr) -> bool {
    address.segments()[0] & 0xffc0 == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn loopback_unspecified_multicast_and_link_local_addresses_are_left_out() {
        let listed = [
            "127.0.0.1",
            "0.0.0.0",
            "224.0.0.251",
            "169.254.10.20",
            "::1",
            "::",
            "ff02::fb",
            "fe80::1",
            "febf::1",
        ]
        .map(ip);
        assert_eq!(candidates(None, listed.to_vec()), Vec::<String>::new());
    }

    #[test]
    fn the_name_comes_first_then_ipv4_then_ipv6_each_once_in_listed_order() {
        let listed = [
            "fd00::20",
            "192.168.1.20",
            "127.0.0.1",
            "100.101.102.103",
            "192.168.1.20",
            "2001:db8::20",
            "fe80::1",
        ]
        .map(ip);
        assert_eq!(
            candidates(Some("Devs-MacBook.local".to_owned()), listed.to_vec()),
            [
                "Devs-MacBook.local",
                "192.168.1.20",
                "100.101.102.103",
                "fd00::20",
                "2001:db8::20",
            ]
        );
    }

    #[test]
    fn localhost_is_no_name_to_offer() {
        for name in ["localhost", "LocalHost.", "a.localhost"] {
            assert_eq!(
                candidates(Some(name.to_owned()), vec![ip("10.0.0.5")]),
                ["10.0.0.5"],
                "{name}"
            );
        }
        assert_eq!(
            candidates(Some("localhost-dev".to_owned()), vec![]),
            ["localhost-dev"]
        );
    }

    #[test]
    fn a_computer_with_no_name_is_offered_by_its_addresses_alone() {
        assert_eq!(candidates(None, vec![ip("10.0.0.5")]), ["10.0.0.5"]);
        assert_eq!(
            candidates(Some(String::new()), vec![ip("10.0.0.5")]),
            ["10.0.0.5"]
        );
    }

    /// The Mac announces the Bonjour local host name with `.local`, the one `scutil` prints.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_mac_is_named_by_its_bonjour_local_host_name() {
        let printed = std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "LocalHostName"])
            .output()
            .unwrap();
        let name = String::from_utf8(printed.stdout).unwrap();
        let name = name.trim();
        if !printed.status.success() || name.is_empty() {
            return;
        }
        assert_eq!(host_name(), Some(format!("{name}.local")));
    }

    /// Linux names itself as the kernel reports its host name, with nothing added.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_computer_is_named_as_the_kernel_reports_it() {
        let reported = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
        let reported = reported.trim();
        if reported.is_empty() {
            return;
        }
        assert_eq!(host_name().as_deref(), Some(reported));
    }

    /// Windows names itself by the computer name in the environment, with nothing added.
    #[cfg(windows)]
    #[test]
    fn a_windows_computer_is_named_by_its_computer_name() {
        let Some(name) = std::env::var("COMPUTERNAME").ok().filter(|n| !n.is_empty()) else {
            return;
        };
        assert_eq!(host_name(), Some(name));
    }

    /// The Unix backend lists loopback addresses too (the filter is the caller's), where the
    /// loopback interface is up.
    #[cfg(unix)]
    #[test]
    fn the_unix_listing_includes_the_loopback_interface_for_the_caller_to_filter() {
        let listed = interface_addresses();
        assert!(listed.contains(&ip("127.0.0.1")), "{listed:?}");
        assert!(!server_addresses().contains(&"127.0.0.1".to_owned()));
    }

    /// The Windows backend leaves the loopback adapter out itself.
    #[cfg(windows)]
    #[test]
    fn the_windows_listing_leaves_the_loopback_adapter_out() {
        let listed = interface_addresses();
        assert!(!listed.contains(&ip("127.0.0.1")), "{listed:?}");
        assert!(!listed.contains(&ip("::1")), "{listed:?}");
    }

    /// As `server_addresses` hands them over: the name first where there is one, and no IPv6
    /// address before an IPv4 one.
    #[test]
    fn what_the_host_lists_is_the_name_then_ipv4_then_ipv6() {
        let listed = server_addresses();
        let addresses: Vec<IpAddr> = listed
            .iter()
            .filter_map(|text| text.parse::<IpAddr>().ok())
            .collect();
        if host_name().is_some_and(|name| !name.is_empty()) {
            assert!(
                listed
                    .first()
                    .is_some_and(|first| first.parse::<IpAddr>().is_err()),
                "{listed:?}"
            );
        }
        let first_v6 = addresses.iter().position(IpAddr::is_ipv6);
        if let Some(at) = first_v6 {
            assert!(addresses[at..].iter().all(IpAddr::is_ipv6), "{listed:?}");
        }
    }

    /// The host's own reading finds nothing unreachable, whatever the machine has.
    #[test]
    fn what_the_host_lists_holds_only_reachable_addresses() {
        for text in server_addresses() {
            if let Ok(address) = text.parse::<IpAddr>() {
                assert!(reachable(address), "{text}");
            }
        }
    }
}
