//! Reading the table of file locks Linux lists in `/proc/locks`, which names the process that took
//! each lock and the device and inode of the locked file, for a command that finds the instance
//! lock held by a daemon that has not recorded itself, so that it can name that process. The
//! lookup that reads the file is `lock_holder`, on the host that has one.

/// The pid of the one process holding a lock on a file, over the text of `/proc/locks` and the
/// locked file's device (`major`, `minor`) and inode. Each line is `<n>: <class> <mode> <access> <pid> <major>:<minor>:<inode> <start>
/// <end>`, the device numbers in hexadecimal; a line whose class is preceded by `->` is a process
/// waiting for the lock, not holding it, and an open file description lock has no pid (`-1`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn holder_in(locks: &str, major: u32, minor: u32, inode: u64) -> Option<u32> {
    let mut holders = locks.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        fields.next()?; // "<n>:"
        let class = fields.next()?;
        if class == "->" {
            return None;
        }
        let pid = fields.nth(2)?;
        let mut file = fields.next()?.split(':');
        let file_major = u32::from_str_radix(file.next()?, 16).ok()?;
        let file_minor = u32::from_str_radix(file.next()?, 16).ok()?;
        let file_inode: u64 = file.next()?.parse().ok()?;
        ((file_major, file_minor, file_inode) == (major, minor, inode))
            .then(|| pid.parse::<u32>().ok())
            .flatten()
            .filter(|pid| *pid != 0)
    });
    let holder = holders.next()?;
    // Several holders (shared locks of commands looking at once, say) name no one process.
    holders.all(|other| other == holder).then_some(holder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_process_holding_the_lock_is_the_holder() {
        let locks = "1: FLOCK  ADVISORY  WRITE 12345 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(locks, 0x08, 0x01, 131_074), Some(12345));
    }

    /// The device numbers are hexadecimal and the inode decimal.
    #[test]
    fn the_device_is_read_in_hexadecimal_and_the_inode_in_decimal() {
        let locks = "1: FLOCK  ADVISORY  WRITE 777 fd:00:2048 0 EOF\n\
                     2: FLOCK  ADVISORY  WRITE 888 08:11:2048 0 EOF\n";
        assert_eq!(holder_in(locks, 0xfd, 0x00, 2048), Some(777));
        assert_eq!(holder_in(locks, 0x08, 0x11, 2048), Some(888));
        // The same digits read as decimal are another device.
        assert_eq!(holder_in(locks, 8, 11, 2048), None);
        assert_eq!(holder_in(locks, 0xfd, 0x00, 0x2048), None);
    }

    #[test]
    fn a_lock_on_another_file_is_not_the_holders() {
        let locks = "1: FLOCK  ADVISORY  WRITE 100 08:01:131074 0 EOF\n\
                     2: POSIX  ADVISORY  WRITE 200 08:01:999 0 EOF\n\
                     3: FLOCK  ADVISORY  WRITE 300 08:02:131074 0 EOF\n\
                     4: FLOCK  ADVISORY  WRITE 400 09:01:131074 0 EOF\n";
        assert_eq!(holder_in(locks, 0x08, 0x01, 131_074), Some(100));
        assert_eq!(holder_in(locks, 0x08, 0x01, 1), None);
        assert_eq!(holder_in(locks, 0x0a, 0x01, 131_074), None);
        assert_eq!(holder_in("", 0x08, 0x01, 131_074), None);
    }

    /// A process waiting for the lock (`->`) does not hold it.
    #[test]
    fn a_process_waiting_for_the_lock_is_not_a_holder() {
        let locks = "1: FLOCK  ADVISORY  WRITE 12345 08:01:131074 0 EOF\n\
                     1: -> FLOCK  ADVISORY  WRITE 999 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(locks, 0x08, 0x01, 131_074), Some(12345));
        let only_waiting = "1: -> FLOCK  ADVISORY  WRITE 999 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(only_waiting, 0x08, 0x01, 131_074), None);
    }

    /// An open file description lock has no pid, and 0 is none.
    #[test]
    fn a_lock_without_a_pid_names_no_holder() {
        for pid in ["-1", "0"] {
            let locks = format!("1: OFDLCK ADVISORY  WRITE {pid} 08:01:131074 0 EOF\n");
            assert_eq!(holder_in(&locks, 0x08, 0x01, 131_074), None, "{pid}");
        }
        let locks = "1: OFDLCK ADVISORY  WRITE -1 08:01:131074 0 EOF\n\
                     2: FLOCK  ADVISORY  WRITE 12345 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(locks, 0x08, 0x01, 131_074), Some(12345));
    }

    /// Several processes holding it (shared locks, say) name no one process, but one process
    /// holding it more than once does.
    #[test]
    fn several_holders_name_no_one_process() {
        let shared = "1: FLOCK  ADVISORY  READ  100 08:01:131074 0 EOF\n\
                      2: FLOCK  ADVISORY  READ  200 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(shared, 0x08, 0x01, 131_074), None);
        let twice = "1: FLOCK  ADVISORY  READ  100 08:01:131074 0 EOF\n\
                     2: POSIX  ADVISORY  READ  100 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(twice, 0x08, 0x01, 131_074), Some(100));
        let three = "1: FLOCK  ADVISORY  READ  100 08:01:131074 0 EOF\n\
                     2: FLOCK  ADVISORY  READ  100 08:01:131074 0 EOF\n\
                     3: FLOCK  ADVISORY  READ  200 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(three, 0x08, 0x01, 131_074), None);
    }

    #[test]
    fn lines_that_are_not_lock_lines_are_skipped() {
        let locks = "garbage\n\
                     \n\
                     1: FLOCK\n\
                     1: FLOCK  ADVISORY  WRITE abc 08:01:131074 0 EOF\n\
                     1: FLOCK  ADVISORY  WRITE 5 nodevice 0 EOF\n\
                     1: FLOCK  ADVISORY  WRITE 5 zz:01:131074 0 EOF\n\
                     1: FLOCK  ADVISORY  WRITE 5 08:01:xyz 0 EOF\n\
                     2: FLOCK  ADVISORY  WRITE 12345 08:01:131074 0 EOF\n";
        assert_eq!(holder_in(locks, 0x08, 0x01, 131_074), Some(12345));
    }
}
