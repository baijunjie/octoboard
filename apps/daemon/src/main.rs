//! `octoboardd` — the Octoboard daemon. It plays two roles in one process: the host role (PTYs,
//! agent processes, directories and repositories) and the coordinator role (the stored consoles,
//! projects and sessions). The desktop application is purely a client of it, over the protocol in
//! `apps/daemon/PROTOCOL.md`.

mod accounts;
mod adapter;
mod agents;
mod blocking_pass;
mod branch_cleanup;
mod branch_origin;
mod browse;
mod browser_socket;
mod cli;
mod config_document;
mod coordinator;
mod cross_project_guard;
mod env_shell;
mod git;
mod git_workflow;
mod handoff;
mod holder;
mod hooks;
mod host_open;
mod host_tools;
mod hostfs;
mod instance_lock;
mod interface_language;
mod keep_awake;
mod key_file;
#[cfg(test)]
mod layering_tests;
mod log_file;
mod loopback;
mod mcp;
mod messaging;
mod owner_key;
mod owner_record;
mod path_links;
mod paths;
mod platform;
mod processes;
mod project_git;
mod protocol;
mod providers;
mod records;
mod removal_safety;
mod request_guard;
mod revert_gate;
mod self_update;
mod server;
mod session;
mod session_git_hooks;
mod session_request;
mod session_tools;
mod state;
mod store;
mod terminal;
#[cfg(test)]
mod test_support;
mod text;
mod time;
mod titles;
mod wire_path;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;

use state::{AppState, StopKind};
use store::Store;

/// The product name, from `config/app.json` (see `build.rs`), for the wording the daemon itself
/// puts in front of the user.
pub(crate) const APP_NAME: &str = env!("OCTOBOARD_APP_NAME");

/// The status a daemon a service manager runs exits with after the planned stop of a planned
/// restart, on which the service restarts it (`RestartForceExitStatus=` in
/// `apps/daemon/dist/systemd/octoboardd.service`), so the next daemon runs in the service: 75,
/// `EX_TEMPFAIL`.
const RESTART_EXIT_STATUS: i32 = 75;

/// How long the end of the daemon waits for the work still running on its runtime once
/// [`run_daemon`] has returned: blocking work nothing waits for any more (a `git` command on a
/// mount that hangs, an update check), which would otherwise hold the exit for as long as it runs,
/// and with it, under a service manager, a planned restart's successor. A running agent CLI update
/// is waited for first, up to its own deadline, and is not part of this bound (see
/// `agents::cli`).
const RUNTIME_SHUTDOWN_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    if let Some(command) = cli.command {
        return cli::run(command);
    }

    // While this is still the only thread.
    processes::ledger::forget_inherited_tag();
    let (key, handed_over_from) = if cli.key_stdin {
        let (key, from) = read_key_from_stdin()?;
        (Some(key), Some(from))
    } else {
        (None, None)
    };
    let log_file = init_logging();
    platform::proc::crash::install();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let state = std::sync::OnceLock::new();
    let exit_status = runtime.block_on(run_daemon(
        cli.parent_pid,
        cli.port,
        key,
        handed_over_from,
        cli.service_managed,
        log_file,
        &state,
    ));
    // An update the stop was asked for is not cut off by the exit, up to its own deadline; what
    // else is still running is then given the usual bound. Read once the daemon has stopped, so an
    // update claimed until then is waited for.
    if let Some(state) = state.get() {
        state.agent_clis.wait_for_updates();
    }
    shut_down_runtime(runtime, RUNTIME_SHUTDOWN_WAIT);
    // Once the runtime is gone.
    if let Some(status) = exit_status? {
        std::process::exit(status);
    }
    Ok(())
}

/// Shuts the daemon's runtime down once [`run_daemon`] has returned, waiting up to `wait`
/// ([`RUNTIME_SHUTDOWN_WAIT`]) for its blocking work, whatever is still running then left to the
/// process's exit; its tasks are dropped, as dropping the runtime drops them. What the stop has to
/// finish (the handover to the holders, the files the next daemon reads, the sessions' records)
/// [`run_daemon`] awaits before it returns, so only work nothing is waiting for is cut short.
fn shut_down_runtime(runtime: tokio::runtime::Runtime, wait: std::time::Duration) {
    let began = std::time::Instant::now();
    runtime.shutdown_timeout(wait);
    if began.elapsed() >= wait {
        tracing::warn!(
            ?wait,
            "work still running at the end of the daemon did not finish in time; it is cut short by the exit"
        );
    }
}

/// The key the daemon a planned restart stopped hands its successor (`--key-stdin`): the first
/// line of stdin, which has to have the form of a key; and the daemon that handed it over, which
/// the line after it names (see `cli::handed_over_from`), or not, from an earlier build.
fn read_key_from_stdin() -> Result<(String, owner_record::Predecessor)> {
    let mut stdin = std::io::stdin().lock();
    let mut line = String::new();
    std::io::BufRead::read_line(&mut stdin, &mut line).context("reading the key from stdin")?;
    let key = line.trim();
    anyhow::ensure!(request_guard::is_key(key), "what stdin gave is not a key");
    // The daemon that wrote the key has closed the pipe, so this ends there at the latest.
    let mut from = String::new();
    std::io::BufRead::read_line(&mut stdin, &mut from).ok();
    Ok((key.to_string(), cli::handed_over_from(&from)))
}

/// Sends the daemon's log, at `INFO` and above, to stderr and to `daemon.log` (`log_file`). Never
/// to stdout: that carries the handshake line (port and key) the application reads. Stderr gets
/// colour codes only when it is a terminal and `NO_COLOR` is unset or empty, so a service
/// manager's journal holds plain text, as the file does. The file is written only once the
/// returned handle is enabled, which the daemon does when it holds the instance lock.
///
/// The application owns the read end of that stderr, so from the moment it is gone every write to
/// the log fails. Reporting those failures is therefore turned off: the report is an `eprintln!`
/// of its own, which panics on the same dead stderr, and that panic would take out whichever task
/// logged — the parent watch among them, leaving the daemon running with no application (see
/// `spawn_parent_watch`). A log that cannot be written is dropped instead. The file is what
/// remains of the log then, and the only one a packaged application ever has: launched from the
/// Finder, it has no stderr to read at all.
fn init_logging() -> log_file::LogFile {
    use std::io::IsTerminal as _;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter};

    let log_file = log_file::LogFile::open();
    // An explicit `with_ansi` overrides the layer's own `NO_COLOR` handling, so it is applied here.
    let colour = std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(colour)
                .log_internal_errors(false),
        )
        .with(
            fmt::layer()
                .with_writer(log_file.clone())
                .with_ansi(false)
                .log_internal_errors(false),
        )
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();
    log_file
}

/// Runs the daemon on `port` when one is given, else on a port the system assigns, with `key` when
/// one is given, else a key of its own; `handed_over_from` is the daemon that handed `key` over,
/// as far as it said. `service_managed` is `--service-managed`. `state_out` is given the daemon's
/// state once it exists, for `main` to read what is still running after this returns, error or
/// not. Returns the status to exit with, when it is not 0.
async fn run_daemon(
    parent_pid: Option<u32>,
    port: Option<u16>,
    key: Option<String>,
    handed_over_from: Option<owner_record::Predecessor>,
    service_managed: bool,
    log_file: log_file::LogFile,
    state_out: &std::sync::OnceLock<Arc<AppState>>,
) -> Result<Option<i32>> {
    // Held for the whole run, up to the start of a successor: see `instance_lock` for what a
    // second daemon would do to the first one's sessions.
    let lock = instance_lock::acquire()?;
    // Only the daemon that holds the lock writes `daemon.log`; a second one has only stderr.
    log_file.enable();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "octoboardd starting"
    );

    // Bound ahead of anything the daemon changes, so that a port in use fails the start cleanly.
    let listener = bind(port).await?;
    let port = listener.local_addr()?.port();

    let store = Store::open(&paths::db_path())?;
    let key = match key {
        Some(key) => key,
        None => request_guard::generate_key().context("generating the daemon's key")?,
    };
    // The running image is what the daemon's helpers run (`self_exe`); the successor is started
    // from the path the daemon was installed at, which may lead elsewhere by then.
    let launch_exe = cli::launch_path()?;
    // The build identity hashes the image, so it is taken now rather than at its first use, which
    // may come after the installation has been replaced.
    holder::build_id();
    let self_exe = platform::proc::running_image()?
        .to_string_lossy()
        .into_owned();
    let state = Arc::new(AppState::new(
        store,
        port,
        key.clone(),
        self_exe,
        launch_exe.to_string_lossy().into_owned(),
        paths::saved_output_dir(),
        paths::consoles_dir(),
        paths::handoff_dir(),
        processes::ledger::ProcessLedger::new(&paths::data_dir()),
    ));
    let _ = state_out.set(state.clone());
    // The browser's own socket, where this daemon serves the browser: it runs without the desktop
    // application and embeds the UI, on a host with such sockets. Bound under the instance lock,
    // which a stale socket file's removal needs. A daemon that cannot bind it, or cannot hold the
    // owner key the socket is for, serves no browser and runs on for everything else.
    let browser_listener =
        if parent_pid.is_none() && !server::ui::EMBEDDED.is_empty() && browser_socket::AVAILABLE {
            match browser_socket::bind(&paths::data_dir()) {
                Ok(listener) => Some(listener),
                Err(err) => {
                    tracing::warn!(
                        "the browser socket could not be opened; no browser is served: {err:#}"
                    );
                    None
                }
            }
        } else {
            None
        };
    // Read or made before the owner record, which says whether the daemon accepts one, is written.
    // The socket is for the owner key: without one held, it is let go and no browser is served.
    let holds_owner_key = state
        .owner_key
        .start(browser_listener.is_some(), &paths::data_dir());
    let browser_listener = if holds_owner_key {
        browser_listener
    } else {
        None
    };
    if parent_pid.is_some() {
        state.set_follows_application();
        // Before anything the application could write is lost to nobody reading it.
        state::application_note::spawn_reader(state.clone());
    }
    // Resolved once: the refusal of a planned restart and the choice of its successor read the
    // same answer.
    state.set_service_managed(platform::proc::restarted_by_service_manager(
        service_managed,
    ));
    // The screens the previous daemon's planned stop saved, which the adoption below restores.
    state.load_restart_screens(&paths::restart_screens_path());
    // An application that quit leaves a record of the holders to end (`holder::quit_targets`),
    // which this start reconciles whether or not it follows the application: a record that names
    // no holder is scoped to those the registry lists now, and what those run is ended, not
    // adopted. A target leaves the record only as its ending is confirmed, so a start cut short
    // before then finds the rest again; the wait below bounds only this start.
    let quit_targets =
        holder::quit_targets::reconcile(&paths::application_quit_record(), &paths::holders_dir());
    if let Some(targets) = &quit_targets {
        tracing::info!(
            holders = targets.remaining().len(),
            "the application quit; the holders it left that still run are ended where they can be verified"
        );
    }
    // What a helper asks through a holder waits until the adopted sessions exist.
    state.begin_adoption();
    // The background work each session's agent had left running, loaded into memory before the
    // holders' relayed reports apply on top of it; the adoption keeps or clears it by the launch.
    state.load_saved_background(&paths::background_tasks_path());
    // The messages the previous daemon's planned stop left queued, written once the adoption is
    // over into the sessions adopted running the process they were queued for.
    state.load_restart_outbox(&paths::restart_outbox_path());
    install_holders(&state, parent_pid);
    if let Some(targets) = quit_targets.clone() {
        match holder::client::current() {
            Some(holders) => holders.end_quit_targets(targets),
            None => tracing::warn!(
                "the holders could not be set up; the application's quit record is left for a later start"
            ),
        }
    }
    // Until the reconcile below has read the previous run's ledger file, adopting a launch must
    // neither stop what that file says was kept nor write over it.
    state.process_ledger.await_reconcile();
    state.install_auto_switch(coordinator::auto_switch::hooks());
    // What the holders kept running while no daemon was is taken over before anything that would
    // end it: the sessions marked interrupted, the run directory cleared, the previous run's
    // processes stopped.
    let adopted = coordinator::adopt::adopt(&state, &paths::run_dir()).await;
    // What no process was adopted or claimed for is of a process that is gone. A session claimed
    // keeps its entry for a holder linked to later.
    state.retain_saved_background(|session| {
        adopted
            .sessions
            .iter()
            .chain(&adopted.claimed)
            .any(|kept| kept == session)
    });
    state.end_adoption();
    if quit_targets.is_some() {
        // Before anything that can start a terminal: the server and the resume of the interrupted
        // sessions begin below.
        wait_for_quit_endings().await;
    }
    if !adopted.sessions.is_empty() || !adopted.shells.is_empty() || adopted.ended > 0 {
        tracing::info!(
            sessions = adopted.sessions.len(),
            shells = adopted.shells.len(),
            ended = adopted.ended,
            "took over what the holders kept running"
        );
    }
    // Every other session's process went with the previous daemon, whatever the stored status said,
    // except one a holder that could not be linked to may still run, which stays claimed
    // (`coordinator::unlinked`).
    let held: Vec<String> = adopted
        .sessions
        .iter()
        .chain(&adopted.claimed)
        .cloned()
        .collect();
    let interrupted = state.store.mark_unheld_sessions_interrupted(&held)?;
    if interrupted > 0 {
        tracing::info!(
            interrupted,
            "sessions left by a previous daemon were marked interrupted"
        );
    }
    clear_run_dir(&paths::run_dir(), &adopted.scratch_dirs);
    // A switch that died with the previous daemon may have left a staging directory in an account's
    // config directory. The default accounts' directories are not known until the shell has been
    // read, so only the ones Octoboard has been told of are swept.
    let known_dirs: Vec<String> = state
        .store
        .list_accounts()?
        .into_iter()
        .map(|account| account.config_dir)
        .chain(
            state
                .store
                .list_sessions()?
                .into_iter()
                .filter_map(|session| session.config_dir),
        )
        .collect();
    accounts::relocate::sweep_staging(known_dirs.iter().map(std::path::Path::new));

    // Saved output whose session is gone (a daemon that died between deleting the record and the
    // file) or whose write was cut short has nothing else to remove it.
    state.sweep_saved_output()?;
    // Likewise for a handoff package whose session is gone.
    handoff::sweep(&state)?;
    // The installer that updated the installation may have left moved-aside files a process still
    // held, which nothing else removes until the next install or the uninstall.
    platform::build_files::remove_installer_set_aside(std::path::Path::new(&state.self_exe));
    // Pages a session pushed before it took the bound role, which no tool of its can list or
    // delete, and which a failed withdrawal or a push racing a binding may have left.
    if let Err(err) = coordinator::pages::withdraw_stray_pages(&state) {
        tracing::warn!(%err, "withdrawing the pages of bound sessions failed");
    }

    // Installed before the owner record is written: whoever finds the daemon through the record
    // may stop it at once, and a termination signal before the handlers exist would end it
    // without ending its sessions. Likewise the planned-restart request, which `restart` sends
    // to a daemon its record says takes it, and which would end one without the handler.
    // Fused: still watched once a planned stop has begun, after this may have resolved.
    let shutdown = futures_util::FutureExt::fuse(platform::proc::shutdown_requested());
    tokio::pin!(shutdown);
    let restart_requests = platform::proc::restart_requested();
    let takes_restart = restart_requests.installed();
    spawn_restart_requests(state.clone(), restart_requests);

    // Before the line below, which is what a caller waits for, so that whoever sees the line can
    // find the daemon through this record. The record says whether a service manager starts the
    // successor, which makes a key of its own, rather than this daemon, which hands over its key;
    // the systemd unit it runs as, which a command asks systemd about; and which daemon it took
    // over from, as the daemon that started it said, or as the note or the record a daemon before
    // it in its service left, read before this record replaces it and the note is settled below.
    let unit = platform::proc::user_service_unit(state.service_managed());
    let predecessor = match handed_over_from {
        Some(from) => from,
        None if state.service_managed() => {
            owner_record::Owner::service_predecessor(unit.as_deref())
        }
        None => owner_record::Predecessor::None,
    };
    let owner =
        owner_record::Owner::current(port, parent_pid.is_some(), takes_restart).map(|owner| {
            owner
                .with_service_manager(state.service_managed())
                .with_owner_key(state.owner_key.is_held())
                .with_unit(unit)
                .with_predecessor(predecessor)
        });
    match &owner {
        Some(owner) => {
            if let Err(err) = owner.record_key(&key) {
                tracing::warn!(%err, "writing the key record failed");
            }
            if let Err(err) = owner.record() {
                tracing::warn!(%err, "writing the owner record failed");
            }
        }
        None => tracing::warn!("reading this process's start time failed; no owner record"),
    }
    // The daemon this one takes over from, if any, is done handing over: whoever looks finds this
    // one now, through the lock, and its record where it could write one.
    owner_record::settle_handover();

    // Printed before anything else can block: the application waits for this line to know where to
    // connect and what to present. Besides this line the key is in memory, in the owner-only key
    // record above, and in what the daemon starts (their arguments and environment, and the
    // owner-only files of a session's scratch directory): the port file below stays port-only.
    println!("octoboardd listening on 127.0.0.1:{port} with key {key}");
    use std::io::Write;
    std::io::stdout().flush().ok();
    // A convenience for a developer attaching to a daemon started by hand, not something the
    // daemon depends on. The port alone is in this file: the key is not.
    let port_file = port_file_path();
    if let Err(err) = std::fs::write(&port_file, port.to_string()) {
        tracing::warn!(%err, path = %port_file.display(), "writing the port file failed");
    }

    if let Some(parent_pid) = parent_pid {
        spawn_parent_watch(state.clone(), parent_pid);
    }

    // Off the runtime rather than ahead of the line above: a ten-second shell snapshot must not
    // put that cost on every application launch, and clients are served before it lands either way
    // (see `apps/daemon/src/agents/availability.rs`).
    agents::availability::spawn_determine(state.clone());
    // The server form's update: checks on a schedule where this build and host carry one.
    self_update::start(&state, parent_pid.is_some(), &paths::data_dir());
    records::collector::spawn(&state);
    accounts::quotas::spawn(&state);
    coordinator::archive_cleanup::spawn(&state);
    // Resuming the interrupted sessions and stopping what the previous run's agents left, unless
    // a holder that could not be linked to may run a session no record places: then both wait
    // until it is linked to or gone.
    coordinator::unlinked::start_resume_and_reconcile(&state);
    // Probed once before the server starts answering, on a blocking thread (it may wait on the
    // host's bus, which the probe itself bounds), so that a snapshot only reads the answer kept.
    let _ = tokio::task::spawn_blocking(platform::sleep::available).await;
    keep_awake::spawn(&state);
    // Likewise off the runtime: it runs `git` in every project's directory, a few at a time.
    project_git::remote::reconcile_remote_urls(&state);
    project_git::watch::start(&state);

    let browser = browser_listener.map(|listener| {
        tracing::info!(
            path = %browser_socket::path_in(&paths::data_dir()).display(),
            "serving the browser on its socket"
        );
        browser_socket::serve(listener, server::browser_router(state.clone()))
    });
    let serve = axum::serve(listener, server::router(state.clone()));
    // The listener goes with `serve` once this ends, and the browser socket's with `browser`
    // below: the daemon accepts no more connections, and the port and the socket are free for a
    // successor.
    let stop = tokio::select! {
        result = serve => {
            if let Err(err) = result {
                tracing::error!(%err, "the server stopped with an error");
                stop_all_processes(&state).await;
                return Err(err.into());
            }
            StopKind::Ordinary
        }
        kind = state.await_stop() => {
            match kind {
                StopKind::Ordinary => tracing::info!("shutdown requested"),
                StopKind::Planned { successor: true } => tracing::info!("planned restart requested"),
                StopKind::Planned { successor: false } => tracing::info!("planned stop requested"),
            }
            kind
        }
        _ = &mut shutdown => {
            tracing::info!("a termination signal arrived");
            StopKind::Ordinary
        }
    };
    drop(browser);

    let successor = match stop {
        StopKind::Ordinary => {
            stop_all_processes(&state).await;
            false
        }
        // An ordinary stop asked for from here on escalates the planned stop: this sequence, the
        // only one that ends anything once the daemon stops, cuts the handover short where it
        // stands and ends what it was to leave running, and no successor is started.
        StopKind::Planned { successor } => match holder::client::current() {
            Some(holders) => {
                let handover = taking_signals(
                    &state,
                    coordinator::planned_stop::hand_over(
                        &state,
                        holders,
                        paths::restart_screens_path(),
                    ),
                    &mut shutdown,
                )
                .await;
                if matches!(handover, coordinator::planned_stop::Handover::Parted(_))
                    && !state.ordinary_stop_asked()
                {
                    // Once no drain begins another write and the terminals are the holders'. Off
                    // the runtime: it waits on a drain still writing, and syncs the file.
                    let saving = state.clone();
                    let outbox = tokio::task::spawn_blocking(move || {
                        saving.save_restart_outbox(&paths::restart_outbox_path())
                    });
                    if let Err(err) = taking_signals(&state, outbox, &mut shutdown).await {
                        tracing::error!(%err, "saving the queued messages for the next daemon panicked; they are lost");
                    }
                }
                if state.ordinary_stop_asked() {
                    tracing::info!("an ordinary stop was asked for during the planned stop; what it was to leave running is ended");
                    stop_all_processes_after_planned(&state, holders).await;
                    false
                } else {
                    successor
                }
            }
            // The terminals run in the daemon and end with it: nothing can be handed over.
            None => {
                tracing::warn!("no holder runs the terminals; the planned stop ends every session as an ordinary stop does");
                taking_signals(&state, stop_all_processes(&state), &mut shutdown).await;
                successor && !state.ordinary_stop_asked()
            }
        },
    };
    remove_port_file(&port_file, port);
    if let Some(owner) = &owner {
        // Ahead of the record going: from then until the next daemon has recorded itself, the
        // note is what shows a command that one is on its way.
        if successor {
            if let Err(err) = owner.note_handover() {
                tracing::warn!(%err, "writing the handover note failed");
            }
        }
        owner.forget();
    }
    if !successor {
        return Ok(None);
    }

    // Released before the successor starts, which takes it.
    drop(lock);
    if state.service_managed() {
        tracing::info!(
            status = RESTART_EXIT_STATUS,
            "exiting for the service manager to start the next daemon"
        );
        return Ok(Some(RESTART_EXIT_STATUS));
    }
    let key = state.key.clone();
    let started =
        tokio::task::spawn_blocking(move || self_update::start_successor(port, &key, &launch_exe))
            .await
            .unwrap_or(false);
    if !started {
        if let Some(owner) = owner {
            owner.withdraw_handover();
        }
    }
    anyhow::ensure!(
        started,
        "the next daemon did not start; the sessions are kept for the next start"
    );
    Ok(None)
}

/// Takes each planned-restart request the host delivers (`octoboardd restart`) as the request
/// `POST /restart` makes, which a daemon that follows an application refuses.
fn spawn_restart_requests(state: Arc<AppState>, mut requests: platform::proc::RestartRequests) {
    tokio::spawn(async move {
        loop {
            requests.next().await;
            match state.request_planned_restart() {
                Ok(()) => tracing::info!("a planned restart was asked for by signal"),
                Err(refusal) => tracing::warn!(
                    reason = refusal.code(),
                    "a planned restart asked for by signal is refused: {}",
                    refusal.message()
                ),
            }
        }
    });
}

/// Binds the loopback port `port`, or one the system assigns for `None`.
async fn bind(port: Option<u16>) -> Result<tokio::net::TcpListener> {
    let requested = port.unwrap_or(0);
    tokio::net::TcpListener::bind(("127.0.0.1", requested))
        .await
        .map_err(|err| match port {
            Some(port) if err.kind() == std::io::ErrorKind::AddrInUse => {
                anyhow::anyhow!("port {port} is already in use")
            }
            Some(port) => anyhow::Error::new(err).context(format!("binding 127.0.0.1:{port}")),
            None => anyhow::Error::new(err).context("binding 127.0.0.1"),
        })
}

/// Every login, MCP sign-in, agent process, shell, quota probe and quota kick is ended with the
/// daemon: the ones it runs in a holder through the holder, which exits once it has nothing left.
/// Each session is left interrupted, which is the status a click resumes from; each shell terminal ends with its
/// record. (What takes the daemon down without this running leaves them to the holders, and to the
/// next daemon.)
async fn stop_all_processes(state: &Arc<AppState>) {
    state.account_quotas.stop_cli_runs();
    state.account_logins.stop_all().await;
    state.mcp_server_auths.stop_all().await;
    tokio::join!(state.stop_all_sessions(), terminal::shell::close_all(state));
    // Every terminal is ended by now; the holders are told the daemon is going, and exit. One
    // that cannot be linked to is ended by its process, and what it ran is interrupted.
    tokio::task::spawn_blocking(|| {
        if let Some(holders) = holder::client::current() {
            holders.shutdown();
        }
    })
    .await
    .ok();
    coordinator::unlinked::interrupt_at_stop(state);
}

/// Runs `work`, a step of a planned stop, to its end, asking for an ordinary stop should the host
/// ask the daemon to end meanwhile (`shutdown`, fused, which stays pending once it has resolved):
/// a termination signal is an ordinary stop during a planned stop too, and escalates it.
async fn taking_signals<T>(
    state: &AppState,
    work: impl std::future::Future<Output = T>,
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> T {
    tokio::pin!(work);
    loop {
        tokio::select! {
            done = &mut work => return done,
            () = &mut *shutdown => {
                tracing::info!("a termination signal arrived during the planned stop");
                state.request_shutdown();
            }
        }
    }
}

/// [`stop_all_processes`] for an ordinary stop that escalates a planned stop over `holders`
/// already begun. What a holder the planned stop handed terminals over from, parted from or left
/// untouched for the next daemon runs is ended with that holder, by its verified process
/// (`Holders::end_handed_over`): a terminal handed over is the next daemon's, which this daemon
/// sends nothing to any more, a holder parted from has no link left, and what a holder left
/// untouched runs was never this daemon's. Everything else is ended as an ordinary stop ends it.
/// Every session's preview is kept first (`AppState::stop_all_sessions_after_handover`), and the
/// saved screens the planned stop wrote stay for the next start, should this daemon be killed
/// before it is done.
async fn stop_all_processes_after_planned(
    state: &Arc<AppState>,
    holders: &'static holder::client::Holders,
) {
    state.account_quotas.stop_cli_runs();
    state.account_logins.stop_all().await;
    state.mcp_server_auths.stop_all().await;
    let left = holders.left_to_successor();
    let ends_here = |shell: &terminal::shell::ShellProcess| {
        !shell
            .held_at()
            .is_some_and(|at| left.contains(&at.holder_id))
    };
    tokio::join!(
        state.stop_all_sessions_after_handover(&left, move || holders.end_handed_over()),
        terminal::shell::close_all_ending(state, ends_here),
    );
    // Every terminal still the daemon's to end is ended by now; the other holders are told the
    // daemon is going, and those that cannot be linked to are ended by their process.
    tokio::task::spawn_blocking(move || holders.shutdown())
        .await
        .ok();
    coordinator::unlinked::interrupt_at_stop(state);
}

/// Has the terminals the daemon starts run in a holder of this build, which the daemon starts from
/// a copy of its own executable (`holder::build`), apart from itself so that they outlive it (as a
/// unit of its own under a systemd user manager), when the first terminal is asked for.
/// `application` is the desktop application's pid, when there is one. Where that cannot be set up
/// the terminals run in the daemon, as they do in a process that never calls this.
fn install_holders(state: &Arc<AppState>, application: Option<u32>) {
    let installed = platform::proc::running_image()
        .context("finding the daemon's own executable")
        .and_then(|exe| {
            holder::client::install(holder::client::ClientConfig {
                registry_dir: paths::holders_dir(),
                exe,
                builds: Some(Arc::new(holder::build::Builds::new(paths::builds_dir()))),
                range: holder::compat::Range::current(),
                launch: holder::client::launch_holder(application, state.service_managed()),
                relay: Arc::new(server::relay::DaemonRelay::new(state.clone())),
            })
        });
    if let Err(err) = installed {
        tracing::warn!(%err, "terminals will run in the daemon itself: the holders could not be set up");
    }
}

/// Waits for the endings of the holders the application's quit left, begun by the adoption, for up
/// to `holder::client::ENDINGS_WAIT`, so that no terminal starts beside what they still run. The
/// wait only bounds this start, which the application gives the startup line only so long: it
/// does not touch the record. A target leaves it as its ending is confirmed
/// (`holder::quit_targets::QuitTargets::resolved`), so one still running when the wait ends carries
/// on in this daemon, and one that cannot be ended stays in the record for a later start. A holder
/// that could not be linked to is ended by its verified process and waited for as well.
async fn wait_for_quit_endings() {
    let Some(holders) = holder::client::current() else {
        return;
    };
    let confirmed =
        tokio::task::spawn_blocking(move || holders.wait_endings(holder::client::ENDINGS_WAIT))
            .await
            .unwrap_or(false);
    if !confirmed {
        tracing::warn!(
            "a holder the quit left is not confirmed ended; the application's quit record keeps it"
        );
    }
}

/// Stops the daemon once the application's process is gone. The daemon lives and dies with the
/// application; only this watch stands in the way of background operation. A quit the application
/// told the daemon of (`state::application_note`) ends every session, as an ordinary stop does;
/// the application gone with no such note (a crash, a kill, the update restart) is a planned stop,
/// which leaves the sessions running in their holders for the next launch to adopt.
fn spawn_parent_watch(state: Arc<AppState>, parent_pid: u32) {
    // The callback may run on a thread of the host's own (Windows), outside the runtime.
    let runtime = tokio::runtime::Handle::current();
    platform::proc::watch_parent(parent_pid, move || {
        runtime.spawn(async move {
            let kind = state
                .stop_for_gone_application(state::application_note::INPUT_END_WAIT)
                .await;
            match kind {
                StopKind::Ordinary => {
                    tracing::info!(parent_pid, "the application quit; shutting down");
                }
                StopKind::Planned { .. } => {
                    tracing::info!(
                        parent_pid,
                        "the application is gone with no word of a quit; handing the sessions over"
                    );
                }
            }
        });
    });
}

/// Removes the per-session scratch directories of `run_dir`, except `keep`, those of the sessions
/// adopted from the holders. Every other directory under it belonged to a session of a daemon that
/// has already exited. A file a dying process of that daemon still holds open can refuse its
/// removal for a while (a sharing violation on Windows): then the rest is still removed, entry by
/// entry, and what is held is left for the next start.
fn clear_run_dir(run_dir: &std::path::Path, keep: &[std::path::PathBuf]) {
    let first = if keep.is_empty() {
        match std::fs::remove_dir_all(run_dir) {
            Ok(()) => return,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
            Err(err) => Some(err),
        }
    } else {
        None
    };
    let entries = match std::fs::read_dir(run_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            let err = first.unwrap_or(err);
            tracing::warn!(path = %run_dir.display(), %err, "clearing the run directory failed");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if keep.contains(&path) {
            continue;
        }
        let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(err) = removed {
            tracing::warn!(path = %path.display(), %err, "a leftover in the run directory is held; it is left for the next start");
        }
    }
}

/// Removed on the way out, so nothing points at a port this daemon no longer holds. A file that
/// names another port is another daemon's and stays.
fn remove_port_file(path: &std::path::Path, port: u16) {
    match std::fs::read_to_string(path) {
        Ok(content) if content.trim() == port.to_string() => {}
        Ok(_) => return,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%err, "reading the port file failed");
            }
            return;
        }
    }
    if let Err(err) = std::fs::remove_file(path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%err, "removing the port file failed");
        }
    }
}

/// In the data directory, which is the user's own, so that daemons of different users or data
/// directories never share it.
fn port_file_path() -> std::path::PathBuf {
    paths::data_dir().join("octoboardd.port")
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::test_support::{app_state, ScratchDir, PATIENCE};

    /// Where [`parent_watch_child`] records that the watch got as far as the shutdown. Its presence
    /// in the environment is also what tells that case it is being run as one.
    const DONE_FILE_VAR: &str = "OCTOBOARDD_TEST_PARENT_WATCH_DONE";

    /// The parent watch asks for the stop, the ordinary one when the application said it was
    /// quitting, even when the log line it writes once the stop is decided cannot be written. The
    /// application owns the read end of the daemon's stderr, so from its death every such write
    /// fails, which is the state this test puts the child in by closing the read end of its
    /// stderr.
    #[test]
    fn the_parent_watch_asks_for_the_stop_with_a_dead_log() {
        let dir = ScratchDir::new("parent-watch");
        let done = dir.join("done");
        let log = dir.join("child.log");
        let mut child = Command::new(std::env::current_exe().expect("the binary"))
            .args([
                "--exact",
                "tests::parent_watch_child",
                "--ignored",
                // The case has to run uncaptured: libtest's capture would take the subscriber's own
                // `eprintln!` about the failed write, which is the very thing that must land on the
                // dead stderr, and the test would pass whether that report is turned off or not.
                "--nocapture",
                "--test-threads=1",
            ])
            .env(DONE_FILE_VAR, &done)
            .stdout(std::fs::File::create(&log).expect("the child's log"))
            .stderr(Stdio::piped())
            .spawn()
            .expect("the child test started");
        drop(child.stderr.take());

        let deadline = Instant::now() + PATIENCE;
        let status = loop {
            match child.try_wait().expect("the child was waited on") {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    child.kill().ok();
                    panic!("the child never ended; it said {}", said(&log));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        assert!(
            status.success(),
            "the child ended with {status}; it said {}",
            said(&log)
        );
        // A child that exits cleanly without having run the case at all — a filter that matched
        // nothing, because the case was renamed or moved — would otherwise read as a pass.
        assert!(
            done.exists(),
            "the child never ran the case; it said {}",
            said(&log)
        );
    }

    /// What the child wrote where libtest reports its result, for a failure to be read by.
    fn said(log: &std::path::Path) -> String {
        std::fs::read_to_string(log).unwrap_or_else(|err| format!("<unreadable: {err}>"))
    }

    /// The case [`the_parent_watch_asks_for_the_stop_with_a_dead_log`] runs in a child process;
    /// does nothing when run any other way. It has to be a process of its own in any case: it
    /// installs the daemon's own log subscriber, which is installed once per process.
    #[test]
    #[ignore = "run in a child process by the_parent_watch_asks_for_the_stop_with_a_dead_log"]
    fn parent_watch_child() {
        let Ok(done) = std::env::var(DONE_FILE_VAR) else {
            return;
        };
        super::init_logging();
        let (state, _dir) = app_state("parent-watch-child");
        // Stands in for the application: spawned and reaped here, so the watch finds its pid gone
        // the first time it looks.
        let mut gone = crate::test_support::programs::echo("")
            .command()
            .stdout(Stdio::null())
            .spawn()
            .expect("the stand-in application ran");
        let parent_pid = gone.id();
        gone.wait().expect("the stand-in application was reaped");
        // What the reader of the application's input would have seen: a quit note, then the end.
        state.note_application_quit();
        state.note_application_input_ended();

        // Unbounded on purpose: the parent bounds the whole case by `PATIENCE`, and a second
        // deadline inside it would only ever expire after that one.
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                super::spawn_parent_watch(state.clone(), parent_pid);
                assert_eq!(state.await_stop().await, crate::state::StopKind::Ordinary);
            });
        std::fs::write(&done, "reached").expect("the marker was written");
    }

    /// Clearing the run directory leaves the scratch directories of the sessions adopted from the
    /// holders, with the hook scripts their agents still call, and removes the rest.
    #[test]
    fn clearing_the_run_directory_keeps_the_scratch_directories_of_adopted_sessions() {
        let dir = ScratchDir::new("run-dir");
        let run = dir.join("run");
        for name in ["kept", "gone"] {
            std::fs::create_dir_all(run.join(name)).unwrap();
            std::fs::write(run.join(name).join("hook"), "script").unwrap();
        }
        std::fs::write(run.join("stray"), "x").unwrap();

        super::clear_run_dir(&run, &[run.join("kept")]);
        assert!(run.join("kept").join("hook").exists());
        assert!(!run.join("gone").exists());
        assert!(!run.join("stray").exists());

        super::clear_run_dir(&run, &[]);
        assert!(!run.exists());
        // Nothing there: nothing to do, and no panic.
        super::clear_run_dir(&run, &[]);
        super::clear_run_dir(&run, &[run.join("kept")]);
    }

    #[tokio::test]
    async fn a_port_in_use_fails_the_bind_naming_the_port() {
        let taken = super::bind(None).await.expect("a port was assigned");
        let port = taken.local_addr().unwrap().port();
        let err = super::bind(Some(port))
            .await
            .expect_err("the port is taken");
        assert_eq!(err.to_string(), format!("port {port} is already in use"));
    }

    #[test]
    fn the_port_file_holds_the_port_and_is_removed_only_by_the_daemon_that_wrote_it() {
        let dir = ScratchDir::new("port-file");
        let path = dir.join("octoboardd.port");
        std::fs::write(&path, "4021").expect("the port file was written");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "4021");

        super::remove_port_file(&path, 4022);
        assert!(path.exists(), "another daemon's port file was removed");
        super::remove_port_file(&path, 4021);
        assert!(!path.exists());
        // Already gone: nothing to do, and no panic.
        super::remove_port_file(&path, 4021);
    }

    /// A runtime whose blocking work stays busy far past the wait is shut down when the wait runs
    /// out, not when the work ends: the work here is released only once the shutdown has
    /// returned, so a shutdown that waited for it would find it finished.
    #[test]
    fn shutting_the_runtime_down_does_not_wait_for_blocking_work_past_the_wait() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel::<()>();
        runtime.spawn_blocking(move || {
            started_tx.send(()).ok();
            // Ends on the release, or on the patience running out, whichever comes first.
            release_rx.recv_timeout(PATIENCE).ok();
            finished_tx.send(()).ok();
        });
        // Only work that is running holds a shutdown; one still queued would just be dropped.
        started_rx
            .recv_timeout(PATIENCE)
            .expect("the blocking work started");

        super::shut_down_runtime(runtime, Duration::from_millis(100));
        let still_running = finished_rx.try_recv().is_err();

        // Let the blocked thread go before judging, so a failure does not leave it behind.
        release_tx.send(()).ok();
        assert!(
            still_running,
            "the shutdown returned only once the work had ended"
        );
        finished_rx
            .recv_timeout(PATIENCE)
            .expect("the blocked thread was released");
    }

    /// Blocking work that finishes within the wait is waited for, as dropping the runtime would
    /// have, and the shutdown returns once it is done instead of using up the whole wait.
    #[test]
    fn shutting_the_runtime_down_waits_for_blocking_work_that_finishes_within_the_wait() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let work_done = done.clone();
        runtime.spawn_blocking(move || {
            started_tx.send(()).ok();
            std::thread::sleep(Duration::from_millis(100));
            work_done.store(true, Ordering::SeqCst);
        });
        started_rx
            .recv_timeout(PATIENCE)
            .expect("the blocking work started");

        let began = Instant::now();
        super::shut_down_runtime(runtime, PATIENCE);
        let elapsed = began.elapsed();

        assert!(
            done.load(Ordering::SeqCst),
            "the shutdown returned before the blocking work was done"
        );
        assert!(
            elapsed < PATIENCE,
            "the shutdown used the whole wait ({elapsed:?}) for work that had finished"
        );
    }

    /// A runtime with nothing running on it is shut down at once.
    #[test]
    fn shutting_down_an_idle_runtime_returns_before_the_wait_is_up() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let began = Instant::now();
        super::shut_down_runtime(runtime, PATIENCE);
        assert!(began.elapsed() < PATIENCE);
    }
}
