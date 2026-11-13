use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;

use tokio_tungstenite::tungstenite::{self, Message, WebSocket};

use super::*;
use crate::platform::ipc::PrivateListen;
#[cfg(unix)]
use crate::platform::proc::Containment;
use crate::platform::term::Command;
use crate::session::term::QueueStart;
#[cfg(unix)]
use crate::terminal::server_host::{LocalHost, LocalServer};
use crate::terminal::server_host::{ServerListen, ServerReady};
use crate::test_support::PATIENCE;

/// A transport to `endpoint`, with no server process behind it.
fn fixture_transport(endpoint: Endpoint) -> CodexTransport {
    CodexTransport {
        server: Arc::new(Mutex::new(None)),
        endpoint,
        shared: Arc::new(Shared::default()),
        operation: Mutex::new(()),
    }
}

/// A transport to a socket the test listens on, with no server process behind it, and the
/// private directory of the socket, removed when it is dropped.
#[cfg(unix)]
fn socket_transport() -> (
    CodexTransport,
    std::os::unix::net::UnixListener,
    PrivateListen,
) {
    let listen = PrivateListen::socket("octo-cdx", "rpc").unwrap();
    let crate::platform::ipc::Address::Socket(socket) = listen.address() else {
        unreachable!("a Unix host listens on a socket");
    };
    let socket = socket.to_path_buf();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let endpoint = Endpoint::Socket(socket);
    (fixture_transport(endpoint), listener, listen)
}

/// A loopback transport, which every host can run, and the listener behind it.
fn loopback() -> (CodexTransport, TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let token = "the-token".to_string();
    let endpoint = Endpoint::Loopback {
        port,
        token: token.clone(),
    };
    (fixture_transport(endpoint), listener, token)
}

/// What the fake server answers a request with.
enum Answer {
    Result(Value),
    Error(&'static str),
    /// Closes the connection without an answer.
    Close,
}

fn receive(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    let message = socket.read().ok()?;
    serde_json::from_str(message.to_text().ok()?).ok()
}

/// The next connection on `listener`; panics, rather than waiting for ever, when none comes
/// within [`PATIENCE`].
fn accepted(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + PATIENCE;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "no connection came");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accepting a connection failed: {error}"),
        }
    }
}

/// Accepts a connection that presents `token`, and answers its handshake.
fn initialized(listener: &TcpListener, token: &str) -> WebSocket<TcpStream> {
    handshake(accepted(listener), token)
}

/// Answers the handshake and the initialization of the client on `stream`, which has to present
/// `token`.
#[allow(
    clippy::result_large_err,
    reason = "the handshake callback's error type is tungstenite's"
)]
fn handshake(stream: TcpStream, token: &str) -> WebSocket<TcpStream> {
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    let expected = format!("Bearer {token}");
    let mut socket = tungstenite::accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request,
         response: tungstenite::handshake::server::Response| {
            assert_eq!(
                request
                    .headers()
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some(expected.as_str())
            );
            Ok(response)
        },
    )
    .unwrap();
    let initialize = receive(&mut socket).unwrap();
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(
        initialize["params"]["capabilities"]["experimentalApi"],
        true
    );
    socket
        .send(Message::Text(
            json!({"id":initialize["id"],"result":{}}).to_string(),
        ))
        .unwrap();
    assert_eq!(receive(&mut socket).unwrap()["method"], "initialized");
    socket
}

/// Serves one connection on `listener`, answering each request as `answer` says, until the client
/// goes; gives the requests it got, by method and parameters.
fn serve(
    listener: TcpListener,
    token: String,
    answer: impl FnMut(&str, &Value) -> Answer + Send + 'static,
) -> std::thread::JoinHandle<Vec<(String, Value)>> {
    serve_connections(listener, token, 1, answer)
}

/// [`serve`] for `connections` connections, one after another.
fn serve_connections(
    listener: TcpListener,
    token: String,
    connections: usize,
    mut answer: impl FnMut(&str, &Value) -> Answer + Send + 'static,
) -> std::thread::JoinHandle<Vec<(String, Value)>> {
    std::thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..connections {
            serve_one(&listener, &token, &mut answer, &mut requests);
        }
        requests
    })
}

fn serve_one(
    listener: &TcpListener,
    token: &str,
    answer: &mut impl FnMut(&str, &Value) -> Answer,
    requests: &mut Vec<(String, Value)>,
) {
    {
        let mut socket = initialized(listener, token);
        while let Some(request) = receive(&mut socket) {
            let method = request["method"].as_str().unwrap_or_default().to_string();
            requests.push((method.clone(), request["params"].clone()));
            let reply = match answer(&method, &request["params"]) {
                Answer::Result(result) => json!({"id":request["id"],"result":result}),
                Answer::Error(message) => {
                    json!({"id":request["id"],"error":{"code":-32600,"message":message}})
                }
                Answer::Close => {
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    break;
                }
            };
            if socket.send(Message::Text(reply.to_string())).is_err() {
                break;
            }
        }
    }
}

fn thread(id: &str, ephemeral: bool, source: &str, parent: Option<&str>) -> Value {
    json!({"id":id,"ephemeral":ephemeral,"threadSource":source,"parentThreadId":parent})
}

fn methods(requests: &[(String, Value)]) -> Vec<&str> {
    requests.iter().map(|(method, _)| method.as_str()).collect()
}

/// A transport whose server takes the identified thread `thread`'s queue request for a message
/// and then goes without answering it, so whether the message was taken is not known; for a test
/// of what an uncertain acceptance does further up, `check` sees the message's text.
pub(crate) fn transport_losing_an_add(
    check: impl Fn(&str) + Send + 'static,
) -> (
    CodexTransport,
    std::thread::JoinHandle<Vec<(String, Value)>>,
) {
    let (transport, listener, token) = loopback();
    transport.shared.identify("thread");
    let server = serve(listener, token, move |method, params| match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["thread"],"nextCursor":null})),
        "thread/queue/add" => {
            check(params["input"][0]["text"].as_str().unwrap_or_default());
            Answer::Close
        }
        other => panic!("unexpected request {other}"),
    });
    (transport, server)
}

/// The fake server's answers for a thread `main` that is loaded and idle, whose queue starts a
/// message added to it itself.
fn idle_main(method: &str, _: &Value) -> Answer {
    match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["main"],"nextCursor":null})),
        "thread/queue/add" => Answer::Result(json!({"queuedSubmission":{"id":"q1"}})),
        "thread/queue/start" => Answer::Error("queued submission not found: q1"),
        "thread/queue/list" => Answer::Result(json!({"data":[],"nextCursor":null})),
        other => panic!("unexpected request {other}"),
    }
}

#[test]
fn queues_text_for_the_user_thread_and_ignores_the_ephemeral_title_thread() {
    let (transport, listener, token) = loopback();
    let server = serve(listener, token, |method, params| match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["title","main"],"nextCursor":null})),
        "thread/read" if params["threadId"] == "title" => {
            Answer::Result(json!({"thread":thread("title", true, "thread_title", None)}))
        }
        "thread/read" => Answer::Result(json!({"thread":thread("main", false, "user", None)})),
        other => idle_main(other, params),
    });
    assert_eq!(
        transport
            .send("line one\nline two", "launch:ticket-12")
            .unwrap(),
        Sent::Queued
    );
    drop(transport.endpoint.connect());
    let requests = server.join().unwrap();
    let add = requests
        .iter()
        .find(|(method, _)| method == "thread/queue/add")
        .unwrap();
    assert_eq!(
        add.1,
        json!({"threadId":"main","clientUserMessageId":"launch:ticket-12",
            "input":[{"type":"text","text":"line one\nline two","text_elements":[]}]})
    );
    assert_eq!(transport.shared.thread().as_deref(), Some("main"));
}

/// Whether a start Codex refused leaves the message to run is read off Codex's queue, not the
/// refusal's words: gone from the queue, or queued behind a running turn, it runs; queued with
/// nothing running, it may never run, and is uncertain.
#[test]
fn a_refused_start_is_judged_by_the_queue_whatever_its_words() {
    for (queued, active, delivered) in [
        (false, false, true),
        (true, true, true),
        (true, false, false),
    ] {
        let (mut transport, listener, token) = loopback();
        // An idle thread is read again for the time allowed before the message is called
        // uncertain; none is allowed here.
        settle_for(&mut transport, Duration::ZERO);
        transport.shared.identify("main");
        let server = serve(listener, token, move |method, params| match method {
            "thread/queue/start" => Answer::Error("a wording no release used"),
            "thread/queue/list" if queued => Answer::Result(
                json!({"data":[{"id":"q1","clientUserMessageId":"id"}],"nextCursor":null}),
            ),
            "thread/read" => Answer::Result(
                json!({"thread":{"status":{"type": if active { "active" } else { "idle" }}}}),
            ),
            other => idle_main(other, params),
        });
        let sent = transport.send("hello", "id");
        drop(transport);
        server.join().unwrap();
        if delivered {
            assert_eq!(sent.unwrap(), Sent::Queued);
        } else {
            let error = sent.unwrap_err();
            assert_eq!(error.written, 1);
            assert!(error
                .error
                .get_ref()
                .unwrap()
                .is::<term::UncertainSubmission>());
        }
    }
}

#[test]
fn the_original_thread_keeps_the_messages_when_another_task_is_open() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    let server = serve(listener, token, |method, params| match method {
        // The identified thread is loaded, so nothing else is read.
        "thread/loaded/list" => Answer::Result(json!({"data":["second","main"],"nextCursor":null})),
        other => idle_main(other, params),
    });
    assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
    drop(transport);
    let requests = server.join().unwrap();
    assert!(!methods(&requests).contains(&"thread/read"));
}

/// The server unloads the session's conversation a few minutes after the terminal switched to
/// another task; a message loads it again, keeping its settings, rather than going to the
/// terminal, and waits where it cannot.
#[test]
fn an_unloaded_original_thread_is_loaded_again_or_waited_for() {
    for loads in [true, false] {
        let (transport, listener, token) = loopback();
        transport.shared.identify("main");
        let server = serve(listener, token, move |method, params| match method {
            "thread/loaded/list" => Answer::Result(json!({"data":["other"],"nextCursor":null})),
            "thread/resume" if loads => Answer::Result(json!({"thread":{"id":"main"}})),
            "thread/resume" => Answer::Error("no rollout found"),
            other => idle_main(other, params),
        });
        let sent = transport.send("hello", "id");
        drop(transport);
        let requests = server.join().unwrap();
        let resume = requests
            .iter()
            .find(|(method, _)| method == "thread/resume")
            .unwrap();
        assert_eq!(resume.1, json!({"threadId":"main","excludeTurns":true}));
        if loads {
            assert_eq!(sent.unwrap(), Sent::Queued);
        } else {
            let error = sent.unwrap_err();
            assert_eq!(
                (error.written, error.error.kind()),
                (0, io::ErrorKind::WouldBlock)
            );
            assert!(!methods(&requests).contains(&"thread/queue/add"));
        }
    }
}

#[test]
fn a_conversation_that_cannot_be_told_apart_goes_to_the_terminal() {
    for (ids, threads) in [
        (
            vec!["one", "two", "helper"],
            vec![
                thread("one", false, "user", None),
                thread("two", false, "user", None),
                thread("helper", false, "user", Some("one")),
            ],
        ),
        (vec!["other"], vec![thread("other", false, "exec", None)]),
    ] {
        let (transport, listener, token) = loopback();
        let server = serve(listener, token, move |method, params| match method {
            "thread/loaded/list" => Answer::Result(json!({"data":ids,"nextCursor":null})),
            "thread/read" => Answer::Result(json!({"thread": threads
                .iter()
                .find(|thread| thread["id"] == params["threadId"])
                .unwrap()})),
            other => panic!("unexpected request {other}"),
        });
        assert!(matches!(
            transport.send("hello", "id").unwrap(),
            Sent::Declined(_)
        ));
        drop(transport.endpoint.connect());
        server.join().unwrap();
        assert_eq!(transport.shared.thread(), None);
    }
}

#[test]
fn a_session_with_no_conversation_yet_holds_the_message_and_retries() {
    let (transport, listener, token) = loopback();
    let retried = Arc::new(AtomicBool::new(false));
    let seen = retried.clone();
    transport.attach(TransportEvents {
        ready: Box::new(move || seen.store(true, Ordering::Release)),
        server_gone: Box::new(|| panic!("the server did not go")),
        held: Box::new(|_| {}),
        released: Box::new(|_| {}),
    });
    let server = serve(listener, token, |method, _| match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["title"],"nextCursor":null})),
        "thread/read" => {
            Answer::Result(json!({"thread":thread("title", true, "thread_title", None)}))
        }
        other => panic!("unexpected request {other}"),
    });
    let error = transport.send("hello", "id").unwrap_err();
    assert_eq!(
        (error.written, error.error.kind()),
        (0, io::ErrorKind::WouldBlock)
    );
    drop(transport.endpoint.connect());
    server.join().unwrap();
    assert!(
        crate::session::wait::wait_for(PATIENCE, || retried.load(Ordering::Acquire)),
        "the queue was never retried"
    );
}

/// Messages that keep waiting for the session's conversation are told of once the wait has
/// lasted, with a notice that names no cause it cannot know, and the notice is withdrawn once
/// they go in.
#[test]
fn a_long_wait_is_told_once_and_withdrawn_when_the_message_goes_in() {
    let (mut transport, listener, token) = loopback();
    Arc::get_mut(&mut transport.shared)
        .unwrap()
        .held_notice_after = Duration::ZERO;
    transport.shared.identify("main");
    let held = Arc::new(Mutex::new(Vec::new()));
    let released = Arc::new(Mutex::new(Vec::new()));
    let (on_held, on_released) = (held.clone(), released.clone());
    transport.attach(TransportEvents {
        ready: Box::new(|| {}),
        server_gone: Box::new(|| {}),
        held: Box::new(move |notice: crate::protocol::Notice| {
            on_held.lock().unwrap().push(notice.code);
        }),
        released: Box::new(move |code| {
            on_released.lock().unwrap().push(code);
        }),
    });
    // The conversation is not loaded, and loading it fails three times.
    let failures = Arc::new(AtomicUsize::new(3));
    let left = failures.clone();
    let server = serve_connections(listener, token, 4, move |method, params| match method {
        "thread/loaded/list" if left.load(Ordering::Acquire) > 0 => {
            Answer::Result(json!({"data":[],"nextCursor":null}))
        }
        "thread/resume" => {
            left.fetch_sub(1, Ordering::AcqRel);
            Answer::Error("no rollout found")
        }
        other => idle_main(other, params),
    });
    for _ in 0..3 {
        let error = transport.send("hello", "id").unwrap_err();
        assert_eq!(error.error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.error.get_ref().unwrap().is::<term::HeldByServer>());
    }
    assert_eq!(*held.lock().unwrap(), [held_code()]);
    assert!(released.lock().unwrap().is_empty());
    assert!(held_notice().message.contains("not taking them yet"));
    assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
    assert_eq!(*released.lock().unwrap(), [held_code()]);
    drop(transport);
    server.join().unwrap();
}

/// Codex takes a message even for a conversation it unloaded, so a refusal of the queue request
/// is of the message itself: it fails, by no other route, and nothing waits.
#[test]
fn a_refused_queue_request_fails_the_message() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    let server = serve(listener, token, |method, params| match method {
        "thread/queue/add" => Answer::Error("input too long"),
        other => idle_main(other, params),
    });
    let error = transport.send("hello", "id").unwrap_err();
    assert_eq!(error.written, 0);
    let refusal = error
        .error
        .get_ref()
        .unwrap()
        .downcast_ref::<term::RefusedByAgent>()
        .unwrap();
    assert!(refusal.0.contains("input too long"), "{refusal}");
    drop(transport);
    server.join().unwrap();
}

/// A message is known as Octoboard's in the queue from before its queue request, so one whose
/// acceptance was lost is still started after a cancel.
#[test]
fn a_message_whose_answer_was_lost_is_still_known_as_octoboards() {
    let (transport, server) = transport_losing_an_add(|_| {});
    assert!(transport.send("lost", "launch:7").is_err());
    server.join().unwrap();
    assert!(transport.shared.was_added("launch:7"));
}

#[test]
fn a_lost_answer_to_a_queue_request_is_uncertain() {
    let (transport, server) = transport_losing_an_add(|text| assert_eq!(text, "accepted or lost"));
    let error = transport.send("accepted or lost", "same-id").unwrap_err();
    server.join().unwrap();
    assert_eq!(error.written, 1);
    assert!(error
        .error
        .get_ref()
        .unwrap()
        .is::<term::UncertainSubmission>());
}

/// Starting the queue starts the first of Octoboard's messages waiting there and says how many of
/// Octoboard's waited; one behind something queued by someone else is left where it is.
#[test]
fn starting_the_queue_starts_octoboards_first_message_unless_another_is_ahead() {
    for (items, start, status, expected) in [
        (
            json!([{"id":"q-1","clientUserMessageId":"mine-1"},
                {"id":"q-2","clientUserMessageId":"mine-2"}]),
            Some(Answer::Result(json!({"turn":{}}))),
            // Codex reports the turn the start began as running, so the start has nothing to wait
            // for.
            "active",
            QueueStart::Started(2),
        ),
        (
            json!([{"id":"q-1","clientUserMessageId":"mine-1"}]),
            Some(Answer::Error("busy")),
            "active",
            QueueStart::Started(1),
        ),
        (
            json!([{"id":"user-item","clientUserMessageId":"typed"},
                {"id":"q-1","clientUserMessageId":"mine-1"}]),
            None,
            "idle",
            QueueStart::BehindOthers(1),
        ),
        (
            json!([{"id":"user-item","clientUserMessageId":"typed"}]),
            None,
            "idle",
            QueueStart::Nothing,
        ),
    ] {
        let (transport, listener, token) = loopback();
        transport.shared.identify("main");
        transport
            .shared
            .added
            .lock()
            .unwrap()
            .extend(["mine-1".to_string(), "mine-2".to_string()]);
        let start = Mutex::new(start);
        let server = serve(listener, token, move |method, params| match method {
            "thread/queue/list" => Answer::Result(json!({"data":items,"nextCursor":null})),
            "thread/queue/start" => {
                assert_eq!(
                    params,
                    &json!({"threadId":"main","queuedSubmissionId":"q-1"})
                );
                start.lock().unwrap().take().expect("no start was expected")
            }
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":status}}})),
            other => panic!("unexpected request {other}"),
        });
        let started = transport.start_queue();
        drop(transport);
        server.join().unwrap();
        assert_eq!(started, expected);
    }
}

/// The cancel's own hook and an owner's interrupt both start the queue: the second, finding
/// nothing of Octoboard's left, reports what the first started.
#[test]
fn a_second_start_reports_what_the_first_started() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .insert("mine-1".into());
    let listed = Arc::new(AtomicUsize::new(0));
    let count = listed.clone();
    let server = serve_connections(listener, token, 2, move |method, _| match method {
        "thread/queue/list" if count.fetch_add(1, Ordering::AcqRel) == 0 => Answer::Result(
            json!({"data":[{"id":"q-1","clientUserMessageId":"mine-1"}],"nextCursor":null}),
        ),
        "thread/queue/list" => Answer::Result(json!({"data":[],"nextCursor":null})),
        "thread/queue/start" => Answer::Result(json!({"turn":{}})),
        "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    server.join().unwrap();
    // Nobody listening: the connection is refused at once rather than left to time out.
    let (transport, listener, _) = loopback();
    drop(listener);
    assert_eq!(transport.start_queue(), QueueStart::Nothing);
    transport.shared.identify("main");
    assert!(matches!(transport.start_queue(), QueueStart::Failed(_)));
}

/// With two of Octoboard's messages waiting, the first start starts the first and counts both; a
/// second start a moment later starts nothing more and still counts both, whatever is left.
#[test]
fn a_second_start_neither_starts_again_nor_under_counts() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .extend(["mine-1".to_string(), "mine-2".to_string()]);
    let lists = Arc::new(AtomicUsize::new(0));
    let count = lists.clone();
    let server = serve_connections(listener, token, 2, move |method, _| match method {
        "thread/queue/list" if count.fetch_add(1, Ordering::AcqRel) == 0 => {
            Answer::Result(json!({"data":[{"id":"q-1","clientUserMessageId":"mine-1"},
                {"id":"q-2","clientUserMessageId":"mine-2"}],"nextCursor":null}))
        }
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q-2","clientUserMessageId":"mine-2"}],"nextCursor":null}),
        ),
        "thread/queue/start" => Answer::Result(json!({"turn":{}})),
        "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(2));
    assert_eq!(transport.start_queue(), QueueStart::Started(2));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(
        methods(&requests)
            .iter()
            .filter(|method| **method == "thread/queue/start")
            .count(),
        1
    );
}

/// Codex answers a start before the thread's status turns active (0.167.1). A second start reading
/// the status in that gap would take the thread for idle, as after a cancel of its own, try to
/// start the next message and count only what is left: the first start waits for the status to
/// settle, so the second still counts everything the first found waiting and starts nothing more.
#[test]
fn a_second_start_counts_all_while_the_status_lags_the_first() {
    let (mut transport, listener, token) = loopback();
    Arc::get_mut(&mut transport.shared)
        .unwrap()
        .start_settle_poll = Duration::ZERO;
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .extend((1..=5).map(|n| format!("mine-{n}")));
    let start = AtomicBool::new(false);
    let reads = AtomicUsize::new(0);
    let server = serve_connections(listener, token, 2, move |method, _| match method {
        "thread/queue/list" => {
            let first = if start.load(Ordering::Acquire) { 2 } else { 1 };
            let data: Vec<Value> = (first..=5)
                .map(
                    |n| json!({"id": format!("q-{n}"), "clientUserMessageId": format!("mine-{n}")}),
                )
                .collect();
            Answer::Result(json!({"data": data, "nextCursor": null}))
        }
        "thread/queue/start" if !start.swap(true, Ordering::AcqRel) => {
            Answer::Result(json!({"turn":{}}))
        }
        "thread/queue/start" => Answer::Error("thread already has an active or pending turn"),
        // The status turns active only on the second read after the start, whenever that comes.
        "thread/read" => {
            let lagging =
                !start.load(Ordering::Acquire) || reads.fetch_add(1, Ordering::AcqRel) < 1;
            let status = if lagging { "idle" } else { "active" };
            Answer::Result(json!({"thread":{"status":{"type": status}}}))
        }
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(5));
    assert_eq!(transport.start_queue(), QueueStart::Started(5));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/start"), 1);
}

/// Sets the time `transport` allows Codex's status to settle after a start to `settle`, read again
/// without pause: a test drives the lag by the requests it makes, never by the clock. Before
/// anything else holds the transport's shared state.
fn settle_for(transport: &mut CodexTransport, settle: Duration) {
    let shared = Arc::get_mut(&mut transport.shared).unwrap();
    shared.start_settle = settle;
    shared.start_settle_poll = Duration::ZERO;
}

/// A loopback transport whose thread `main` has one of Octoboard's messages, `mine-1`, waiting,
/// with the wait for a started turn to be reported running set to `settle`.
fn transport_with_one_waiting(settle: Duration) -> (CodexTransport, TcpListener, String) {
    let (mut transport, listener, token) = loopback();
    settle_for(&mut transport, settle);
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .insert("mine-1".into());
    (transport, listener, token)
}

/// What the fake server answers while one message waits and a start is accepted.
fn one_waiting_answer(method: &str) -> Option<Answer> {
    match method {
        "thread/queue/list" => Some(Answer::Result(
            json!({"data":[{"id":"q-1","clientUserMessageId":"mine-1"}],"nextCursor":null}),
        )),
        "thread/queue/start" => Some(Answer::Result(json!({"turn":{}}))),
        _ => None,
    }
}

fn count_of(requests: &[(String, Value)], name: &str) -> usize {
    methods(requests).iter().filter(|m| **m == name).count()
}

/// [`one_waiting_answer`] for a server whose start names the turn `turn` it began.
fn one_waiting_answer_naming(method: &str, turn: &str) -> Option<Answer> {
    match method {
        "thread/queue/start" => Some(Answer::Result(json!({"turn":{"id":turn}}))),
        other => one_waiting_answer(other),
    }
}

/// What the fake server answers the request for a thread's latest turn when that is `turn`.
fn latest_turn_is(turn: &str) -> Answer {
    Answer::Result(json!({"data":[{"id":turn}],"nextCursor":null}))
}

/// What the fake server answers a status read with.
fn status_is(status: &str) -> Answer {
    Answer::Result(json!({"thread":{"status":{"type":status}}}))
}

/// A turn Codex never reports as running within the time allowed does not fail the start: it still
/// reports what it started, after reading the status once past the deadline.
#[test]
fn a_start_whose_turn_is_never_reported_running_still_reports_what_it_started() {
    let (transport, listener, token) = transport_with_one_waiting(Duration::ZERO);
    let server = serve(listener, token, |method, _| {
        one_waiting_answer(method).unwrap_or_else(|| match method {
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"idle"}}})),
            other => panic!("unexpected request {other}"),
        })
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/start"), 1);
    assert_eq!(count_of(&requests, "thread/read"), 1);
}

/// While the time allowed has not run out, the status is read again until it turns active, and
/// then no more: reads, not the clock, drive the lag here, and the allowance is far past any wait
/// the test can meet.
#[test]
fn a_start_reads_the_status_until_it_turns_active_and_no_further() {
    const LAGGING_READS: usize = 3;
    let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let server = serve(listener, token, move |method, _| {
        one_waiting_answer(method).unwrap_or_else(|| match method {
            "thread/read" => {
                let read = count.fetch_add(1, Ordering::AcqRel);
                // A read past the one that reported active means the wait went on: fail the
                // test through the server rather than leave the client polling for the allowance.
                assert!(
                    read <= LAGGING_READS,
                    "the status was read after it turned active"
                );
                let status = if read < LAGGING_READS {
                    "idle"
                } else {
                    "active"
                };
                Answer::Result(json!({"thread":{"status":{"type": status}}}))
            }
            other => panic!("unexpected request {other}"),
        })
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/read"), LAGGING_READS + 1);
}

/// A status that cannot be read after an accepted start, or one that is neither idle nor active,
/// is no lag to wait out: it ends the wait at once, with the whole time allowed still left, and
/// the start still reports what it started.
#[test]
fn a_start_whose_status_is_no_lag_does_not_wait_for_it() {
    // No status: the read fails.
    for status in [None, Some("notLoaded")] {
        let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
        let server = serve(listener, token, move |method, _| {
            one_waiting_answer(method).unwrap_or_else(|| match method {
                "thread/read" => match status {
                    Some(status) => Answer::Result(json!({"thread":{"status":{"type": status}}})),
                    None => Answer::Error("thread not loaded"),
                },
                other => panic!("unexpected request {other}"),
            })
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/read"), 1);
    }
}

/// An active status right after a start can still be the turn before it, whose end the server has
/// not handled yet (0.167.1): a cancel's own start finds the cancelled turn still active. A start
/// that reported the turn it began waits for that turn to be the latest the thread has, so the
/// stale active status is waited out as an idle one is, and a second start still counts everything
/// the first found waiting and starts nothing more.
#[test]
fn a_second_start_counts_all_while_the_status_still_reads_the_turn_before() {
    let (mut transport, listener, token) = loopback();
    settle_for(&mut transport, PATIENCE);
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .extend((1..=5).map(|n| format!("mine-{n}")));
    let start = AtomicBool::new(false);
    let reads = AtomicUsize::new(0);
    let server = serve_connections(listener, token, 2, move |method, _| match method {
        "thread/queue/list" => {
            let first = if start.load(Ordering::Acquire) { 2 } else { 1 };
            let data: Vec<Value> = (first..=5)
                .map(
                    |n| json!({"id": format!("q-{n}"), "clientUserMessageId": format!("mine-{n}")}),
                )
                .collect();
            Answer::Result(json!({"data": data, "nextCursor": null}))
        }
        "thread/queue/start" if !start.swap(true, Ordering::AcqRel) => {
            Answer::Result(json!({"turn":{"id":"t-new"}}))
        }
        "thread/queue/start" => Answer::Error("thread already has an active or pending turn"),
        // After the start: active (the turn before, not ended yet), idle (it ended), then active
        // (the new turn), whenever the reads come. The fourth read is the second start's own.
        "thread/read" => {
            let read = reads.fetch_add(1, Ordering::AcqRel);
            assert!(read <= 3, "the status was read after the second start's");
            status_is(if read == 1 { "idle" } else { "active" })
        }
        // The turn before is the latest until the status has been read three times.
        "thread/turns/list" => latest_turn_is(if reads.load(Ordering::Acquire) < 3 {
            "t-old"
        } else {
            "t-new"
        }),
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(5));
    assert_eq!(transport.start_queue(), QueueStart::Started(5));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/start"), 1);
    // Both reads that found the status active looked at the latest turn; the idle one did not.
    assert_eq!(count_of(&requests, "thread/read"), 4);
    assert_eq!(count_of(&requests, "thread/turns/list"), 2);
}

/// A start that names the turn it began is waited for until that turn is the latest the thread has,
/// asked for as the one latest turn without its items, and no further.
#[test]
fn a_start_naming_its_turn_waits_until_that_turn_is_the_latest() {
    const STALE_LISTINGS: usize = 3;
    let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
    let listings = AtomicUsize::new(0);
    let server = serve(listener, token, move |method, _| {
        one_waiting_answer_naming(method, "t-new").unwrap_or_else(|| match method {
            "thread/read" => status_is("active"),
            "thread/turns/list" => {
                let listing = listings.fetch_add(1, Ordering::AcqRel);
                assert!(
                    listing <= STALE_LISTINGS,
                    "the turns were listed after the started turn showed"
                );
                latest_turn_is(if listing < STALE_LISTINGS {
                    "t-old"
                } else {
                    "t-new"
                })
            }
            other => panic!("unexpected request {other}"),
        })
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/read"), STALE_LISTINGS + 1);
    assert_eq!(count_of(&requests, "thread/turns/list"), STALE_LISTINGS + 1);
    let listing = requests
        .iter()
        .find(|(method, _)| method == "thread/turns/list")
        .unwrap();
    assert_eq!(
        listing.1,
        json!({"threadId":"main","limit":1,"sortDirection":"desc","itemsView":"notLoaded"})
    );
}

/// Whatever the wait ends on, the start reports what it started: a turn that never becomes the
/// latest, or a thread with no turn listed, costs the time allowed, which is none here, and no
/// more.
#[test]
fn a_start_whose_turn_never_becomes_the_latest_still_reports_what_it_started() {
    for latest in [Some("t-old"), None] {
        let (transport, listener, token) = transport_with_one_waiting(Duration::ZERO);
        let server = serve(listener, token, move |method, _| {
            one_waiting_answer_naming(method, "t-new").unwrap_or_else(|| match method {
                "thread/read" => status_is("active"),
                "thread/turns/list" => match latest {
                    Some(turn) => latest_turn_is(turn),
                    None => Answer::Result(json!({"data":[],"nextCursor":null})),
                },
                other => panic!("unexpected request {other}"),
            })
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/queue/start"), 1);
        assert_eq!(count_of(&requests, "thread/read"), 1);
        assert_eq!(count_of(&requests, "thread/turns/list"), 1);
    }
}

/// Naming its turn does not make a status that is no lag one: unreadable, or neither idle nor
/// active, it ends the wait at once with the turns never asked for.
#[test]
fn a_start_naming_its_turn_still_ends_the_wait_at_once_on_a_status_that_is_no_lag() {
    for status in [None, Some("notLoaded"), Some("systemError")] {
        let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
        let server = serve(listener, token, move |method, _| {
            one_waiting_answer_naming(method, "t-new").unwrap_or_else(|| match method {
                "thread/read" => match status {
                    Some(status) => status_is(status),
                    None => Answer::Error("thread not loaded"),
                },
                other => panic!("unexpected request {other}"),
            })
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/read"), 1);
        assert_eq!(count_of(&requests, "thread/turns/list"), 0);
    }
}

/// The latest turn that cannot be read is no lag either: it ends the wait at once, with the whole
/// time allowed still left, and the start still reports what it started.
#[test]
fn a_start_whose_latest_turn_cannot_be_read_does_not_wait_for_it() {
    let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
    let server = serve(listener, token, |method, _| {
        one_waiting_answer_naming(method, "t-new").unwrap_or_else(|| match method {
            "thread/read" => status_is("active"),
            "thread/turns/list" => Answer::Error("thread not found"),
            other => panic!("unexpected request {other}"),
        })
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/read"), 1);
    assert_eq!(count_of(&requests, "thread/turns/list"), 1);
}

/// A start whose answer names no turn has none to look for: the status alone counts, as before, and
/// the turns are never asked for.
#[test]
fn a_start_naming_no_turn_goes_by_the_status_alone() {
    for answer in [json!({"turn":{}}), json!({})] {
        let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
        let server = serve(listener, token, move |method, _| {
            if method == "thread/queue/start" {
                return Answer::Result(answer.clone());
            }
            one_waiting_answer(method).unwrap_or_else(|| match method {
                "thread/read" => status_is("active"),
                other => panic!("unexpected request {other}"),
            })
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/read"), 1);
        assert_eq!(count_of(&requests, "thread/turns/list"), 0);
    }
}

/// A start Codex refuses has no turn of its own to wait for: one that finds the message gone from
/// the queue, or the thread active, reads nothing further and looks up no turn.
#[test]
fn a_refused_start_that_finds_the_message_running_reads_nothing_further() {
    // (does the message still wait after the refusal, status reads expected)
    for (still_waiting, status_reads) in [(false, 0), (true, 1)] {
        let (transport, listener, token) = transport_with_one_waiting(START_SETTLE);
        let lists = Arc::new(AtomicUsize::new(0));
        let count = lists.clone();
        let server = serve(listener, token, move |method, _| match method {
            // Read once to find the message, once more to see whether the refusal left it there.
            "thread/queue/list" if !still_waiting && count.fetch_add(1, Ordering::AcqRel) >= 1 => {
                Answer::Result(json!({"data":[],"nextCursor":null}))
            }
            "thread/queue/start" => Answer::Error("busy"),
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
            other => {
                one_waiting_answer(other).unwrap_or_else(|| panic!("unexpected request {other}"))
            }
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/queue/start"), 1);
        assert_eq!(count_of(&requests, "thread/read"), status_reads);
    }
}

/// A refused start Codex gives while a turn is just beginning leaves the thread reading idle for a
/// moment, so the queue and the status are read again, until the status turns active or the message
/// leaves the queue, and the message is then taken as running rather than failed.
#[test]
fn a_refused_start_reads_an_idle_thread_again_until_it_runs_or_the_message_leaves() {
    for message_leaves in [false, true] {
        let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
        let lists = AtomicUsize::new(0);
        let reads = AtomicUsize::new(0);
        let server = serve(listener, token, move |method, _| match method {
            "thread/queue/start" => Answer::Error("thread already has an active or pending turn"),
            // Read to find the message, then once before each status read after the refusal; the
            // message leaves at the fourth.
            "thread/queue/list" => {
                let list = lists.fetch_add(1, Ordering::AcqRel);
                if message_leaves && list >= 3 {
                    Answer::Result(json!({"data":[],"nextCursor":null}))
                } else {
                    one_waiting_answer(method).unwrap()
                }
            }
            "thread/read" => {
                let read = reads.fetch_add(1, Ordering::AcqRel);
                assert!(
                    read < 3,
                    "the status was read after the start counted as run"
                );
                status_is(if !message_leaves && read == 2 {
                    "active"
                } else {
                    "idle"
                })
            }
            other => panic!("unexpected request {other}"),
        });
        assert_eq!(transport.start_queue(), QueueStart::Started(1));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/queue/start"), 1);
        assert_eq!(count_of(&requests, "thread/queue/list"), 4);
        assert_eq!(
            count_of(&requests, "thread/read"),
            if message_leaves { 2 } else { 3 }
        );
    }
}

/// Past the time allowed, a thread still reading idle with the message queued runs nothing: the
/// refusal stands, after the one read the queue and the status each had.
#[test]
fn a_refused_start_of_a_thread_idle_past_the_time_allowed_is_not_running() {
    let (transport, listener, token) = transport_with_one_waiting(Duration::ZERO);
    let server = serve(listener, token, |method, _| match method {
        "thread/queue/start" => Answer::Error("busy"),
        "thread/read" => status_is("idle"),
        other => one_waiting_answer(other).unwrap_or_else(|| panic!("unexpected request {other}")),
    });
    assert!(matches!(transport.start_queue(), QueueStart::Failed(_)));
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/start"), 1);
    assert_eq!(count_of(&requests, "thread/queue/list"), 2);
    assert_eq!(count_of(&requests, "thread/read"), 1);
}

/// Only an idle thread is read again: another status after a refused start ends the judgement at
/// once, with the whole time allowed left; a thread the server unloaded is loaded again once, not
/// for ever.
#[test]
fn a_refused_start_is_not_read_again_for_a_status_that_is_no_lag() {
    // (status, loads of the thread, status reads)
    for (status, loads, reads) in [("systemError", 0, 1), ("notLoaded", 1, 2)] {
        let (transport, listener, token) = transport_with_one_waiting(PATIENCE);
        let server = serve(listener, token, move |method, _| match method {
            "thread/queue/start" => Answer::Error("busy"),
            "thread/read" => status_is(status),
            "thread/resume" => Answer::Result(json!({"thread":{}})),
            other => {
                one_waiting_answer(other).unwrap_or_else(|| panic!("unexpected request {other}"))
            }
        });
        assert!(matches!(transport.start_queue(), QueueStart::Failed(_)));
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(count_of(&requests, "thread/resume"), loads, "{status}");
        assert_eq!(count_of(&requests, "thread/read"), reads, "{status}");
    }
}

/// The message just added is judged the same way: its start refused while the status still reads
/// idle, the message is taken as running once the status turns active, not failed as uncertain.
#[test]
fn a_sent_message_whose_start_is_refused_while_the_status_lags_is_taken_as_running() {
    let (mut transport, listener, token) = loopback();
    settle_for(&mut transport, PATIENCE);
    transport.shared.identify("main");
    let reads = AtomicUsize::new(0);
    let server = serve(listener, token, move |method, params| match method {
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q1","clientUserMessageId":"id"}],"nextCursor":null}),
        ),
        "thread/queue/start" => Answer::Error("thread already has an active or pending turn"),
        "thread/read" => {
            let read = reads.fetch_add(1, Ordering::AcqRel);
            assert!(
                read < 2,
                "the status was read after the message counted as run"
            );
            status_is(if read == 0 { "idle" } else { "active" })
        }
        other => idle_main(other, params),
    });
    assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/read"), 2);
}

/// A message sent after a start whose turn the status has not shown yet is started where it waits
/// and refused, as Codex has a turn pending: the first send waited for its turn to be reported
/// running, so the second finds it running and succeeds, rather than failing with a message that
/// ran or will run.
#[test]
fn a_message_sent_behind_a_turn_the_status_has_not_shown_yet_is_taken_as_running() {
    let (mut transport, listener, token) = loopback();
    settle_for(&mut transport, PATIENCE);
    transport.shared.identify("main");
    let adds = AtomicUsize::new(0);
    let starts = AtomicUsize::new(0);
    let reads = AtomicUsize::new(0);
    let server = serve_connections(listener, token, 2, move |method, params| match method {
        "thread/queue/add" => {
            let n = adds.fetch_add(1, Ordering::AcqRel) + 1;
            Answer::Result(json!({"queuedSubmission":{"id": format!("q{n}")}}))
        }
        // The first message has been started when the second is added, so the one added last is
        // the only one queued.
        "thread/queue/list" => {
            let n = adds.load(Ordering::Acquire);
            Answer::Result(json!({"data":[
                {"id": format!("q{n}"), "clientUserMessageId": format!("id-{n}")}
            ],"nextCursor":null}))
        }
        "thread/queue/start" if starts.fetch_add(1, Ordering::AcqRel) == 0 => {
            Answer::Result(json!({"turn":{"id":"t-1"}}))
        }
        "thread/queue/start" => Answer::Error("thread already has an active or pending turn"),
        // Idle for the first read after the first start, active from then on.
        "thread/read" => status_is(if reads.fetch_add(1, Ordering::AcqRel) == 0 {
            "idle"
        } else {
            "active"
        }),
        "thread/turns/list" => latest_turn_is("t-1"),
        other => idle_main(other, params),
    });
    assert_eq!(transport.send("first", "id-1").unwrap(), Sent::Queued);
    assert_eq!(transport.send("second", "id-2").unwrap(), Sent::Queued);
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/add"), 2);
    assert_eq!(count_of(&requests, "thread/queue/start"), 2);
    // The first send read the status twice, until the turn it started showed as the latest; the
    // second read it once, past the refusal.
    assert_eq!(count_of(&requests, "thread/read"), 3);
    assert_eq!(count_of(&requests, "thread/turns/list"), 1);
}

/// What a start just made is waited for in the send path too, as after a cancel: the message's
/// start is accepted and the turn it names is waited for until it is the latest.
#[test]
fn a_sent_message_waits_for_the_turn_it_started_to_be_reported_running() {
    let (mut transport, listener, token) = loopback();
    settle_for(&mut transport, PATIENCE);
    transport.shared.identify("main");
    let reads = AtomicUsize::new(0);
    let server = serve(listener, token, move |method, params| match method {
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q1","clientUserMessageId":"id"}],"nextCursor":null}),
        ),
        "thread/queue/start" => Answer::Result(json!({"turn":{"id":"t-new"}})),
        // Idle, then active as the turn before, then active as the turn started.
        "thread/read" => {
            let read = reads.fetch_add(1, Ordering::AcqRel);
            assert!(
                read < 3,
                "the status was read after the started turn showed"
            );
            status_is(if read == 0 { "idle" } else { "active" })
        }
        "thread/turns/list" => latest_turn_is(if reads.load(Ordering::Acquire) < 3 {
            "t-old"
        } else {
            "t-new"
        }),
        other => idle_main(other, params),
    });
    assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
    drop(transport);
    let requests = server.join().unwrap();
    assert_eq!(count_of(&requests, "thread/queue/start"), 1);
    assert_eq!(count_of(&requests, "thread/read"), 3);
    assert_eq!(count_of(&requests, "thread/turns/list"), 2);
}

/// A start moments after another, on a thread that is idle again, is for a cancel of its own: what
/// waits is started as after any cancel, rather than left waiting for good.
#[test]
fn a_start_after_another_cancel_starts_what_waits() {
    let (mut transport, listener, token) = loopback();
    // Nothing reports the started turn as running here; the start need not wait for it.
    Arc::get_mut(&mut transport.shared).unwrap().start_settle = Duration::ZERO;
    transport.shared.identify("main");
    transport
        .shared
        .added
        .lock()
        .unwrap()
        .insert("mine-2".into());
    *transport.shared.last_start.lock().unwrap() = Some((Instant::now(), 1));
    let server = serve(listener, token, |method, params| match method {
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q-2","clientUserMessageId":"mine-2"}],"nextCursor":null}),
        ),
        "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"idle"}}})),
        "thread/queue/start" => {
            assert_eq!(params["queuedSubmissionId"], "q-2");
            Answer::Result(json!({"turn":{}}))
        }
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.start_queue(), QueueStart::Started(1));
    drop(transport);
    let requests = server.join().unwrap();
    assert!(methods(&requests).contains(&"thread/queue/start"));
}

/// A start Codex refuses as overloaded, with the message still queued and nothing running, holds
/// the message back; written again, it is started where it waits, not added a second time, and
/// one that has run meanwhile is done with.
#[test]
fn an_overloaded_start_holds_the_message_until_it_is_written_again() {
    for runs_meanwhile in [false, true] {
        let (mut transport, listener, token) = loopback();
        // The thread reads idle throughout: none of the time allowed for it to turn active is
        // spent.
        settle_for(&mut transport, Duration::ZERO);
        transport.shared.identify("main");
        let starts = AtomicUsize::new(0);
        let lists = AtomicUsize::new(0);
        let server = serve_connections(listener, token, 2, move |method, params| match method {
            "thread/queue/start" if starts.fetch_add(1, Ordering::AcqRel) == 0 => {
                Answer::Error("Server overloaded; retry later.")
            }
            "thread/queue/start" => Answer::Result(json!({"turn":{}})),
            // Read twice around the refused start, then once for the message written again.
            "thread/queue/list" if runs_meanwhile && lists.fetch_add(1, Ordering::AcqRel) >= 2 => {
                Answer::Result(json!({"data":[],"nextCursor":null}))
            }
            "thread/queue/list" => Answer::Result(
                json!({"data":[{"id":"q1","clientUserMessageId":"id"}],"nextCursor":null}),
            ),
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"idle"}}})),
            other => idle_main(other, params),
        });
        let error = transport.send("hello", "id").unwrap_err();
        assert!(error.error.get_ref().unwrap().is::<term::HeldByServer>());
        assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
        drop(transport);
        let requests = server.join().unwrap();
        let count = |name: &str| methods(&requests).iter().filter(|m| **m == name).count();
        assert_eq!(count("thread/queue/add"), 1);
        assert_eq!(
            count("thread/queue/start"),
            if runs_meanwhile { 1 } else { 2 }
        );
    }
}

/// A message added behind others never overtakes them: an idle thread leaves an item added behind
/// others waiting (Codex 0.166.0), so the queue's first item is started where it is Octoboard's,
/// and nothing is started where someone else's is first. One the thread started itself is left.
#[test]
fn a_message_added_behind_others_does_not_overtake_them() {
    for (queue, starts) in [
        (
            json!([{"id":"q-0","clientUserMessageId":"earlier"},
                {"id":"q1","clientUserMessageId":"id"}]),
            Some("q-0"),
        ),
        (
            json!([{"id":"user","clientUserMessageId":"typed"},
                {"id":"q1","clientUserMessageId":"id"}]),
            None,
        ),
        (json!([]), None),
    ] {
        let (transport, listener, token) = loopback();
        transport.shared.identify("main");
        transport
            .shared
            .added
            .lock()
            .unwrap()
            .insert("earlier".into());
        let queue = Mutex::new(Some(queue));
        let server = serve(listener, token, move |method, params| match method {
            "thread/queue/list" => {
                let items = queue.lock().unwrap().take().unwrap_or(json!([]));
                Answer::Result(json!({"data":items,"nextCursor":null}))
            }
            "thread/queue/start" => {
                assert_eq!(params["queuedSubmissionId"], starts.unwrap());
                Answer::Result(json!({"turn":{}}))
            }
            // Read once after an accepted start, which waits for the turn to be reported running.
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
            other => idle_main(other, params),
        });
        assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
        drop(transport);
        let requests = server.join().unwrap();
        assert_eq!(
            methods(&requests).contains(&"thread/queue/start"),
            starts.is_some()
        );
    }
}

/// Codex answers a request it has no room for with "Server overloaded; retry later.": the message
/// waits rather than failing, and is not known as queued.
#[test]
fn an_overloaded_server_holds_the_message_back() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    let server = serve(listener, token, |method, params| match method {
        "thread/queue/add" => Answer::Error("Server overloaded; retry later."),
        other => idle_main(other, params),
    });
    let error = transport.send("hello", "id").unwrap_err();
    assert!(error.error.get_ref().unwrap().is::<term::HeldByServer>());
    assert!(!transport.shared.was_added("id"));
    drop(transport);
    server.join().unwrap();
}

/// `transport` with its wait told of to the session at once; gives how many times the session
/// was told the wait ended.
fn telling_of_waits(transport: &mut CodexTransport) -> Arc<AtomicUsize> {
    Arc::get_mut(&mut transport.shared)
        .unwrap()
        .held_notice_after = Duration::ZERO;
    let released = Arc::new(AtomicUsize::new(0));
    let on_released = released.clone();
    transport.attach(TransportEvents {
        ready: Box::new(|| {}),
        server_gone: Box::new(|| {}),
        held: Box::new(|_| {}),
        released: Box::new(move |_| {
            on_released.fetch_add(1, Ordering::AcqRel);
        }),
    });
    transport.shared.note_held("waiting");
    released
}

/// What the session was told of a wait is withdrawn when the wait ends with an unconfirmed
/// acceptance.
#[test]
fn a_wait_told_of_is_withdrawn_when_the_acceptance_is_unconfirmed() {
    let (mut transport, server) = transport_losing_an_add(|_| {});
    let released = telling_of_waits(&mut transport);
    assert!(transport.send("lost", "id").is_err());
    assert_eq!(released.load(Ordering::Acquire), 1);
    drop(transport);
    server.join().unwrap();
}

/// What the session was told of a wait is withdrawn when the server is stopped.
#[test]
fn a_wait_told_of_is_withdrawn_when_the_server_stops() {
    let (mut transport, listener, _) = loopback();
    drop(listener);
    transport.shared.identify("thread");
    let released = telling_of_waits(&mut transport);
    transport.stop();
    assert_eq!(released.load(Ordering::Acquire), 1);
}

/// A message refused outright is forgotten as Octoboard's, so nothing of it is looked for in the
/// queue.
#[test]
fn a_refused_message_is_not_known_as_octoboards() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    let server = serve(listener, token, |method, params| match method {
        "thread/queue/add" => Answer::Error("input too long"),
        other => idle_main(other, params),
    });
    assert!(transport.send("hello", "id").is_err());
    assert!(!transport.shared.was_added("id"));
    drop(transport);
    server.join().unwrap();
}

/// A conversation identified by a message ends the thread watch, which closes its connection
/// without letting the queue be written out again.
#[test]
fn the_thread_watch_ends_once_a_message_identified_the_conversation() {
    let (transport, listener, token) = loopback();
    let ready = Arc::new(AtomicBool::new(false));
    let seen = ready.clone();
    transport.attach(TransportEvents {
        ready: Box::new(move || seen.store(true, Ordering::Release)),
        server_gone: Box::new(|| {}),
        held: Box::new(|_| {}),
        released: Box::new(|_| {}),
    });
    let server = std::thread::spawn(move || {
        let mut socket = initialized(&listener, &token);
        // The watch reads nothing more and closes its connection.
        while receive(&mut socket).is_some() {}
    });
    let rpc = transport.endpoint.connect().unwrap();
    transport.shared.identify("main");
    watch_for_thread(rpc, transport.shared.clone());
    server.join().unwrap();
    assert!(!ready.load(Ordering::Acquire));
}

/// A start refused because the server unloaded the thread meanwhile loads it again, which runs its
/// queue, before anything is called uncertain.
#[test]
fn a_refused_start_on_an_unloaded_thread_loads_it_again() {
    let (transport, listener, token) = loopback();
    transport.shared.identify("main");
    let loaded = Arc::new(AtomicBool::new(false));
    let reloaded = loaded.clone();
    let server = serve(listener, token, move |method, params| match method {
        "thread/queue/start" => Answer::Error("not loaded"),
        "thread/queue/list" if reloaded.load(Ordering::Acquire) => {
            Answer::Result(json!({"data":[],"nextCursor":null}))
        }
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q1","clientUserMessageId":"id"}],"nextCursor":null}),
        ),
        "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"notLoaded"}}})),
        "thread/resume" => {
            reloaded.store(true, Ordering::Release);
            Answer::Result(json!({"thread":{}}))
        }
        other => idle_main(other, params),
    });
    assert_eq!(transport.send("hello", "id").unwrap(), Sent::Queued);
    drop(transport);
    let requests = server.join().unwrap();
    assert!(methods(&requests).contains(&"thread/resume"));
}

/// A running server that cannot be reached hands the message to the terminal while the session's
/// conversation is not known, and holds it once it is; one that has exited holds it, since its
/// terminal is about to end with it.
#[cfg(unix)]
#[test]
fn an_unreachable_server_holds_the_message_once_the_conversation_is_known() {
    let (transport, listener, _listen) = socket_transport();
    drop(listener);
    *transport.server.lock().unwrap() = Some(Box::new(stand_in_server("exec sleep 30", None)));
    assert!(matches!(
        transport.send("hello", "id").unwrap(),
        Sent::Declined(_)
    ));
    transport.shared.identify("main");
    let error = transport.send("hello", "id").unwrap_err();
    assert_eq!(
        (error.written, error.error.kind()),
        (0, io::ErrorKind::WouldBlock)
    );
    *transport.server.lock().unwrap() = Some(Box::new(stand_in_server("exit 0", None)));
    let deadline = Instant::now() + PATIENCE;
    while !transport.server_exited() {
        assert!(Instant::now() < deadline, "the stand-in did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
    let error = transport.send("hello", "id").unwrap_err();
    assert_eq!(
        (error.written, error.error.kind()),
        (0, io::ErrorKind::WouldBlock)
    );
}

#[test]
fn the_thread_watch_takes_the_first_user_thread_started() {
    let (transport, listener, token) = loopback();
    let ready = Arc::new(AtomicBool::new(false));
    let seen = ready.clone();
    transport.attach(TransportEvents {
        ready: Box::new(move || seen.store(true, Ordering::Release)),
        server_gone: Box::new(|| {}),
        held: Box::new(|_| {}),
        released: Box::new(|_| {}),
    });
    let server = std::thread::spawn(move || {
        let mut socket = initialized(&listener, &token);
        let started =
            |started: Value| json!({"method":"thread/started","params":{"thread":started}});
        // Everything up to the user thread is read before the watch can end: a request of the
        // server's, which the watch leaves unanswered, and the threads it passes over.
        for message in [
            json!({"id":7,"method":"item/commandExecution/requestApproval","params":{}}),
            started(thread("title", true, "thread_title", None)),
            started(thread("helper", false, "user", Some("parent"))),
            started(thread("main", false, "user", None)),
        ] {
            socket.send(Message::Text(message.to_string())).unwrap();
        }
        // The watch may already have taken `main` and closed its connection, which fails this
        // write; a later user thread does not replace the first either way.
        let _ = socket.send(Message::Text(
            started(thread("second", false, "user", None)).to_string(),
        ));
        // The watch ends once it has the thread, which closes its connection; it answered
        // nothing on the way.
        let mut answered = false;
        while let Some(message) = receive(&mut socket) {
            answered |= message.get("id").is_some();
        }
        assert!(!answered);
    });
    let rpc = transport.endpoint.connect().unwrap();
    watch_for_thread(rpc, transport.shared.clone());
    assert!(
        crate::session::wait::wait_for(PATIENCE, || ready.load(Ordering::Acquire)),
        "no thread was identified"
    );
    assert_eq!(transport.shared.thread().as_deref(), Some("main"));
    server.join().unwrap();
}

#[test]
fn events_before_the_session_attaches_are_passed_on_when_it_does() {
    let (transport, _listener, _) = loopback();
    transport.shared.ready();
    transport.shared.server_gone();
    transport.shared.events.lock().unwrap().held = true;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (ready, gone, held) = (calls.clone(), calls.clone(), calls.clone());
    transport.attach(TransportEvents {
        ready: Box::new(move || ready.lock().unwrap().push("ready")),
        server_gone: Box::new(move || gone.lock().unwrap().push("gone")),
        held: Box::new(move |_| held.lock().unwrap().push("held")),
        released: Box::new(|_| {}),
    });
    assert_eq!(*calls.lock().unwrap(), ["gone", "held", "ready"]);
}

#[test]
fn a_resume_applies_the_whole_permission_preset_to_the_thread() {
    let params = resume::resume_params(
        "saved-thread",
        Path::new("/workspace"),
        super::super::private_server::ResumePermissions {
            profile: ":workspace",
            approval_policy: "on-request",
            reviewer: "auto_review",
        },
    );
    assert_eq!(
        params,
        json!({
            "threadId":"saved-thread", "cwd":"/workspace", "permissions":":workspace",
            "approvalPolicy":"on-request", "approvalsReviewer":"auto_review"
        })
    );
}

#[test]
fn resume_fingerprints_only_the_generated_session_flag_hook() {
    let metadata = json!({"data":[{"hooks":[
        {"handlerType":"command","source":"sessionFlags","command":"/scratch/hook","key":"/<session-flags>/config.toml:stop:0:0","currentHash":"sha256:own"},
        {"handlerType":"command","source":"user","command":"/scratch/hook","key":"user-hook","currentHash":"sha256:user"},
        {"handlerType":"command","source":"sessionFlags","command":"user-custom-command","key":"other-hook","currentHash":"sha256:other"}
    ]}]});
    let overrides = resume::own_hook_trust(&metadata, &["/scratch/hook"]).unwrap();
    assert_eq!(overrides[0], "-c");
    assert_eq!(
        overrides[1],
        "hooks.state={\"/<session-flags>/config.toml:stop:0:0\"={trusted_hash=\"sha256:own\"}}"
    );
    let parsed: toml::Table = overrides[1].parse().unwrap();
    assert_eq!(parsed["hooks"]["state"].as_table().unwrap().len(), 1);
}

#[test]
fn resume_fingerprints_the_guard_hook_with_the_status_hook_and_nothing_else() {
    let metadata = json!({"data":[{"hooks":[
        {"handlerType":"command","source":"sessionFlags","command":"/scratch/hook","key":"status-key","currentHash":"sha256:status"},
        {"handlerType":"command","source":"sessionFlags","command":"/install/octoboardd guard-hook","key":"guard-key","currentHash":"sha256:guard"},
        {"handlerType":"command","source":"user","command":"/install/octoboardd guard-hook","key":"user-guard","currentHash":"sha256:user"},
        {"handlerType":"command","source":"sessionFlags","command":"user-custom-command","key":"other-hook","currentHash":"sha256:other"}
    ]}]});

    let overrides = resume::own_hook_trust(
        &metadata,
        &["/scratch/hook", "/install/octoboardd guard-hook"],
    )
    .unwrap();

    let parsed: toml::Table = overrides[1].parse().unwrap();
    let state = parsed["hooks"]["state"].as_table().unwrap();
    assert_eq!(state.len(), 2, "{state:?}");
    assert_eq!(
        state["status-key"]["trusted_hash"].as_str(),
        Some("sha256:status")
    );
    assert_eq!(
        state["guard-key"]["trusted_hash"].as_str(),
        Some("sha256:guard")
    );
    // Without a guard command the guard's hook is not trusted.
    let overrides = resume::own_hook_trust(&metadata, &["/scratch/hook"]).unwrap();
    let parsed: toml::Table = overrides[1].parse().unwrap();
    assert_eq!(parsed["hooks"]["state"].as_table().unwrap().len(), 1);
}

#[test]
fn a_loopback_server_is_given_only_the_token_hash_and_the_terminal_reads_the_token() {
    let listen = PrivateListen::loopback().unwrap();
    let crate::platform::ipc::Address::Loopback { token } = listen.address() else {
        unreachable!();
    };
    let token = token.to_string();
    assert_eq!(token.len(), 64);
    let args = server::server_args(&ServerEndpoint::Loopback {
        token: token.clone(),
        port: None,
    });
    assert_eq!(
        args[..6],
        [
            "app-server",
            "--listen",
            "ws://127.0.0.1:0",
            "--ws-auth",
            "capability-token",
            "--ws-token-sha256"
        ]
    );
    use sha2::Digest;
    assert_eq!(
        args[6],
        server::hex(&sha2::Sha256::digest(token.as_bytes()))
    );
    assert!(!args.iter().any(|arg| arg.contains(token.as_str())));
    let endpoint = Endpoint::Loopback {
        port: 50865,
        token: token.clone(),
    };
    assert_eq!(
        endpoint.client_args(),
        [
            "--remote",
            "ws://127.0.0.1:50865",
            "--remote-auth-token-env",
            rpc::TOKEN_VARIABLE
        ]
    );
    assert_eq!(
        endpoint.client_env(),
        [(rpc::TOKEN_VARIABLE.to_string(), token.clone())]
    );
}

#[test]
fn the_port_is_read_from_the_line_the_server_prints_once_it_listens() {
    use server::listening_port;
    assert_eq!(
        listening_port("  listening on: ws://127.0.0.1:50865\n"),
        Some(50865)
    );
    assert_eq!(
        listening_port("  readyz: http://127.0.0.1:50865/readyz"),
        None
    );
    assert_eq!(listening_port("listening on: ws://127.0.0.1:0"), None);
    assert_eq!(listening_port("codex app-server (WebSockets)"), None);
}

/// A line of the server's that is not UTF-8 is read as best it can be, and the lines after it
/// still arrive: a reader that stopped there would leave the pipe to fill and the server to block.
#[test]
fn a_line_that_is_not_utf8_does_not_stop_the_output_reader() {
    let output: &[u8] = b"codex \xff\xfe app-server\n  listening on: ws://127.0.0.1:4321\n";
    let lines = crate::platform::proc::server::read_lines_for_test(output);
    assert_eq!(lines.len(), 2);
    assert_eq!(server::listening_port(&lines[1]), Some(4321));
}

/// A server process standing in for Codex's, running `script`, the daemon's own as a local host
/// starts it, listening on `listen` when given.
#[cfg(unix)]
fn stand_in_server(script: &str, listen: Option<PrivateListen>) -> LocalServer {
    use crate::platform::proc::server::ServerChild;
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    let child = ServerChild::start(command, false, None, "stand-in server").unwrap();
    LocalServer::new(child, listen)
}

#[cfg(unix)]
#[test]
fn stopping_the_session_reaps_its_server_and_removes_the_socket() {
    let (transport, listener, listen) = socket_transport();
    let server = stand_in_server("exec sleep 30", Some(listen));
    let pid = server.pid();
    *transport.server.lock().unwrap() = Some(Box::new(server));
    transport.stop();
    assert!(!crate::platform::proc::crash::is_registered(pid));
    assert!(crate::platform::proc::unreaped_state(pid).is_none());
    let Endpoint::Socket(socket) = &transport.endpoint else {
        unreachable!("a Unix host listens on a socket");
    };
    let path = socket.parent().unwrap().to_path_buf();
    drop(transport);
    assert!(!path.exists());
    drop(listener);
}

/// Watches `transport`'s server as a started one is watched, its end reported to `gone`, which the
/// watch holds a reference to until it ends.
#[cfg(unix)]
fn watch(transport: &CodexTransport, gone: Arc<AtomicBool>) {
    let stopping = transport.shared.clone();
    watch_for_exit(
        transport.server.clone(),
        move || stopping.stopping.load(Ordering::Acquire),
        move || gone.store(true, Ordering::Release),
    );
}

#[cfg(unix)]
#[test]
fn a_server_that_dies_ends_the_session_and_a_stopped_one_does_not() {
    let (transport, _listener, _listen) = socket_transport();
    let gone = Arc::new(AtomicBool::new(false));
    *transport.server.lock().unwrap() = Some(Box::new(stand_in_server("exit 3", None)));
    watch(&transport, gone.clone());
    let deadline = Instant::now() + PATIENCE;
    while !gone.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "the server's end was not reported"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let (transport, _listener, _listen) = socket_transport();
    let gone = Arc::new(AtomicBool::new(false));
    *transport.server.lock().unwrap() = Some(Box::new(stand_in_server("exec sleep 30", None)));
    watch(&transport, gone.clone());
    transport.stop();
    // The watch lets go of `gone` as it ends, after which it can report nothing.
    assert!(
        crate::session::wait::wait_for(PATIENCE, || Arc::strong_count(&gone) == 1),
        "the watch did not end with the stop"
    );
    assert!(!gone.load(Ordering::Acquire));
}

/// The fake Codex a start test runs: this test binary, run as [`fake_app_server`].
#[cfg(unix)]
mod fake {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// The environment variable naming what the fake does.
    pub(super) const MODE: &str = "OCTOBOARD_FAKE_CODEX_MODE";
    /// The environment variable naming the file each start of the fake appends a line to: its pid,
    /// the socket it was told to listen on, and whether it was given hook trust.
    pub(super) const LOG: &str = "OCTOBOARD_FAKE_CODEX_LOG";
    /// The hook command the fake reports its session-flag hook as.
    pub(super) const HOOK: &str = "fake-hook";

    /// The binary that runs the fake.
    pub(super) fn binary() -> ResolvedBinary {
        ResolvedBinary {
            program: std::env::current_exe().expect("the test binary"),
            leading_args: [
                "fake-codex",
                "--exact",
                "adapter::codex::transport::tests::fake::fake_app_server",
                "--ignored",
                "--nocapture",
                "--quiet",
                "--test-threads=1",
                "--",
            ]
            .iter()
            .map(std::ffi::OsString::from)
            .collect(),
        }
    }

    /// One start of the fake, as its log line records it.
    pub(super) struct Started {
        pub(super) pid: u32,
        pub(super) socket: std::path::PathBuf,
        pub(super) trusted: bool,
    }

    pub(super) fn starts(log: &Path) -> Vec<Started> {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                Started {
                    pid: fields.next().unwrap().parse().unwrap(),
                    socket: fields.next().unwrap().into(),
                    trusted: fields.next() == Some("trusted"),
                }
            })
            .collect()
    }

    /// Runs as Codex's app server when [`binary`] starts it; does nothing in an ordinary run.
    #[test]
    #[ignore = "run as a process of its own by the tests that start the private server"]
    fn fake_app_server() {
        let args: Vec<String> = std::env::args().collect();
        let Some(at) = args.iter().position(|arg| arg == "--listen") else {
            return;
        };
        let socket = args[at + 1].strip_prefix("unix://").unwrap().to_string();
        let trusted = args.iter().any(|arg| arg.starts_with("hooks.state="));
        if let Ok(log) = std::env::var(LOG) {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(log)
                .unwrap();
            writeln!(
                file,
                "{}\t{socket}\t{}",
                std::process::id(),
                if trusted { "trusted" } else { "untrusted" }
            )
            .unwrap();
        }
        let mode = std::env::var(MODE).unwrap_or_default();
        match mode.as_str() {
            "exit" => std::process::exit(2),
            "silent" => loop {
                std::thread::sleep(Duration::from_secs(1));
            },
            _ => {}
        }
        // The start takes the socket's path appearing for the server listening, as Codex makes it
        // by linking the path to a socket elsewhere once that listens. Binding at the path itself
        // would make it appear before the listen, and a connection in between would be refused.
        let listening = format!("{socket}.listening");
        let listener = UnixListener::bind(&listening).unwrap();
        std::os::unix::fs::symlink(&listening, &socket).unwrap();
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mode = mode.clone();
            std::thread::spawn(move || {
                let Ok(mut socket) = tungstenite::accept(stream) else {
                    return;
                };
                while let Ok(message) = socket.read() {
                    let Ok(request) =
                        serde_json::from_str::<Value>(message.to_text().unwrap_or(""))
                    else {
                        continue;
                    };
                    if request.get("id").is_none() {
                        continue;
                    }
                    let result = match request["method"].as_str().unwrap_or_default() {
                        "hooks/list" => json!({"data":[{"hooks":[{"handlerType":"command",
                            "source":"sessionFlags","command":HOOK,
                            "key":"/<session-flags>/config.toml:stop:0:0",
                            "currentHash":"sha256:fake"}]}]}),
                        "thread/resume" if mode == "refuse-resume" => {
                            let _ = socket.send(Message::Text(
                                json!({"id":request["id"],
                                    "error":{"code":-32600,"message":"refused"}})
                                .to_string(),
                            ));
                            continue;
                        }
                        _ => json!({}),
                    };
                    let _ = socket.send(Message::Text(
                        json!({"id":request["id"],"result":result}).to_string(),
                    ));
                }
            });
        }
    }
}

/// Starts the fake Codex as a session's private server, doing `mode`, for a resume of `thread-1`
/// with a permission preset; gives the start's outcome and the fake's starts.
#[cfg(unix)]
fn start_fake(mode: &str) -> (Result<CodexTransport>, Vec<fake::Started>) {
    let scratch = crate::test_support::ScratchDir::new("codex-fake-server");
    let log = scratch.join("starts");
    let environment: HashMap<String, String> = [
        (fake::MODE.to_string(), mode.to_string()),
        (fake::LOG.to_string(), log.to_string_lossy().into_owned()),
    ]
    .into_iter()
    .collect();
    let plan = PrivateServerPlan {
        args: vec!["-c".into(), "model=\"m\"".into()],
        tui_args: Vec::new(),
        task: None,
        hook_command: fake::HOOK.into(),
        guard_command: None,
        resume_thread: Some("thread-1".into()),
        resume_permissions: Some(super::super::private_server::ResumePermissions {
            profile: ":workspace",
            approval_policy: "on-request",
            reviewer: "user",
        }),
    };
    let containment = Containment::create("test", "codex-fake-server").unwrap();
    let started = CodexTransport::start(
        &fake::binary(),
        &plan,
        &environment,
        &scratch,
        &LocalHost::new(&containment),
        "launch",
    );
    let starts = fake::starts(&log);
    (started, starts)
}

/// Whether the process `pid` is gone: no longer a process of this one's, nor running.
#[cfg(unix)]
fn gone(pid: u32) -> bool {
    !crate::test_support::is_running(pid as i32)
}

/// A resume starts the server again with trust for the generated hook, on a socket of its own,
/// and the terminal is given that socket; the first server and its socket are gone.
#[cfg(unix)]
#[test]
fn a_resume_restarts_the_server_with_hook_trust_and_attaches_to_the_new_one() {
    let (started, starts) = start_fake("serve");
    let transport = started.unwrap();
    assert_eq!(starts.len(), 2);
    assert!(!starts[0].trusted && starts[1].trusted);
    assert_ne!(starts[0].socket, starts[1].socket);
    assert!(gone(starts[0].pid));
    assert!(!starts[0].socket.parent().unwrap().exists());
    assert_eq!(
        transport.client_args(),
        [
            "--remote".to_string(),
            format!("unix://{}", starts[1].socket.display())
        ]
    );
    assert_eq!(transport.shared.thread().as_deref(), Some("thread-1"));
    drop(transport);
    assert!(gone(starts[1].pid));
    assert!(!starts[1].socket.parent().unwrap().exists());
}

/// A preset the server refuses fails the start, which the launch then makes without the server:
/// the server is ended and its socket removed.
#[cfg(unix)]
#[test]
fn a_refused_permission_preset_fails_the_start_and_leaves_nothing() {
    let (started, starts) = start_fake("refuse-resume");
    assert!(started.is_err());
    assert_eq!(starts.len(), 2);
    for start in &starts {
        assert!(gone(start.pid));
        assert!(!start.socket.parent().unwrap().exists());
    }
}

#[cfg(unix)]
#[test]
fn a_server_that_exits_or_never_listens_fails_the_start() {
    let error = start_fake("exit").0.err().unwrap();
    assert!(
        format!("{error:#}").contains("exited before it was ready"),
        "{error:#}"
    );
    // A server that never listens; tried again until the fake had started within the short
    // timeout, which a loaded machine may not let it do at the first attempt.
    let deadline = Instant::now() + PATIENCE;
    let (started, starts) = loop {
        server::START_TIMEOUT_OVERRIDE.with(|timeout| timeout.set(Some(Duration::from_secs(1))));
        let (started, starts) = start_fake("silent");
        server::START_TIMEOUT_OVERRIDE.with(|timeout| timeout.set(None));
        if !starts.is_empty() || Instant::now() >= deadline {
            break (started, starts);
        }
    };
    let error = started.err().unwrap();
    assert!(
        format!("{error:#}").contains("was not ready in time"),
        "{error:#}"
    );
    assert!(gone(starts[0].pid));
    assert!(!starts[0].socket.parent().unwrap().exists());
}

/// A listener on a loopback port, and the token a client has to present to it.
fn listening() -> (TcpListener, u16, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port, "the-token".to_string())
}

/// A fake server that takes every connection on a thread of its own, for a transport that opens
/// connections besides the one it sends over (the thread watch of an adopted one): each is answered
/// as `answer` says, one request at a time, and the requests are kept. Stops taking connections
/// when dropped.
struct ServedAll {
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    accepting: Option<std::thread::JoinHandle<()>>,
}

impl ServedAll {
    fn start(
        listener: TcpListener,
        token: String,
        answer: impl FnMut(&str, &Value) -> Answer + Send + 'static,
    ) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let answer = Arc::new(Mutex::new(answer));
        let (log, halt) = (requests.clone(), stop.clone());
        listener.set_nonblocking(true).unwrap();
        let accepting = std::thread::spawn(move || {
            while !halt.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let (token, answer, log) = (token.clone(), answer.clone(), log.clone());
                        std::thread::spawn(move || serve_connection(stream, &token, &answer, &log));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            requests,
            stop,
            accepting: Some(accepting),
        }
    }

    fn requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for ServedAll {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(accepting) = self.accepting.take() {
            accepting.join().ok();
        }
    }
}

/// Answers the requests on one connection, as [`serve_one`] does, until the client goes.
fn serve_connection<F: FnMut(&str, &Value) -> Answer>(
    stream: TcpStream,
    token: &str,
    answer: &Mutex<F>,
    log: &Mutex<Vec<(String, Value)>>,
) {
    let mut socket = handshake(stream, token);
    while let Some(request) = receive(&mut socket) {
        let method = request["method"].as_str().unwrap_or_default().to_string();
        log.lock()
            .unwrap()
            .push((method.clone(), request["params"].clone()));
        let outcome = {
            let mut answer = answer.lock().unwrap();
            (*answer)(&method, &request["params"])
        };
        let reply = match outcome {
            Answer::Result(result) => json!({"id":request["id"],"result":result}),
            Answer::Error(message) => {
                json!({"id":request["id"],"error":{"code":-32600,"message":message}})
            }
            Answer::Close => {
                let _ = socket.close(None);
                let _ = socket.flush();
                break;
            }
        };
        if socket.send(Message::Text(reply.to_string())).is_err() {
            break;
        }
    }
}

/// A server a holder runs, as the transport sees it: whether it has exited is the test's to say,
/// and the times it was asked to end are counted. `with_terminal` says whether it ends with its
/// terminal, as a holder's does.
struct HeldStandIn {
    gone: Arc<AtomicBool>,
    ended: Arc<AtomicUsize>,
    with_terminal: bool,
}

impl RunningServer for HeldStandIn {
    fn exited(&self) -> bool {
        self.gone.load(Ordering::Acquire)
    }

    fn end(&self) {
        self.ended.fetch_add(1, Ordering::AcqRel);
    }

    fn ends_with_its_terminal(&self) -> bool {
        self.with_terminal
    }
}

/// A [`HeldStandIn`], with whether it is gone and how often it was asked to end.
fn held_server(with_terminal: bool) -> (Box<dyn RunningServer>, Arc<AtomicBool>, Arc<AtomicUsize>) {
    let gone = Arc::new(AtomicBool::new(false));
    let ended = Arc::new(AtomicUsize::new(0));
    let server = HeldStandIn {
        gone: gone.clone(),
        ended: ended.clone(),
        with_terminal,
    };
    (Box::new(server), gone, ended)
}

/// The transport of the launch `launch` taken over by a daemon, to the server on `port`.
fn adopted(
    port: u16,
    token: &str,
    thread: Option<&str>,
    server: Box<dyn RunningServer>,
) -> CodexTransport {
    CodexTransport::adopt(
        ServerEndpoint::Loopback {
            token: token.to_string(),
            port: Some(port),
        },
        server,
        "launch",
        thread,
    )
    .expect("the endpoint names a port")
}

/// An adopted transport whose conversation is known queues for it at once: the connection it
/// sends over is the only one, and nothing is read to find the conversation. A message of its
/// launch is looked for in the queue before it is added.
#[test]
fn an_adopted_transport_with_the_conversation_known_queues_without_looking_for_it() {
    let (listener, port, token) = listening();
    let (server, _, _) = held_server(true);
    let transport = adopted(port, &token, Some("main"), server);
    assert_eq!(transport.shared.thread().as_deref(), Some("main"));
    let serving = serve(listener, token, idle_main);
    assert_eq!(transport.send("hello", "launch:7").unwrap(), Sent::Queued);
    drop(transport);
    let requests = serving.join().unwrap();
    let methods = methods(&requests);
    assert!(!methods.contains(&"thread/read"));
    let position = |wanted: &str| methods.iter().position(|method| *method == wanted);
    assert!(position("thread/queue/add").is_some(), "{methods:?}");
    assert!(
        position("thread/queue/list") < position("thread/queue/add"),
        "{methods:?}"
    );
}

/// Without the conversation the adopted transport finds it as any session's does: among the
/// threads the server holds loaded, once a message is written.
#[test]
fn an_adopted_transport_without_the_conversation_finds_it_among_the_loaded_threads() {
    let (listener, port, token) = listening();
    let (server, _, _) = held_server(true);
    let serving = ServedAll::start(listener, token.clone(), |method, params| match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["title","main"],"nextCursor":null})),
        "thread/read" if params["threadId"] == "title" => {
            Answer::Result(json!({"thread":thread("title", true, "thread_title", None)}))
        }
        "thread/read" => Answer::Result(json!({"thread":thread("main", false, "user", None)})),
        other => idle_main(other, params),
    });
    let transport = adopted(port, &token, None, server);
    assert_eq!(transport.shared.thread(), None);
    assert_eq!(transport.send("hello", "launch:7").unwrap(), Sent::Queued);
    assert_eq!(transport.shared.thread().as_deref(), Some("main"));
    drop(transport);
    assert!(methods(&serving.requests()).contains(&"thread/queue/add"));
}

/// A thread the user starts after the adoption is the session's conversation, which the watch
/// the adoption opened learns, and the session's queue is written out again.
#[test]
fn an_adopted_transport_without_the_conversation_takes_the_user_thread_started_later() {
    let (listener, port, token) = listening();
    let (server, _, _) = held_server(true);
    let transport = adopted(port, &token, None, server);
    let ready = Arc::new(AtomicBool::new(false));
    let seen = ready.clone();
    transport.attach(TransportEvents {
        ready: Box::new(move || seen.store(true, Ordering::Release)),
        server_gone: Box::new(|| {}),
        held: Box::new(|_| {}),
        released: Box::new(|_| {}),
    });
    let serving = std::thread::spawn(move || {
        let mut socket = initialized(&listener, &token);
        for started in [
            thread("title", true, "thread_title", None),
            thread("main", false, "user", None),
        ] {
            socket
                .send(Message::Text(
                    json!({"method":"thread/started","params":{"thread":started}}).to_string(),
                ))
                .unwrap();
        }
        // The watch ends once it has the thread, which closes its connection.
        while receive(&mut socket).is_some() {}
    });
    assert!(
        crate::session::wait::wait_for(PATIENCE, || ready.load(Ordering::Acquire)),
        "no thread was identified"
    );
    assert_eq!(transport.shared.thread().as_deref(), Some("main"));
    serving.join().unwrap();
}

/// A server that is gone when the daemon adopts its terminal is reported once the session
/// attaches, which ends the terminal as for a server that dies while the session runs.
#[test]
fn an_adopted_transport_over_a_server_that_is_gone_reports_it_once_the_session_attaches() {
    let (_listener, port, token) = listening();
    let (server, gone, _) = held_server(true);
    gone.store(true, Ordering::Release);
    let transport = adopted(port, &token, Some("main"), server);
    let reported = Arc::new(AtomicBool::new(false));
    let seen = reported.clone();
    transport.attach(TransportEvents {
        ready: Box::new(|| {}),
        server_gone: Box::new(move || seen.store(true, Ordering::Release)),
        held: Box::new(|_| {}),
        released: Box::new(|_| {}),
    });
    assert!(
        crate::session::wait::wait_for(PATIENCE, || reported.load(Ordering::Acquire)),
        "the server's end was not reported"
    );
}

/// What waits in the queue under an id that starts with the launch's was queued by Octoboard,
/// whichever daemon run queued it, so a cancel after a restart starts it as it would have been
/// started without the restart.
#[test]
fn messages_an_earlier_run_queued_are_octoboards_by_their_ids_and_started_after_a_cancel() {
    for (items, start, expected) in [
        (
            json!([{"id":"q-1","clientUserMessageId":"launch:41"},
                {"id":"q-2","clientUserMessageId":"launch:42"}]),
            Some(Answer::Result(json!({"turn":{}}))),
            QueueStart::Started(2),
        ),
        (
            json!([{"id":"user-item","clientUserMessageId":"typed"},
                {"id":"q-1","clientUserMessageId":"launch:41"}]),
            None,
            QueueStart::BehindOthers(1),
        ),
        (
            json!([{"id":"q-1","clientUserMessageId":"other:41"}]),
            None,
            QueueStart::Nothing,
        ),
        (
            // The prefix is the launch's id and a colon, not whatever starts with the id.
            json!([{"id":"q-1","clientUserMessageId":"launch-2:41"}]),
            None,
            QueueStart::Nothing,
        ),
    ] {
        let (listener, port, token) = listening();
        let (server, _, _) = held_server(true);
        let transport = adopted(port, &token, Some("main"), server);
        let start = Mutex::new(start);
        let serving = serve(listener, token, move |method, params| match method {
            "thread/queue/list" => Answer::Result(json!({"data":items,"nextCursor":null})),
            "thread/queue/start" => {
                assert_eq!(
                    params,
                    &json!({"threadId":"main","queuedSubmissionId":"q-1"})
                );
                start.lock().unwrap().take().expect("no start was expected")
            }
            // Read only after an accepted start: Codex reports the turn it began as running.
            "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
            other => panic!("unexpected request {other}"),
        });
        let started = transport.start_queue();
        drop(transport);
        serving.join().unwrap();
        assert_eq!(started, expected);
    }
}

/// A message found in the queue under its own id, which a daemon that is gone added, is started
/// where it waits and not added a second time.
#[test]
fn a_message_already_queued_under_its_id_is_started_in_place_and_not_added_again() {
    let (listener, port, token) = listening();
    let (server, _, _) = held_server(true);
    let transport = adopted(port, &token, Some("main"), server);
    let serving = serve(listener, token, |method, _| match method {
        "thread/loaded/list" => Answer::Result(json!({"data":["main"],"nextCursor":null})),
        "thread/queue/list" => Answer::Result(
            json!({"data":[{"id":"q-9","clientUserMessageId":"launch:7"}],"nextCursor":null}),
        ),
        "thread/queue/start" => Answer::Result(json!({"turn":{}})),
        // Read once after the accepted start, which waits for the turn to be reported running.
        "thread/read" => Answer::Result(json!({"thread":{"status":{"type":"active"}}})),
        other => panic!("unexpected request {other}"),
    });
    assert_eq!(transport.send("again", "launch:7").unwrap(), Sent::Queued);
    drop(transport);
    let requests = serving.join().unwrap();
    assert!(!methods(&requests).contains(&"thread/queue/add"));
    let starts: Vec<_> = requests
        .iter()
        .filter(|(method, _)| method == "thread/queue/start")
        .collect();
    assert_eq!(starts.len(), 1, "{requests:?}");
    assert_eq!(
        starts[0].1,
        json!({"threadId":"main","queuedSubmissionId":"q-9"})
    );
}

/// The server a holder runs ends with the terminal, so a session that stops first leaves it; one
/// the daemon runs is ended with the session.
#[test]
fn stopping_a_transport_leaves_a_server_its_holder_runs_and_ends_one_the_daemon_runs() {
    for (with_terminal, expected_ends) in [(true, 0), (false, 1)] {
        let (_listener, port, token) = listening();
        let (server, _, ended) = held_server(with_terminal);
        let transport = adopted(port, &token, Some("main"), server);
        transport.stop();
        assert_eq!(ended.load(Ordering::Acquire), expected_ends);
        // Stopped once: nothing more is done by a second stop, nor by the drop.
        drop(transport);
        assert_eq!(ended.load(Ordering::Acquire), expected_ends);
    }
}

/// A host whose servers are one fake app server the test listens for, which records what it was
/// asked and in which order.
struct RecordingHost {
    port: u16,
    token: String,
    calls: Arc<Mutex<Vec<String>>>,
    starts: Arc<Mutex<Vec<String>>>,
}

struct RecordedServer {
    calls: Arc<Mutex<Vec<String>>>,
}

impl RunningServer for RecordedServer {
    fn exited(&self) -> bool {
        false
    }

    fn end(&self) {
        self.calls.lock().unwrap().push("end server".to_string());
    }

    fn ends_with_its_terminal(&self) -> bool {
        false
    }
}

impl ServerHost for RecordingHost {
    fn open_endpoint(&self, _: ServerListen) -> Result<ServerEndpoint> {
        self.calls.lock().unwrap().push("open endpoint".to_string());
        Ok(ServerEndpoint::Loopback {
            token: self.token.clone(),
            port: None,
        })
    }

    fn start(
        &self,
        command: Command,
        ready: ServerReady<'_>,
        _: Duration,
    ) -> Result<(Box<dyn RunningServer>, Option<u16>)> {
        self.calls.lock().unwrap().push("start".to_string());
        self.starts
            .lock()
            .unwrap()
            .push(format!("{ready:?} {command:?}"));
        let server = RecordedServer {
            calls: self.calls.clone(),
        };
        Ok((Box::new(server), Some(self.port)))
    }

    fn end(&self) {
        self.calls.lock().unwrap().push("end host".to_string());
    }

    fn companion(&self) -> bool {
        false
    }
}

/// A resume starts the server twice through its host, the first to read the fingerprints of the
/// hooks to trust: the first goes, and with it its endpoint, before a new endpoint is opened for
/// the second, which the terminal is then given.
#[test]
fn a_resume_restarts_the_server_through_the_host_on_an_endpoint_of_its_own() {
    let (listener, port, token) = listening();
    let serving = ServedAll::start(listener, token.clone(), |method, _| match method {
        "hooks/list" => Answer::Result(json!({"data":[{"hooks":[{"handlerType":"command",
            "source":"sessionFlags","command":"the-hook",
            "key":"/<session-flags>/config.toml:stop:0:0","currentHash":"sha256:the-hash"}]}]})),
        "thread/resume" => Answer::Result(json!({"thread":{"id":"thread-1"}})),
        other => panic!("unexpected request {other}"),
    });
    let host = RecordingHost {
        port,
        token,
        calls: Arc::default(),
        starts: Arc::default(),
    };
    let plan = PrivateServerPlan {
        args: vec!["-c".into(), "model=\"m\"".into()],
        tui_args: Vec::new(),
        task: None,
        hook_command: "the-hook".into(),
        guard_command: None,
        resume_thread: Some("thread-1".into()),
        resume_permissions: Some(super::super::private_server::ResumePermissions {
            profile: ":workspace",
            approval_policy: "on-request",
            reviewer: "user",
        }),
    };
    let transport = CodexTransport::start(
        &ResolvedBinary::new("codex"),
        &plan,
        &HashMap::new(),
        Path::new("."),
        &host,
        "launch",
    )
    .expect("the server starts");

    assert_eq!(
        *host.calls.lock().unwrap(),
        [
            "open endpoint",
            "start",
            "end server",
            "open endpoint",
            "start"
        ]
    );
    let starts = host.starts.lock().unwrap().clone();
    assert!(!starts[0].contains("hooks.state"), "{}", starts[0]);
    assert!(starts[1].contains("hooks.state="), "{}", starts[1]);
    for start in &starts {
        assert!(
            start.contains(r#"PortAfter("listening on: ws://127.0.0.1:")"#),
            "{start}"
        );
    }
    assert_eq!(
        transport.client_args(),
        [
            "--remote".to_string(),
            format!("ws://127.0.0.1:{port}"),
            "--remote-auth-token-env".to_string(),
            rpc::TOKEN_VARIABLE.to_string(),
        ]
    );
    assert_eq!(transport.shared.thread().as_deref(), Some("thread-1"));
    drop(transport);
    drop(serving);
}

/// A loopback endpoint with no port names no place to reach, so nothing is linked to again.
#[test]
fn an_adopted_loopback_endpoint_without_a_port_names_nothing_to_reach() {
    let (server, _, ended) = held_server(true);
    let adopted = CodexTransport::adopt(
        ServerEndpoint::Loopback {
            token: "token".to_string(),
            port: None,
        },
        server,
        "launch",
        Some("main"),
    );
    assert!(adopted.is_err());
    assert_eq!(
        ended.load(Ordering::Acquire),
        0,
        "a server its holder runs is left to it"
    );
}

/// The transport the provider of `agent` rebuilds for an adopted launch of a session whose
/// conversation is `thread-1`, the server found with its terminal being `server`.
fn rebuilt_transport(
    agent: crate::protocol::AgentId,
    server: Option<crate::terminal::server_host::FoundServer>,
) -> Option<Arc<dyn crate::session::term::MessageTransport>> {
    crate::adapter::provider(agent)
        .rebuild_launch(crate::adapter::AdoptedLaunch {
            adoption: None,
            launch_id: "launch",
            agent_session_id: Some("thread-1"),
            server,
        })
        .transport
}

#[test]
fn only_codex_links_an_adopted_launch_to_the_server_found_with_its_terminal() {
    use crate::protocol::AgentId;
    use crate::terminal::server_host::FoundServer;
    let found = |port: Option<u16>| {
        let (server, _, _) = held_server(true);
        Some(FoundServer {
            endpoint: ServerEndpoint::Loopback {
                token: "token".to_string(),
                port,
            },
            server,
        })
    };
    assert!(rebuilt_transport(AgentId::CODEX, found(Some(1))).is_some());
    // None found, or one whose endpoint names no port: the session's messages go to its terminal.
    assert!(rebuilt_transport(AgentId::CODEX, None).is_none());
    assert!(rebuilt_transport(AgentId::CODEX, found(None)).is_none());
    // Other providers start no server of their own.
    assert!(rebuilt_transport(AgentId::GROK, found(Some(1))).is_none());
    assert!(rebuilt_transport(AgentId::TEST, found(Some(1))).is_none());
}
