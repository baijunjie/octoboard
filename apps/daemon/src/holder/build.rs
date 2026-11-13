//! A build's identity, and the copy of it the holders run from.
//!
//! A holder, the helpers of its launches (hook, MCP bridge, status line) and the console host it
//! starts run from `~/.octoboard/builds/<build id>/`, not from the installation: on Windows a file
//! in use cannot be replaced, so an installer that finds the holder running from its own folder
//! stops on it, while one beside a copy replaces every file it ships. A project session's git hooks
//! are the exception: a job the session leaves running outlives the copy, which goes once no holder
//! runs from it, so they run the installation's own executable (`crate::session_git_hooks`).
//!
//! The copy is made at the first holder start, which can come after a self-update replaced the
//! installed file (`crate::self_update`), so the running executable's bytes are read from the open
//! image, not from its path (`platform::proc::image_source`): a copy of the new build under the old
//! build's id would run the wrong holder.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

use super::registry;
use crate::platform::build_files;
use crate::platform::proc::ProcessIdentity;

/// How much of the executable's hash the build id names.
const HASH_BYTES: usize = 8;

/// The name a directory that is being made is given, until it is renamed into place.
const MAKING_PREFIX: &str = ".making-";

/// The build the daemon or holder is running as the handshake names it: the version and a short
/// hash of the executable's bytes, so two builds of one version (a development build, a rebuilt
/// release) are told apart. Computed once; without a readable executable it is the version alone.
pub fn build_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let version = env!("CARGO_PKG_VERSION");
        match crate::platform::proc::running_image().and_then(|exe| hash_of(&exe)) {
            Ok(hash) => format!("{version}+{hash}"),
            Err(err) => {
                tracing::warn!(%err, "hashing the executable failed; the build is named by its version alone");
                version.to_string()
            }
        }
    })
}

/// The first bytes of the SHA-256 of the file `path`, in hex.
fn hash_of(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(hasher.finalize()[..HASH_BYTES]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The directory of the copies of builds. Making a copy, starting a holder from it and removing
/// the copies nothing runs from are done one at a time, so that a copy is never removed between
/// being made and being started from.
pub struct Builds {
    dir: PathBuf,
    serial: Mutex<()>,
}

impl Builds {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            serial: Mutex::new(()),
        }
    }

    /// Starts something from the copy of the build `build_id`, made from `source` (the running
    /// executable and what stands beside it) if there is none, and gives back what `start` does
    /// with the copy's executable. The copies are not removed meanwhile. If the copy cannot be
    /// made, `start` is given `source` itself, with a warning: the holder then runs from the
    /// installation, which on Windows keeps the installer from replacing the files in use.
    pub fn run_from<T>(&self, build_id: &str, source: &Path, start: impl FnOnce(&Path) -> T) -> T {
        let _serial = self.serial.lock().unwrap_or_else(|err| err.into_inner());
        let exe_from = crate::platform::proc::image_source(source);
        let exe = match copy_build(
            &self.dir,
            build_id,
            source,
            &exe_from,
            build_files::companions(),
        ) {
            Ok(exe) => exe,
            Err(err) => {
                tracing::warn!(%err, build = build_id, "copying the build failed; the holder runs from the installation");
                source.to_path_buf()
            }
        };
        start(&exe)
    }

    /// Removes every copy no live holder of the registry in `registry_dir` runs from, and any copy
    /// that was left half made. A holder whose process cannot be told from another by its identity
    /// counts as live while something listens on its port. A copy that cannot be removed (a file of
    /// it is in use, on Windows) is left for the next time. Returns whether nothing that should go
    /// is left.
    pub fn sweep(&self, registry_dir: &Path) -> bool {
        let _serial = self.serial.lock().unwrap_or_else(|err| err.into_inner());
        let in_use = builds_in_use(&self.dir, registry_dir);
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return true;
        };
        let mut clean = true;
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = entry.file_name();
            let making = name.to_string_lossy().starts_with(MAKING_PREFIX);
            if !making && in_use.contains(&name) {
                continue;
            }
            match std::fs::remove_dir_all(entry.path()) {
                Ok(()) => {
                    tracing::info!(build = %name.to_string_lossy(), "removed a copy of a build nothing runs from")
                }
                Err(err) => {
                    tracing::debug!(build = %name.to_string_lossy(), %err, "a copy of a build could not be removed; it is left for the next time");
                    clean = false;
                }
            }
        }
        clean
    }
}

/// The names of the build directories under `dir` that a live holder of the registry runs from.
fn builds_in_use(dir: &Path, registry_dir: &Path) -> Vec<OsString> {
    let canonical = crate::hostfs::canonical(dir).ok();
    registry::list(registry_dir)
        .into_iter()
        .filter(|entry| {
            // A file whose process cannot be verified counts as live, unless nothing listens on
            // its port any more.
            match ProcessIdentity::parse(&entry.started) {
                Some(identity) => identity.is_running(),
                None => !registry::nothing_listens(entry.port),
            }
        })
        .filter_map(|entry| {
            let exe = PathBuf::from(&entry.exe);
            let resolved = crate::hostfs::canonical(&exe).ok();
            let bases = [Some(dir), canonical.as_deref()];
            let first = [Some(&exe), resolved.as_ref()]
                .into_iter()
                .flatten()
                .find_map(|exe| {
                    bases
                        .into_iter()
                        .flatten()
                        .filter_map(|base| exe.strip_prefix(base).ok())
                        .find_map(|inside| inside.components().next())
                        .map(|first| first.as_os_str().to_os_string())
                });
            first
        })
        .collect()
}

/// The executable of the copy of `build_id` in `dir`, made from `source` when there is none: the
/// executable, whose bytes are read from `exe_from` (`source` itself, or the open image where the
/// file at `source` is no longer the build being copied) and, of `companions` (paths beside
/// `source`), those that are there. The copy is made in a
/// directory of its own beside the final one and renamed into place, so one that is not whole is
/// never used; a whole copy is reused as it is. A plain copy of the bytes keeps an embedded code
/// signature, and the copy keeps the executable bit.
fn copy_build(
    dir: &Path,
    build_id: &str,
    source: &Path,
    exe_from: &Path,
    companions: &[&str],
) -> io::Result<PathBuf> {
    let name = source
        .file_name()
        .ok_or_else(|| io::Error::other("the executable has no file name"))?;
    let from = source.parent().unwrap_or(Path::new(""));
    let target = dir.join(build_id);
    let exe = target.join(name);
    let present: Vec<&str> = companions
        .iter()
        .copied()
        .filter(|companion| from.join(companion).is_file())
        .collect();
    if exe.is_file() && present.iter().all(|c| target.join(c).is_file()) {
        return Ok(exe);
    }
    std::fs::create_dir_all(dir)?;
    let making = dir.join(format!("{MAKING_PREFIX}{}", uuid::Uuid::new_v4()));
    let made = (|| {
        std::fs::create_dir(&making)?;
        std::fs::copy(exe_from, making.join(name))?;
        for companion in &present {
            let to = making.join(companion);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(from.join(companion), to)?;
        }
        // What stands there is not whole (a copy cut short by a removal that met a file in use):
        // it goes before the whole one takes its place.
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&making, &target)
    })();
    if let Err(err) = made {
        std::fs::remove_dir_all(&making).ok();
        return Err(err);
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder::registry::RegistryEntry;
    use crate::test_support::ScratchDir;

    fn source_in(dir: &Path, content: &[u8]) -> PathBuf {
        let source = dir.join("install").join("octoboardd");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        source
    }

    #[test]
    fn the_build_id_is_the_version_and_a_hash_of_the_executable() {
        let id = build_id();
        let version = env!("CARGO_PKG_VERSION");
        let hash = id
            .strip_prefix(version)
            .and_then(|rest| rest.strip_prefix('+'))
            .expect("the version and a hash");
        assert_eq!(hash.len(), HASH_BYTES * 2);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(build_id(), id);
    }

    #[test]
    fn the_hash_follows_the_bytes() {
        let dir = ScratchDir::new("build-hash");
        let (a, b, c) = (dir.join("a"), dir.join("b"), dir.join("c"));
        std::fs::write(&a, b"one").unwrap();
        std::fs::write(&b, b"one").unwrap();
        std::fs::write(&c, b"two").unwrap();
        assert_eq!(hash_of(&a).unwrap(), hash_of(&b).unwrap());
        assert_ne!(hash_of(&a).unwrap(), hash_of(&c).unwrap());
    }

    #[test]
    fn a_copy_is_made_whole_and_reused() {
        let dir = ScratchDir::new("build-copy");
        let builds = dir.join("builds");
        let source = source_in(&dir, b"first");
        let exe = copy_build(&builds, "1.0.0+aa", &source, &source, &[]).expect("copied");
        assert_eq!(exe, builds.join("1.0.0+aa").join("octoboardd"));
        assert_eq!(std::fs::read(&exe).unwrap(), b"first");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
            assert_ne!(mode & 0o100, 0, "the executable bit is kept");
        }
        // Nothing of the making is left beside it.
        let names: Vec<_> = std::fs::read_dir(&builds)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["1.0.0+aa"]);

        // A whole copy is reused, not made again.
        std::fs::write(&source, b"changed").unwrap();
        let again = copy_build(&builds, "1.0.0+aa", &source, &source, &[]).expect("reused");
        assert_eq!(again, exe);
        assert_eq!(std::fs::read(&exe).unwrap(), b"first");
    }

    /// The copy takes the executable's bytes from where it is told, not from `source`'s path: after
    /// an update replaced the file there, the path holds the next build.
    #[test]
    fn the_copy_is_made_from_the_bytes_it_is_told_to_read_not_from_the_path_it_is_named_after() {
        let dir = ScratchDir::new("build-copy-image");
        let builds = dir.join("builds");
        let source = source_in(&dir, b"the build that was installed later");
        let image = dir.join("image");
        std::fs::write(&image, b"the build that runs").unwrap();
        let exe = copy_build(&builds, "1.0.0+dd", &source, &image, &[]).expect("copied");
        assert_eq!(exe, builds.join("1.0.0+dd").join("octoboardd"));
        assert_eq!(std::fs::read(&exe).unwrap(), b"the build that runs");
    }

    /// Where the open image can be read, the running executable's own path stands for it; any other
    /// path is read as it is.
    #[test]
    fn only_the_running_images_path_is_read_from_the_open_image() {
        let image = crate::platform::proc::running_image().unwrap();
        let read_from = crate::platform::proc::image_source(&image);
        if cfg!(target_os = "linux") {
            assert_eq!(read_from, Path::new("/proc/self/exe"));
        } else {
            assert_eq!(read_from, image);
        }
        let other = Path::new("/somewhere/else/octoboardd");
        assert_eq!(crate::platform::proc::image_source(other), other);
    }

    #[test]
    fn a_copy_that_is_not_whole_is_made_again() {
        let dir = ScratchDir::new("build-partial");
        let builds = dir.join("builds");
        let source = source_in(&dir, b"bytes");
        std::fs::create_dir_all(builds.join("1.0.0+aa").join("sub")).unwrap();
        std::fs::write(builds.join("1.0.0+aa").join("sub").join("stray"), b"x").unwrap();
        let exe = copy_build(&builds, "1.0.0+aa", &source, &source, &[]).expect("copied");
        assert_eq!(std::fs::read(&exe).unwrap(), b"bytes");
        assert!(!builds.join("1.0.0+aa").join("sub").exists());
    }

    #[test]
    fn the_companion_files_that_are_beside_the_executable_are_copied() {
        let dir = ScratchDir::new("build-companions");
        let builds = dir.join("builds");
        let source = source_in(&dir, b"exe");
        let install = source.parent().unwrap();
        std::fs::write(install.join("conpty.dll"), b"dll").unwrap();
        std::fs::create_dir_all(install.join("x64")).unwrap();
        std::fs::write(install.join("x64").join("OpenConsole.exe"), b"host").unwrap();
        let companions = ["conpty.dll", "x64/OpenConsole.exe", "arm64/OpenConsole.exe"];
        let exe = copy_build(&builds, "1.0.0+bb", &source, &source, &companions).expect("copied");
        let copy = exe.parent().unwrap();
        assert_eq!(std::fs::read(copy.join("conpty.dll")).unwrap(), b"dll");
        assert_eq!(
            std::fs::read(copy.join("x64").join("OpenConsole.exe")).unwrap(),
            b"host"
        );
        assert!(
            !copy.join("arm64").exists(),
            "a file that is not there is not made up"
        );
    }

    #[test]
    fn a_copy_that_cannot_be_made_starts_from_the_source() {
        let dir = ScratchDir::new("build-fallback");
        let source = source_in(&dir, b"exe");
        // A file where the builds directory should be.
        let blocked = dir.join("builds");
        std::fs::write(&blocked, b"not a directory").unwrap();
        let builds = Builds::new(blocked);
        let started = builds.run_from("1.0.0+cc", &source, |exe| exe.to_path_buf());
        assert_eq!(started, source);
    }

    fn live_entry(id: &str, exe: &Path) -> RegistryEntry {
        let pid = std::process::id();
        let start = crate::platform::proc_table::start_of(pid as i32).expect("this process runs");
        RegistryEntry {
            holder_id: id.to_string(),
            pid,
            started: format!("{pid}:{start}"),
            port: 1,
            secret: "s".to_string(),
            version: "1.0.0".to_string(),
            build_id: "b".to_string(),
            exe: exe.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn only_the_builds_no_live_holder_runs_from_are_removed() {
        let dir = ScratchDir::new("build-sweep");
        let builds_dir = dir.join("builds");
        let registry_dir = dir.join("holders");
        for name in ["kept", "dead", "unused", ".making-x"] {
            std::fs::create_dir_all(builds_dir.join(name)).unwrap();
            std::fs::write(builds_dir.join(name).join("octoboardd"), b"x").unwrap();
        }
        std::fs::write(builds_dir.join("notes.txt"), b"x").unwrap();
        registry::write(
            &registry_dir,
            &live_entry("live", &builds_dir.join("kept").join("octoboardd")),
        )
        .unwrap();
        let mut gone = live_entry("gone", &builds_dir.join("dead").join("octoboardd"));
        gone.started = "999999:1".to_string();
        registry::write(&registry_dir, &gone).unwrap();
        // A holder outside the builds directory says nothing about it.
        registry::write(
            &registry_dir,
            &live_entry("elsewhere", Path::new("/somewhere/else/octoboardd")),
        )
        .unwrap();

        let builds = Builds::new(builds_dir.clone());
        assert!(builds.sweep(&registry_dir));
        assert!(builds_dir.join("kept").join("octoboardd").is_file());
        for name in ["dead", "unused", ".making-x"] {
            assert!(!builds_dir.join(name).exists(), "{name}");
        }
        assert!(builds_dir.join("notes.txt").is_file());
    }

    #[test]
    fn a_holder_whose_identity_is_unknown_keeps_its_build() {
        let dir = ScratchDir::new("build-sweep-unknown");
        let builds_dir = dir.join("builds");
        let registry_dir = dir.join("holders");
        std::fs::create_dir_all(builds_dir.join("odd")).unwrap();
        let mut entry = live_entry("odd", &builds_dir.join("odd").join("octoboardd"));
        entry.started = String::new();
        // Something listens on the port, so the holder may still run.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        entry.port = listener.local_addr().unwrap().port();
        registry::write(&registry_dir, &entry).unwrap();
        assert!(Builds::new(builds_dir.clone()).sweep(&registry_dir));
        assert!(builds_dir.join("odd").is_dir());

        // Once nothing does, it is gone.
        drop(listener);
        assert!(Builds::new(builds_dir.clone()).sweep(&registry_dir));
        assert!(!builds_dir.join("odd").exists());
    }

    #[test]
    fn a_build_is_not_removed_while_a_holder_is_being_started_from_it() {
        let dir = ScratchDir::new("build-serial");
        let builds_dir = dir.join("builds");
        let registry_dir = dir.join("holders");
        let source = source_in(&dir, b"exe");
        let builds = std::sync::Arc::new(Builds::new(builds_dir.clone()));
        let (copied, copied_signal) = std::sync::mpsc::channel();
        let (sweeping, sweeping_signal) = std::sync::mpsc::channel::<()>();
        let starting = {
            let builds = builds.clone();
            std::thread::spawn(move || {
                builds.run_from("1.0.0+dd", &source, |exe| {
                    copied.send(exe.to_path_buf()).unwrap();
                    // The sweep is asked for while the start is under way, and the start ends
                    // only after it has had its chance to remove the copy.
                    sweeping_signal.recv().unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    exe.is_file()
                })
            })
        };
        let exe = copied_signal.recv().unwrap();
        let sweep = {
            let builds = builds.clone();
            let registry_dir = registry_dir.clone();
            std::thread::spawn(move || builds.sweep(&registry_dir))
        };
        sweeping.send(()).unwrap();
        assert!(
            starting.join().unwrap(),
            "the copy was there until the start ended"
        );
        sweep.join().unwrap();
        assert!(!exe.exists(), "the sweep ran once the start was done");
    }
}
