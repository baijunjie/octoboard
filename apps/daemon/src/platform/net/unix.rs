//! The Unix backend: `getifaddrs` for the addresses, and the host name through `gethostname`,
//! which on macOS is replaced by the Bonjour local host name.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The name this computer announces: on macOS `<LocalHostName>.local`, as `scutil --get
/// LocalHostName` prints it (the name `gethostname` gives there follows the network's DHCP and DNS
/// and may be anything, a `.lan` or `.home` name included), falling back to `gethostname`'s;
/// elsewhere `gethostname`'s name as it is.
pub(crate) fn host_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Some(local) = bonjour_name() {
        return Some(local);
    }
    crate::platform::fs::host_name()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.is_empty())
}

#[cfg(target_os = "macos")]
fn bonjour_name() -> Option<String> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "LocalHostName"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8(output.stdout).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| format!("{name}.local"))
}

/// The addresses of the interfaces that are up and running, loopback ones included (the caller
/// filters them), in the order `getifaddrs` lists them.
pub(crate) fn interface_addresses() -> Vec<IpAddr> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `list` is a valid place for the head of the list the call allocates; it is freed
    // below once, and nothing borrowed from it outlives that.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut entry = list;
    while !entry.is_null() {
        // SAFETY: `entry` is a node of the list `getifaddrs` returned, valid until `freeifaddrs`.
        let node = unsafe { &*entry };
        entry = node.ifa_next;
        let flags = node.ifa_flags as libc::c_int;
        let wanted = (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_int;
        if flags & wanted != wanted || node.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: `ifa_addr` is non-null and points at a socket address of the family it names,
        // which is read as that family's structure only after the family is checked.
        let address = unsafe {
            match libc::c_int::from((*node.ifa_addr).sa_family) {
                libc::AF_INET => {
                    let v4 = &*(node.ifa_addr as *const libc::sockaddr_in);
                    Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr))))
                }
                libc::AF_INET6 => {
                    let v6 = &*(node.ifa_addr as *const libc::sockaddr_in6);
                    Some(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr)))
                }
                _ => None,
            }
        };
        found.extend(address);
    }
    // SAFETY: `list` came from `getifaddrs` and is freed once; `found` holds copies only.
    unsafe { libc::freeifaddrs(list) };
    // `getifaddrs` carries no IPv6 address flags, so those that call an address deprecated or temporary are read apart,
    // where the system lists them (Linux). macOS keeps them in an ioctl of its own, which is not read: its temporary
    // addresses stay among the candidates, after the stable ones that come first in the list.
    #[cfg(target_os = "linux")]
    if let Ok(table) = std::fs::read_to_string("/proc/net/if_inet6") {
        let unwanted = unwanted_ipv6(&table);
        found.retain(|address| !matches!(address, IpAddr::V6(v6) if unwanted.contains(v6)));
    }
    found
}

/// The IPv6 addresses `table`, the text of `/proc/net/if_inet6`, marks as temporary (privacy extensions, which change),
/// deprecated, tentative or failed duplicate address detection: a line is the address as 32 hex digits, the interface
/// index, prefix length and scope in hex, the flags in hex, and the interface name.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unwanted_ipv6(table: &str) -> HashSet<Ipv6Addr> {
    // IFA_F_TEMPORARY, IFA_F_DADFAILED, IFA_F_DEPRECATED and IFA_F_TENTATIVE.
    const UNWANTED: u32 = 0x01 | 0x08 | 0x20 | 0x40;
    table
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [address, _, _, _, flags, ..] = fields[..] else {
                return None;
            };
            let flags = u32::from_str_radix(flags, 16).ok()?;
            if flags & UNWANTED == 0 || address.len() != 32 {
                return None;
            }
            let bytes: Vec<u8> = (0..16)
                .map(|i| u8::from_str_radix(&address[2 * i..2 * i + 2], 16))
                .collect::<Result<_, _>>()
                .ok()?;
            Some(Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_deprecated_and_tentative_ipv6_addresses_are_named_unwanted() {
        let table = "\
20010db8000000000000000000000001 02 40 00 80 eth0
20010db8000000000000000000000002 02 40 00 01 eth0
20010db8000000000000000000000003 02 40 00 21 eth0
20010db8000000000000000000000004 02 40 00 40 eth0
fe800000000000000000000000000001 02 40 20 80 eth0
";
        let unwanted = unwanted_ipv6(table);
        let v6 = |text: &str| text.parse::<Ipv6Addr>().unwrap();
        assert!(!unwanted.contains(&v6("2001:db8::1")));
        assert!(unwanted.contains(&v6("2001:db8::2")));
        assert!(unwanted.contains(&v6("2001:db8::3")));
        assert!(unwanted.contains(&v6("2001:db8::4")));
        assert_eq!(unwanted.len(), 3);
    }
}
