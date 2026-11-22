//! This computer's own daemon's part of the clean-up: what its features have in effect outside
//! Octoboard's own directories (each account that shares its agent's default configuration), and
//! clearing all of it.
//!
//! The shell asks its own daemon on the two local routes that take the launch key alone
//! (`POST /system-changes`, `POST /system-changes/clear`), with the address and key the daemon
//! gave it at start, as it asks it to stop. It never goes through the window's connection: no
//! client connection can list or clear, a remote window's and a browser's included.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

use super::{Change, Cleared, DaemonFailure, Failed, Outcome};
use crate::sidecar::request::{self, Answer, Reply, Step};
use crate::sidecar::{self, DaemonAddress};

/// The `kind` of a failure of the daemon's part as a whole: the daemon could not be asked, or
/// what it answered could not be read.
pub const KIND: &str = "daemon";

/// The route that lists, and the one that clears.
const LIST: &str = "/system-changes";
const CLEAR: &str = "/system-changes/clear";

/// How long the daemon is given to say what is in effect: it reads its store.
const LIST_WAIT: Duration = Duration::from_secs(15);

/// How long the daemon is given to clear everything: each account's idle sessions are ended and
/// resumed around its change, one account after another.
const CLEAR_WAIT: Duration = Duration::from_secs(30 * 60);

/// What a request answers, once it has: the body of a `200`, or why there is none.
pub type Asked<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// What asks the daemon: the application's own daemon, but for the tests.
pub trait Daemon: Send + Sync {
    /// Posts to the route `path`, giving the daemon `wait` to answer.
    fn post<'a>(&'a self, path: &'static str, wait: Duration) -> Asked<'a>;
}

/// The daemon the application runs.
pub struct Local<'a>(pub &'a AppHandle);

impl Daemon for Local<'_> {
    fn post<'a>(&'a self, path: &'static str, wait: Duration) -> Asked<'a> {
        Box::pin(async move {
            let Some(address) = sidecar::current_address(self.0) else {
                return Err("this computer's daemon is not running".to_owned());
            };
            tauri::async_runtime::spawn_blocking(move || request(&address, path, wait))
                .await
                .map_err(|err| err.to_string())?
        })
    }
}

/// One `POST` to the daemon at `address` on `path`, with its launch key: the body of a `200`,
/// else what went wrong. Connecting and sending get a short while; reading the answer gets
/// `wait`, since a clearing answers only once it has ended.
fn request(address: &DaemonAddress, path: &str, wait: Duration) -> Result<String, String> {
    const CONNECT_WAIT: Duration = Duration::from_secs(10);
    let waits = |step| match step {
        Step::Read => wait,
        Step::Connect | Step::Send => CONNECT_WAIT,
    };
    let answer = request::post(address, path, Some("{}"), Reply::Whole, waits)
        .map_err(|err| format!("asking the daemon ({path}) failed: {err}"))?;
    answer_of(answer).map_err(|status| format!("the daemon answered {path} with {status}"))
}

/// The body of a `200` answer, or the status line of any other.
fn answer_of(answer: Answer) -> Result<String, String> {
    match answer.status() {
        Some(200) => Ok(answer.body),
        _ if answer.status_line.is_empty() => Err("nothing".to_owned()),
        _ => Err(answer.status_line),
    }
}

/// One modification as the daemon answers it; a kind this application has no words for is kept
/// as [`DaemonChange::Unknown`].
#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DaemonChange {
    AccountSharing {
        account: String,
        name: String,
        agent: String,
    },
    #[serde(other)]
    Unknown,
}

impl DaemonChange {
    fn into_change(self) -> Option<Change> {
        match self {
            DaemonChange::AccountSharing {
                account,
                name,
                agent,
            } => Some(Change::AccountSharing {
                account,
                name,
                agent,
            }),
            DaemonChange::Unknown => None,
        }
    }
}

#[derive(Deserialize, Debug)]
struct Listed {
    changes: Vec<DaemonChange>,
}

#[derive(Deserialize, Debug)]
struct Refusal {
    code: String,
    #[serde(default)]
    params: BTreeMap<String, String>,
    message: String,
}

#[derive(Deserialize, Debug)]
struct ResultOf {
    change: DaemonChange,
    #[serde(default)]
    failure: Option<Refusal>,
}

#[derive(Deserialize, Debug)]
struct FeatureFailure {
    kind: String,
    failure: Refusal,
}

#[derive(Deserialize, Debug)]
struct ClearedAll {
    results: Vec<ResultOf>,
    #[serde(default)]
    failed: Vec<FeatureFailure>,
}

/// Why the daemon's part as a whole failed.
fn failed(detail: impl Into<String>) -> Failed {
    Failed {
        kind: KIND,
        detail: detail.into(),
    }
}

/// The items of a kind this application does not know, as a failure beside the ones it does.
fn unknown(count: usize) -> Option<Failed> {
    (count > 0).then(|| {
        failed(format!(
            "{count} of the daemon's items are of a kind this application does not know"
        ))
    })
}

/// What the daemon's features have in effect, and why its part could not be read, if it could
/// not. Changes nothing.
pub async fn list(daemon: &dyn Daemon) -> (Vec<Change>, Vec<Failed>) {
    let body = match daemon.post(LIST, LIST_WAIT).await {
        Ok(body) => body,
        Err(detail) => return (Vec::new(), vec![failed(detail)]),
    };
    let listed: Listed = match serde_json::from_str(body.trim()) {
        Ok(listed) => listed,
        Err(err) => {
            return (
                Vec::new(),
                vec![failed(format!(
                    "the daemon's list could not be read: {err}"
                ))],
            )
        }
    };
    let total = listed.changes.len();
    let changes: Vec<Change> = listed
        .changes
        .into_iter()
        .filter_map(DaemonChange::into_change)
        .collect();
    let failures = unknown(total - changes.len()).into_iter().collect();
    (changes, failures)
}

/// Clears everything [`list`] names, and answers what became of each item: one the daemon refused
/// to clear carries its refusal. A failure beside them is the daemon's part failing as a whole,
/// or a feature of the daemon's that could not tell what it has.
pub async fn clear(daemon: &dyn Daemon) -> (Vec<Cleared>, Vec<Failed>) {
    let body = match daemon.post(CLEAR, CLEAR_WAIT).await {
        Ok(body) => body,
        Err(detail) => return (Vec::new(), vec![failed(detail)]),
    };
    let cleared: ClearedAll = match serde_json::from_str(body.trim()) {
        Ok(cleared) => cleared,
        Err(err) => {
            return (
                Vec::new(),
                vec![failed(format!(
                    "the daemon's answer to the clearing could not be read: {err}"
                ))],
            )
        }
    };
    let total = cleared.results.len();
    let results: Vec<Cleared> = cleared
        .results
        .into_iter()
        .filter_map(|result| {
            let change = result.change.into_change()?;
            let outcome = match result.failure {
                None => Outcome::Cleared,
                Some(refusal) => Outcome::AccountSharingLeft {
                    failure: DaemonFailure {
                        code: refusal.code,
                        params: refusal.params,
                        message: refusal.message,
                    },
                },
            };
            Some(Cleared { change, outcome })
        })
        .collect();
    let mut failures: Vec<Failed> = unknown(total - results.len()).into_iter().collect();
    failures.extend(
        cleared
            .failed
            .into_iter()
            .map(|feature| failed(format!("{}: {}", feature.kind, feature.failure.message))),
    );
    (results, failures)
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A daemon that answers every request with what it was given, noting the routes asked.
    pub struct StandIn {
        answer: Result<String, String>,
        pub asked: Mutex<Vec<&'static str>>,
    }

    impl StandIn {
        pub fn answering(body: &str) -> Self {
            Self {
                answer: Ok(body.to_owned()),
                asked: Mutex::default(),
            }
        }

        pub fn failing(detail: &str) -> Self {
            Self {
                answer: Err(detail.to_owned()),
                asked: Mutex::default(),
            }
        }

        pub fn routes(&self) -> Vec<&'static str> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl Daemon for StandIn {
        fn post<'a>(&'a self, path: &'static str, _wait: Duration) -> Asked<'a> {
            Box::pin(async move {
                self.asked.lock().unwrap().push(path);
                self.answer.clone()
            })
        }
    }

    pub const SHARING: &str = r#"{"kind":"account_sharing","account":"acct-1","name":"Work","agent":"claude","config_dir":"/home/u/.claude-work"}"#;

    pub fn sharing() -> Change {
        Change::AccountSharing {
            account: "acct-1".into(),
            name: "Work".into(),
            agent: "claude".into(),
        }
    }

    #[tokio::test]
    async fn the_list_is_asked_of_the_daemons_local_route() {
        let daemon = StandIn::answering(&format!(r#"{{"changes":[{SHARING}],"later":1}}"#));
        assert_eq!(list(&daemon).await, (vec![sharing()], vec![]));
        assert_eq!(daemon.routes(), ["/system-changes"]);
        let none = StandIn::answering("{\"changes\":[]}\n");
        assert_eq!(list(&none).await, (vec![], vec![]));
    }

    #[tokio::test]
    async fn a_list_that_cannot_be_read_is_the_daemons_part_failing() {
        let down = StandIn::failing("this computer's daemon is not running");
        assert_eq!(
            list(&down).await,
            (
                vec![],
                vec![failed("this computer's daemon is not running")]
            )
        );
        let garbled = StandIn::answering("not json");
        let (changes, failures) = list(&garbled).await;
        assert!(changes.is_empty());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, "daemon");
    }

    #[tokio::test]
    async fn an_item_of_a_kind_not_known_is_told_beside_the_ones_that_are() {
        let daemon = StandIn::answering(&format!(
            r#"{{"changes":[{SHARING},{{"kind":"later_feature","path":"/x"}}]}}"#
        ));
        let (changes, failures) = list(&daemon).await;
        assert_eq!(changes, [sharing()]);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, "daemon");
    }

    #[tokio::test]
    async fn clearing_asks_the_clearing_route_and_reads_each_result() {
        let body = format!(
            r#"{{"results":[{{"change":{SHARING},"kept":{{"dir":"/home/u/.claude","entries":["projects"]}}}},{{"change":{SHARING},"failure":{{"code":"account_in_use","params":{{"name":"Work"}},"message":"in use"}}}}],"failed":[]}}"#
        );
        let daemon = StandIn::answering(&body);
        let (results, failures) = clear(&daemon).await;
        assert_eq!(daemon.routes(), ["/system-changes/clear"]);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            results,
            [
                Cleared {
                    change: sharing(),
                    outcome: Outcome::Cleared
                },
                Cleared {
                    change: sharing(),
                    outcome: Outcome::AccountSharingLeft {
                        failure: DaemonFailure {
                            code: "account_in_use".into(),
                            params: BTreeMap::from([("name".to_owned(), "Work".to_owned())]),
                            message: "in use".into(),
                        }
                    }
                }
            ]
        );
    }

    #[tokio::test]
    async fn a_clearing_that_could_not_be_asked_is_the_daemons_part_failing() {
        let down = StandIn::failing("this computer's daemon is not running");
        assert_eq!(
            clear(&down).await,
            (
                vec![],
                vec![failed("this computer's daemon is not running")]
            )
        );
        let garbled = StandIn::answering("");
        let (results, failures) = clear(&garbled).await;
        assert!(results.is_empty());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, "daemon");
    }

    #[tokio::test]
    async fn a_feature_of_the_daemons_that_could_not_tell_is_a_failure_of_its_part() {
        let daemon = StandIn::answering(
            r#"{"results":[],"failed":[{"kind":"account_sharing","failure":{"code":"internal_error","params":{},"message":"unreadable"}}]}"#,
        );
        assert_eq!(
            clear(&daemon).await,
            (vec![], vec![failed("account_sharing: unreadable")])
        );
    }

    /// Only a `200` is an answer; anything else says its status line, and nothing says so.
    #[test]
    fn only_a_200_is_an_answer() {
        let answer = |status_line: &str, body: &str| Answer {
            status_line: status_line.to_owned(),
            body: body.to_owned(),
        };
        assert_eq!(
            answer_of(answer("HTTP/1.1 200 OK", "{\"changes\":[]}")),
            Ok("{\"changes\":[]}".to_owned())
        );
        assert_eq!(
            answer_of(answer("HTTP/1.1 202 Accepted", "")),
            Err("HTTP/1.1 202 Accepted".to_owned())
        );
        assert_eq!(
            answer_of(answer("HTTP/1.1 401 Unauthorized", "")),
            Err("HTTP/1.1 401 Unauthorized".to_owned())
        );
        assert_eq!(answer_of(answer("", "")), Err("nothing".to_owned()));
    }

    /// The daemon's own bodies, as `apps/daemon/testdata/system_changes_wire.json` holds them
    /// (its test in `apps/daemon/src/coordinator/system_changes/wire.rs` writes them from the
    /// daemon's types), read as this module reads them: each field the shell keeps, from the
    /// same file, so neither side's shapes can change alone.
    #[tokio::test]
    async fn the_daemons_own_bodies_are_read() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../daemon/testdata/system_changes_wire.json"
        ))
        .unwrap();
        let daemon = StandIn::answering(&fixture["listed"].to_string());
        assert_eq!(list(&daemon).await, (vec![sharing()], vec![]));
        let daemon = StandIn::answering(&fixture["cleared"].to_string());
        assert_eq!(
            clear(&daemon).await,
            (
                vec![
                    Cleared {
                        change: sharing(),
                        outcome: Outcome::Cleared
                    },
                    Cleared {
                        change: sharing(),
                        outcome: Outcome::AccountSharingLeft {
                            failure: DaemonFailure {
                                code: "account_in_use".into(),
                                params: BTreeMap::from([("name".to_owned(), "Work".to_owned())]),
                                message: "in use".into(),
                            }
                        }
                    }
                ],
                vec![failed("account_sharing: unreadable")]
            )
        );
    }

    /// The request goes to the daemon's port with its launch key, and its answer is read to the
    /// end of the connection.
    #[test]
    fn the_request_carries_the_launch_key_to_the_daemons_port() {
        use std::io::{BufRead, BufReader, Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let mut body = [0u8; 2];
            reader.read_exact(&mut body).unwrap();
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"changes\":[]}")
                .unwrap();
            request_line
        });
        let address = DaemonAddress {
            port,
            key: "ab12".to_owned(),
        };
        assert_eq!(
            request(&address, LIST, Duration::from_secs(30)),
            Ok("{\"changes\":[]}".to_owned())
        );
        assert_eq!(
            served.join().unwrap(),
            "POST /system-changes?key=ab12 HTTP/1.1\r\n"
        );
    }
}
