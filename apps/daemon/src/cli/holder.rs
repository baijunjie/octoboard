//! The `holder` mode: a process that owns the terminals of the programs the daemon starts (see
//! `crate::holder`). It runs until it has nothing left to serve, and logs to `holder-<id>.log`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

/// How long a holder waits for the daemon that started it to link before it gives up.
const FIRST_LINK_DEADLINE: Duration = Duration::from_secs(30);

/// How long the log of a holder that has ended is kept.
const STALE_LOG: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub(super) fn run(holder_id: String, registry_dir: Option<PathBuf>) -> Result<()> {
    let registry_dir = registry_dir.unwrap_or_else(crate::paths::holders_dir);
    init_logging(&holder_id, &registry_dir);
    // The programs a holder runs lead sessions of their own, out of reach of whatever ends the
    // holder's group: a failure that takes the holder down ends them first.
    crate::platform::proc::crash::install();
    let config = crate::holder::server::HolderConfig {
        registry_dir,
        holder_id,
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_id: crate::holder::build_id().to_string(),
        exe: crate::platform::proc::running_image()?,
        first_link_deadline: FIRST_LINK_DEADLINE,
        relay_deadline: crate::holder::RELAY_DEADLINE,
    };
    // Asked to end (SIGTERM, on Unix) by the daemon that cannot link to this holder, the holder
    // ends every program it runs before it exits, as it does when it has nothing left to serve.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(crate::holder::server::run_until(
            config,
            crate::platform::proc::shutdown_requested(),
        ))
}

/// Whether the log file `file` (`holder-<id>.log`, or one of its rotations) is of a holder that
/// still runs: its registry file names a process that does. A quiet holder keeps its log.
fn holder_runs(registry_dir: &Path, file: &str) -> bool {
    let Some(id) = file
        .strip_prefix("holder-")
        .and_then(|rest| rest.split(".log").next())
    else {
        return false;
    };
    crate::holder::registry::read(registry_dir, id)
        .and_then(|entry| crate::platform::proc::ProcessIdentity::parse(&entry.started))
        .is_some_and(|identity| identity.is_running())
}

/// Sends the log, at `INFO` and above, to `holder-<id>.log` and to stderr, which the daemon
/// discards.
fn init_logging(holder_id: &str, registry_dir: &Path) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter};

    // One file per holder, so that holders running together never rotate each other's; the files
    // of holders long gone are removed. Whether one runs is read from the registry directory this
    // holder was given, which is not always the user's own.
    crate::log_file::LogFile::remove_stale("holder-", STALE_LOG, |file| {
        holder_runs(registry_dir, file)
    });
    let log_file = crate::log_file::LogFile::open_named(&format!("holder-{holder_id}.log"));
    log_file.enable();
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .log_internal_errors(false),
        )
        .with(
            fmt::layer()
                .with_writer(log_file)
                .with_ansi(false)
                .log_internal_errors(false),
        )
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holder::registry::{self, RegistryEntry};
    use crate::platform::proc::{ProcessId, ProcessIdentity};
    use crate::test_support::ScratchDir;

    fn entry(holder_id: &str, started: String) -> RegistryEntry {
        RegistryEntry {
            holder_id: holder_id.to_string(),
            pid: std::process::id(),
            started,
            port: 1,
            secret: "s".to_string(),
            version: "0.0.0".to_string(),
            build_id: "b".to_string(),
            exe: "x".to_string(),
        }
    }

    #[test]
    fn the_log_of_a_holder_that_runs_is_in_use_and_that_of_one_that_does_not_is_not() {
        let dir = ScratchDir::new("holder-logs");
        let me = ProcessIdentity {
            pid: ProcessId(std::process::id()),
            start_micros: crate::platform::proc_table::start_of(std::process::id() as i32)
                .expect("this process's start"),
        };
        registry::write(&dir, &entry("alive", me.key())).unwrap();
        registry::write(&dir, &entry("gone", "1:1".to_string())).unwrap();

        assert!(holder_runs(&dir, "holder-alive.log"));
        assert!(holder_runs(&dir, "holder-alive.log.2"), "a rotation too");
        assert!(!holder_runs(&dir, "holder-gone.log"));
        assert!(!holder_runs(&dir, "holder-unknown.log"));
        assert!(!holder_runs(&dir, "daemon.log"));
    }
}
