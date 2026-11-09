//! The frames a daemon and a holder exchange over the daemon link (`GET /daemon`, a WebSocket).
//!
//! Control messages are JSON text frames tagged by `type`; bulk bytes (a terminal's output and its
//! tail) are binary frames. A receiver ignores a message type or a field it does not know, so a
//! holder and a daemon of builds a little apart keep talking. A request that expects a reply
//! carries a `req` number the reply repeats.
//!
//! The rule that keeps builds in the compatible range talking: a message may gain fields and an
//! enum may gain variants, never lose or change them. So every field a later build may add to a
//! struct that crosses the link is `#[serde(default)]`, a field is read when it is there and
//! ignored when it is not known, and every enum that can grow has a `#[serde(other)]` variant
//! that the receiver treats as something it does not understand and does nothing for. The
//! protocol is kept compatible within the promised range: `super::compat::FLOOR` only moves by
//! the previous-stable release rule, never to admit a change that breaks the protocol.

use std::io;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::proc::Polite;
use crate::platform::term::{Command, Foreground, Input, PartialWrite, Size, TermFacts};

/// Which launch's processes a terminal's process belongs to; the holder opens the launch's
/// containment from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainmentSpec {
    pub instance: String,
    pub launch: String,
}

/// What a holder is asked to start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnSpec {
    pub id: String,
    pub command: Command,
    pub size: Size,
    pub containment: Option<ContainmentSpec>,
    /// Kill everything left in the process's session once it has exited; see
    /// `terminal::pty::NewPty::kill_session_leftovers`.
    pub kill_session_leftovers: bool,
    /// The daemon's own record of what the terminal is. The holder keeps it and gives it back in
    /// the handshake, and reads none of it.
    #[serde(default)]
    pub meta: Value,
}

/// A signal by what is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    PoliteTerminate,
    PoliteHangUp,
    Kill,
    #[serde(other)]
    Unknown,
}

impl SignalKind {
    pub fn polite(how: Polite) -> Self {
        match how {
            Polite::Terminate => Self::PoliteTerminate,
            Polite::HangUp => Self::PoliteHangUp,
        }
    }

    /// The polite request this is, or `None` for a kill or a signal this build does not know.
    pub fn as_polite(self) -> Option<Polite> {
        match self {
            Self::PoliteTerminate => Some(Polite::Terminate),
            Self::PoliteHangUp => Some(Polite::HangUp),
            Self::Kill | Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryKind {
    Size,
    ForegroundJob,
    SlavePath,
    GroupHasMembers,
    ExitedUnreaped,
    #[serde(other)]
    Unknown,
}

/// The helper routes a holder relays to the daemon, as the daemon's own HTTP routes of the same
/// names (`crate::server`) take them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayRoute {
    Hook,
    GitRef,
    ContextWindow,
    RevertGate,
    Mcp,
    #[serde(other)]
    Unknown,
}

/// Daemon to holder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToHolder {
    /// The first frame of a link.
    Hello {
        version: String,
        build_id: String,
        /// Tells the daemon process from every other. A holder that was taken over from a daemon
        /// refuses that daemon's hello from then on, so a daemon that was stalled while another
        /// took over cannot take the holder back. Absent from builds that predate it, whose hello
        /// is accepted.
        #[serde(default)]
        run_id: Option<String>,
        /// The terminals this daemon handed over ([`Self::Handover`]) on a link that dropped
        /// since, when it links again on its way out: to write its and its clients' answers to the
        /// queries in their output before the mark, and to part. The holder lists them in the
        /// welcome but sends this link neither their tails nor their output, so no daemon is
        /// linked for their output and the holder goes on answering the queries in it it may.
        /// Empty from builds that predate it, and on every first link.
        #[serde(default)]
        handed_over: Vec<String>,
    },
    /// Starts a terminal. A second spawn of a known `id` is refused, never run again.
    Spawn(Box<SpawnSpec>),
    /// Writes `bytes` as they are, answered by [`FromHolder::Written`]. Taken for a terminal
    /// whether or not its output goes to this link: after a handover the stopping daemon still
    /// writes over it its own and its clients' answers to the queries in the output it read
    /// before the mark.
    Write {
        id: String,
        req: u64,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    /// Writes `inputs` encoded as the holder's host takes them, answered by
    /// [`FromHolder::Written`].
    WriteInput {
        id: String,
        req: u64,
        inputs: Vec<Input>,
    },
    Resize {
        id: String,
        req: u64,
        size: Size,
    },
    Query {
        id: String,
        req: u64,
        what: QueryKind,
    },
    Signal {
        id: String,
        signal: SignalKind,
    },
    HangUp {
        id: String,
    },
    PoliteStopLead {
        id: String,
        req: u64,
        polite: Polite,
    },
    /// Kills what the terminal's process started, other than the processes `identities` name
    /// (`<pid>:<start>`).
    KillLeftovers {
        id: String,
        req: u64,
        identities: Vec<String>,
    },
    Reap {
        id: String,
        req: u64,
    },
    /// The daemon has recorded the terminal's end; the holder forgets it.
    Release {
        id: String,
    },
    /// Sent before a planned disconnect. A link that drops without it counts as a crash.
    Parting {
        handover: bool,
    },
    /// The holder takes no new terminals and exits once its last one ends.
    Drain,
    /// Issues the key the helpers of the launch of the terminal `id` (and of the session
    /// `session`) call the holder with, answered by [`FromHolder::LaunchIssued`]. It goes ahead of
    /// the spawn, because the program's command line and environment name the key.
    IssueLaunch {
        id: String,
        req: u64,
        session: String,
        /// The MCP token of the launch, which the key is then good for alone on the MCP route.
        #[serde(default)]
        mcp_token: Option<String>,
    },
    /// The key of the launch `id` is not wanted after all (the spawn will not come).
    RevokeLaunch {
        id: String,
    },
    /// The daemon's answer to a relayed helper request.
    RelayReply {
        req: u64,
        status: u16,
        /// What the daemon's HTTP route would have answered with: JSON, or nothing.
        body: String,
    },
    /// The daemon hands the terminals this link follows over to the daemon that links next. Each
    /// one's output stops going to this link after a [`FromHolder::HandoverMark`] queued behind
    /// the output already sent, and its tail records where that mark falls under `token`, so that
    /// the next daemon can tell the output since the mark from the history before it. Answered by
    /// [`FromHolder::Done`] after every mark. Sent only to a holder whose welcome says it carries
    /// it out (`handover`).
    Handover {
        req: u64,
        token: String,
    },
    #[serde(other)]
    Unknown,
}

/// How the holder's last link to a daemon ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastParting {
    None,
    Handover,
    Crash,
    #[serde(other)]
    Unknown,
}

/// A terminal query a holder answers itself while no daemon is linked: one whose answer needs no
/// screen, given as the daemon gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineAnswer {
    /// The primary device attributes (`CSI c`, `CSI 0 c`).
    PrimaryDeviceAttributes,
    #[serde(other)]
    Unknown,
}

/// How old the output from a place in a tail on is: it was read `age_ms` milliseconds before the
/// welcome was made, or later. `offset` counts bytes from the handover's mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailAge {
    pub offset: u64,
    pub age_ms: u64,
}

/// Where a handover ([`ToHolder::Handover`]) marked a tail: the tail holds the history before the
/// mark and, from `since` on, the output since it, unless `truncated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoverTail {
    /// The token of the handover that marked the tail last.
    pub token: String,
    /// The offset into the tail sent with the welcome at which the output since the mark begins.
    /// Zero from builds that predate it, whose tail held nothing before the mark.
    #[serde(default)]
    pub since: u64,
    /// The mark has fallen off the front of the tail, which kept only the last of the output: some
    /// of the output since it was dropped, and `since` tells nothing.
    #[serde(default)]
    pub truncated: bool,
    /// How old the output since the mark is, by place, in the order of the places (offset 0 is the
    /// first byte after the mark); the output before the first place's is of no known age.
    #[serde(default)]
    pub ages: Vec<TailAge>,
}

/// One terminal a holder runs, as the handshake lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: String,
    pub pid: u32,
    /// The process's `<pid>:<start>`, when its start time could be read.
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub size: Option<Size>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default)]
    pub exited: bool,
    /// Present when a handover marked the terminal's tail: where the output since that handover's
    /// mark begins in it. Absent from builds that predate handovers.
    #[serde(default)]
    pub handover: Option<HandoverTail>,
}

/// What a query is answered with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    Size {
        size: Option<Size>,
    },
    ForegroundJob {
        foreground: ForegroundKind,
    },
    SlavePath {
        path: Option<String>,
    },
    /// A yes or no the host may not be able to give.
    Flag {
        value: Option<bool>,
    },
    Lead {
        millis: u64,
    },
    Swept {
        killed: usize,
        complete: bool,
    },
    Reaped {
        reaped: bool,
        error: Option<String>,
    },
    /// The holder runs no terminal of that id.
    UnknownTerminal,
    /// A reply of a kind this build does not know, or the answer to a query it does not know.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForegroundKind {
    Own,
    Other,
    Unsupported,
    #[serde(other)]
    Unknown,
}

impl From<Foreground> for ForegroundKind {
    fn from(foreground: Foreground) -> Self {
        match foreground {
            Foreground::Own => Self::Own,
            Foreground::Other => Self::Other,
            Foreground::Unsupported => Self::Unsupported,
        }
    }
}

impl From<ForegroundKind> for Foreground {
    fn from(foreground: ForegroundKind) -> Self {
        match foreground {
            ForegroundKind::Own => Self::Own,
            ForegroundKind::Other => Self::Other,
            ForegroundKind::Unsupported | ForegroundKind::Unknown => Self::Unsupported,
        }
    }
}

/// An I/O error as it crosses the link: its kind, so a caller that tells a timeout from a broken
/// pipe still can, and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub kind: String,
    pub message: String,
}

impl WireError {
    pub fn of(err: &io::Error) -> Self {
        Self {
            kind: format!("{:?}", err.kind()),
            message: err.to_string(),
        }
    }

    pub fn into_io(self) -> io::Error {
        let kind = match self.kind.as_str() {
            "BrokenPipe" => io::ErrorKind::BrokenPipe,
            "TimedOut" => io::ErrorKind::TimedOut,
            "WouldBlock" => io::ErrorKind::WouldBlock,
            "NotFound" => io::ErrorKind::NotFound,
            "PermissionDenied" => io::ErrorKind::PermissionDenied,
            "UnexpectedEof" => io::ErrorKind::UnexpectedEof,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, self.message)
    }
}

impl From<&PartialWrite> for WireError {
    fn from(failed: &PartialWrite) -> Self {
        Self::of(&failed.error)
    }
}

/// Holder to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromHolder {
    /// The answer to `hello`: who the holder is and what it runs. One binary [`BulkKind::Tail`]
    /// frame per terminal follows.
    Welcome {
        holder_id: String,
        instance_id: String,
        version: String,
        build_id: String,
        last_parting: LastParting,
        terminals: Vec<TerminalInfo>,
        facts: TermFacts,
        /// The terminal queries the holder answers itself, with the daemon's own answers, while
        /// no daemon is linked; what it reads while one is linked it leaves to that daemon. Empty
        /// from builds that predate it, which answer none.
        #[serde(default)]
        answers_offline: Vec<OfflineAnswer>,
        /// The holder carries out [`ToHolder::Handover`]. False from builds that predate it,
        /// which are never sent one.
        #[serde(default)]
        handover: bool,
    },
    Spawned {
        id: String,
        pid: u32,
        identity: Option<String>,
    },
    SpawnFailed {
        id: String,
        error: String,
    },
    /// The terminal's process has exited and everything it printed up to now has been sent.
    Exited {
        id: String,
        /// The PTY is still open, held by something the program left behind, so more output may
        /// follow, until the terminal is hung up or released.
        #[serde(default)]
        output_open: bool,
    },
    /// How a write ended: how many bytes went in, and why the rest did not. The count is only
    /// meant for a write that failed; one that went in whole has gone in whole.
    Written {
        req: u64,
        written: usize,
        error: Option<WireError>,
    },
    Done {
        req: u64,
        error: Option<String>,
    },
    Answer {
        req: u64,
        answer: Answer,
    },
    /// The answer to [`ToHolder::IssueLaunch`]; no key when the holder could not make one.
    LaunchIssued {
        req: u64,
        key: Option<String>,
    },
    /// A helper's request for the daemon, answered by [`ToHolder::RelayReply`]. A request that
    /// does not `waits` for an answer was answered to its helper already, and gets none.
    Relay {
        req: u64,
        route: RelayRoute,
        /// The path's parameter: the session id, or for [`RelayRoute::Mcp`] the token.
        param: String,
        #[serde(with = "base64_bytes")]
        body: Vec<u8>,
        waits: bool,
    },
    /// The helper that sent the relayed request `req` is gone, so its answer is not wanted.
    RelayGone {
        req: u64,
    },
    /// The last frame of a link that another link has taken the holder over from: the link is
    /// closed, and the holder is the new link's now. `by` is the run id of the daemon that link is
    /// from, when it sent one: a daemon taken over by a link of its own run (one it gave up whose
    /// connection got through late) treats this as a dropped link and opens it again; from
    /// another daemon, or with no run id, the holder is that daemon's, not to be linked to again
    /// or ended. Absent from builds that predate it.
    Superseded {
        #[serde(default)]
        by: Option<String>,
    },
    /// The terminal `id` is handed over ([`ToHolder::Handover`]): every frame of its output for
    /// this link came before this one, and none comes after it.
    HandoverMark {
        id: String,
    },
    #[serde(other)]
    Unknown,
}

/// What a binary frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkKind {
    /// What a terminal's process printed, as read.
    Output,
    /// A terminal's tail, once, right after the welcome.
    Tail,
}

impl BulkKind {
    fn byte(self) -> u8 {
        match self {
            Self::Output => 1,
            Self::Tail => 2,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Output),
            2 => Some(Self::Tail),
            _ => None,
        }
    }
}

/// A binary frame: `[kind: u8][id length: u16, big endian][terminal id][payload]`.
pub fn encode_bulk(kind: BulkKind, id: &str, payload: &[u8]) -> Vec<u8> {
    let id = id.as_bytes();
    let mut frame = Vec::with_capacity(3 + id.len() + payload.len());
    frame.push(kind.byte());
    frame.extend_from_slice(&(id.len() as u16).to_be_bytes());
    frame.extend_from_slice(id);
    frame.extend_from_slice(payload);
    frame
}

/// The kind, the terminal id and the payload of a binary frame; `None` for one of a kind this
/// build does not know and for a malformed one.
pub fn decode_bulk(frame: &[u8]) -> Option<(BulkKind, &str, &[u8])> {
    let kind = BulkKind::from_byte(*frame.first()?)?;
    let id_len = u16::from_be_bytes([*frame.get(1)?, *frame.get(2)?]) as usize;
    let id = std::str::from_utf8(frame.get(3..3 + id_len)?).ok()?;
    Some((kind, id, &frame[3 + id_len..]))
}

/// `Vec<u8>` as base64 text.
mod base64_bytes {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::platform::term::QuitKeys;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de>>(value: &T) -> T {
        serde_json::from_str(&serde_json::to_string(value).expect("encodes")).expect("decodes")
    }

    #[test]
    fn a_write_carries_its_bytes_whole_as_base64() {
        let bytes: Vec<u8> = (0..=255).collect();
        let frame = ToHolder::Write {
            id: "t".into(),
            req: 7,
            bytes: bytes.clone(),
        };
        let text = serde_json::to_string(&frame).unwrap();
        assert!(text.contains(r#""type":"write""#), "{text}");
        match round_trip(&frame) {
            ToHolder::Write {
                id,
                req,
                bytes: back,
            } => {
                assert_eq!((id.as_str(), req, back), ("t", 7, bytes));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_spawn_keeps_the_command_the_daemon_built() {
        let mut command = Command::new("/bin/echo");
        command
            .args(["a b", "c"])
            .env_clear()
            .env("K", "v")
            .cwd("/tmp");
        command.quit_keys(QuitKeys {
            key: &[Input::Ctrl('c')],
            presses: 2,
            apart: Duration::from_millis(100),
            wait: Duration::from_secs(1),
        });
        let spawn = ToHolder::Spawn(Box::new(SpawnSpec {
            id: "t".into(),
            command,
            size: Size { rows: 24, cols: 80 },
            containment: Some(ContainmentSpec {
                instance: "i".into(),
                launch: "l".into(),
            }),
            kill_session_leftovers: true,
            meta: serde_json::json!({"kind": "shell"}),
        }));
        let ToHolder::Spawn(back) = round_trip(&spawn) else {
            panic!("not a spawn");
        };
        let ToHolder::Spawn(sent) = spawn else {
            unreachable!()
        };
        assert_eq!(format!("{:?}", back.command), format!("{:?}", sent.command));
        assert_eq!(back.size, sent.size);
        assert_eq!(back.containment, sent.containment);
        assert!(back.kill_session_leftovers);
        assert_eq!(back.meta, sent.meta);
    }

    #[test]
    fn a_message_of_a_type_or_with_fields_this_build_does_not_know_is_ignored() {
        let unknown: ToHolder =
            serde_json::from_str(r#"{"type":"from_the_future","x":1}"#).unwrap();
        assert!(matches!(unknown, ToHolder::Unknown));
        let extra: ToHolder = serde_json::from_str(
            r#"{"type":"resize","id":"t","req":1,"size":{"rows":2,"cols":3},"new":true}"#,
        )
        .unwrap();
        assert!(matches!(extra, ToHolder::Resize { req: 1, .. }));
        let unknown: FromHolder = serde_json::from_str(r#"{"type":"later"}"#).unwrap();
        assert!(matches!(unknown, FromHolder::Unknown));
        // A takeover told by a build that names no run.
        let superseded: FromHolder = serde_json::from_str(r#"{"type":"superseded"}"#).unwrap();
        assert!(matches!(superseded, FromHolder::Superseded { by: None }));
    }

    #[test]
    fn an_enum_value_or_a_missing_field_a_later_build_adds_is_read_as_unknown_or_default() {
        let signal: ToHolder =
            serde_json::from_str(r#"{"type":"signal","id":"t","signal":"suspend"}"#).unwrap();
        assert!(matches!(
            signal,
            ToHolder::Signal {
                signal: SignalKind::Unknown,
                ..
            }
        ));
        let query: ToHolder =
            serde_json::from_str(r#"{"type":"query","id":"t","req":1,"what":"cwd"}"#).unwrap();
        assert!(matches!(
            query,
            ToHolder::Query {
                what: QueryKind::Unknown,
                ..
            }
        ));
        let answer: FromHolder =
            serde_json::from_str(r#"{"type":"answer","req":1,"answer":{"kind":"cwd"}}"#).unwrap();
        assert!(matches!(
            answer,
            FromHolder::Answer {
                answer: Answer::Unknown,
                ..
            }
        ));
        assert_eq!(
            serde_json::from_str::<LastParting>(r#""abandoned""#).unwrap(),
            LastParting::Unknown
        );
        assert_eq!(
            Foreground::from(serde_json::from_str::<ForegroundKind>(r#""nobody""#).unwrap()),
            Foreground::Unsupported
        );
        // A terminal listed with only what the first builds sent.
        let info: TerminalInfo = serde_json::from_str(r#"{"id":"t","pid":3}"#).unwrap();
        assert_eq!(
            (
                info.identity,
                info.size,
                info.meta,
                info.exited,
                info.handover
            ),
            (None, None, Value::Null, false, None)
        );
        // A welcome from a build that answers no query itself and carries out no handover.
        let welcome: FromHolder = serde_json::from_str(
            r#"{"type":"welcome","holder_id":"h","instance_id":"i","version":"v","build_id":"b",
                "last_parting":"none","terminals":[],"facts":{}}"#,
        )
        .unwrap();
        let FromHolder::Welcome {
            answers_offline,
            handover,
            ..
        } = welcome
        else {
            panic!("not a welcome");
        };
        assert!(answers_offline.is_empty() && !handover);
        assert_eq!(
            serde_json::from_str::<OfflineAnswer>(r#""cursor_position""#).unwrap(),
            OfflineAnswer::Unknown
        );
        let facts: crate::platform::term::TermFacts = serde_json::from_str("{}").unwrap();
        assert!(!facts.host_renders && !facts.paste_as_keys);
        assert_eq!(
            serde_json::to_string(&Polite::HangUp).unwrap(),
            r#""hang_up""#
        );
    }

    #[test]
    fn a_handover_and_the_tail_it_marked_cross_the_link() {
        let handover = ToHolder::Handover {
            req: 4,
            token: "tok".into(),
        };
        let text = serde_json::to_string(&handover).unwrap();
        assert!(text.contains(r#""type":"handover""#), "{text}");
        assert!(matches!(
            round_trip(&handover),
            ToHolder::Handover { req: 4, token } if token == "tok"
        ));
        let mark = FromHolder::HandoverMark { id: "t".into() };
        assert!(matches!(round_trip(&mark), FromHolder::HandoverMark { id } if id == "t"));
        // The hello of a link opened again to finish a parting names what it handed over; one
        // from a build that predates it names nothing.
        let hello = ToHolder::Hello {
            version: "v".into(),
            build_id: "b".into(),
            run_id: Some("run".into()),
            handed_over: vec!["t".into()],
        };
        assert!(matches!(
            round_trip(&hello),
            ToHolder::Hello { handed_over, .. } if handed_over == ["t"]
        ));
        let older: ToHolder =
            serde_json::from_str(r#"{"type":"hello","version":"v","build_id":"b"}"#).unwrap();
        assert!(matches!(
            older,
            ToHolder::Hello { run_id: None, handed_over, .. } if handed_over.is_empty()
        ));

        let tail = HandoverTail {
            token: "tok".into(),
            since: 4096,
            truncated: true,
            ages: vec![
                TailAge {
                    offset: 0,
                    age_ms: 900,
                },
                TailAge {
                    offset: 12,
                    age_ms: 40,
                },
            ],
        };
        assert_eq!(round_trip(&tail), tail);
        // A marked tail as a build that knows only its token tells it.
        let info: TerminalInfo =
            serde_json::from_str(r#"{"id":"t","pid":3,"handover":{"token":"tok"}}"#).unwrap();
        assert_eq!(
            info.handover,
            Some(HandoverTail {
                token: "tok".into(),
                since: 0,
                truncated: false,
                ages: Vec::new(),
            })
        );
    }

    #[test]
    fn a_partial_write_keeps_its_kind_across_the_link() {
        let failed = PartialWrite {
            written: 12,
            error: io::Error::new(io::ErrorKind::TimedOut, "stuck"),
        };
        let reply = FromHolder::Written {
            req: 3,
            written: failed.written,
            error: Some(WireError::from(&failed)),
        };
        let FromHolder::Written { written, error, .. } = round_trip(&reply) else {
            panic!("not a write reply");
        };
        let error = error.expect("the error").into_io();
        assert_eq!((written, error.kind()), (12, io::ErrorKind::TimedOut));
    }

    #[test]
    fn a_bulk_frame_names_its_kind_terminal_and_payload() {
        let frame = encode_bulk(BulkKind::Output, "terminal-1", b"\x1b[2Jhello");
        assert_eq!(&frame[..3], &[1, 0, 10]);
        let (kind, id, payload) = decode_bulk(&frame).expect("decodes");
        assert_eq!(
            (kind, id, payload),
            (BulkKind::Output, "terminal-1", &b"\x1b[2Jhello"[..])
        );
        let empty = encode_bulk(BulkKind::Tail, "", b"");
        let (kind, id, payload) = decode_bulk(&empty).unwrap();
        assert_eq!((kind, id, payload), (BulkKind::Tail, "", &b""[..]));
    }

    #[test]
    fn a_malformed_or_unknown_bulk_frame_decodes_to_nothing() {
        assert!(decode_bulk(&[]).is_none());
        assert!(decode_bulk(&[1, 0]).is_none());
        assert!(
            decode_bulk(&[1, 0, 9, b'a']).is_none(),
            "an id that runs past the frame"
        );
        assert!(
            decode_bulk(&[99, 0, 0]).is_none(),
            "a kind this build does not know"
        );
        assert!(
            decode_bulk(&[1, 0, 1, 0xff]).is_none(),
            "an id that is not text"
        );
    }
}
