//! The file a holder leaves for the daemon to find it by: `<holder id>.json` in the registry
//! directory, written whole and for its owner alone (it holds the secret the daemon link is
//! opened with), and removed by the holder when it exits. The directory is passed in, so that a
//! test never touches the user's own.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hostfs::replace::{replace_beside, Permissions};
use crate::platform::fs as host_fs;

/// Where to find a holder and how to open its daemon link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub holder_id: String,
    pub pid: u32,
    /// The holder process's `<pid>:<start>`, which tells it from another process with the pid.
    pub started: String,
    /// The loopback port its listener is bound to.
    pub port: u16,
    pub secret: String,
    pub version: String,
    pub build_id: String,
    /// The executable the holder runs from.
    pub exe: String,
}

/// The file of the holder `holder_id`.
pub fn path_of(dir: &Path, holder_id: &str) -> PathBuf {
    dir.join(format!("{holder_id}.json"))
}

/// Writes `entry` in place of any earlier file of its holder. **Blocks.**
pub fn write(dir: &Path, entry: &RegistryEntry) -> io::Result<()> {
    host_fs::create_private_dir(dir)?;
    let bytes = serde_json::to_vec(entry).map_err(io::Error::other)?;
    replace_beside(
        &path_of(dir, &entry.holder_id),
        &bytes,
        Permissions::Private,
    )
}

/// The file of the holder `holder_id`, or `None` when there is none or it cannot be read.
pub fn read(dir: &Path, holder_id: &str) -> Option<RegistryEntry> {
    let bytes = std::fs::read(path_of(dir, holder_id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Every holder's file in `dir`; one that cannot be read is left out.
pub fn list(dir: &Path) -> Vec<RegistryEntry> {
    let Ok(files) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    files
        .flatten()
        .filter_map(|file| {
            let path = file.path();
            if path.extension()? != "json" {
                return None;
            }
            read(dir, path.file_stem()?.to_str()?)
        })
        .collect()
}

/// Removes the file of the holder `holder_id`; one that is not there is as good as removed.
pub fn remove(dir: &Path, holder_id: &str) {
    if let Err(err) = std::fs::remove_file(path_of(dir, holder_id)) {
        if err.kind() != io::ErrorKind::NotFound {
            tracing::warn!(holder = holder_id, %err, "removing the holder's registry file failed");
        }
    }
}

/// How long [`nothing_listens`] waits for the connection to be refused: Windows reports a refused
/// loopback connection only after about two seconds.
const LISTEN_PROBE: Duration = Duration::from_secs(3);

/// Whether nothing listens on the loopback `port` any more: the connection is refused, so the
/// holder whose file names the port is gone, which a file whose process identity cannot be read
/// has no other way to tell. A port something answers on, or that does not answer in time, is not
/// told apart from a holder that is busy. **Blocks** for up to [`LISTEN_PROBE`].
pub fn nothing_listens(port: u16) -> bool {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    matches!(
        TcpStream::connect_timeout(&address, LISTEN_PROBE),
        Err(err) if err.kind() == io::ErrorKind::ConnectionRefused
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    fn entry(holder_id: &str) -> RegistryEntry {
        RegistryEntry {
            holder_id: holder_id.to_string(),
            pid: 4242,
            started: "4242:99".to_string(),
            port: 51000,
            secret: "s3cret".to_string(),
            version: "1.2.3".to_string(),
            build_id: "b".to_string(),
            exe: "/x/octoboardd".to_string(),
        }
    }

    #[test]
    fn an_entry_written_is_read_back_and_gone_once_removed() {
        let dir = ScratchDir::new("holder-registry");
        let registry = dir.join("holders");
        write(&registry, &entry("h1")).expect("written");
        assert_eq!(read(&registry, "h1"), Some(entry("h1")));
        assert_eq!(read(&registry, "h2"), None);
        remove(&registry, "h1");
        assert_eq!(read(&registry, "h1"), None);
        remove(&registry, "h1");
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;

        let dir = ScratchDir::new("holder-registry-private");
        let registry = dir.join("holders");
        write(&registry, &entry("h1")).expect("written");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path_of(&registry, "h1")), 0o600);
        assert_eq!(mode(&registry), 0o700);
    }

    #[test]
    fn the_listing_holds_every_readable_file() {
        let dir = ScratchDir::new("holder-registry-list");
        assert!(list(&dir.join("none")).is_empty());
        write(&dir, &entry("h1")).expect("written");
        write(&dir, &entry("h2")).expect("written");
        std::fs::write(dir.join("broken.json"), b"{").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let mut ids: Vec<String> = list(&dir).into_iter().map(|e| e.holder_id).collect();
        ids.sort();
        assert_eq!(ids, ["h1", "h2"]);
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let dir = ScratchDir::new("holder-registry-replace");
        write(&dir, &entry("h1")).expect("written");
        let mut later = entry("h1");
        later.port = 52000;
        write(&dir, &later).expect("replaced");
        assert_eq!(read(&dir, "h1"), Some(later));
    }

    #[test]
    fn a_port_nobody_listens_on_is_told_from_one_that_is_listened_on() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!nothing_listens(port));
        drop(listener);
        assert!(nothing_listens(port));
    }
}
