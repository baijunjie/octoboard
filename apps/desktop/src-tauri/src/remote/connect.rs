//! Making a connection to a remote daemon: the SSH connection, `octoboardd` found on the host, the
//! requested action run, the owner key read and the generation checked, a device key installed
//! where the owner has agreed to it, and the loopback port and its tunnel set up, then the daemon
//! shown in a window of its own, or the browser. One window at most shows a daemon, however many entries name it,
//! and a second request for a daemon a connection is being made to waits for that one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::PrivateKey;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use super::authorized_keys::{self, ChangeError, KeyChange};
use super::credentials::Store;
use super::entries::{self, Entry};
use super::key_removal;
use super::octoboardd::{self, Address, Remote, Shell};
use super::outcome::{Action, ConnectOutcome, ConnectRequest, SecretKindName};
use super::saved::Saved;
use super::ssh::{self, ConnectError, RunError, Secret, Session, Target};
use super::{
    browser, connections_changed, device_key, forward, next_order, tunnel, window_closed, Live,
    RemoteState, Via, BROWSER_LABEL_PREFIX, CONNECTION_UP_EVENT, FORWARDS_CHANGED_EVENT,
};
use crate::daemon_window::{self, RemoteDaemon};
use crate::diagnostics::diagnostic;

/// How long running an action (`octoboardd start`, which waits for the daemon to come up) may take.
const ACTION_TIMEOUT: Duration = Duration::from_secs(120);

/// The daemons a connection is being made to, by identity: a second request for one waits until
/// the first is over, and then finds its window, or makes its own connection where the first made
/// none.
#[derive(Default)]
pub struct Connecting(Mutex<HashMap<String, watch::Receiver<bool>>>);

/// A connection being made, until it is dropped.
pub struct Making<'a> {
    connecting: &'a Connecting,
    identity: String,
    done: watch::Sender<bool>,
}

impl Drop for Making<'_> {
    fn drop(&mut self) {
        self.connecting
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.identity);
        self.done.send_replace(true);
    }
}

impl Connecting {
    /// Marks a connection to `identity` as being made, or, where one already is, gives what says
    /// when it is over.
    pub fn begin(&self, identity: &str) -> Result<Making<'_>, watch::Receiver<bool>> {
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(under_way) = all.get(identity) {
            return Err(under_way.clone());
        }
        let (done, over) = watch::channel(false);
        all.insert(identity.to_owned(), over);
        Ok(Making {
            connecting: self,
            identity: identity.to_owned(),
            done,
        })
    }

    /// Marks a connection to `identity` as being made, once none is.
    pub async fn wait_and_begin(&self, identity: &str) -> Making<'_> {
        loop {
            match self.begin(identity) {
                Ok(making) => return making,
                Err(mut over) => {
                    let _ = over.wait_for(|over| *over).await;
                }
            }
        }
    }
}

/// A connection that gives nothing of the owner's: no secret, no fingerprint confirmed, no action,
/// no answer about this device's key. What it needs of the owner it ends with, as its outcome.
pub(super) fn quiet_request(id: &str) -> ConnectRequest {
    ConnectRequest {
        id: id.to_owned(),
        secret: None,
        confirm_fingerprint: None,
        action: None,
        browser: false,
        device_key: None,
        without_asking: false,
    }
}

/// A connection made at launch, which asks nothing: a `quiet_request` that also goes on without
/// this device's key where the owner would have to be asked about it.
pub(super) fn restoring_request(id: &str) -> ConnectRequest {
    ConnectRequest {
        without_asking: true,
        ..quiet_request(id)
    }
}

/// What a command that brought nothing back ends the connection with: a host that stopped
/// answering is unreachable, anything else a failure.
fn run_failed(err: RunError) -> ConnectOutcome {
    match err {
        RunError::TimedOut(_) => ConnectOutcome::Unreachable {
            detail: err.to_string(),
        },
        RunError::Ssh(_) => ConnectOutcome::Failed {
            detail: err.to_string(),
        },
    }
}

/// The SSH target `entry` names, with the secret the owner gave, or for an entry using this
/// device's key, `device_key` as it is kept. A device key entry is signed in to with the password
/// the owner gives instead, which is how a key removed on the host, or lost here, is installed
/// again.
fn target_for(
    entry: &Entry,
    request: &ConnectRequest,
    device_key: Option<Arc<PrivateKey>>,
) -> Result<Target, ConnectOutcome> {
    let secret = match entry.auth {
        entries::Auth::Device => match (&request.secret, device_key) {
            (Some(password), _) => Secret::Password(password.clone()),
            (None, Some(key)) => Secret::DeviceKey(key),
            (None, None) => {
                return Err(ConnectOutcome::SecretNeeded {
                    kind: SecretKindName::Password,
                })
            }
        },
        entries::Auth::Password => match &request.secret {
            Some(password) => Secret::Password(password.clone()),
            None => {
                return Err(ConnectOutcome::SecretNeeded {
                    kind: SecretKindName::Password,
                })
            }
        },
        entries::Auth::Key => Secret::Key {
            path: ssh::expand_home(entry.key_path.as_deref().unwrap_or_default()),
            passphrase: request.secret.clone(),
        },
    };
    Ok(Target {
        host: entry.host.clone(),
        port: entry.port,
        user: entry.user.clone(),
        secret,
        // A pinned fingerprint always wins over one confirmed for an entry that had none.
        pinned: entry
            .fingerprint
            .clone()
            .or_else(|| request.confirm_fingerprint.clone()),
    })
}

/// A daemon reached and accepted: the connection, the `octoboardd` found and what it said.
struct Reached {
    target: Target,
    session: Session,
    remote: Remote,
    key: String,
}

/// Tells the shell the host's SSH server runs a command through, with one run of `SHELL_PROBE`.
pub(super) async fn shell_of(session: &Session) -> Result<Shell, RunError> {
    let output = session.run(octoboardd::SHELL_PROBE).await?;
    Ok(Shell::classify(&String::from_utf8_lossy(&output.stdout)))
}

async fn detect_shell(session: &Session) -> Result<Shell, ConnectOutcome> {
    shell_of(session).await.map_err(run_failed)
}

/// Runs `address --json` with each place `octoboardd` may be until one is found; the `octoboardd`
/// found and what it said.
async fn discover(
    session: &Session,
    shell: Shell,
    places: &[String],
) -> Result<(Remote, Address), ConnectOutcome> {
    for place in places {
        let remote = Remote::new(shell, place);
        let output = session
            .run(&remote.address_command())
            .await
            .map_err(run_failed)?;
        match remote.read_address(output.status, &output.stdout, &output.stderr) {
            Address::NotFound => continue,
            found => return Ok((remote, found)),
        }
    }
    Err(ConnectOutcome::OctoboarddNotFound {
        tried: places.to_vec(),
    })
}

/// Runs `command` for `action`, failing with what it printed when it does not exit 0, or with the
/// time limit when it does not finish within `ACTION_TIMEOUT`.
async fn run_action(
    session: &Session,
    action: Action,
    command: &str,
) -> Result<(), ConnectOutcome> {
    let failed = |detail: String| ConnectOutcome::ActionFailed { action, detail };
    let output = session
        .run_within(command, ACTION_TIMEOUT)
        .await
        .map_err(|err| failed(err.to_string()))?;
    if output.status == Some(0) {
        return Ok(());
    }
    // Its error output, else its standard output, with any key-shaped value taken out: a daemon
    // started in the foreground prints its key there, and the detail reaches the page.
    let mut detail = octoboardd::without_keys(String::from_utf8_lossy(&output.stderr).trim());
    if detail.is_empty() {
        detail = octoboardd::without_keys(String::from_utf8_lossy(&output.stdout).trim());
    }
    if detail.is_empty() {
        detail = match output.status {
            Some(status) => format!("`{command}` exited with status {status}"),
            None => format!("`{command}` ended without an exit status"),
        };
    }
    Err(failed(detail))
}

/// Connects to the daemon `entry` names: SSH, `octoboardd` found, the requested action run, the
/// owner key read and the generation checked. The connection is ended on every failure.
async fn reach(
    entry: &Entry,
    request: &ConnectRequest,
    device_key: Option<Arc<PrivateKey>>,
) -> Result<Reached, ConnectOutcome> {
    let target = target_for(entry, request, device_key)?;
    let session = match ssh::connect(&target).await {
        Ok(session) => session,
        Err(ConnectError::AuthFailed(_)) if matches!(target.secret, Secret::DeviceKey(_)) => {
            return Err(ConnectOutcome::DeviceKeyRefused)
        }
        Err(err) => return Err(err.into()),
    };
    match reach_over(&session, entry, request.action, !request.browser).await {
        Ok((remote, key)) => Ok(Reached {
            target: Target {
                pinned: Some(session.fingerprint.clone()),
                ..target
            },
            session,
            remote,
            key,
        }),
        Err(outcome) => {
            session.disconnect().await;
            Err(outcome)
        }
    }
}

/// `check_generation` refuses a daemon of another protocol generation, as a window of the
/// application needs; the browser shows the daemon's own UI, of its own generation.
async fn reach_over(
    session: &Session,
    entry: &Entry,
    action: Option<Action>,
    check_generation: bool,
) -> Result<(Remote, String), ConnectOutcome> {
    let shell = detect_shell(session).await?;
    let places = match &entry.octoboardd_path {
        Some(path) => vec![path.clone()],
        None => octoboardd::candidates(shell),
    };
    let (remote, mut found) = discover(session, shell, &places).await?;
    if let Some(action) = action {
        match action {
            Action::Start => run_action(session, action, &remote.start_command()).await?,
        }
        let output = session
            .run(&remote.address_command())
            .await
            .map_err(run_failed)?;
        found = remote.read_address(output.status, &output.stdout, &output.stderr);
    }
    match found {
        Address::Answer(answer) => {
            if check_generation {
                octoboardd::check_generation(&answer, octoboardd::LOCAL_GENERATION)?;
            }
            Ok((remote, answer.owner_key))
        }
        Address::NotFound => Err(ConnectOutcome::OctoboarddNotFound { tried: places }),
        Address::NoDaemon => Err(ConnectOutcome::NoDaemon),
        Address::ApplicationNotRunning(detail) => {
            Err(ConnectOutcome::ApplicationNotRunning { detail })
        }
        Address::NoOwnerEndpoint(detail) => Err(ConnectOutcome::NoOwnerEndpoint { detail }),
        Address::EndpointUnreachable(detail) => Err(ConnectOutcome::EndpointUnreachable { detail }),
        Address::Failed(detail) => Err(ConnectOutcome::Failed { detail }),
    }
}

/// What a connection does about this device's key once its host is reached.
#[derive(Debug, PartialEq, Eq)]
enum KeyStep {
    /// Nothing: it signed in with the key, the owner said no before, or it asks nothing.
    Nothing,
    /// The owner is asked, and the connection ended until they answer.
    Ask,
    /// The owner said yes: the key is installed.
    Install,
    /// The owner said no: nothing is installed, and the answer is remembered.
    Decline,
}

/// What the connection `request` asks for does about this device's key, signed in with `secret`
/// to the daemon `entry` names. The owner's answer is taken only for a connection made with their
/// own password or key file; one made with the device key has it installed already.
fn key_step(entry: &Entry, request: &ConnectRequest, secret: &Secret) -> KeyStep {
    if matches!(secret, Secret::DeviceKey(_)) {
        return KeyStep::Nothing;
    }
    match request.device_key {
        Some(true) => KeyStep::Install,
        Some(false) => KeyStep::Decline,
        // A no that was noted and not carried out, the connection made with it having failed: the
        // entry still says its key is installed, and this sign-in takes it back.
        None if entry.device_key.declined && entry.has_device_key() => KeyStep::Decline,
        None if entry.device_key.declined || request.without_asking => KeyStep::Nothing,
        None => KeyStep::Ask,
    }
}

/// What the owner is asked before this device's key is installed over `reached`: the mark of the
/// line that would be added, and the file it would be added to. Nothing is written by asking.
async fn key_offer(reached: &Reached) -> Result<ConnectOutcome, String> {
    let shell = reached.remote.shell;
    Ok(ConnectOutcome::DeviceKeyOffered {
        mark: device_key::comment(&device_key::device_name()),
        file: authorized_keys::authorized_keys_file(&reached.session, shell).await?,
    })
}

/// A device key installed on a host: the file its line was added to and what the line is marked
/// with, as the owner is told of them, and the key.
struct Installed {
    file: String,
    mark: String,
    key: Arc<PrivateKey>,
}

/// Takes the line of `key` out of the host's file over the connection `reached` made, where it is
/// there; `whose` says which key it is, where that fails.
async fn take_line_out(reached: &Reached, key: &PrivateKey, whose: &str) {
    match key_removal::remove_line(&reached.session, reached.remote.shell, key).await {
        Ok(()) | Err(ChangeError::NotFound) => {}
        Err(err) => diagnostic!("cannot take {whose} out of the host's authorized keys: {err}"),
    }
}

/// Removes the device key kept for `entry` from `store`, `whose` saying which key it is where the
/// store does not remove it.
fn forget_key(store: &dyn Store, service: &str, entry: &Entry, whose: &str) {
    if let Err(err) = device_key::forget(store, service, &entry.id) {
        diagnostic!(
            "cannot remove {whose} of {} from the credential store: {err}",
            entry.id
        );
    }
}

/// Installs this device's key for the remote user over the connection `reached` made with the
/// owner's own password or key, and checks that the host accepts it. Called only once the owner
/// has agreed to it. The key pair is made once per entry and kept in `store` under `service`.
/// An install that fails leaves nothing behind: a key the host does not accept is taken out of
/// its file again, and a key pair made for this attempt is removed from the credential store.
async fn install_device_key(
    store: &dyn Store,
    service: &str,
    entry: &Entry,
    reached: &Reached,
) -> Result<Installed, String> {
    let shell = reached.remote.shell;
    let file = authorized_keys::authorized_keys_file(&reached.session, shell).await?;
    let kept = device_key::load_or_make(store, service, &entry.id)?;
    let installed = add_key(reached, &kept.key, file).await;
    if installed.is_err() && kept.made {
        forget_key(
            store,
            service,
            entry,
            "the device key made for a failed install",
        );
    }
    installed
}

/// Adds the line of `key` to the host's `file` over the connection `reached` made, and checks
/// that the host accepts the key; a key it does not accept is taken out again.
async fn add_key(
    reached: &Reached,
    key: &Arc<PrivateKey>,
    file: String,
) -> Result<Installed, String> {
    let mark = device_key::comment(&device_key::device_name());
    let line = device_key::authorized_line(key, &mark)?;
    let body = device_key::public_body(key)?;
    let add = KeyChange::Add {
        line: &line,
        body: &body,
    };
    authorized_keys::change_authorized_keys(&reached.session, reached.remote.shell, add)
        .await
        .map_err(|err| err.to_string())?;
    // A host may read keys from elsewhere, or refuse a file it finds too open: the key is used only
    // once a connection made with it has been accepted.
    let with_key = Target {
        secret: Secret::DeviceKey(key.clone()),
        ..reached.target.clone()
    };
    match ssh::connect(&with_key).await {
        Ok(check) => {
            check.disconnect().await;
            Ok(Installed {
                file,
                mark,
                key: key.clone(),
            })
        }
        Err(err) => {
            take_line_out(reached, key, "the key the host did not accept").await;
            Err(format!(
                "the host did not accept the installed key: {err:?}"
            ))
        }
    }
}

/// Installs this device's key over `reached` and has `record` write down for its saved daemon
/// that it is installed; from then on the connection's reconnections sign in with the key. Where
/// `record` says it could not (the saved daemon is gone or names another destination, or its file
/// could not be written), the line is taken out again over the connection still open and the key
/// removed from `store`: a line on a host that no saved daemon says is installed would stay there
/// unseen.
async fn install_and_record(
    store: &dyn Store,
    service: &str,
    entry: &Entry,
    reached: &mut Reached,
    record: impl FnOnce(&Installed) -> bool,
) -> Result<(), String> {
    let installed = install_device_key(store, service, entry, reached).await?;
    if record(&installed) {
        reached.target.secret = Secret::DeviceKey(installed.key);
        return Ok(());
    }
    take_line_out(reached, &installed.key, "the key no saved daemon records").await;
    forget_key(
        store,
        service,
        entry,
        "the device key no saved daemon records",
    );
    Err("the installed key could not be recorded for its saved daemon, and was taken back".into())
}

/// Leaves the host and this device without the key the owner said no to, over the connection
/// `reached` made: a key kept for the entry (one the host stopped accepting, say) has its line
/// taken out where it is still there, and is removed from the credential store.
async fn drop_declined_key(store: &dyn Store, service: &str, entry: &Entry, reached: &Reached) {
    let Some(key) = device_key::load(store, service, &entry.id) else {
        return;
    };
    take_line_out(reached, &key, "the declined device key").await;
    forget_key(store, service, entry, "the declined device key");
}

/// Notes the owner's no to this device's key for `entry`, where `request` carries one, before
/// the host is spoken to: the answer is remembered whatever becomes of the connection, so the
/// owner is not asked again because a reconnection failed. Nothing else of the entry changes yet:
/// one whose key is installed goes on saying so until the connection has taken the key out of the
/// host (`settle_key`), so that a failed reconnection leaves no line that no entry shows. The
/// entry as it then is saved.
fn record_answer(saved: &Saved<'_>, mut entry: Entry, request: &ConnectRequest) -> Entry {
    if request.device_key == Some(false) {
        saved.note_declined_device_key(&entry.id);
        entries::note_declined_device_key(std::slice::from_mut(&mut entry), &request.id);
    }
    entry
}

/// Does what `key_step` says about this device's key over the connection `reached` made for
/// `entry`, one of the daemons `saved` keeps. A connection that has to ask the owner is ended, and
/// the question is what it ends with; every other step lets it go on, a failed install included.
async fn settle_key(
    saved: &Saved<'_>,
    entry: &Entry,
    request: &ConnectRequest,
    reached: &mut Reached,
) -> Result<(), ConnectOutcome> {
    let identity = entry.identity();
    match key_step(entry, request, &reached.target.secret) {
        KeyStep::Nothing => {}
        KeyStep::Ask => match key_offer(reached).await {
            Ok(offer) => {
                reached.session.disconnect().await;
                return Err(offer);
            }
            // A host that does not say where its keys are kept is not asked about: the connection
            // goes on with what the owner signed in with, and the next one asks again.
            Err(err) => diagnostic!("cannot offer a device key for {identity}: {err}"),
        },
        KeyStep::Install => {
            let record = |installed: &Installed| {
                saved.use_device_key(&entry.id, &identity, &installed.file, &installed.mark)
            };
            let installed =
                install_and_record(saved.store, &saved.service, entry, reached, record).await;
            // The window opens all the same; the next connection asks again.
            if let Err(err) = installed {
                diagnostic!("cannot install a device key on {identity}: {err}");
            }
        }
        KeyStep::Decline => {
            drop_declined_key(saved.store, &saved.service, entry, reached).await;
            saved.decline_device_key(&entry.id);
        }
    }
    Ok(())
}

/// The window's query with `key` in place of the key it carried, everything else kept.
fn with_key(query: &str, key: &str) -> String {
    query
        .split('&')
        .map(|pair| {
            if pair.starts_with("key=") {
                format!("key={key}")
            } else {
                pair.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Points the window labelled `label` at `key`, which its daemon's key changed to.
fn point_window_at_key(app: &AppHandle, label: &str, key: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let result = window.url().and_then(|mut url| {
        let query = with_key(url.query().unwrap_or_default(), key);
        url.set_query(Some(&query));
        window.navigate(url)
    });
    if let Err(err) = result {
        diagnostic!("cannot point the window {label} at its daemon's new key: {err}");
    }
}

/// What already holds a connection to a daemon shows it in.
#[derive(Debug, PartialEq)]
enum Showing {
    /// Its window, labelled so; one whose connection is held but which is not built yet counts too:
    /// it is on its way, and reveals itself once built.
    Window(String),
    /// The browser, opened at this address.
    Browser(String),
}

/// What shows the daemon `identity` already, from where its connection is held; `None` when nothing
/// holds one.
fn showing(state: &RemoteState, identity: &str) -> Option<Showing> {
    state
        .windows()
        .iter()
        .find(|(_, live)| live.identity == identity)
        .map(|(label, live)| match &live.via {
            Via::Window => Showing::Window(label.clone()),
            Via::Browser { url } => Showing::Browser(url.clone()),
        })
}

/// Shows what already holds a connection to the daemon `identity`: brings its window forward, or
/// has the browser opened at its address again; `None` when nothing holds one.
fn show_existing(app: &AppHandle, identity: &str) -> Option<ConnectOutcome> {
    match showing(&app.state::<RemoteState>(), identity)? {
        Showing::Browser(url) => Some(opened_in_browser(&url)),
        Showing::Window(label) => {
            let showing = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(window) = showing.get_webview_window(&label) {
                    crate::background::show_window(&showing, &window);
                }
            });
            Some(ConnectOutcome::Opened)
        }
    }
}

/// Opens `url` in the system's browser: `OpenedInBrowser`, or why it could not.
fn opened_in_browser(url: &str) -> ConnectOutcome {
    match browser::open(url) {
        Ok(()) => ConnectOutcome::OpenedInBrowser,
        Err(detail) => ConnectOutcome::Failed { detail },
    }
}

/// Connects to the remote daemon `request` names, showing it in a new window, or in the browser;
/// see `remote_daemon_connect`. `order` is the rank its connection takes among the others
/// (`Live::order`), where it was reserved before; otherwise it is ranked as it is made.
pub(super) async fn connect(
    app: &AppHandle,
    request: ConnectRequest,
    order: Option<u64>,
) -> Result<ConnectOutcome, String> {
    let saved = super::saved(app)?;
    // A request for a daemon a connection is being made to waits for it, and then shows its window;
    // one for a daemon that is being removed or pointed elsewhere waits for that, and connects to
    // what is then saved.
    let (entry, _making) = saved
        .hold(&request.id)
        .await
        .ok_or_else(|| "This remote daemon has been removed.".to_owned())?;
    // Before anything else can end this: the answer is the owner's whatever becomes of the request.
    let entry = record_answer(&saved, entry, &request);
    if let Some(shown) = show_existing(app, &entry.identity()) {
        return Ok(shown);
    }
    match hold(app, &saved, &entry, &request, order).await {
        Ok(held) => Ok(show(app, held)),
        Err(outcome) => Ok(outcome),
    }
}

/// A connection held for a daemon, to be shown.
struct Held {
    /// The label it is kept under: its window's, or the browser's for the entry.
    label: String,
    via: Via,
    /// What its window is opened on.
    remote: RemoteDaemon,
}

/// Holds a connection to the daemon `entry` names: reaches it, asks about this device's key or does
/// as the owner answered (`key_step`), and serves its tunnel on a loopback port, kept in
/// `RemoteState` under the label of a window registered for it, or of the browser.
async fn hold(
    app: &AppHandle,
    saved: &Saved<'_>,
    entry: &Entry,
    request: &ConnectRequest,
    order: Option<u64>,
) -> Result<Held, ConnectOutcome> {
    let identity = entry.identity();
    let device_key = match entry.auth {
        entries::Auth::Device => device_key::load(saved.store, &saved.service, &entry.id),
        _ => None,
    };
    let mut reached = reach(entry, request, device_key).await?;
    if entry.fingerprint.is_none() {
        saved.pin(&entry.id, &reached.session.fingerprint);
    }
    settle_key(saved, entry, request, &mut reached).await?;
    let bound = tunnel::bind().await.and_then(|listener| {
        let port = listener.local_addr()?.port();
        Ok((listener, port))
    });
    let (listener, port) = match bound {
        Ok(bound) => bound,
        Err(err) => {
            reached.session.disconnect().await;
            return Err(ConnectOutcome::Failed {
                detail: format!("cannot listen on a loopback port: {err}"),
            });
        }
    };
    let remote = RemoteDaemon {
        identity: identity.clone(),
        name: entry.name.clone(),
        port,
        key: reached.key.clone(),
    };
    let (label, via) = if request.browser {
        let url = octoboardd::browser_address(port, &reached.key);
        (
            format!("{BROWSER_LABEL_PREFIX}{}", entry.id),
            Via::Browser { url },
        )
    } else {
        // The label is taken before the window exists, so the link can point it at a new key.
        (daemon_window::register(app), Via::Window)
    };
    let following = app.clone();
    let followed = label.clone();
    let browser = request.browser;
    let link = Arc::new(tunnel::Link::new(
        reached.target,
        reached.remote,
        reached.session,
        reached.key,
        move |key| {
            if browser {
                // The page in the browser asks again itself; the address it is opened at next
                // carries the new key.
                browser_key_changed(&following, &followed, port, &key);
                return;
            }
            let app = following.clone();
            let label = followed.clone();
            let _ = following.run_on_main_thread(move || point_window_at_key(&app, &label, &key));
        },
    ));
    let (stop, stopped) = watch::channel(false);
    let connected = link.live();
    let forwards = (!request.browser).then(|| {
        let changing = app.clone();
        let window = label.clone();
        Arc::new(forward::Forwards::new(
            link.clone(),
            stopped.clone(),
            move || {
                let _ = changing.emit_to(&window, FORWARDS_CHANGED_EVENT, ());
            },
        ))
    });
    app.state::<RemoteState>().windows().insert(
        label.clone(),
        Live {
            identity,
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            order: order.unwrap_or_else(next_order),
            stop,
            connected: connected.clone(),
            via: via.clone(),
            forwards,
            link: Some(link.clone()),
        },
    );
    tauri::async_runtime::spawn(tunnel::serve(listener, link, stopped));
    // The rail is told of every change of the connection's state, and the window of the connection
    // coming back, until the link is gone.
    let watching = app.clone();
    let window = (!request.browser).then(|| label.clone());
    tauri::async_runtime::spawn(follow(
        connected,
        {
            let app = app.clone();
            move || connections_changed(&app)
        },
        move || {
            if let Some(window) = &window {
                let _ = watching.emit_to(window, CONNECTION_UP_EVENT, ());
            }
        },
    ));
    connections_changed(app);
    Ok(Held { label, via, remote })
}

/// Follows whether a link's SSH connection is up until the link is gone: `changed` at every change,
/// and then `up` where it came up again.
async fn follow(mut connected: watch::Receiver<bool>, changed: impl Fn(), up: impl Fn()) {
    while connected.changed().await.is_ok() {
        let is_up = *connected.borrow_and_update();
        changed();
        if is_up {
            up();
        }
    }
}

/// Shows the connection `held` holds: opens the browser at its address, or a new window on its
/// daemon. A connection whose browser or window cannot be opened is let go.
fn show(app: &AppHandle, held: Held) -> ConnectOutcome {
    let Held { label, via, remote } = held;
    if let Via::Browser { url } = via {
        let opened = opened_in_browser(&url);
        if opened != ConnectOutcome::OpenedInBrowser {
            window_closed(app, &label);
        }
        return opened;
    }
    let opening = app.clone();
    let shown = label.clone();
    let queued = app.run_on_main_thread(move || {
        crate::window_factory::open_remote_window(&opening, shown, remote)
    });
    if let Err(err) = queued {
        crate::window_factory::window_lost(app, &label);
        return ConnectOutcome::Failed {
            detail: err.to_string(),
        };
    }
    ConnectOutcome::Opened
}

/// Keeps the address a connection held for the browser opens at in step with its daemon's key.
fn browser_key_changed(app: &AppHandle, label: &str, port: u16, key: &str) {
    if let Some(live) = app.state::<RemoteState>().windows().get_mut(label) {
        live.via = Via::Browser {
            url: octoboardd::browser_address(port, key),
        };
    }
}

#[cfg(test)]
mod tests {
    // Used by the tests that run a host's commands for real, which need a POSIX `sh`.
    #[cfg(unix)]
    use super::super::credentials;
    use super::super::live;
    use super::super::ssh::test_server::{Behaviour, TestServer};
    use super::*;

    #[tokio::test]
    async fn a_link_coming_back_up_is_told_once_per_return_and_every_change_to_the_rail() {
        let (state, connected) = watch::channel(true);
        let changes = Arc::new(Mutex::new(0usize));
        let ups = Arc::new(Mutex::new(0usize));
        let (counted_changes, counted_ups) = (changes.clone(), ups.clone());
        let following = tokio::spawn(follow(
            connected,
            move || *counted_changes.lock().unwrap() += 1,
            move || *counted_ups.lock().unwrap() += 1,
        ));
        for (seen, up) in [false, true, false, true].into_iter().enumerate() {
            state.send(up).unwrap();
            // Each change is seen before the next is made.
            while *changes.lock().unwrap() <= seen {
                tokio::task::yield_now().await;
            }
        }
        drop(state);
        following.await.unwrap();
        assert_eq!(*ups.lock().unwrap(), 2);
        assert_eq!(*changes.lock().unwrap(), 4);
    }

    fn entry(server: &TestServer) -> Entry {
        Entry {
            id: "e".into(),
            name: "server".into(),
            host: "127.0.0.1".into(),
            port: server.port,
            user: "owner".into(),
            auth: entries::Auth::Password,
            key_path: None,
            fingerprint: Some(server.fingerprint.clone()),
            octoboardd_path: None,
            device_key: Default::default(),
        }
    }

    fn request(action: Option<Action>) -> ConnectRequest {
        ConnectRequest {
            secret: Some("right".into()),
            action,
            ..quiet_request("e")
        }
    }

    async fn outcome(entry: &Entry, request: &ConnectRequest) -> Result<String, ConnectOutcome> {
        reach(entry, request, None).await.map(|reached| reached.key)
    }

    #[tokio::test]
    async fn a_password_entry_asks_for_the_password_first() {
        let server = TestServer::start(Behaviour::default()).await;
        let mut asked = request(None);
        asked.secret = None;
        assert_eq!(
            outcome(&entry(&server), &asked).await,
            Err(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password
            })
        );
        assert_eq!(server.connections(), 0);
    }

    #[tokio::test]
    async fn a_first_connection_is_made_only_with_the_fingerprint_confirmed() {
        let server = TestServer::start(Behaviour::default()).await;
        let mut unpinned = entry(&server);
        unpinned.fingerprint = None;
        assert_eq!(
            outcome(&unpinned, &request(None)).await,
            Err(ConnectOutcome::HostKeyUnknown {
                fingerprint: server.fingerprint.clone()
            })
        );
        let mut confirmed = request(None);
        confirmed.confirm_fingerprint = Some(server.fingerprint.clone());
        let reached = reach(&unpinned, &confirmed, None).await.unwrap();
        assert_eq!(reached.session.fingerprint, server.fingerprint);
        // Reconnections verify the key now pinned.
        assert_eq!(reached.target.pinned, Some(server.fingerprint.clone()));
        // A pinned fingerprint is not overridden by a confirmed one.
        let mut pinned = entry(&server);
        pinned.fingerprint = Some("SHA256:other".into());
        assert!(matches!(
            outcome(&pinned, &confirmed).await,
            Err(ConnectOutcome::HostKeyMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn octoboardd_is_looked_for_in_the_install_places() {
        let server = TestServer::start(Behaviour {
            missing: vec!["'octoboardd'".into()],
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Ok("aa".into())
        );
        assert_eq!(
            server.commands(),
            vec![
                octoboardd::SHELL_PROBE.to_owned(),
                "'octoboardd' address --json".to_owned(),
                "\"$HOME\"/'.local/bin/octoboardd' address --json".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn an_octoboardd_found_nowhere_is_reported_with_the_places_tried() {
        let candidates = octoboardd::candidates(Shell::Posix);
        let server = TestServer::start(Behaviour {
            missing: candidates
                .iter()
                .map(|place| octoboardd::shell_word(place))
                .chain(["'/opt/ob'".to_owned()])
                .collect(),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::OctoboarddNotFound {
                tried: candidates.clone()
            })
        );
        // The entry's own path is the only place tried once given.
        let mut given = entry(&server);
        given.octoboardd_path = Some("/opt/ob".into());
        assert_eq!(
            outcome(&given, &request(None)).await,
            Err(ConnectOutcome::OctoboarddNotFound {
                tried: vec!["/opt/ob".into()]
            })
        );
    }

    #[tokio::test]
    async fn no_daemon_and_no_owner_endpoint_are_reported_as_such() {
        let server = TestServer::start(Behaviour {
            address: Some((3, Vec::new(), b"not running".to_vec())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::NoDaemon)
        );
        server.set_address(Some((4, Vec::new(), b"remote access is off".to_vec())));
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::NoOwnerEndpoint {
                detail: "remote access is off".into()
            })
        );
        server.set_address(Some((5, Vec::new(), b"refused".to_vec())));
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::EndpointUnreachable {
                detail: "refused".into()
            })
        );
    }

    #[tokio::test]
    async fn starting_the_daemon_runs_start_and_reads_the_key_after() {
        let server = TestServer::start(Behaviour::default()).await;
        assert_eq!(
            outcome(&entry(&server), &request(Some(Action::Start))).await,
            Ok("aa".into())
        );
        assert_eq!(
            server.commands(),
            vec![
                octoboardd::SHELL_PROBE.to_owned(),
                "'octoboardd' address --json".to_owned(),
                "'octoboardd' start".to_owned(),
                "'octoboardd' address --json".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn a_daemon_of_another_generation_is_refused() {
        let other = octoboardd::LOCAL_GENERATION - 1;
        let line =
            format!("{{\"owner_key\":\"aa\",\"generation\":{other},\"daemon_version\":\"0.9.0\"}}");
        let server = TestServer::start(Behaviour {
            address: Some((0, line.into_bytes(), Vec::new())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::GenerationMismatch {
                remote_generation: other,
                local_generation: octoboardd::LOCAL_GENERATION,
                remote_version: "0.9.0".into(),
                local_version: octoboardd::LOCAL_VERSION.into(),
            })
        );
        // Refusing it runs nothing on the server: no update, no restart.
        assert!(!server
            .commands()
            .iter()
            .any(|command| command.ends_with(" update") || command.ends_with(" restart")));
    }

    #[tokio::test]
    async fn the_browser_reaches_a_daemon_of_another_generation_which_a_window_cannot() {
        for other in [
            octoboardd::LOCAL_GENERATION - 1,
            octoboardd::LOCAL_GENERATION + 1,
        ] {
            let line = format!(
                "{{\"owner_key\":\"bb\",\"generation\":{other},\"daemon_version\":\"9.0.0\"}}"
            );
            let server = TestServer::start(Behaviour {
                address: Some((0, line.into_bytes(), Vec::new())),
                ..Behaviour::default()
            })
            .await;
            assert!(matches!(
                outcome(&entry(&server), &request(None)).await,
                Err(ConnectOutcome::GenerationMismatch { .. })
            ));
            let browser = ConnectRequest {
                browser: true,
                ..request(None)
            };
            assert_eq!(outcome(&entry(&server), &browser).await, Ok("bb".into()));
        }
    }

    /// A stand-in host whose `~/.ssh` is a scratch directory of the test's own.
    #[cfg(unix)]
    async fn host_with_home(name: &str) -> (TestServer, crate::config_dir::test_support::Scratch) {
        let home = crate::config_dir::test_support::Scratch::new("remote-device-key", name);
        std::fs::create_dir_all(&home.0).unwrap();
        let server = TestServer::start(Behaviour {
            home: Some(home.0.clone()),
            ..Behaviour::default()
        })
        .await;
        (server, home)
    }

    #[cfg(unix)]
    fn device_entry(server: &TestServer) -> Entry {
        Entry {
            auth: entries::Auth::Device,
            ..entry(server)
        }
    }

    #[cfg(unix)]
    fn without_secret() -> ConnectRequest {
        ConnectRequest {
            secret: None,
            ..request(None)
        }
    }

    /// What a `Saved` borrows, kept by the test.
    #[cfg(unix)]
    #[derive(Default)]
    struct Kept {
        lock: Mutex<()>,
        connecting: Connecting,
        store: credentials::Memory,
    }

    /// The saved daemons kept in `home`, holding `entry`, with their keys in the test's store.
    #[cfg(unix)]
    fn saved_in<'a>(
        kept: &'a Kept,
        home: &crate::config_dir::test_support::Scratch,
        entry: &Entry,
    ) -> Saved<'a> {
        let path = home.0.join(entries::FILE_NAME);
        entries::save(&path, std::slice::from_ref(entry)).unwrap();
        Saved {
            path,
            lock: &kept.lock,
            connecting: &kept.connecting,
            store: &kept.store,
            service: "svc".into(),
        }
    }

    #[cfg(unix)]
    fn answering(answer: bool) -> ConnectRequest {
        ConnectRequest {
            device_key: Some(answer),
            ..request(None)
        }
    }

    #[cfg(unix)]
    fn keys_of(home: &crate::config_dir::test_support::Scratch) -> std::path::PathBuf {
        home.0.join(".ssh/authorized_keys")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_yes_installs_the_key_records_it_and_later_connections_sign_in_with_it() {
        let (server, home) = host_with_home("install").await;
        let kept = Kept::default();
        let entry = entry(&server);
        let saved = saved_in(&kept, &home, &entry);
        let mut reached = reach(&entry, &answering(true), None).await.unwrap();
        assert_eq!(
            settle_key(&saved, &entry, &answering(true), &mut reached).await,
            Ok(())
        );
        // The connection's reconnections sign in with the key from now on.
        assert!(matches!(reached.target.secret, Secret::DeviceKey(_)));
        let key = device_key::load(&kept.store, "svc", "e").unwrap();
        let installed = std::fs::read_to_string(keys_of(&home)).unwrap();
        assert_eq!(installed.lines().count(), 1);
        assert!(installed.contains(&device_key::public_body(&key).unwrap()));
        // The saved daemon says the key is installed, where its line is and what it is marked
        // with, as the file has it.
        let recorded = saved.list().remove(0);
        assert_eq!(recorded.auth, entries::Auth::Device);
        assert_eq!(
            recorded.device_key.file.as_deref(),
            Some("~/.ssh/authorized_keys")
        );
        let mark = recorded.device_key.mark.clone().unwrap();
        assert!(mark.starts_with("Octoboard on "), "{mark}");
        assert!(installed.trim_end().ends_with(&mark), "{installed}");
        reached.session.disconnect().await;

        // With no password given, the saved daemon connects with its key, and nothing more is
        // asked or installed.
        let mut again = reach(&recorded, &without_secret(), Some(key.clone()))
            .await
            .unwrap();
        assert_eq!(
            settle_key(&saved, &recorded, &without_secret(), &mut again).await,
            Ok(())
        );
        assert_eq!(std::fs::read_to_string(keys_of(&home)).unwrap(), installed);
        // A device entry whose key is not kept here asks for the password.
        assert_eq!(
            outcome(&recorded, &without_secret()).await,
            Err(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password
            })
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_device_key_removed_on_the_host_refuses_that_device_alone_and_its_password_sign_in_asks_about_the_key_again(
    ) {
        let (server, home) = host_with_home("revoke").await;
        let (laptop, desktop) = (
            credentials::Memory::default(),
            credentials::Memory::default(),
        );
        let entry = entry(&server);
        for store in [&laptop, &desktop] {
            let reached = reach(&entry, &request(None), None).await.unwrap();
            install_device_key(store, "svc", &entry, &reached)
                .await
                .map(|_| ())
                .unwrap();
            reached.session.disconnect().await;
        }
        let laptop_key = device_key::load(&laptop, "svc", "e").unwrap();
        let desktop_key = device_key::load(&desktop, "svc", "e").unwrap();
        let file = keys_of(&home);
        let laptop_body = device_key::public_body(&laptop_key).unwrap();
        let without_laptop: String = std::fs::read_to_string(&file)
            .unwrap()
            .lines()
            .filter(|line| !line.contains(&laptop_body))
            .map(|line| format!("{line}\n"))
            .collect();
        std::fs::write(&file, &without_laptop).unwrap();

        let device = device_entry(&server);
        assert_eq!(
            reach(&device, &without_secret(), Some(laptop_key.clone()))
                .await
                .err(),
            Some(ConnectOutcome::DeviceKeyRefused)
        );
        assert!(reach(&device, &without_secret(), Some(desktop_key))
            .await
            .is_ok());
        // The password signs the refused device in, and nothing is added again by that alone: the
        // owner is asked first, as at a first connection.
        let relogin = reach(&device, &request(None), Some(laptop_key.clone()))
            .await
            .unwrap();
        assert!(matches!(relogin.target.secret, Secret::Password(_)));
        assert_eq!(
            key_step(&device, &request(None), &relogin.target.secret),
            KeyStep::Ask
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), without_laptop);
        // On a yes the key is installed again, and signs in.
        install_device_key(&laptop, "svc", &device, &relogin)
            .await
            .map(|_| ())
            .unwrap();
        assert!(reach(&device, &without_secret(), Some(laptop_key))
            .await
            .is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_key_that_could_not_be_installed_leaves_the_connection_on_the_password_and_no_key_behind(
    ) {
        let (server, home) = host_with_home("blocked").await;
        // `~/.ssh` is a file, so nothing can be made in it.
        std::fs::write(home.0.join(".ssh"), b"").unwrap();
        let kept = Kept::default();
        let entry = entry(&server);
        let saved = saved_in(&kept, &home, &entry);
        let mut reached = reach(&entry, &answering(true), None).await.unwrap();
        let failed = install_device_key(&kept.store, "svc", &entry, &reached)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(
            failed.starts_with("changing the host's authorized keys ended with Some(1)"),
            "{failed}"
        );
        // The key pair made for this attempt is not kept: nothing would show it.
        assert!(kept.store.accounts().is_empty());
        // The connection goes on all the same, signed in as the owner did, and the saved daemon
        // does not say a key is installed.
        assert_eq!(
            settle_key(&saved, &entry, &answering(true), &mut reached).await,
            Ok(())
        );
        assert!(matches!(reached.target.secret, Secret::Password(_)));
        assert_eq!(saved.list(), std::slice::from_ref(&entry));
        assert!(kept.store.accounts().is_empty());
        // A key that was kept before this attempt is not this attempt's to remove.
        let before = device_key::load_or_make(&kept.store, "svc", "e")
            .unwrap()
            .key;
        assert!(install_device_key(&kept.store, "svc", &entry, &reached)
            .await
            .is_err());
        assert_eq!(
            device_key::load(&kept.store, "svc", "e")
                .unwrap()
                .public_key(),
            before.public_key()
        );
        reached.session.disconnect().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_key_the_host_does_not_accept_is_taken_out_of_its_file_again() {
        let (server, home) = host_with_home("unaccepted").await;
        std::fs::create_dir_all(home.0.join(".ssh")).unwrap();
        std::fs::write(keys_of(&home), "ssh-rsa AAAAother laptop\n").unwrap();
        let store = credentials::Memory::default();
        let entry = entry(&server);
        let mut reached = reach(&entry, &request(None), None).await.unwrap();
        // The connection that checks the installed key is refused, here because it expects another
        // host key, as it is on a host that does not accept the key it was just given.
        reached.target.pinned = Some("SHA256:other".into());
        let failed = install_device_key(&store, "svc", &entry, &reached)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(
            failed.starts_with("the host did not accept the installed key: HostKeyMismatch"),
            "{failed}"
        );
        // The line was added and taken out again, and the key made for it is not kept.
        assert!(server
            .commands()
            .iter()
            .any(|command| command.contains("grep -a -vF")));
        assert_eq!(
            std::fs::read_to_string(keys_of(&home)).unwrap(),
            "ssh-rsa AAAAother laptop\n"
        );
        assert!(store.accounts().is_empty());
        reached.session.disconnect().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_key_that_cannot_be_recorded_for_its_saved_daemon_is_taken_back_at_once() {
        // The saved daemon was removed, or pointed elsewhere, or its file could not be written,
        // by the time the key was installed: nothing may stay on the host that no entry shows.
        let (server, home) = host_with_home("unrecorded").await;
        let store = credentials::Memory::default();
        let entry = entry(&server);
        let mut reached = reach(&entry, &answering(true), None).await.unwrap();
        let mut seen = None;
        let refused = install_and_record(&store, "svc", &entry, &mut reached, |installed| {
            // Installed at this moment: the line is in the file.
            seen = Some(std::fs::read_to_string(keys_of(&home)).unwrap());
            assert_eq!(installed.file, "~/.ssh/authorized_keys");
            false
        })
        .await;
        assert_eq!(
            refused,
            Err(
                "the installed key could not be recorded for its saved daemon, and was taken back"
                    .to_owned()
            )
        );
        assert_eq!(seen.unwrap().lines().count(), 1);
        assert_eq!(std::fs::read_to_string(keys_of(&home)).unwrap(), "");
        assert!(store.accounts().is_empty());
        // The connection is still the one the owner signed in with.
        assert!(matches!(reached.target.secret, Secret::Password(_)));
        assert!(reached.session.run("true").await.is_ok());
        reached.session.disconnect().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_key_installed_while_its_saved_daemon_was_removed_is_taken_back() {
        let (server, home) = host_with_home("removed-meanwhile").await;
        let kept = Kept::default();
        let entry = entry(&server);
        let saved = saved_in(&kept, &home, &entry);
        let mut reached = reach(&entry, &answering(true), None).await.unwrap();
        // The saved daemon goes while the connection is under way.
        entries::save(&saved.path, &[]).unwrap();
        assert_eq!(
            settle_key(&saved, &entry, &answering(true), &mut reached).await,
            Ok(())
        );
        assert_eq!(std::fs::read_to_string(keys_of(&home)).unwrap(), "");
        assert!(kept.store.accounts().is_empty());
        assert!(saved.list().is_empty());
        assert!(matches!(reached.target.secret, Secret::Password(_)));
        reached.session.disconnect().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn asking_ends_the_connection_with_the_question_and_writes_nothing_on_the_host() {
        let (server, home) = host_with_home("asked").await;
        let kept = Kept::default();
        let entry = entry(&server);
        let saved = saved_in(&kept, &home, &entry);
        let mut reached = reach(&entry, &request(None), None).await.unwrap();
        let before = server.commands().len();
        assert_eq!(
            settle_key(&saved, &entry, &request(None), &mut reached).await,
            Err(ConnectOutcome::DeviceKeyOffered {
                mark: device_key::comment(&device_key::device_name()),
                file: "~/.ssh/authorized_keys".into(),
            })
        );
        // The connection is ended: it is made again with the owner's answer, or not at all.
        tokio::time::timeout(crate::test_support::PATIENCE, reached.session.closed())
            .await
            .expect("the connection that asked is ended");
        // Nothing was run on the host to ask, its home holds nothing of the application's, no key
        // was made, and the saved daemon is as it was.
        assert_eq!(server.commands().len(), before);
        assert!(!home.0.join(".ssh").exists());
        assert!(kept.store.accounts().is_empty());
        assert_eq!(saved.list(), [entry]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_no_is_remembered_installs_nothing_and_the_connection_goes_on_with_the_password() {
        let (server, home) = host_with_home("declined").await;
        let kept = Kept::default();
        let asked = entry(&server);
        let saved = saved_in(&kept, &home, &asked);
        // The answer is recorded before the host is spoken to.
        let entry = record_answer(&saved, asked.clone(), &answering(false));
        assert!(entry.device_key.declined);
        assert_eq!(saved.list(), std::slice::from_ref(&entry));
        let mut reached = reach(&entry, &answering(false), None).await.unwrap();
        assert_eq!(
            settle_key(&saved, &entry, &answering(false), &mut reached).await,
            Ok(())
        );
        assert!(!home.0.join(".ssh").exists());
        assert!(kept.store.accounts().is_empty());
        assert_eq!(saved.list(), std::slice::from_ref(&entry));
        // The connection is the one the owner signed in with, and still carries commands.
        assert!(matches!(reached.target.secret, Secret::Password(_)));
        assert!(reached.session.run("true").await.is_ok());
        reached.session.disconnect().await;
        // The next connection asks nothing.
        assert_eq!(
            key_step(&entry, &request(None), &Secret::Password("right".into())),
            KeyStep::Nothing
        );
        // A request that carries no answer, or a yes, records nothing.
        for unanswered in [request(None), answering(true)] {
            assert_eq!(record_answer(&saved, asked.clone(), &unanswered), asked);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_no_is_remembered_though_the_connection_made_with_it_then_fails() {
        let (server, home) = host_with_home("declined-failed").await;
        let kept = Kept::default();
        let asked = entry(&server);
        let saved = saved_in(&kept, &home, &asked);
        let wrong = ConnectRequest {
            secret: Some("wrong".into()),
            ..answering(false)
        };
        let entry = record_answer(&saved, asked, &wrong);
        assert!(matches!(
            reach(&entry, &wrong, None).await.err(),
            Some(ConnectOutcome::AuthFailed { .. })
        ));
        // The owner is not asked again because the reconnection failed.
        assert!(saved.list()[0].device_key.declined);
        assert_eq!(
            key_step(
                &saved.list()[0],
                &request(None),
                &Secret::Password("right".into())
            ),
            KeyStep::Nothing
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_no_to_a_key_this_computer_still_holds_takes_its_line_out_and_forgets_it() {
        let (server, home) = host_with_home("declined-held").await;
        let kept = Kept::default();
        let entry = entry(&server);
        let saved = saved_in(&kept, &home, &entry);
        let mut first = reach(&entry, &answering(true), None).await.unwrap();
        assert_eq!(
            settle_key(&saved, &entry, &answering(true), &mut first).await,
            Ok(())
        );
        first.session.disconnect().await;
        let file = keys_of(&home);
        std::fs::write(
            &file,
            format!(
                "ssh-rsa AAAAother laptop\n{}",
                std::fs::read_to_string(&file).unwrap()
            ),
        )
        .unwrap();
        // Signed in with the password again, the owner says no this time.
        let installed = saved.list().remove(0);
        assert_eq!(installed.auth, entries::Auth::Device);
        // The no is noted before the host is spoken to, and the key still shown as installed: a
        // reconnection that failed now would leave nothing on the host that the entry hides.
        let device = record_answer(&saved, installed.clone(), &answering(false));
        assert_eq!(device.auth, entries::Auth::Device);
        assert!(device.device_key.declined);
        assert_eq!(device.device_key.file, installed.device_key.file);
        assert_eq!(saved.list(), std::slice::from_ref(&device));
        let wrong = ConnectRequest {
            secret: Some("wrong".into()),
            ..answering(false)
        };
        assert!(matches!(
            reach(&device, &wrong, None).await.err(),
            Some(ConnectOutcome::AuthFailed { .. })
        ));
        assert_eq!(saved.list(), std::slice::from_ref(&device));
        assert_eq!(kept.store.accounts(), ["e"]);
        // The next sign-in with the password carries the no out, whether or not it repeats it.
        assert_eq!(
            key_step(&device, &request(None), &Secret::Password("right".into())),
            KeyStep::Decline
        );
        let mut reached = reach(&device, &request(None), None).await.unwrap();
        assert_eq!(
            settle_key(&saved, &device, &request(None), &mut reached).await,
            Ok(())
        );
        reached.session.disconnect().await;
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "ssh-rsa AAAAother laptop\n"
        );
        assert!(kept.store.accounts().is_empty());
        // The saved daemon signs in with a password and says no key is installed.
        let now = saved.list().remove(0);
        assert_eq!(now.auth, entries::Auth::Password);
        assert!(now.device_key.declined);
        assert_eq!(now.device_key.file, None);
    }

    #[tokio::test]
    async fn a_windows_host_that_does_not_name_its_keys_file_is_not_asked_about() {
        let server = TestServer::start(Behaviour {
            shell: Shell::Cmd,
            ..Behaviour::default()
        })
        .await;
        let reached = reach(&entry(&server), &request(None), None).await.unwrap();
        // The stand-in host prints nothing for the script that asks for the file, as a host that
        // could not say would: the owner is not asked about a file that cannot be named.
        assert_eq!(
            key_offer(&reached).await,
            Err("asking the host for its authorized keys file ended with Some(0): ".to_owned())
        );
        let asked = server.commands().pop().unwrap();
        assert!(
            asked.starts_with("powershell.exe -NoProfile -NonInteractive -EncodedCommand "),
            "{asked}"
        );
        reached.session.disconnect().await;
    }

    #[tokio::test]
    async fn the_application_not_running_is_reported_with_the_reason_and_offers_no_start() {
        let server = TestServer::start(Behaviour {
            address: Some((6, Vec::new(), b"the application is not running".to_vec())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::ApplicationNotRunning {
                detail: "the application is not running".into()
            })
        );
    }

    #[tokio::test]
    async fn a_windows_host_is_reached_with_the_commands_of_its_shell() {
        for shell in [Shell::Cmd, Shell::PowerShell] {
            let server = TestServer::start(Behaviour {
                shell,
                ..Behaviour::default()
            })
            .await;
            let reached = reach(&entry(&server), &request(None), None).await.unwrap();
            assert_eq!(reached.remote.shell, shell);
            let first = octoboardd::candidates(shell).remove(0);
            assert_eq!(reached.remote.path, first);
            let address = reached.remote.address_command();
            assert!(!address.starts_with("'"), "{address}");
            let listener = tunnel::bind().await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let remote = reached.remote.clone();
            let link = Arc::new(tunnel::Link::new(
                reached.target,
                reached.remote,
                reached.session,
                reached.key,
                |_| {},
            ));
            let (stop, stopped) = watch::channel(false);
            tokio::spawn(tunnel::serve(listener, link, stopped));
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut stream, b"hi")
                .await
                .unwrap();
            tokio::io::AsyncWriteExt::shutdown(&mut stream)
                .await
                .unwrap();
            let mut back = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut back)
                .await
                .unwrap();
            assert_eq!(back, b"hi");
            assert_eq!(
                server.commands(),
                vec![
                    octoboardd::SHELL_PROBE.to_owned(),
                    address,
                    remote.proxy_command(),
                ]
            );
            let _ = stop.send(true);
        }
    }

    #[tokio::test]
    async fn a_windows_host_without_the_application_running_is_reported_as_such() {
        let server = TestServer::start(Behaviour {
            shell: Shell::Cmd,
            address: Some((3, Vec::new(), Vec::new())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(None)).await,
            Err(ConnectOutcome::ApplicationNotRunning {
                detail: String::new()
            })
        );
    }

    #[test]
    fn only_the_key_of_the_window_s_query_changes() {
        assert_eq!(
            with_key("port=1&key=aa&daemon=x&remote=y&host=%7B%7D", "bb"),
            "port=1&key=bb&daemon=x&remote=y&host=%7B%7D"
        );
    }

    /// Against a real host: `OCTOBOARD_E2E_SSH="<host> <port> <user> <password>"` naming a login
    /// whose `~/.local/bin/octoboardd` (built with the embedded UI) has no daemon running yet, and
    /// from which `pkill` may end that user's SSH sessions. Run with `--ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs a real SSH host, see OCTOBOARD_E2E_SSH"]
    async fn end_to_end_against_a_real_host() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let spec = std::env::var("OCTOBOARD_E2E_SSH").expect("OCTOBOARD_E2E_SSH");
        let parts: Vec<&str> = spec.split(' ').collect();
        let mut entry = Entry {
            id: "e2e".into(),
            name: parts[0].into(),
            host: parts[0].into(),
            port: parts[1].parse().unwrap(),
            user: parts[2].into(),
            auth: entries::Auth::Password,
            key_path: None,
            fingerprint: None,
            octoboardd_path: None,
            device_key: Default::default(),
        };
        let mut asked = ConnectRequest {
            secret: Some(parts[3].into()),
            ..quiet_request("e2e")
        };
        // A first connection shows the fingerprint and connects only once it is confirmed.
        let Err(ConnectOutcome::HostKeyUnknown { fingerprint }) = reach(&entry, &asked, None).await
        else {
            panic!("expected the fingerprint to confirm");
        };
        println!("host key: {fingerprint}");
        asked.confirm_fingerprint = Some(fingerprint.clone());
        assert_eq!(
            reach(&entry, &asked, None).await.err(),
            Some(ConnectOutcome::NoDaemon)
        );
        entry.fingerprint = Some(fingerprint.clone());
        let mut wrong = entry.clone();
        wrong.fingerprint = Some("SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into());
        assert!(matches!(
            reach(&wrong, &asked, None).await.err(),
            Some(ConnectOutcome::HostKeyMismatch { .. })
        ));
        let mut bad = asked.clone();
        bad.secret = Some("not the password".into());
        assert!(matches!(
            reach(&entry, &bad, None).await.err(),
            Some(ConnectOutcome::AuthFailed { .. })
        ));
        // Starting the daemon over the connection.
        asked.action = Some(Action::Start);
        let reached = reach(&entry, &asked, None).await.unwrap();
        println!("octoboardd: {:?}, key read", reached.remote);
        let first_key = reached.key.clone();

        let keys = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = keys.clone();
        let link = Arc::new(tunnel::Link::new(
            reached.target,
            reached.remote,
            reached.session,
            reached.key,
            move |key| seen.lock().unwrap().push(key),
        ));
        let listener = tunnel::bind().await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop, stopped) = watch::channel(false);
        let serving = tokio::spawn(tunnel::serve(listener, link, stopped));

        let check = |key: String| async move {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
            let request = format!(
                "GET /key-check?key={key} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes()).await?;
            let mut response = String::new();
            stream.read_to_string(&mut response).await?;
            Ok::<_, std::io::Error>(response.lines().next().unwrap_or_default().to_owned())
        };
        let status = check(first_key.clone()).await.unwrap();
        println!("with the key: {status}");
        assert!(status.contains(" 204"), "{status}");
        let status = check("00".into()).await.unwrap();
        println!("without the key: {status}");
        assert!(status.contains(" 401"), "{status}");

        // A helper login ends every SSH session of the user, this connection's included.
        let helper = |command: &'static str| {
            let entry = entry.clone();
            let asked = ConnectRequest {
                action: None,
                ..asked.clone()
            };
            async move {
                let target = target_for(&entry, &asked, None).unwrap();
                let session = ssh::connect(&target).await.unwrap();
                let output = session.run(command).await;
                session.disconnect().await;
                output
            }
        };
        let _ = helper("pkill -f 'sshd-session: [a-z]*@notty'").await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let status = check(first_key.clone()).await.unwrap();
        println!("after the SSH connection dropped, same port {port}: {status}");
        assert!(status.contains(" 204"), "{status}");

        // A key rotated on the server is followed at the window's next connection.
        let rotated = helper("~/.local/bin/octoboardd address --rotate >/dev/null 2>&1; echo $?")
            .await
            .unwrap();
        println!(
            "rotate: {}",
            String::from_utf8_lossy(&rotated.stdout).trim()
        );
        let status = check(first_key.clone()).await.unwrap();
        println!("old key after rotation: {status}");
        let followed = keys.lock().unwrap().clone();
        println!("keys followed: {}", followed.len());
        assert_eq!(followed.len(), 1);
        assert_ne!(followed[0], first_key);
        let status = check(followed[0].clone()).await.unwrap();
        println!("new key: {status}");
        assert!(status.contains(" 204"), "{status}");

        stop.send(true).unwrap();
        serving.await.unwrap();
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
        let listening = helper("ss -ltnH; ss -lxH | grep -i octoboard")
            .await
            .unwrap();
        println!(
            "listening on the host:\n{}",
            String::from_utf8_lossy(&listening.stdout)
        );
    }

    fn plain_entry(auth: entries::Auth) -> Entry {
        Entry {
            id: "e".into(),
            name: "n".into(),
            host: "h.example".into(),
            port: 2222,
            user: "u".into(),
            auth,
            key_path: Some("~/.ssh/id_rsa".into()),
            fingerprint: None,
            octoboardd_path: None,
            device_key: Default::default(),
        }
    }

    fn asking(secret: Option<&str>, confirm: Option<&str>) -> ConnectRequest {
        ConnectRequest {
            secret: secret.map(str::to_owned),
            confirm_fingerprint: confirm.map(str::to_owned),
            ..quiet_request("e")
        }
    }

    #[test]
    fn a_password_entry_signs_in_with_the_password_given_and_never_without_one() {
        let entry = plain_entry(entries::Auth::Password);
        let target = target_for(&entry, &asking(Some("pw"), None), None).unwrap();
        assert!(matches!(&target.secret, Secret::Password(pw) if pw == "pw"));
        assert_eq!(
            (target.host.as_str(), target.port, target.user.as_str()),
            ("h.example", 2222, "u")
        );
        assert_eq!(
            target_for(&entry, &asking(None, None), None).err(),
            Some(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password
            })
        );
    }

    #[test]
    fn a_key_file_entry_signs_in_with_the_file_and_the_passphrase_given() {
        let entry = plain_entry(entries::Auth::Key);
        let target = target_for(&entry, &asking(Some("phrase"), None), None).unwrap();
        match &target.secret {
            Secret::Key { path, passphrase } => {
                assert_eq!(path, &ssh::expand_home("~/.ssh/id_rsa"));
                assert!(path.is_absolute());
                assert_eq!(passphrase.as_deref(), Some("phrase"));
            }
            _ => panic!("a key file entry signs in with its file"),
        }
        // An unencrypted key asks for nothing.
        let target = target_for(&entry, &asking(None, None), None).unwrap();
        assert!(matches!(
            target.secret,
            Secret::Key {
                passphrase: None,
                ..
            }
        ));
    }

    #[test]
    fn a_device_key_entry_uses_its_key_and_a_password_given_replaces_it() {
        let entry = plain_entry(entries::Auth::Device);
        let key = Arc::new(device_key::generate().unwrap());
        let target = target_for(&entry, &asking(None, None), Some(key.clone())).unwrap();
        assert!(
            matches!(&target.secret, Secret::DeviceKey(used) if used.public_key() == key.public_key())
        );
        // A key removed on the host is installed again by signing in with the password.
        let target = target_for(&entry, &asking(Some("pw"), None), Some(key)).unwrap();
        assert!(matches!(target.secret, Secret::Password(_)));
        // A key lost here asks for the password.
        assert_eq!(
            target_for(&entry, &asking(None, None), None).err(),
            Some(ConnectOutcome::SecretNeeded {
                kind: SecretKindName::Password
            })
        );
    }

    #[test]
    fn the_pinned_fingerprint_wins_over_the_one_confirmed_and_the_confirmed_one_stands_in_for_none()
    {
        let mut entry = plain_entry(entries::Auth::Password);
        let pinned = |entry: &Entry, confirm| {
            target_for(entry, &asking(Some("pw"), confirm), None)
                .unwrap()
                .pinned
        };
        assert_eq!(pinned(&entry, None), None);
        assert_eq!(pinned(&entry, Some("SHA256:c")), Some("SHA256:c".into()));
        entry.fingerprint = Some("SHA256:p".into());
        assert_eq!(pinned(&entry, Some("SHA256:c")), Some("SHA256:p".into()));
    }

    #[tokio::test]
    async fn a_failing_start_is_reported_with_what_the_host_said() {
        let server = TestServer::start(Behaviour {
            action: Some((1, b"out".to_vec(), b"  boom \n".to_vec())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(Some(Action::Start))).await,
            Err(ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail: "boom".into()
            })
        );
        // Where it said nothing on stderr, what it printed is the reason.
        let server = TestServer::start(Behaviour {
            action: Some((1, b"only stdout\n".to_vec(), Vec::new())),
            ..Behaviour::default()
        })
        .await;
        assert_eq!(
            outcome(&entry(&server), &request(Some(Action::Start))).await,
            Err(ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail: "only stdout".into()
            })
        );
        // Where it said nothing at all, the status is.
        let server = TestServer::start(Behaviour {
            action: Some((9, Vec::new(), Vec::new())),
            ..Behaviour::default()
        })
        .await;
        match outcome(&entry(&server), &request(Some(Action::Start))).await {
            Err(ConnectOutcome::ActionFailed {
                action: Action::Start,
                detail,
            }) => assert!(detail.contains('9'), "{detail}"),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_failed_action_s_detail_never_repeats_a_key_it_printed() {
        let key = "0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9";
        for (stdout, stderr) in [
            (
                format!("octoboardd listening on 127.0.0.1:3369 with key {key}\n"),
                String::new(),
            ),
            (
                String::new(),
                format!("cannot start: the daemon with key {key} still runs\n"),
            ),
        ] {
            let server = TestServer::start(Behaviour {
                address: Some((3, Vec::new(), b"not running".to_vec())),
                action: Some((1, stdout.clone().into_bytes(), stderr.clone().into_bytes())),
                ..Behaviour::default()
            })
            .await;
            match outcome(&entry(&server), &request(Some(Action::Start))).await {
                Err(ConnectOutcome::ActionFailed { action, detail }) => {
                    assert_eq!(action, Action::Start);
                    assert!(!detail.contains(key), "{detail}");
                    assert!(!detail.is_empty());
                }
                other => panic!("expected the start to fail, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn no_outcome_handed_to_a_page_repeats_the_owner_key() {
        let key = "deadbeef0123456789";
        let outcomes = [
            // A daemon of another generation.
            format!(
                "{{\"owner_key\":\"{key}\",\"generation\":{},\"daemon_version\":\"9\"}}",
                octoboardd::LOCAL_GENERATION + 1
            ),
            // An answer the application cannot read.
            format!("{{\"owner_key\":\"{key}\",\"generation\":\"x\"}}"),
            // The address a browser opens, from a daemon that does not know `--json`.
            format!("http://127.0.0.1:3369/?key={key}"),
        ];
        for printed in outcomes {
            let server = TestServer::start(Behaviour {
                address: Some((0, printed.clone().into_bytes(), Vec::new())),
                ..Behaviour::default()
            })
            .await;
            let refused = outcome(&entry(&server), &request(None))
                .await
                .expect_err("none of these opens a window");
            let handed = serde_json::to_string(&refused).unwrap();
            assert!(!handed.contains(key), "{printed} leaked into {handed}");
        }
    }

    #[test]
    fn a_window_is_pointed_at_a_new_key_by_the_key_parameter_alone() {
        // Another parameter that merely ends or starts like `key` is left as it is.
        assert_eq!(
            with_key("monkey=1&key=aa&keys=2", "bb"),
            "monkey=1&key=bb&keys=2"
        );
        assert_eq!(with_key("port=1", "bb"), "port=1");
        assert_eq!(with_key("", "bb"), "");
        assert_eq!(with_key("key=aa", "bb"), "key=bb");
    }

    #[test]
    fn the_quiet_request_gives_nothing_of_the_owner_s() {
        let quiet = quiet_request("e");
        assert_eq!(quiet.id, "e");
        assert!(quiet.secret.is_none() && quiet.confirm_fingerprint.is_none());
        assert!(quiet.action.is_none() && !quiet.browser);
        // It carries no answer about this device's key, and ends with the question where the
        // owner has to be asked; a connection made at launch goes on without the key instead.
        assert!(quiet.device_key.is_none() && !quiet.without_asking);
        let restoring = restoring_request("e");
        assert!(restoring.without_asking && restoring.device_key.is_none());
        assert!(restoring.secret.is_none() && restoring.action.is_none());
    }

    #[test]
    fn the_owner_is_asked_about_the_device_key_once_and_their_answer_is_done() {
        let password = Secret::Password("pw".into());
        let key_file = Secret::Key {
            path: "/k".into(),
            passphrase: None,
        };
        let device = Secret::DeviceKey(Arc::new(device_key::generate().unwrap()));
        let entry = plain_entry(entries::Auth::Password);
        let answered = |answer| ConnectRequest {
            device_key: answer,
            ..asking(Some("pw"), None)
        };
        // A sign-in with the owner's own password or key file asks first.
        for secret in [&password, &key_file] {
            assert_eq!(key_step(&entry, &answered(None), secret), KeyStep::Ask);
            assert_eq!(
                key_step(&entry, &answered(Some(true)), secret),
                KeyStep::Install
            );
            assert_eq!(
                key_step(&entry, &answered(Some(false)), secret),
                KeyStep::Decline
            );
        }
        // A no is remembered: nothing is asked and nothing installed.
        let mut declined = entry.clone();
        declined.device_key.declined = true;
        assert_eq!(
            key_step(&declined, &answered(None), &password),
            KeyStep::Nothing
        );
        // A connection made at launch asks nothing, and installs nothing.
        assert_eq!(
            key_step(&entry, &restoring_request("e"), &key_file),
            KeyStep::Nothing
        );
        // A connection signed in with the key has it installed already, whatever it was sent.
        for answer in [None, Some(true), Some(false)] {
            assert_eq!(
                key_step(&entry, &answered(answer), &device),
                KeyStep::Nothing
            );
        }
        // A device key entry signed in to with the password, its key refused or lost, is asked
        // again like any other.
        let mut refused = plain_entry(entries::Auth::Device);
        assert_eq!(key_step(&refused, &answered(None), &password), KeyStep::Ask);
        // One whose no was noted while its key was still installed has the key taken back by the
        // next sign-in with the password, with nothing asked.
        refused.device_key.declined = true;
        assert_eq!(
            key_step(&refused, &answered(None), &password),
            KeyStep::Decline
        );
        assert_eq!(
            key_step(&refused, &answered(None), &device),
            KeyStep::Nothing
        );
    }

    #[test]
    fn a_daemon_is_found_by_its_identity_whichever_entry_connected_it() {
        // The first connection has let go of its marker and holds the daemon for a window still
        // queued to be built: a second request (a restore, the menu, the jump list) is shown that
        // window, and makes no connection of its own. No window exists anywhere here.
        let state = RemoteState::default();
        let (held, _) = live("ssh:owner@host:22", "first", 0, Via::Window);
        state.windows().insert("daemon-1".into(), held);
        // A second entry naming the same destination finds the window the first one holds the
        // connection for.
        let second = Entry {
            id: "second".into(),
            name: "again".into(),
            host: "host".into(),
            port: 22,
            user: "owner".into(),
            auth: entries::Auth::Password,
            key_path: None,
            fingerprint: None,
            octoboardd_path: None,
            device_key: Default::default(),
        };
        assert_eq!(second.identity(), "ssh:owner@host:22");
        assert_eq!(
            showing(&state, &second.identity()),
            Some(Showing::Window("daemon-1".into()))
        );
        assert_eq!(showing(&state, "ssh:owner@other:22"), None);
        // A connection held for the browser is shown in the browser again.
        let browser = RemoteState::default();
        let (held, _) = live(
            "ssh:owner@host:22",
            "e",
            0,
            Via::Browser {
                url: "http://127.0.0.1:1/".into(),
            },
        );
        browser.windows().insert("browser-e".into(), held);
        assert_eq!(
            showing(&browser, "ssh:owner@host:22"),
            Some(Showing::Browser("http://127.0.0.1:1/".into()))
        );
    }

    #[tokio::test]
    async fn a_second_connection_to_a_daemon_waits_for_the_one_being_made() {
        let connecting = Connecting::default();
        let first = connecting.begin("ssh:u@h:22").ok().unwrap();
        // Another daemon is not held up.
        let other = connecting.begin("ssh:u@other:22");
        assert!(other.is_ok());
        let Err(mut over) = connecting.begin("ssh:u@h:22") else {
            panic!("a second connection began while the first was being made");
        };
        assert!(!*over.borrow());
        let waiting = connecting.wait_and_begin("ssh:u@h:22");
        tokio::pin!(waiting);
        // Still waiting while the first is under way.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut waiting)
                .await
                .is_err()
        );
        drop(first);
        assert!(*over.borrow_and_update());
        // Once the first is over, the second goes ahead, and holds the daemon in turn.
        let second = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("the second goes ahead once the first is over");
        assert!(connecting.begin("ssh:u@h:22").is_err());
        drop(second);
        assert!(connecting.begin("ssh:u@h:22").is_ok());
    }

    #[tokio::test]
    async fn a_host_that_stops_answering_a_command_is_unreachable_rather_than_waited_on_forever() {
        let server = TestServer::start(Behaviour {
            hang: vec!["'octoboardd' address --json".into()],
            ..Behaviour::default()
        })
        .await;
        match outcome(&entry(&server), &request(None)).await {
            Err(ConnectOutcome::Unreachable { detail }) => {
                assert!(detail.contains("did not answer"), "{detail}")
            }
            other => panic!("{other:?}"),
        }
    }
}
