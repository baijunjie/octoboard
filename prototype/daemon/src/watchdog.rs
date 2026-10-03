//! Daemon-exit cleanup (T4): kills every agent process the daemon started on every exit path —
//! SIGINT/SIGTERM, the `--parent-pid` parent going away, `axum::serve` returning an error, or a
//! panic — so a crash or shutdown never leaves agents running unattended. This matters because
//! `portable-pty` calls `setsid()` on each agent, putting it in its own process group rather than
//! the daemon's, so without explicit handling on each of these paths none of them would reach it
//! (see `state::kill_all_children`, which kills by process group for the same reason).

use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Checks liveness with `kill -0`, the standard "is this pid still around" probe. Avoided a
/// `libc` dependency for one syscall; shelling out once a second is cheap enough here.
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Installs a panic hook that kills every registered agent process before the default hook
/// prints and the process unwinds/aborts. Installed once at startup, before anything that could
/// plausibly panic.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "panicking — killing all agent processes before unwinding");
        crate::state::kill_all_children();
        default_hook(info);
    }));
}

/// Spawns the background task that polls `parent_pid` and exits the whole process once it is
/// gone. Runs for the lifetime of the daemon; intentionally never joined.
pub fn spawn(parent_pid: u32) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            if !process_alive(parent_pid) {
                tracing::info!(parent_pid, "parent process gone, shutting down");
                crate::state::kill_all_children();
                std::process::exit(0);
            }
        }
    });
}

/// Installs the daemon's actual "clean shutdown" path. Before this existed there was no signal
/// handling at all: an unhandled SIGINT/SIGTERM would tear the daemon down while leaving every
/// agent process (and its own process group of tool subprocesses) running under `launchd`/the
/// shell with no parent. Runs for the lifetime of the daemon; intentionally never joined.
pub fn spawn_signal_handler() {
    tokio::spawn(async move {
        let mut sigterm =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(err) => {
                    tracing::warn!(%err, "failed to install SIGTERM handler");
                    return;
                }
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT, killing all agent processes and shutting down");
            }
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM, killing all agent processes and shutting down");
            }
        }
        crate::state::kill_all_children();
        std::process::exit(0);
    });
}
