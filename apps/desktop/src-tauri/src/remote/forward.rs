//! The loopback ports of the remote machine a remote window opens in this computer's browser: an
//! address on the remote machine's own loopback (`http://localhost:3000/`, as a development server
//! prints it) names nothing on this computer, so opening one listens on a loopback port here and
//! carries every connection made to it over the window's SSH connection (`direct-tcpip`) to the same
//! port of the remote machine's loopback, and the browser is opened at that port.
//!
//! Only a loopback address is ever forwarded (`target`), and only to the remote machine's own
//! loopback: no address makes the application connect onward to a third host. The forward listens on
//! this computer's `127.0.0.1` and `::1` and nothing else. One forward is held for each remote port of
//! a window, until it is stopped or the window closes; a dropped SSH connection leaves it in place,
//! since a `direct-tcpip` channel has no state on the host to re-establish, and the connections made
//! through it meanwhile fail until the connection is back.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::Msg;
use russh::{Channel, ChannelStream};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use super::ssh::{self, ForwardError};
use super::tunnel::Link;
use crate::diagnostics::diagnostic;

/// How long the remote port is tried for at each of its addresses before the browser opens.
#[cfg(not(test))]
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Longer under test than a test waits for anything (`PATIENCE`): where a test's host holds one
/// address's answer back, an opening that waits for that answer is then seen not to end, rather
/// than to end at this limit.
#[cfg(test)]
const PROBE_TIMEOUT: Duration = crate::test_support::PATIENCE.saturating_mul(2);

/// How long a connection to a port of this computer is waited for when finding out whether anything
/// answers there.
const LOCAL_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// How many ports the system is asked for before a free `127.0.0.1` and `::1` pair is given up on.
const BIND_TRIES: usize = 8;

/// The remote machine's loopback addresses a name or an any-address is tried at, in this order. Not `localhost`:
/// Win32-OpenSSH connects only to the first address it resolves the name to (`::1` on Windows), so a server
/// listening on `127.0.0.1` alone would not be reached. `127.0.0.1` first, where most servers listen.
const EITHER_FAMILY: [&str; 2] = ["127.0.0.1", "::1"];

/// An address on the remote machine's own loopback.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub url: tauri::Url,
    /// The port on the remote machine: the one written, or the scheme's own.
    pub port: u16,
    /// The loopback addresses of the remote machine its connections may go to: the address as written for
    /// `127.0.0.1` and `::1`, so that a server on one family is not mistaken for one on the other, and both
    /// (`EITHER_FAMILY`) for the names and the any-addresses.
    pub hosts: &'static [&'static str],
}

/// Whether `host` (as `Url::host_str` gives it) is one that names the machine's own loopback: `localhost`,
/// a name ending in `.localhost`, `127.0.0.1`, `[::1]`, and the any-address `0.0.0.0` and `[::]` that
/// servers print for every interface. Compared without regard to case, a trailing dot ignored; the rest of
/// `127.0.0.0/8`, a name that merely resolves to loopback and the remote machine's own name are not.
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || matches!(host.as_str(), "127.0.0.1" | "[::1]" | "0.0.0.0" | "[::]")
}

/// Whether `host` is an any-address, which a browser does not reach on every system.
fn is_any_host(host: &str) -> bool {
    matches!(host, "0.0.0.0" | "[::]")
}

/// The address `url` names on the remote machine's loopback, or `None` when it is not one: an `http` or
/// `https` address with a loopback host (see `is_loopback_host`) and a port other than 0, written or the
/// scheme's own. The one place that says what loopback is.
pub fn target(url: &str) -> Option<Target> {
    let url = tauri::Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !is_loopback_host(url.host_str()?) {
        return None;
    }
    let port = url.port_or_known_default().filter(|port| *port != 0)?;
    let hosts: &'static [&'static str] = match url.host_str()?.trim_end_matches('.') {
        "127.0.0.1" => &["127.0.0.1"],
        "[::1]" => &["::1"],
        _ => &EITHER_FAMILY,
    };
    Some(Target { url, port, hosts })
}

/// The address `target` is opened at when its forward listens on `local`: the port replaced, an
/// any-address host replaced by `localhost`, everything else (user information, path, query and
/// fragment) kept.
pub fn rewritten(target: &Target, local: u16) -> String {
    let mut url = target.url.clone();
    if url.host_str().is_some_and(is_any_host) {
        let _ = url.set_host(Some("localhost"));
    }
    if target.port != local {
        let _ = url.set_port(Some(local));
    }
    url.to_string()
}

/// `address` without the user name and password it may carry, which are not kept or listed.
fn without_credentials(address: &str) -> String {
    match tauri::Url::parse(address) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.to_string()
        }
        Err(_) => address.to_owned(),
    }
}

/// Why an address could not be forwarded, as the page is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// Nothing on the remote machine listens on the port.
    NothingListening,
    /// The remote machine's SSH server does not allow forwarding.
    ForwardingRefused,
    /// The SSH connection is down.
    Disconnected,
    /// Anything else: a port of this computer that could not be listened on, a remote port that did not
    /// answer in time.
    Failed,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub kind: FailureKind,
    /// The port on the remote machine.
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Failure {
    pub fn disconnected(port: u16) -> Self {
        Self::new(FailureKind::Disconnected, port)
    }

    fn new(kind: FailureKind, port: u16) -> Self {
        Self {
            kind,
            port,
            detail: None,
        }
    }

    fn failed(port: u16, detail: impl std::fmt::Display) -> Self {
        Self {
            kind: FailureKind::Failed,
            port,
            detail: Some(detail.to_string()),
        }
    }

    /// What a forward that failed with `err` over `session` is told as.
    fn forwarding(port: u16, err: ForwardError, session: &ssh::Session) -> Self {
        match err {
            ForwardError::NothingListening => Self::new(FailureKind::NothingListening, port),
            ForwardError::Refused => Self::new(FailureKind::ForwardingRefused, port),
            ForwardError::TimedOut(_) | ForwardError::Ssh(_) if session.is_closed() => {
                Self::new(FailureKind::Disconnected, port)
            }
            ForwardError::TimedOut(limit) => Self::failed(
                port,
                format!("the host did not answer within {} seconds", limit.as_secs()),
            ),
            ForwardError::Ssh(err) => Self::failed(port, err),
        }
    }
}

/// A forward, as the page lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardInfo {
    pub remote_port: u16,
    pub local_port: u16,
    /// The address last opened through the forward, as it was written.
    pub address: String,
    /// What the browser was given for it.
    pub local_address: String,
}

/// Where on the remote machine a forward's connections go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Route {
    /// The loopback addresses of the address last opened through the forward (`Target::hosts`).
    hosts: &'static [&'static str],
    /// The one of them that last answered, tried first.
    answered: &'static str,
}

impl Route {
    /// `hosts` in the order they are tried: `first` ahead of the rest where it is one of them.
    fn order(hosts: &'static [&'static str], first: Option<&str>) -> Vec<&'static str> {
        let mut order = hosts.to_vec();
        if let Some(at) = first.and_then(|first| order.iter().position(|host| *host == first)) {
            let first = order.remove(at);
            order.insert(0, first);
        }
        order
    }
}

/// A forward held: its listeners run until `stop` is set or the window's stop is.
struct Held {
    info: ForwardInfo,
    /// Where its connections go, shared with its listeners.
    route: Arc<Mutex<Route>>,
    stop: watch::Sender<bool>,
}

/// The first of `hosts` found to have something on the remote machine listening on `port`: all are tried
/// (`Session::probe`) at once, since a Windows host takes its own connect attempt's time (a few seconds where
/// nothing listens) to tell, and one after the other would add those up. `windows` says the host runs Windows. A
/// try still under way when another has answered runs on by itself, closing what it opened. Where none answers,
/// a failure other than nothing listening (forwarding refused, the connection gone) is told before that.
async fn probe(
    session: &Arc<ssh::Session>,
    hosts: &[&'static str],
    port: u16,
    windows: bool,
) -> Result<&'static str, ForwardError> {
    let (told, mut results) = tokio::sync::mpsc::unbounded_channel();
    for &host in hosts {
        let (session, told) = (session.clone(), told.clone());
        tokio::spawn(async move {
            let _ = told.send((
                host,
                session.probe(host, port, PROBE_TIMEOUT, windows).await,
            ));
        });
    }
    drop(told);
    let mut failure = ForwardError::NothingListening;
    while let Some((host, result)) = results.recv().await {
        match result {
            Ok(()) => return Ok(host),
            Err(ForwardError::NothingListening) => {}
            Err(err) => failure = err,
        }
    }
    Err(failure)
}

/// The forwards of one remote window, over its `Link`, in the order they were made.
pub struct Forwards {
    link: Arc<Link>,
    /// Set when the window closes: every forward ends with it.
    stopped: watch::Receiver<bool>,
    /// Called when a forward is made, when the address last opened through one changes, and when one is
    /// stopped.
    on_change: Box<dyn Fn() + Send + Sync>,
    /// One lock for each remote port, held while an address is opened at it, so two clicks on one port make one
    /// forward while another port's opening does not wait.
    opening: Mutex<std::collections::HashMap<u16, Arc<tokio::sync::Mutex<()>>>>,
    held: Mutex<Vec<Held>>,
}

impl Forwards {
    pub fn new(
        link: Arc<Link>,
        stopped: watch::Receiver<bool>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            link,
            stopped,
            on_change: Box::new(on_change),
            opening: Mutex::default(),
            held: Mutex::new(Vec::new()),
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Vec<Held>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The address to open `address` (which `target` read) at in this computer's browser, once the port
    /// is forwarded. The remote port is tried first, so that a port nothing listens on, a host that
    /// forbids forwarding and a connection that is down are told apart before anything opens; the
    /// connection is not made again for it.
    pub async fn open(&self, address: &str, target: &Target) -> Result<String, Failure> {
        let remote = target.port;
        let lock = self
            .opening
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(remote)
            .or_default()
            .clone();
        let opened = {
            let _opening = lock.lock().await;
            self.open_locked(address, target).await
        };
        drop(lock);
        // A port that did not come to be forwarded keeps no lock of its own behind.
        if opened.is_err() {
            self.prune_opening(remote);
        }
        opened
    }

    /// `open` with the port's opening lock held.
    async fn open_locked(&self, address: &str, target: &Target) -> Result<String, Failure> {
        let remote = target.port;
        let session = self
            .link
            .current()
            .ok_or_else(|| Failure::disconnected(remote))?;
        let answered = probe(&session, target.hosts, remote, self.link.host_is_windows())
            .await
            .map_err(|err| Failure::forwarding(remote, err, &session))?;
        let route = Route {
            hosts: target.hosts,
            answered,
        };
        let shown = without_credentials(address);
        // A forward stopped between finding it and updating it is made again.
        for _ in 0..3 {
            let existing = self
                .held()
                .iter()
                .find_map(|held| (held.info.remote_port == remote).then_some(held.info.local_port));
            let Some(local) = existing else {
                let local = self.listen(remote, address, target, route).await?;
                (self.on_change)();
                return Ok(rewritten(target, local));
            };
            let local_address = rewritten(target, local);
            let changed = {
                let mut held = self.held();
                let Some(held) = held.iter_mut().find(|held| held.info.remote_port == remote)
                else {
                    continue;
                };
                *held.route.lock().unwrap_or_else(|e| e.into_inner()) = route;
                let shown_local = without_credentials(&local_address);
                let changed = held.info.address != shown || held.info.local_address != shown_local;
                held.info.address = shown;
                held.info.local_address = shown_local;
                changed
            };
            if changed {
                (self.on_change)();
            }
            return Ok(local_address);
        }
        Err(Failure::failed(
            remote,
            "the forward was stopped while it was opened",
        ))
    }

    /// Listens on this computer for `remote` and starts carrying what comes in, holding the forward with its
    /// addresses complete; the port it listens on.
    async fn listen(
        &self,
        remote: u16,
        address: &str,
        target: &Target,
        route: Route,
    ) -> Result<u16, Failure> {
        let (listeners, local) = bind(remote).await.map_err(|err| {
            Failure::failed(remote, format!("cannot listen on a loopback port: {err}"))
        })?;
        let (stop, stopped) = watch::channel(false);
        let route = Arc::new(Mutex::new(route));
        for listener in listeners {
            tokio::spawn(accept(
                listener,
                remote,
                route.clone(),
                self.link.clone(),
                self.stopped.clone(),
                stopped.clone(),
            ));
        }
        self.held().push(Held {
            info: ForwardInfo {
                remote_port: remote,
                local_port: local,
                address: without_credentials(address),
                local_address: without_credentials(&rewritten(target, local)),
            },
            route,
            stop,
        });
        Ok(local)
    }

    /// Drops the opening lock of `remote` unless a forward of the port is held (the lock goes with it) or an opening
    /// still holds or waits for the lock.
    fn prune_opening(&self, remote: u16) {
        if self
            .held()
            .iter()
            .any(|held| held.info.remote_port == remote)
        {
            return;
        }
        let mut opening = self.opening.lock().unwrap_or_else(|e| e.into_inner());
        if opening
            .get(&remote)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            opening.remove(&remote);
        }
    }

    /// The forwards held, in the order they were made.
    pub fn list(&self) -> Vec<ForwardInfo> {
        self.held().iter().map(|held| held.info.clone()).collect()
    }

    /// Stops the forward of `remote`: its port is let go and the connections passing through it are cut.
    pub fn stop(&self, remote: u16) {
        let stopped = {
            let mut held = self.held();
            held.iter()
                .position(|held| held.info.remote_port == remote)
                .map(|at| held.remove(at))
        };
        self.prune_opening(remote);
        if let Some(held) = stopped {
            let _ = held.stop.send(true);
            (self.on_change)();
        }
    }
}

/// Whether anything on this computer answers at `address`.
async fn answers(address: SocketAddr) -> bool {
    matches!(
        tokio::time::timeout(LOCAL_PROBE_TIMEOUT, TcpStream::connect(address)).await,
        Ok(Ok(_))
    )
}

/// Binds `::1` at `port`: `None` where the system has no IPv6 loopback, which is the one case it is left out.
fn bind_v6(port: u16) -> std::io::Result<Option<std::net::TcpListener>> {
    match std::net::TcpListener::bind((Ipv6Addr::LOCALHOST, port)) {
        Ok(listener) => Ok(Some(listener)),
        Err(err) if err.kind() == std::io::ErrorKind::AddrNotAvailable => Ok(None),
        Err(err) => Err(err),
    }
}

/// Binds `127.0.0.1` and `::1` at `port`, 0 for the system's pick (the same for both).
fn bind_pair(port: u16) -> std::io::Result<(Vec<std::net::TcpListener>, u16)> {
    let v4 = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    let port = v4.local_addr()?.port();
    let mut listeners = vec![v4];
    listeners.extend(bind_v6(port)?);
    Ok((listeners, port))
}

/// Listens on `127.0.0.1` and `::1` at the port of the remote machine's own number where nothing on this
/// computer answers at it and both can be bound (a connection is tried first, because a bind alone does
/// not show that nothing is there: a local server on `::1` alone always lets a `127.0.0.1` bind succeed,
/// and on some systems so does one on every interface), else at a port the system gives.
async fn bind(remote: u16) -> std::io::Result<(Vec<TcpListener>, u16)> {
    let answered = answers((Ipv4Addr::LOCALHOST, remote).into()).await
        || answers((Ipv6Addr::LOCALHOST, remote).into()).await;
    let mut bound = if answered {
        None
    } else {
        bind_pair(remote).ok()
    };
    let mut last = None;
    for _ in 0..BIND_TRIES {
        if bound.is_some() {
            break;
        }
        match bind_pair(0) {
            Ok(pair) => bound = Some(pair),
            Err(err) => last = Some(err),
        }
    }
    let (listeners, port) = bound.ok_or_else(|| {
        last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AddrInUse))
    })?;
    let listeners = listeners
        .into_iter()
        .map(|listener| {
            listener.set_nonblocking(true)?;
            TcpListener::from_std(listener)
        })
        .collect::<std::io::Result<_>>()?;
    Ok((listeners, port))
}

/// Accepts the connections made to `listener` and carries each to `remote` over `link`, until the window
/// closes (`window`) or the forward is stopped (`own`); the port is let go then.
async fn accept(
    listener: TcpListener,
    remote: u16,
    route: Arc<Mutex<Route>>,
    link: Arc<Link>,
    mut window: watch::Receiver<bool>,
    mut own: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    tokio::spawn(carry(stream, remote, route.clone(), link.clone(), window.clone(), own.clone()));
                }
                Err(err) => {
                    diagnostic!("a forwarded port failed to accept: {err}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            _ = window.changed() => break,
            _ = own.changed() => break,
        }
    }
}

/// Carries `stream` to `remote` until either side closes it, or the window or the forward ends. The
/// connection is made over the SSH connection as it is now, which a window that lost it makes again,
/// to the address of the route that answered last, then to its others where nothing listens there
/// (remembering the one that took it): where the host refuses the channel so, and on a Windows host
/// also where it closes the channel at the start (`start_on_windows`).
async fn carry(
    mut stream: TcpStream,
    remote: u16,
    route: Arc<Mutex<Route>>,
    link: Arc<Link>,
    mut window: watch::Receiver<bool>,
    mut own: watch::Receiver<bool>,
) {
    let Some(session) = link.session().await else {
        return;
    };
    #[cfg(test)]
    let from = stream.peer_addr().ok();
    let tried = *route.lock().unwrap_or_else(|e| e.into_inner());
    let windows = link.host_is_windows();
    let opening = async {
        let mut start = Start::default();
        for host in Route::order(tried.hosts, Some(tried.answered)) {
            #[cfg(test)]
            if let Some(from) = from {
                let mut tries = link.forward_tries.lock().unwrap_or_else(|e| e.into_inner());
                tries.push((from, host));
            }
            let channel = match session.forward(host, remote, ssh::COMMAND_TIMEOUT).await {
                Ok(channel) => channel,
                Err(ForwardError::NothingListening) => continue,
                Err(err) => {
                    diagnostic!("cannot forward a connection to port {remote}: {err:?}");
                    return None;
                }
            };
            if !windows {
                return Some((host, channel.into_stream()));
            }
            match start_on_windows(&mut stream, channel, &mut start).await {
                Ok(Some(channel)) => return Some((host, channel)),
                Ok(None) => {}
                // The browser's side failed: there is nothing left to carry.
                Err(_) => return None,
            }
        }
        diagnostic!("nothing on the remote machine took a connection to port {remote}");
        None
    };
    let opened = tokio::select! {
        opened = opening => opened,
        _ = window.changed() => return,
        _ = own.changed() => return,
    };
    let Some((host, mut channel)) = opened else {
        return;
    };
    {
        let mut route = route.lock().unwrap_or_else(|e| e.into_inner());
        if host != tried.answered && route.hosts == tried.hosts {
            route.answered = host;
        }
    }
    tokio::select! {
        _ = tokio::io::copy_bidirectional(&mut stream, &mut channel) => {}
        _ = window.changed() => {}
        _ = own.changed() => {}
    }
}

/// The most of a connection's first bytes kept to send again to the next address, should the host close the
/// channel they went to at the start. A connection that sends more within `ssh::WINDOWS_EARLY_CLOSE` stays where
/// it is.
const RESENT_AT_MOST: usize = 64 * 1024;

/// What a connection to a Windows host has sent so far, for the next address to be sent it again.
#[derive(Default)]
struct Start {
    sent: Vec<u8>,
    /// Whether the browser has finished sending.
    ended: bool,
}

/// Starts carrying `stream` through `channel` to a Windows host, whose SSH server confirms a channel to where
/// nothing listens and then closes it: the channel that closes within `ssh::WINDOWS_EARLY_CLOSE` with nothing
/// sent back is `None`, for the next address, which is sent what this one was (`start`). The browser's bytes are
/// passed on at once, not held for that long. The channel, once anything came back, the window passed, or more
/// than `RESENT_AT_MOST` was sent; an error is the browser's side failing.
async fn start_on_windows(
    stream: &mut TcpStream,
    channel: Channel<Msg>,
    start: &mut Start,
) -> std::io::Result<Option<ChannelStream<Msg>>> {
    let mut channel = channel.into_stream();
    if channel.write_all(&start.sent).await.is_err() {
        return Ok(None);
    }
    let deadline = tokio::time::Instant::now() + ssh::WINDOWS_EARLY_CLOSE;
    let (mut from_browser, mut from_host) = (vec![0; 16 * 1024], vec![0; 16 * 1024]);
    loop {
        tokio::select! {
            read = stream.read(&mut from_browser), if !start.ended => {
                let read = read?;
                if read == 0 {
                    start.ended = true;
                    continue;
                }
                start.sent.extend_from_slice(&from_browser[..read]);
                if channel.write_all(&from_browser[..read]).await.is_err() {
                    return Ok(None);
                }
                if start.sent.len() > RESENT_AT_MOST {
                    return Ok(Some(channel));
                }
            }
            read = channel.read(&mut from_host) => match read {
                Ok(0) | Err(_) => return Ok(None),
                Ok(read) => {
                    stream.write_all(&from_host[..read]).await?;
                    return Ok(Some(channel));
                }
            },
            _ = tokio::time::sleep_until(deadline) => return Ok(Some(channel)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::octoboardd::{Remote, Shell};
    use super::super::ssh::test_server::{Behaviour, Refusal, TestServer};
    use super::super::ssh::{Secret, Target as SshTarget};
    use super::*;
    use crate::test_support::{is_port_probe, port_probed, ClosedPort, PATIENCE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn loopback(url: &str) -> Option<(String, u16)> {
        target(url).map(|target| (target.url.to_string(), target.port))
    }

    #[test]
    fn only_a_loopback_address_with_a_web_scheme_is_a_target() {
        for (url, port) in [
            ("http://localhost:3000/", 3000),
            ("http://LOCALHOST:3000/", 3000),
            ("http://localhost.:3000/", 3000),
            ("http://app.localhost:8080/x", 8080),
            ("http://127.0.0.1:5173/", 5173),
            ("https://[::1]:8443/", 8443),
            ("http://0.0.0.0:4000/", 4000),
            ("http://[::]:4000/", 4000),
            ("http://localhost/", 80),
            ("https://localhost/", 443),
            ("http://user:pw@localhost:9/", 9),
        ] {
            assert_eq!(loopback(url).map(|(_, port)| port), Some(port), "{url}");
        }
        for url in [
            "http://127.0.0.2:3000/",
            "http://example.com:3000/",
            "http://localhost.example.com/",
            "http://notlocalhost:3000/",
            "http://localhost:0/",
            "ftp://localhost:21/",
            "mailto:a@localhost",
            "file:///tmp/x",
            "not a url",
        ] {
            assert_eq!(loopback(url), None, "{url}");
        }
    }

    #[test]
    fn the_port_is_replaced_and_an_any_address_becomes_localhost_while_the_rest_is_kept() {
        let rewrite = |url: &str, local: u16| rewritten(&target(url).unwrap(), local);
        assert_eq!(
            rewrite("http://localhost:3000/a/b?q=1#top", 4000),
            "http://localhost:4000/a/b?q=1#top"
        );
        assert_eq!(
            rewrite("http://user:pw@127.0.0.1:3000/", 3000),
            "http://user:pw@127.0.0.1:3000/"
        );
        assert_eq!(
            rewrite("http://0.0.0.0:3000/x", 3000),
            "http://localhost:3000/x"
        );
        assert_eq!(
            rewrite("http://[::]:3000/x", 5000),
            "http://localhost:5000/x"
        );
        assert_eq!(rewrite("http://[::1]:3000/", 5000), "http://[::1]:5000/");
        assert_eq!(
            rewrite("https://localhost/", 7000),
            "https://localhost:7000/"
        );
        assert_eq!(rewrite("http://localhost:3000/", 80), "http://localhost/");
    }

    /// A listener that sends back what it is sent, and its port.
    async fn echo_server() -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut read, mut write) = stream.split();
                    let _ = tokio::io::copy(&mut read, &mut write).await;
                });
            }
        });
        port
    }

    struct Running {
        server: TestServer,
        forwards: Arc<Forwards>,
        link: Arc<Link>,
        stop: watch::Sender<bool>,
        changes: Arc<AtomicUsize>,
    }

    /// A window's forwards over a host that carries `direct-tcpip` channels to `forward_to`.
    async fn run(forward_to: Option<u16>) -> Running {
        run_on(Behaviour {
            forward_to,
            ..Behaviour::default()
        })
        .await
    }

    /// A window's forwards over a host that behaves as `behaviour` says, its shell among it.
    async fn run_on(behaviour: Behaviour) -> Running {
        let shell = behaviour.shell;
        let server = TestServer::start(behaviour).await;
        let target = SshTarget {
            host: "127.0.0.1".into(),
            port: server.port,
            user: "owner".into(),
            secret: Secret::Password("right".into()),
            pinned: Some(server.fingerprint.clone()),
        };
        let session = ssh::connect(&target).await.unwrap();
        let link = Arc::new(Link::new(
            target,
            Remote::new(shell, "octoboardd"),
            session,
            "aa".into(),
            |_| {},
        ));
        let (stop, stopped) = watch::channel(false);
        let changes = Arc::new(AtomicUsize::new(0));
        let counted = changes.clone();
        let forwards = Arc::new(Forwards::new(link.clone(), stopped, move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }));
        Running {
            server,
            forwards,
            link,
            stop,
            changes,
        }
    }

    async fn open(running: &Running, url: &str) -> Result<String, Failure> {
        let target = target(url).unwrap();
        running.forwards.open(url, &target).await
    }

    fn port_of(address: &str) -> u16 {
        tauri::Url::parse(address)
            .unwrap()
            .port_or_known_default()
            .unwrap()
    }

    /// Sends `text` to `address` on `local` and reads what comes back until the end.
    async fn through(address: SocketAddr, text: &[u8]) -> Vec<u8> {
        through_from(address, text).await.0
    }

    /// `through`, and where the connection came from, for `tried_for`.
    async fn through_from(address: SocketAddr, text: &[u8]) -> (Vec<u8>, SocketAddr) {
        let mut stream = TcpStream::connect(address).await.unwrap();
        let from = stream.local_addr().unwrap();
        stream.write_all(text).await.unwrap();
        stream.shutdown().await.unwrap();
        let mut back = Vec::new();
        tokio::time::timeout(PATIENCE, stream.read_to_end(&mut back))
            .await
            .expect("the bytes came back")
            .unwrap();
        (back, from)
    }

    /// The addresses of the remote machine the connection made from `from` was tried at, in order.
    /// A forwarded port takes a connection from anything on the machine
    /// (`crate::test_support::port_probe`) and carries it like any other, so what the host was
    /// asked for after a moment says nothing of one connection: a test reads its own.
    fn tried_for(running: &Running, from: SocketAddr) -> Vec<&'static str> {
        let tries = running.link.forward_tries.lock().unwrap();
        tries
            .iter()
            .filter(|(came_from, _)| *came_from == from)
            .map(|(_, host)| *host)
            .collect()
    }

    /// The addresses the host was asked for at `port`, sorted, each named once however often it
    /// was asked for: which addresses an opening asks for, whatever connections were carried to
    /// the port besides (they go to the same addresses).
    fn asked_for(running: &Running, port: u16) -> Vec<String> {
        let mut hosts: Vec<String> = running
            .server
            .forwarded()
            .into_iter()
            .filter(|(_, asked)| *asked == u32::from(port))
            .map(|(host, _)| host)
            .collect();
        hosts.sort();
        hosts.dedup();
        hosts
    }

    /// Sends the daemon's probe to the forwarded port `local`, as anything on the machine may at
    /// any moment: it is carried to the echo server, which sends its request back.
    async fn stranger(local: u16) {
        assert!(is_port_probe(&port_probed(local).await));
    }

    fn has_ipv6() -> bool {
        std::net::TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).is_ok()
    }

    /// Whether `port` on `127.0.0.1` is let go, waited for until it is or `PATIENCE` has passed. The accept loop that
    /// holds the listeners ends on its own task after a stop, and nothing signals when it has dropped them, so the
    /// bind itself is the condition; a poll returns as soon as it holds, so a passing run waits no longer than it takes.
    async fn port_freed(port: u16) -> bool {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            if std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn bytes_are_carried_both_ways_on_both_loopback_addresses_at_the_remote_port_s_number() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/p"))
            .await
            .unwrap();
        assert_eq!(opened, format!("http://localhost:{remote}/p"));
        stranger(remote).await;
        assert_eq!(
            through((Ipv4Addr::LOCALHOST, remote).into(), b"hello").await,
            b"hello"
        );
        if has_ipv6() {
            assert_eq!(
                through((Ipv6Addr::LOCALHOST, remote).into(), b"six").await,
                b"six"
            );
        }
        // The host was asked for its own loopback at the port, and for nothing else.
        assert!(running.server.forwarded().iter().all(|(host, port)| {
            ["127.0.0.1", "::1"].contains(&host.as_str()) && *port == u32::from(remote)
        }));
    }

    #[tokio::test]
    async fn another_port_is_taken_where_something_here_listens_at_the_number() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        // `echo` listens on `127.0.0.1` here, and is also the remote port asked for.
        let opened = open(&running, &format!("http://localhost:{echo}/"))
            .await
            .unwrap();
        let local = port_of(&opened);
        assert_ne!(local, echo);
        assert_eq!(
            through((Ipv4Addr::LOCALHOST, local).into(), b"x").await,
            b"x"
        );
    }

    #[tokio::test]
    async fn another_port_is_taken_where_something_here_listens_on_the_other_family_alone() {
        if !has_ipv6() {
            return;
        }
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let _held = std::net::TcpListener::bind((Ipv6Addr::LOCALHOST, remote));
        let Ok(_held) = _held else { return };
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        assert_ne!(port_of(&opened), remote);
    }

    #[tokio::test]
    async fn two_openings_of_a_port_make_one_forward_whatever_the_address() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let first = open(&running, &format!("http://localhost:{remote}/a"))
            .await
            .unwrap();
        let second = open(&running, &format!("https://127.0.0.1:{remote}/b?c"))
            .await
            .unwrap();
        assert_eq!(port_of(&first), port_of(&second));
        let list = running.forwards.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].remote_port, remote);
        assert_eq!(list[0].address, format!("https://127.0.0.1:{remote}/b?c"));
        assert_eq!(list[0].local_address, second);
    }

    #[tokio::test]
    async fn nothing_listening_forwarding_refused_and_a_dropped_connection_are_told_apart() {
        let closed = ClosedPort::new();
        let running = run(Some(closed.port())).await;
        let failure = open(&running, "http://localhost:4321/").await.unwrap_err();
        assert_eq!(
            (failure.kind, failure.port),
            (FailureKind::NothingListening, 4321)
        );
        assert!(
            running.forwards.list().is_empty(),
            "nothing is held for a port nothing listens on"
        );

        running.server.set_forward_to(None);
        let failure = open(&running, "http://localhost:4321/").await.unwrap_err();
        assert_eq!(failure.kind, FailureKind::ForwardingRefused);

        let session = running.link.current().unwrap();
        running.server.drop_connections();
        session.closed().await;
        let failure = open(&running, "http://localhost:4321/").await.unwrap_err();
        assert_eq!(failure.kind, FailureKind::Disconnected);
        // Told at once, with no connection made again for it.
        assert_eq!(running.server.connections(), 1);
    }

    #[tokio::test]
    async fn a_forward_carries_again_after_the_connection_is_made_again_with_no_new_call() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let local = port_of(&opened);
        let session = running.link.current().unwrap();
        running.server.drop_connections();
        session.closed().await;
        // Whichever connection comes first has the SSH connection made again: a stranger's here.
        stranger(local).await;
        assert_eq!(
            through((Ipv4Addr::LOCALHOST, local).into(), b"back").await,
            b"back"
        );
        assert_eq!(running.server.connections(), 2);
        assert_eq!(running.forwards.list().len(), 1);
    }

    #[tokio::test]
    async fn the_list_follows_the_order_made_and_says_when_it_changes() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let (closed_a, closed_b) = (ClosedPort::new(), ClosedPort::new());
        let (a, b) = (closed_a.port(), closed_b.port());
        assert_eq!(running.changes.load(Ordering::SeqCst), 0);
        open(&running, &format!("http://localhost:{a}/"))
            .await
            .unwrap();
        open(&running, &format!("http://localhost:{b}/"))
            .await
            .unwrap();
        assert_eq!(running.changes.load(Ordering::SeqCst), 2);
        // The same address again changes nothing; another address at the port does.
        open(&running, &format!("http://localhost:{a}/"))
            .await
            .unwrap();
        assert_eq!(running.changes.load(Ordering::SeqCst), 2);
        open(&running, &format!("http://localhost:{a}/other"))
            .await
            .unwrap();
        assert_eq!(running.changes.load(Ordering::SeqCst), 3);
        let remotes: Vec<u16> = running
            .forwards
            .list()
            .iter()
            .map(|f| f.remote_port)
            .collect();
        assert_eq!(remotes, [a, b]);
        running.forwards.stop(a);
        assert_eq!(running.changes.load(Ordering::SeqCst), 4);
        running.forwards.stop(a);
        assert_eq!(
            running.changes.load(Ordering::SeqCst),
            4,
            "a port not held is nothing to stop"
        );
        let remotes: Vec<u16> = running
            .forwards
            .list()
            .iter()
            .map(|f| f.remote_port)
            .collect();
        assert_eq!(remotes, [b]);
    }

    #[tokio::test]
    async fn the_remote_machine_is_asked_for_an_address_as_written_and_a_name_at_both_families() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let both = ["127.0.0.1", "::1"].as_slice();
        for (url, hosts) in [
            ("http://127.0.0.1", ["127.0.0.1"].as_slice()),
            ("http://[::1]", ["::1"].as_slice()),
            ("http://localhost", both),
            ("http://0.0.0.0", both),
            ("http://[::]", both),
            ("http://app.localhost", both),
        ] {
            let closed = ClosedPort::new();
            let url = format!("{url}:{}/", closed.port());
            let opened = open(&running, &url).await.unwrap();
            // A stranger's connection to the forwarded port goes to the same addresses.
            stranger(port_of(&opened)).await;
            assert_eq!(asked_for(&running, closed.port()), hosts, "{url}");
        }
    }

    #[tokio::test]
    async fn a_name_reaches_a_server_on_ipv6_alone_where_the_host_confirms_then_closes_the_ipv4_channel(
    ) {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["127.0.0.1".into()],
            confirms_then_closes: true,
            shell: Shell::Cmd,
            ..Behaviour::default()
        })
        .await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        assert_eq!(asked_for(&running, remote), ["127.0.0.1", "::1"]);
        // Each connection goes where the server answered: a stranger's, and the one made here.
        let local = port_of(&opened);
        stranger(local).await;
        let (back, from) = through_from((Ipv4Addr::LOCALHOST, local).into(), b"six").await;
        assert_eq!(back, b"six");
        assert_eq!(tried_for(&running, from), ["::1"]);
    }

    #[tokio::test]
    async fn nothing_listening_is_told_where_the_host_confirms_then_closes_every_channel() {
        let closed = ClosedPort::new();
        let running = run_on(Behaviour {
            forward_to: Some(closed.port()),
            confirms_then_closes: true,
            shell: Shell::PowerShell,
            ..Behaviour::default()
        })
        .await;
        let failure = open(&running, "http://localhost:4321/").await.unwrap_err();
        assert_eq!(
            (failure.kind, failure.port),
            (FailureKind::NothingListening, 4321)
        );
        // Nothing comes to be forwarded here, so no port is listened on and the host is asked by
        // these openings alone.
        let asked = |from: usize| running.server.forwarded()[from..].to_vec();
        let mut both = asked(0);
        both.sort();
        assert_eq!(
            both,
            [("127.0.0.1".to_owned(), 4321), ("::1".to_owned(), 4321)]
        );
        // An address as written is tried at that address alone.
        let failure = open(&running, "http://127.0.0.1:4321/").await.unwrap_err();
        assert_eq!(failure.kind, FailureKind::NothingListening);
        assert_eq!(asked(2), [("127.0.0.1".to_owned(), 4321)]);
        assert!(running.forwards.list().is_empty());
    }

    #[tokio::test]
    async fn a_connection_goes_to_the_other_family_once_the_one_that_answered_is_refused() {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["::1".into()],
            ..Behaviour::default()
        })
        .await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let local = (Ipv4Addr::LOCALHOST, port_of(&opened)).into();
        // The server moved to `::1`.
        running.server.set_not_listening_on(&["127.0.0.1"]);
        let (back, from) = through_from(local, b"moved").await;
        assert_eq!(back, b"moved");
        assert_eq!(tried_for(&running, from), ["127.0.0.1", "::1"]);
        stranger(port_of(&opened)).await;
        let (back, from) = through_from(local, b"again").await;
        assert_eq!(back, b"again");
        assert_eq!(tried_for(&running, from), ["::1"]);
    }

    #[tokio::test]
    async fn a_channel_confirmed_then_closed_is_not_waited_for_on_a_host_that_is_not_windows() {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["127.0.0.1".into(), "::1".into()],
            confirms_then_closes: true,
            ..Behaviour::default()
        })
        .await;
        // OpenSSH on Unix refuses a channel to where nothing listens, so a confirmed one is taken as it is: no wait
        // for the close tells this host apart from a Windows one, which says nothing listens.
        let closed = ClosedPort::new();
        open(&running, &format!("http://localhost:{}/", closed.port()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn nothing_listening_on_either_family_is_told_after_one_refusal_s_time_not_two() {
        for (shell, confirms_then_closes) in [(Shell::Posix, false), (Shell::Cmd, true)] {
            let closed = ClosedPort::new();
            // The host takes its time over each address: it answers once both wait for it.
            let running = run_on(Behaviour {
                forward_to: Some(closed.port()),
                not_listening_on: vec!["127.0.0.1".into(), "::1".into()],
                refusal: Refusal::Together(2),
                confirms_then_closes,
                shell,
                ..Behaviour::default()
            })
            .await;
            let opened = open(&running, "http://localhost:4321/").await;
            assert_eq!(
                opened.unwrap_err().kind,
                FailureKind::NothingListening,
                "{shell:?}"
            );
            // Both addresses were asked for at once, so the host's time was waited out once: an
            // opening that asks for one after the other never has two waiting.
            assert_eq!(running.server.most_refusals_waiting(), 2, "{shell:?}");
        }
    }

    #[tokio::test]
    async fn a_name_answered_at_one_family_does_not_wait_for_the_other_to_be_refused() {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["127.0.0.1".into()],
            refusal: Refusal::Held,
            confirms_then_closes: true,
            shell: Shell::Cmd,
            ..Behaviour::default()
        })
        .await;
        let closed = ClosedPort::new();
        // The host never says that nothing listens on `127.0.0.1` here, and the opening's own limit
        // for an address is longer than this wait (`PROBE_TIMEOUT`): the address is opened only if
        // that answer is not waited for.
        let url = format!("http://localhost:{}/", closed.port());
        let opened = tokio::time::timeout(PATIENCE, open(&running, &url))
            .await
            .expect("opened with the other family's answer still to come");
        running.server.refuse();
        // `::1` answered first, and is where the connections go.
        let local = port_of(&opened.unwrap());
        stranger(local).await;
        let (back, from) = through_from((Ipv4Addr::LOCALHOST, local).into(), b"first").await;
        assert_eq!(back, b"first");
        assert_eq!(tried_for(&running, from), ["::1"]);
    }

    #[tokio::test]
    async fn a_connection_closed_at_the_start_on_a_windows_host_is_sent_again_to_the_other_family()
    {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["::1".into()],
            confirms_then_closes: true,
            shell: Shell::Cmd,
            ..Behaviour::default()
        })
        .await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let local = (Ipv4Addr::LOCALHOST, port_of(&opened)).into();
        // The server moved to `::1`, or `127.0.0.1`'s channel closes later than the probe watched it for.
        running.server.set_not_listening_on(&["127.0.0.1"]);
        // The bytes sent before the close reach the server at `::1`.
        let (back, from) = through_from(local, b"moved").await;
        assert_eq!(back, b"moved");
        assert_eq!(tried_for(&running, from), ["127.0.0.1", "::1"]);
        stranger(port_of(&opened)).await;
        let (back, from) = through_from(local, b"again").await;
        assert_eq!(back, b"again");
        assert_eq!(tried_for(&running, from), ["::1"]);
    }

    #[tokio::test]
    async fn a_connection_on_a_windows_host_stays_where_it_is_answered() {
        let echo = echo_server().await;
        let running = run_on(Behaviour {
            forward_to: Some(echo),
            not_listening_on: vec!["::1".into()],
            confirms_then_closes: true,
            shell: Shell::Cmd,
            ..Behaviour::default()
        })
        .await;
        let closed = ClosedPort::new();
        let opened = open(&running, &format!("http://localhost:{}/", closed.port()))
            .await
            .unwrap();
        stranger(port_of(&opened)).await;
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port_of(&opened)))
            .await
            .unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut back = [0u8; 4];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"ping");
        stream.write_all(b"pong").await.unwrap();
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"pong");
        let from = stream.local_addr().unwrap();
        assert_eq!(tried_for(&running, from), ["127.0.0.1"]);
    }

    #[test]
    fn the_address_that_answered_is_tried_first_where_it_is_one_of_the_targets() {
        assert_eq!(
            Route::order(&EITHER_FAMILY, Some("::1")),
            ["::1", "127.0.0.1"]
        );
        assert_eq!(Route::order(&EITHER_FAMILY, None), ["127.0.0.1", "::1"]);
        assert_eq!(Route::order(&["127.0.0.1"], Some("::1")), ["127.0.0.1"]);
    }

    #[tokio::test]
    async fn a_user_name_and_password_are_not_kept_or_listed() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://me:secret@localhost:{remote}/x"))
            .await
            .unwrap();
        // The browser is still given them.
        assert!(opened.contains("me:secret@"), "{opened}");
        open(&running, &format!("http://me:secret@localhost:{remote}/y"))
            .await
            .unwrap();
        let list = serde_json::to_string(&running.forwards.list()).unwrap();
        assert!(!list.contains("secret") && !list.contains("me:"), "{list}");
        assert_eq!(
            running.forwards.list()[0].address,
            format!("http://localhost:{remote}/y")
        );
    }

    #[tokio::test]
    async fn a_slow_opening_of_one_port_does_not_hold_up_another() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let (closed_a, closed_b) = (ClosedPort::new(), ClosedPort::new());
        let (a, b) = (closed_a.port(), closed_b.port());
        let lock = running
            .forwards
            .opening
            .lock()
            .unwrap()
            .entry(a)
            .or_default()
            .clone();
        let _held = lock.lock().await;
        let other =
            tokio::time::timeout(PATIENCE, open(&running, &format!("http://localhost:{b}/"))).await;
        assert!(other.expect("another port is not waited for").is_ok());
    }

    #[tokio::test]
    async fn stopping_frees_the_port_and_cuts_a_connection_passing_through() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let local = port_of(&opened);
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, local))
            .await
            .unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut back = [0u8; 4];
        stream.read_exact(&mut back).await.unwrap();
        running.forwards.stop(remote);
        let mut rest = Vec::new();
        let ended = tokio::time::timeout(PATIENCE, stream.read_to_end(&mut rest)).await;
        assert!(ended.is_ok(), "the connection was cut");
        assert!(port_freed(local).await, "the port is let go");
    }

    #[tokio::test]
    async fn stopping_a_forward_drops_its_ports_opening_lock() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let locks = |running: &Running| running.forwards.opening.lock().unwrap().len();
        assert_eq!(locks(&running), 1);
        running.forwards.stop(remote);
        assert_eq!(locks(&running), 0);
    }

    #[tokio::test]
    async fn a_failed_opening_drops_its_ports_opening_lock() {
        // No server behind the link's forwards: nothing answers at the remote port.
        let running = run(None).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let failed = open(&running, &format!("http://localhost:{remote}/")).await;
        assert!(failed.is_err());
        assert_eq!(running.forwards.opening.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn closing_the_window_lets_every_port_go() {
        let echo = echo_server().await;
        let running = run(Some(echo)).await;
        let closed = ClosedPort::new();
        let remote = closed.port();
        let opened = open(&running, &format!("http://localhost:{remote}/"))
            .await
            .unwrap();
        let local = port_of(&opened);
        running.stop.send(true).unwrap();
        assert!(port_freed(local).await, "the port is let go");
    }

    #[test]
    fn the_page_is_told_a_failure_and_a_forward_in_the_shapes_it_reads() {
        let nothing =
            serde_json::to_value(Failure::new(FailureKind::NothingListening, 3000)).unwrap();
        assert_eq!(
            nothing,
            serde_json::json!({ "kind": "nothing_listening", "port": 3000 })
        );
        for (kind, name) in [
            (FailureKind::ForwardingRefused, "forwarding_refused"),
            (FailureKind::Disconnected, "disconnected"),
        ] {
            let told = serde_json::to_value(Failure::new(kind, 80)).unwrap();
            assert_eq!(told, serde_json::json!({ "kind": name, "port": 80 }));
        }
        let failed = serde_json::to_value(Failure::failed(9, "no listener")).unwrap();
        assert_eq!(
            failed,
            serde_json::json!({ "kind": "failed", "port": 9, "detail": "no listener" })
        );
        let info = ForwardInfo {
            remote_port: 3000,
            local_port: 4000,
            address: "http://localhost:3000/".into(),
            local_address: "http://localhost:4000/".into(),
        };
        assert_eq!(
            serde_json::to_value(info).unwrap(),
            serde_json::json!({
                "remotePort": 3000, "localPort": 4000,
                "address": "http://localhost:3000/", "localAddress": "http://localhost:4000/"
            })
        );
    }

    #[test]
    fn the_permission_is_in_the_remote_window_capability_alone() {
        assert!(include_str!("../../capabilities/remote-window.json")
            .contains("\"allow-remote-forward\""));
        for other in [
            include_str!("../../capabilities/default.json"),
            include_str!("../../capabilities/remote.json"),
            include_str!("../../capabilities/flyout.json"),
        ] {
            assert!(!other.contains("allow-remote-forward"));
        }
        let permission = include_str!("../../permissions/remote-forward.toml");
        for command in [
            "remote_link_address",
            "remote_forwards_list",
            "remote_forward_stop",
        ] {
            assert!(permission.contains(&format!("\"{command}\"")), "{command}");
        }
    }
}
