//! The Windows backend: `GetAdaptersAddresses` for the addresses, and the computer name Windows
//! sets in the environment.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{
    IpDadStatePreferred, AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6,
};

/// The name of this computer, as Windows sets it in the environment.
pub(crate) fn host_name() -> Option<String> {
    crate::platform::fs::host_name().and_then(|name| name.into_string().ok())
}

/// The unicast addresses of the adapters that are up, other than the loopback one, as `GetAdaptersAddresses` lists
/// them, leaving out those not (or no longer) preferred.
pub(crate) fn interface_addresses() -> Vec<IpAddr> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    // The buffer is of `u64`s, so that the adapter structures in it are aligned.
    let mut buffer: Vec<u64> = vec![0; 4096];
    for _ in 0..4 {
        let mut length = (buffer.len() * 8) as u32;
        // SAFETY: the buffer holds `length` bytes and outlives the call.
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC),
                flags,
                std::ptr::null(),
                buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut length,
            )
        };
        if status == ERROR_BUFFER_OVERFLOW {
            buffer = vec![0; (length as usize).div_ceil(8)];
            continue;
        }
        if status != ERROR_SUCCESS {
            return Vec::new();
        }
        return read(buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>());
    }
    Vec::new()
}

/// The addresses of the adapters in the list `first` starts, which the call filled.
fn read(first: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<IpAddr> {
    let mut found = Vec::new();
    let mut adapter = first;
    // SAFETY: the list was written by `GetAdaptersAddresses` into a buffer that outlives this
    // read; every pointer in it is null or points into that buffer.
    unsafe {
        while !adapter.is_null() {
            let node = &*adapter;
            adapter = node.Next;
            if node.OperStatus != IfOperStatusUp || node.IfType == IF_TYPE_SOFTWARE_LOOPBACK {
                continue;
            }
            let mut unicast = node.FirstUnicastAddress;
            while !unicast.is_null() {
                let entry = &*unicast;
                unicast = entry.Next;
                // Only a preferred address is one to offer: not one still being checked for a duplicate (tentative), found
                // to be one, or deprecated. Temporary addresses are kept: Windows gives stable addresses random interface
                // IDs by default, so its `SuffixOrigin` cannot tell the two apart.
                if entry.DadState != IpDadStatePreferred {
                    continue;
                }
                let socket = entry.Address.lpSockaddr;
                if socket.is_null() {
                    continue;
                }
                match (*socket).sa_family {
                    AF_INET => {
                        let v4 = &*(socket as *const SOCKADDR_IN);
                        found.push(IpAddr::V4(Ipv4Addr::from(u32::from_be(
                            v4.sin_addr.S_un.S_addr,
                        ))));
                    }
                    AF_INET6 => {
                        let v6 = &*(socket as *const SOCKADDR_IN6);
                        found.push(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.u.Byte)));
                    }
                    _ => {}
                }
            }
        }
    }
    found
}
