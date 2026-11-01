//! Every process id on the system, from the native process table: a superset of whichever
//! processes a caller is after, for it to filter. A process missing from the listing reads as gone
//! to every caller (a sweep leaves it running, the ledger takes it as ended), so a listing that may
//! have been cut short is never answered.

/// Every process id on the system but the kernel's, or `None` when the system cannot list them.
#[cfg(target_os = "linux")]
pub(crate) fn all() -> Option<Vec<libc::pid_t>> {
    let mut pids = Vec::new();
    // A listing cut short by a read error is not answered.
    for entry in std::fs::read_dir("/proc").ok()? {
        if let Some(pid) = entry
            .ok()?
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        {
            pids.push(pid);
        }
    }
    Some(pids)
}

/// As the Linux one, from `libproc`.
#[cfg(target_os = "macos")]
pub(crate) fn all() -> Option<Vec<libc::pid_t>> {
    // SAFETY: a null buffer of size 0 only asks how many processes there are.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    listed(count, |pids| {
        let size = std::mem::size_of_val(pids) as libc::c_int;
        // SAFETY: `pids` is writable for `size` bytes.
        unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) }
    })
}

/// What `list`, which fills the buffer it is given and answers how many pids it wrote, lists when
/// `count` processes were running a moment before. A listing that fills the buffer may have been
/// cut short, so it is taken again with more room, and given up on when it still fills it.
#[cfg(any(target_os = "macos", test))]
fn listed(
    count: libc::c_int,
    mut list: impl FnMut(&mut [libc::pid_t]) -> libc::c_int,
) -> Option<Vec<libc::pid_t>> {
    if count <= 0 {
        return None;
    }
    // Room beyond that count for processes started since.
    let mut room = count as usize * 2 + 256;
    for _ in 0..4 {
        let mut pids = vec![0 as libc::pid_t; room];
        let filled = list(&mut pids);
        if filled <= 0 {
            return None;
        }
        if (filled as usize) < pids.len() {
            pids.truncate(filled as usize);
            // Pid 0 is the kernel, which `getsid(0)` or `getpgid(0)` would answer for the caller
            // instead.
            pids.retain(|&pid| pid > 0);
            return Some(pids);
        }
        room *= 2;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the system's listing of `running` processes, writing as many as fit.
    fn listing(running: usize) -> impl FnMut(&mut [libc::pid_t]) -> libc::c_int {
        move |pids| {
            let filled = running.min(pids.len());
            for (slot, pid) in pids.iter_mut().zip(0..filled) {
                *slot = pid as libc::pid_t;
            }
            filled as libc::c_int
        }
    }

    #[test]
    fn a_listing_with_room_to_spare_is_every_pid_but_the_kernels() {
        assert_eq!(listed(3, listing(3)), Some(vec![1, 2]));
    }

    /// Processes started between the count and the listing, more than the room left for them, fill
    /// the buffer; the listing is taken again with more room rather than answered cut short.
    #[test]
    fn a_listing_that_fills_the_buffer_is_taken_again_with_more_room() {
        let running = 10 * 2 + 256 + 50;
        let pids = listed(10, listing(running)).expect("a listing");
        assert_eq!(pids.len(), running - 1);
    }

    #[test]
    fn a_listing_that_keeps_filling_the_buffer_is_given_up_on() {
        assert_eq!(listed(10, listing(usize::MAX)), None);
    }

    #[test]
    fn a_failed_listing_is_none() {
        assert_eq!(listed(0, listing(3)), None);
        assert_eq!(listed(3, |_| -1), None);
    }

    #[test]
    fn the_system_lists_this_process() {
        let me = std::process::id() as libc::pid_t;
        assert!(all().expect("a listing").contains(&me));
    }
}
