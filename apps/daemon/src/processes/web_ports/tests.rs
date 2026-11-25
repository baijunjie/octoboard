//! Which ports serve web pages: the answers to an injected probe, kept and expiring as set, and
//! the probe itself against servers started on loopback.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::*;
use crate::test_support::{ClosedPort, PATIENCE};

fn tree(id: &str, ports: &[u16]) -> ProcessTree {
    ProcessTree {
        id: id.to_string(),
        session_id: Some("s1".to_string()),
        kept: false,
        command: "node server.js".to_string(),
        cwd: None,
        cwd_missing: false,
        started_at: 0,
        process_count: 1,
        memory_bytes: 0,
        ports: ports.to_vec(),
        http_ports: None,
        keepable: true,
        unkeepable_reason: None,
    }
}

fn http_ports(trees: &[ProcessTree]) -> Vec<Option<Vec<u16>>> {
    trees.iter().map(|tree| tree.http_ports.clone()).collect()
}

/// A probe that answers `answer(port)` and records each port it was asked.
struct Probe {
    asked: Mutex<Vec<u16>>,
}

impl Probe {
    fn new() -> Self {
        Self {
            asked: Mutex::new(Vec::new()),
        }
    }

    fn ask(&self, port: u16, answer: bool) -> bool {
        self.asked.lock().unwrap().push(port);
        answer
    }

    /// The ports asked since the last call, sorted.
    fn take(&self) -> Vec<u16> {
        let mut asked = std::mem::take(&mut *self.asked.lock().unwrap());
        asked.sort_unstable();
        asked
    }
}

#[test]
fn the_ports_that_answer_are_kept_in_the_order_of_the_trees_ports() {
    let web = WebPorts::default();
    let mut trees = vec![
        tree("1:1", &[9000, 3000, 5173]),
        tree("2:2", &[]),
        tree("3:3", &[4000]),
    ];

    web.fill(
        &mut trees,
        |port| port != 5173 && port != 4000,
        Instant::now(),
    );

    assert_eq!(
        http_ports(&trees),
        vec![Some(vec![9000, 3000]), Some(Vec::new()), Some(Vec::new())]
    );
}

#[test]
fn an_answer_is_kept_for_its_time_and_the_port_is_asked_again_after_it() {
    let web = WebPorts::default();
    let probe = Probe::new();
    let start = Instant::now();
    let ms = Duration::from_millis(1);
    // 3000 serves pages, 4000 does not.
    let ask = |port: u16| probe.ask(port, port == 3000);

    let mut first = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut first, ask, start);
    assert_eq!(probe.take(), vec![3000, 4000]);
    assert_eq!(http_ports(&first), vec![Some(vec![3000])]);

    // Within both times, nothing is asked and the answers stand.
    let mut again = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut again, ask, start + UNANSWERED_FOR - ms);
    assert_eq!(probe.take(), Vec::<u16>::new());
    assert_eq!(http_ports(&again), vec![Some(vec![3000])]);

    // Past the shorter time of the port that did not answer, only that port is asked again.
    let mut later = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut later, ask, start + UNANSWERED_FOR + ms);
    assert_eq!(probe.take(), vec![4000]);
    assert_eq!(http_ports(&later), vec![Some(vec![3000])]);

    // The one asked again is kept from when it was asked again, for the same time.
    let mut kept = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut kept, ask, start + UNANSWERED_FOR * 2);
    assert_eq!(probe.take(), Vec::<u16>::new());

    // The positive answer stands to the end of its own time; the other port, whose time ran out
    // meanwhile, is asked.
    let mut nearly = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut nearly, ask, start + ANSWERED_FOR - ms);
    assert_eq!(probe.take(), vec![4000]);

    // Past the time of the positive answer, that port is asked again too.
    let mut expired = vec![tree("1:1", &[3000, 4000])];
    web.fill(&mut expired, ask, start + ANSWERED_FOR + ms);
    assert_eq!(probe.take(), vec![3000]);
    assert_eq!(http_ports(&expired), vec![Some(vec![3000])]);
}

#[test]
fn a_port_that_starts_serving_is_listed_once_its_unanswered_time_is_up() {
    let web = WebPorts::default();
    let start = Instant::now();
    let serving = AtomicBool::new(false);
    let ask = |_: u16| serving.load(Ordering::SeqCst);

    let mut trees = vec![tree("1:1", &[3000])];
    web.fill(&mut trees, ask, start);
    assert_eq!(http_ports(&trees), vec![Some(Vec::new())]);

    serving.store(true, Ordering::SeqCst);
    web.fill(
        &mut trees,
        ask,
        start + UNANSWERED_FOR + Duration::from_millis(1),
    );
    assert_eq!(http_ports(&trees), vec![Some(vec![3000])]);
}

#[test]
fn a_port_is_asked_apart_for_each_tree_that_lists_it() {
    let web = WebPorts::default();
    let probe = Probe::new();
    let start = Instant::now();
    let ask = |port: u16| probe.ask(port, true);

    // Two trees listing one port at once.
    let mut both = vec![tree("1:100", &[3000]), tree("2:200", &[3000])];
    web.fill(&mut both, ask, start);
    assert_eq!(probe.take(), vec![3000, 3000]);
    assert_eq!(http_ports(&both), vec![Some(vec![3000]), Some(vec![3000])]);

    // The same port under another process (a later start of the same pid is another id).
    let mut restarted = vec![tree("1:100", &[3000]), tree("1:300", &[3000])];
    web.fill(&mut restarted, ask, start);
    assert_eq!(probe.take(), vec![3000]);
}

#[test]
fn answers_past_their_time_are_dropped_with_the_next_listing() {
    let web = WebPorts::default();
    let start = Instant::now();
    let mut trees = vec![tree("1:1", &[3000, 4000]), tree("2:2", &[5000])];
    web.fill(&mut trees, |port| port == 3000, start);
    assert_eq!(web.answers.lock().unwrap().len(), 3);

    // Past the time of the negative answers only: the positive one stays.
    web.fill(
        &mut [],
        |_| true,
        start + UNANSWERED_FOR + Duration::from_millis(1),
    );
    assert_eq!(
        web.answers.lock().unwrap().keys().collect::<Vec<_>>(),
        vec![&("1:1".to_string(), 3000)]
    );

    web.fill(
        &mut [],
        |_| true,
        start + ANSWERED_FOR + Duration::from_millis(1),
    );
    assert!(web.answers.lock().unwrap().is_empty());
}

#[test]
fn ports_are_asked_concurrently_and_never_more_than_the_limit_at_once() {
    /// How many probes are in flight, and the most there were.
    struct Flight {
        running: usize,
        most: usize,
        asked: Vec<u16>,
    }
    let flight = Mutex::new(Flight {
        running: 0,
        most: 0,
        asked: Vec::new(),
    });
    let changed = Condvar::new();
    // Every probe holds until `PROBES_AT_ONCE` are in flight together, so a serial probe never
    // gets there and a limit that is too high is seen as a larger peak.
    let ask = |port: u16| {
        let mut now = flight.lock().unwrap();
        now.running += 1;
        now.most = now.most.max(now.running);
        now.asked.push(port);
        changed.notify_all();
        let (mut now, _) = changed
            .wait_timeout_while(now, PATIENCE, |now| now.most < PROBES_AT_ONCE)
            .unwrap();
        now.running -= 1;
        true
    };
    let ports: Vec<u16> = (3000..3000 + 3 * PROBES_AT_ONCE as u16).collect();
    let mut trees = vec![tree("1:1", &ports)];

    WebPorts::default().fill(&mut trees, ask, Instant::now());

    let flight = flight.lock().unwrap();
    assert_eq!(flight.most, PROBES_AT_ONCE);
    let mut asked = flight.asked.clone();
    asked.sort_unstable();
    assert_eq!(asked, ports);
    assert_eq!(trees[0].http_ports, Some(ports));
}

#[test]
fn a_probe_that_is_slow_does_not_hold_back_the_ports_asked_after_it() {
    let ports: Vec<u16> = (3000..3000 + 5 * PROBES_AT_ONCE as u16).collect();
    let slow = ports[0];
    // How many of the other ports have been answered.
    let others = Mutex::new(0usize);
    let changed = Condvar::new();
    let released_by_the_others = AtomicBool::new(false);
    let ask = |port: u16| {
        let mut done = others.lock().unwrap();
        if port == slow {
            // Held until every other port is through, which only happens when they are asked
            // while this one is still in flight.
            let (_unused, waited) = changed
                .wait_timeout_while(done, PATIENCE, |done| *done < ports.len() - 1)
                .unwrap();
            released_by_the_others.store(!waited.timed_out(), Ordering::SeqCst);
        } else {
            *done += 1;
            changed.notify_all();
        }
        true
    };
    let mut trees = vec![tree("1:1", &ports)];

    WebPorts::default().fill(&mut trees, ask, Instant::now());

    assert!(released_by_the_others.load(Ordering::SeqCst));
    assert_eq!(trees[0].http_ports, Some(ports));
}

/// What a [`Server`] does with a connection.
#[derive(Clone, Copy)]
enum Behavior {
    /// Reads the request, writes this, and closes.
    Answer(&'static [u8]),
    /// Reads the request, writes the first part, then the second, and closes.
    AnswerInTwo(&'static [u8], &'static [u8]),
    /// Closes at once, answering nothing.
    Close,
    /// Takes the connection and says nothing, until the server is dropped.
    Silent,
}

/// A server on loopback, in a thread of its own, that stops when dropped.
struct Server {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    wake: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn on_v4(behavior: Behavior) -> Self {
        Self::on(
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
            behavior,
        )
    }

    fn on(listener: TcpListener, behavior: Behavior) -> Self {
        let wake = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (requests, stop) = (Arc::clone(&requests), Arc::clone(&stop));
            std::thread::spawn(move || {
                let mut held = Vec::new();
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    match behavior {
                        Behavior::Close => drop(stream),
                        Behavior::Silent => held.push(stream),
                        Behavior::Answer(reply) => {
                            requests.lock().unwrap().push(read_request(&mut stream));
                            let _ = stream.write_all(reply);
                            let _ = stream.shutdown(Shutdown::Both);
                        }
                        Behavior::AnswerInTwo(first, second) => {
                            requests.lock().unwrap().push(read_request(&mut stream));
                            let _ = stream.set_nodelay(true);
                            let _ = stream.write_all(first);
                            let _ = stream.flush();
                            let _ = stream.write_all(second);
                            let _ = stream.shutdown(Shutdown::Both);
                        }
                    }
                }
            })
        };
        Self {
            port: wake.port(),
            requests,
            stop,
            wake,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accept the thread is blocked in.
        let _ = TcpStream::connect_timeout(&self.wake, PATIENCE);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The request on `stream`, up to its blank line.
fn read_request(stream: &mut TcpStream) -> String {
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    let mut request = Vec::new();
    let mut byte = [0u8];
    while !request.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => request.push(byte[0]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

/// [`answers_http`] on `port`, with the window an answer has under test, run apart so that a probe
/// that never returns fails the test instead of hanging it.
fn answered(port: u16) -> bool {
    answered_within(port, answer_within())
}

/// [`answered`], giving the answer `within`.
fn answered_within(port: u16, within: Duration) -> bool {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(answers_http(port, within));
    });
    // Past the probe's own window, so a probe that uses all of it answers `false` rather than
    // losing the race to this bound.
    receiver
        .recv_timeout(within + PATIENCE)
        .expect("the probe never came back")
}

/// A server on `[::1]` alone, with the `ClosedPort` that keeps its port number refused on
/// `127.0.0.1` (and out of every other test's hands) for as long as the pair lives, since the probe
/// tries `127.0.0.1` first. `None` where the host has no IPv6 loopback.
fn v6_only_server(behavior: Behavior) -> Option<(Server, ClosedPort)> {
    // Ports taken on `[::1]` by something else are kept apart, so the next try is another number.
    let mut skipped = Vec::new();
    for _ in 0..50 {
        let closed = ClosedPort::new();
        match TcpListener::bind((Ipv6Addr::LOCALHOST, closed.port())) {
            Ok(listener) => return Some((Server::on(listener, behavior), closed)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrNotAvailable => return None,
            Err(_) => skipped.push(closed),
        }
    }
    None
}

#[test]
fn a_port_that_answers_with_an_http_status_line_serves_pages() {
    // Any status but 400 and 426: pages, redirects, errors and missing pages are all a server's
    // answer.
    for reply in [
        &b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"[..],
        b"HTTP/1.0 404 Not Found\r\n\r\n",
        b"HTTP/1.1 301 Moved Permanently\r\nLocation: /app\r\n\r\n",
        b"HTTP/1.1 500 Internal Server Error\r\n\r\n",
        b"HTTP/2 200 \r\n\r\n",
    ] {
        let server = Server::on_v4(Behavior::Answer(reply));
        assert!(answered(server.port), "{}", String::from_utf8_lossy(reply));
    }
}

#[test]
fn the_request_is_a_head_for_the_root_with_the_host_a_browser_sends() {
    let server = Server::on_v4(Behavior::Answer(b"HTTP/1.1 200 OK\r\n\r\n"));

    assert!(answered(server.port));

    // A running daemon may ask the port too, with the very same request, so every request the
    // server got is held to the shape and none is counted.
    let requests = server.requests();
    assert!(!requests.is_empty());
    for request in &requests {
        assert!(request.starts_with("HEAD / HTTP/1.1\r\n"), "{requests:?}");
        assert!(
            request.contains(&format!("\r\nHost: localhost:{}\r\n", server.port)),
            "{requests:?}"
        );
    }
}

#[test]
fn a_status_line_that_arrives_in_pieces_still_counts() {
    let server = Server::on_v4(Behavior::AnswerInTwo(b"HTTP/", b"1.1 200 OK\r\n\r\n"));

    assert!(answered(server.port));
}

#[test]
fn a_websocket_server_answering_upgrade_required_serves_no_pages() {
    let server = Server::on_v4(Behavior::Answer(
        b"HTTP/1.1 426 Upgrade Required\r\nUpgrade: websocket\r\n\r\n",
    ));

    assert!(!answered(server.port));
}

#[test]
fn a_debugger_answering_bad_request_to_a_plain_request_serves_no_pages() {
    // Node's inspector, which takes only its own protocol.
    let server = Server::on_v4(Behavior::Answer(
        b"HTTP/1.0 400 Bad Request\r\nContent-Type: text/html\r\n\r\n",
    ));

    assert!(!answered(server.port));
}

#[test]
fn a_port_that_does_not_answer_http_serves_no_pages() {
    let garbage = Server::on_v4(Behavior::Answer(b"SSH-2.0-OpenSSH_9.6\r\n"));
    let closes = Server::on_v4(Behavior::Close);
    // Waits for its own protocol's greeting: bounded by the time an answer may take.
    let silent = Server::on_v4(Behavior::Silent);
    let closed = ClosedPort::new();

    assert!(!answered(garbage.port));
    assert!(!answered(closes.port));
    // The real window, which a silent server always waits out.
    assert!(!answered_within(silent.port, ANSWER_WITHIN));
    assert!(!answered(closed.port()));
}

#[test]
fn a_server_listening_on_ipv6_loopback_alone_serves_pages() {
    let Some((server, _closed)) = v6_only_server(Behavior::Answer(b"HTTP/1.1 200 OK\r\n\r\n"))
    else {
        eprintln!("no IPv6 loopback on this host");
        return;
    };

    assert!(answered(server.port));
}

#[test]
fn a_websocket_server_on_ipv6_loopback_serves_no_pages_either() {
    let Some((server, _closed)) =
        v6_only_server(Behavior::Answer(b"HTTP/1.1 426 Upgrade Required\r\n\r\n"))
    else {
        eprintln!("no IPv6 loopback on this host");
        return;
    };

    assert!(!answered(server.port));
}

#[test]
fn a_connection_the_ipv4_server_takes_is_not_retried_over_ipv6() {
    // The same port on both loopbacks: IPv4 answers something that is not HTTP, IPv6 serves pages.
    // Only a connection that fails goes on to the next address.
    for _ in 0..50 {
        let v4 = Server::on_v4(Behavior::Answer(b"SSH-2.0-OpenSSH_9.6\r\n"));
        let Ok(listener) = TcpListener::bind((Ipv6Addr::LOCALHOST, v4.port)) else {
            continue;
        };
        let _v6 = Server::on(listener, Behavior::Answer(b"HTTP/1.1 200 OK\r\n\r\n"));
        assert!(!answered(v4.port));
        return;
    }
    eprintln!("no port free on both loopbacks, or no IPv6 loopback on this host");
}

#[test]
fn fill_marks_the_ports_that_serve_pages_among_a_trees_listening_ports() {
    let pages = Server::on_v4(Behavior::Answer(b"HTTP/1.1 200 OK\r\n\r\n"));
    let live_reload = Server::on_v4(Behavior::Answer(b"HTTP/1.1 426 Upgrade Required\r\n\r\n"));
    let database = Server::on_v4(Behavior::Answer(b"+PONG\r\n\r\n\r\n"));
    let debugger = Server::on_v4(Behavior::Answer(b"HTTP/1.0 400 Bad Request\r\n\r\n"));
    let closed = ClosedPort::new();
    // Ids of this test's own: the answers are kept for every listing of the process.
    let mut trees = vec![
        tree(
            "web-ports-test-fill:1",
            &[
                live_reload.port,
                pages.port,
                database.port,
                debugger.port,
                closed.port(),
            ],
        ),
        tree("web-ports-test-fill:2", &[]),
    ];

    fill(&mut trees);

    assert_eq!(
        http_ports(&trees),
        vec![Some(vec![pages.port]), Some(Vec::new())]
    );
}

#[test]
fn serves_pages_judges_the_start_of_an_answer() {
    for head in [
        &b"HTTP/1.1 200"[..],
        b"HTTP/1.0 404",
        b"HTTP/1.1 301",
        b"HTTP/1.1 101",
        b"HTTP/2 200 O",
        b"HTTP/1.1 427",
    ] {
        assert!(serves_pages(head), "{}", String::from_utf8_lossy(head));
    }
    for head in [
        &b"HTTP/1.1 426"[..],
        b"HTTP/2 426 U",
        b"HTTP/1.0 400",
        b"HTTP/1.1 400",
        b"HTTP/1.1 4x6",
        b"HTTP/1.1 20",
        b"HTTP/1.1 ",
        b"HTTP/1.1abc",
        b"SSH-2.0-Open",
        b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03\x00",
        b"+PONG\r\n\r\n\r\n",
        b"HTTP/",
        b"",
    ] {
        assert!(!serves_pages(head), "{}", String::from_utf8_lossy(head));
    }
}
