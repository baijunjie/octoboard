//! The status a program reports about itself through OSC 7501, the Program Status Protocol
//! (revision 0.4): a report's fields read out of its body, and the records a terminal keeps by id.
//!
//! A report is `OSC 7501 ; key=value:key=value… ST`. [`crate::terminal::queries::QueryScanner`]
//! cuts it out of the output whole; [`parse`] reads its body, and [`ProgramStatus`] keeps what the
//! reports say. The root record, the one a report without an `id` addresses, is the program's own
//! status, which a session may take as its status (`crate::session::program_status`).
//!
//! A report that breaks one of the limits, carries base64 that does not decode or text with a
//! control character, or names no known state, is discarded whole. A malformed pair (no `=`, an
//! empty key, a byte outside the value set) is skipped and the rest of the report is read; an
//! unknown key is ignored, and of a key given twice the last value counts.

use std::collections::HashMap;
use std::fmt::Write as _;

use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::Engine as _;

/// Standard base64 whose padding may be left out, as the protocol allows; where it is given, it
/// must be right.
const PADDING_OPTIONAL: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// The longest whole report, from its `ESC` to the end of its `ST`.
pub const LONGEST_REPORT: usize = 4096;
/// The longest key.
const LONGEST_KEY: usize = 16;
/// The longest `msg`, encoded and decoded.
const LONGEST_MSG_ENCODED: usize = 2732;
const LONGEST_MSG: usize = 2048;
/// The longest `title`, encoded and decoded.
const LONGEST_TITLE_ENCODED: usize = 256;
const LONGEST_TITLE: usize = 192;
/// The longest `app`.
const LONGEST_APP: usize = 32;
/// The longest `id`, its longest segment and its deepest nesting.
const LONGEST_ID: usize = 128;
const LONGEST_SEGMENT: usize = 32;
const DEEPEST_ID: usize = 8;
/// The most records a terminal keeps; the least recently updated one makes room for another.
pub const MOST_RECORDS: usize = 256;

/// The body of the answer to the support query (`OSC 7501 ; ? ST`), between `OSC` and `ST`: the
/// protocol is supported, with every state and kind of revision 0.4.
pub const SUPPORT_ANSWER: &str =
    "7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

/// A state a program reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportedState {
    /// At rest, waiting for input.
    Idle,
    /// Running.
    Working,
    /// Finished, its result not yet seen.
    Done,
    /// Held up by something only the user can do.
    Blocked,
    /// Failed and stopped.
    Error,
}

impl ReportedState {
    /// Whether a program in this state is in the middle of a turn: from the moment its root record
    /// goes to working or blocked from neither until it leaves both (a blocked program is held up
    /// within its turn, as an agent asking for a permission is).
    pub fn in_turn(self) -> bool {
        matches!(self, Self::Working | Self::Blocked)
    }

    /// The value of `state` that reports it.
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }
}

/// What a blocked program waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedKind {
    Permission,
    Question,
    Auth,
}

impl BlockedKind {
    /// The value of `kind` that reports it.
    fn name(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Auth => "auth",
        }
    }
}

/// A record a report sets: everything the report said, and nothing kept from the record it
/// replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub state: ReportedState,
    /// Only with [`ReportedState::Blocked`].
    pub kind: Option<BlockedKind>,
    /// 0 to 100, only with [`ReportedState::Working`] or [`ReportedState::Blocked`]; absent is
    /// indeterminate.
    pub progress: Option<u8>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub msg: Option<String>,
}

/// What a report asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// Replace the record `id` addresses (`None`, the root) with `record`.
    Set { id: Option<String>, record: Record },
    /// Remove the record `id` addresses and its children, or every record when `id` is `None`.
    Clear { id: Option<String> },
}

/// Reads the body of a report, the bytes between `OSC 7501 ;` and `ST`. `None` for a report to
/// discard.
pub fn parse(body: &[u8]) -> Option<Report> {
    let mut fields: HashMap<&str, &str> = HashMap::new();
    for pair in body.split(|&byte| byte == b':') {
        let Some(equals) = pair.iter().position(|&byte| byte == b'=') else {
            continue;
        };
        let (key, value) = (pair[..equals].trim_ascii(), pair[equals + 1..].trim_ascii());
        let key_fits = !key.is_empty() && key.iter().all(u8::is_ascii_lowercase);
        let value_fits = value.iter().all(|&byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
        });
        if !key_fits || !value_fits {
            continue;
        }
        if key.len() > LONGEST_KEY {
            return None;
        }
        // Both are ASCII, which their sets have just checked.
        let (Ok(key), Ok(value)) = (std::str::from_utf8(key), std::str::from_utf8(value)) else {
            continue;
        };
        fields.insert(key, value);
    }

    let state = match *fields.get("state")? {
        "idle" => Some(ReportedState::Idle),
        "working" => Some(ReportedState::Working),
        "done" => Some(ReportedState::Done),
        "blocked" => Some(ReportedState::Blocked),
        "error" => Some(ReportedState::Error),
        "clear" => None,
        _ => return None,
    };
    let id = match fields.get("id") {
        Some(id) => Some(valid_id(id)?.to_string()),
        None => None,
    };
    let app = match fields.get("app") {
        Some(app) if app.len() > LONGEST_APP => return None,
        Some(app) => Some(app.to_string()),
        None => None,
    };
    let title = match fields.get("title") {
        Some(title) => Some(text(title, LONGEST_TITLE_ENCODED, LONGEST_TITLE)?),
        None => None,
    };
    let msg = match fields.get("msg") {
        Some(msg) => Some(text(msg, LONGEST_MSG_ENCODED, LONGEST_MSG)?),
        None => None,
    };
    let Some(state) = state else {
        return Some(Report::Clear { id });
    };
    let kind = match fields.get("kind").copied() {
        Some("permission") => Some(BlockedKind::Permission),
        Some("question") => Some(BlockedKind::Question),
        Some("auth") => Some(BlockedKind::Auth),
        _ => None,
    }
    .filter(|_| state == ReportedState::Blocked);
    let progress = fields
        .get("progress")
        .filter(|progress| {
            progress.len() <= 3 && progress.bytes().all(|byte| byte.is_ascii_digit())
        })
        .and_then(|progress| progress.parse::<u8>().ok())
        .filter(|progress| *progress <= 100)
        .filter(|_| matches!(state, ReportedState::Working | ReportedState::Blocked));
    Some(Report::Set {
        id,
        record: Record {
            state,
            kind,
            progress,
            app,
            title,
            msg,
        },
    })
}

/// `id` if it is a valid id: 1 to 8 `/`-separated segments of 1 to 32 bytes from
/// `[A-Za-z0-9_.+-]`, 128 bytes in all.
fn valid_id(id: &str) -> Option<&str> {
    if id.len() > LONGEST_ID {
        return None;
    }
    let mut depth = 0;
    for segment in id.split('/') {
        depth += 1;
        let fits = !segment.is_empty()
            && segment.len() <= LONGEST_SEGMENT
            && segment.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-')
            });
        if !fits || depth > DEEPEST_ID {
            return None;
        }
    }
    Some(id)
}

/// The text a base64 value holds, padded or not: `None` where it is longer than `encoded` or, once
/// decoded, `decoded` bytes, is not UTF-8 or holds a control character.
fn text(value: &str, encoded: usize, decoded: usize) -> Option<String> {
    if value.len() > encoded {
        return None;
    }
    let bytes = PADDING_OPTIONAL.decode(value).ok()?;
    if bytes.len() > decoded {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

/// The records a terminal keeps, by id; the root record (`""`) is the one a report without an
/// `id` addresses.
#[derive(Debug, Default)]
pub struct ProgramStatus {
    records: HashMap<String, (Record, u64)>,
    /// Counts the updates, so the least recently updated record can be found.
    updates: u64,
}

impl ProgramStatus {
    /// Applies a report. Says whether the root record's state changed, which includes it coming
    /// or going.
    pub fn apply(&mut self, report: Report) -> bool {
        let before = self.root();
        match report {
            Report::Set { id, record } => {
                let id = id.unwrap_or_default();
                self.updates += 1;
                if !self.records.contains_key(&id) && self.records.len() >= MOST_RECORDS {
                    let oldest = self
                        .records
                        .iter()
                        .min_by_key(|(_, (_, updated))| *updated)
                        .map(|(id, _)| id.clone());
                    if let Some(oldest) = oldest {
                        self.records.remove(&oldest);
                    }
                }
                self.records.insert(id, (record, self.updates));
            }
            Report::Clear { id: None } => self.records.clear(),
            Report::Clear { id: Some(id) } => {
                let children = format!("{id}/");
                self.records
                    .retain(|kept, _| kept != &id && !kept.starts_with(&children));
            }
        }
        self.root() != before
    }

    /// Forgets the records of programs still running or held up (`working` and `blocked`), whatever
    /// their id, as a new shell prompt (OSC 133 A) and the end of the process do: what they said
    /// can no longer be true. `done`, `error` and `idle` records stay. Says whether the root record
    /// went.
    pub fn end_volatile(&mut self) -> bool {
        let before = self.root();
        self.records
            .retain(|_, (record, _)| !record.state.in_turn());
        self.root() != before
    }

    /// Forgets every record, as a terminal reset (RIS) does. Says whether the root record went.
    pub fn reset(&mut self) -> bool {
        let had_root = self.root().is_some();
        self.records.clear();
        had_root
    }

    /// The status reports that rebuild these records in a terminal that keeps none, each a whole
    /// `OSC 7501 ; … ST`, the least recently updated record's first: applied in order, they leave
    /// the same records, ranked alike for the one to make room for another.
    pub fn reports(&self) -> String {
        let mut records: Vec<_> = self.records.iter().collect();
        records.sort_by_key(|(_, (_, updated))| *updated);
        let mut reports = String::new();
        for (id, (record, _)) in records {
            reports.push_str("\x1b]7501;state=");
            reports.push_str(record.state.name());
            if !id.is_empty() {
                write!(reports, ":id={id}").expect("writing a String");
            }
            if let Some(kind) = record.kind {
                write!(reports, ":kind={}", kind.name()).expect("writing a String");
            }
            if let Some(progress) = record.progress {
                write!(reports, ":progress={progress}").expect("writing a String");
            }
            if let Some(app) = &record.app {
                write!(reports, ":app={app}").expect("writing a String");
            }
            for (key, text) in [("title", &record.title), ("msg", &record.msg)] {
                if let Some(text) = text {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
                    write!(reports, ":{key}={encoded}").expect("writing a String");
                }
            }
            reports.push_str("\x1b\\");
        }
        reports
    }

    /// The state of the root record, the program's own status; `None` while it has none.
    pub fn root(&self) -> Option<ReportedState> {
        self.records.get("").map(|(record, _)| record.state)
    }

    /// The record `id` addresses (`""`, the root).
    #[cfg(test)]
    pub fn record(&self, id: &str) -> Option<&Record> {
        self.records.get(id).map(|(record, _)| record)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    fn set(body: &str) -> Record {
        match parse(body.as_bytes()) {
            Some(Report::Set { record, .. }) => record,
            other => panic!("{body:?} read as {other:?}"),
        }
    }

    #[test]
    fn a_report_reads_its_fields() {
        let body = format!(
            "state=blocked:id=build/step-1:kind=question:progress=40:app=my.tool:title={}:msg={}",
            b64("Build"),
            b64("Which target?")
        );
        assert_eq!(
            parse(body.as_bytes()),
            Some(Report::Set {
                id: Some("build/step-1".into()),
                record: Record {
                    state: ReportedState::Blocked,
                    kind: Some(BlockedKind::Question),
                    progress: Some(40),
                    app: Some("my.tool".into()),
                    title: Some("Build".into()),
                    msg: Some("Which target?".into()),
                },
            })
        );
        assert_eq!(parse(b"state=clear"), Some(Report::Clear { id: None }));
        assert_eq!(
            parse(b"state=clear:id=a/b"),
            Some(Report::Clear {
                id: Some("a/b".into())
            })
        );
    }

    #[test]
    fn a_report_with_no_known_state_is_ignored() {
        for body in ["", "id=a", "state=sleeping", "state=WORKING", "state"] {
            assert_eq!(parse(body.as_bytes()), None, "{body:?}");
        }
    }

    #[test]
    fn fields_that_do_not_apply_are_dropped_and_malformed_pairs_skipped() {
        let record = set("state=idle:kind=permission:progress=50");
        assert_eq!((record.kind, record.progress), (None, None));
        let record = set("state=working:progress=101:kind=auth");
        assert_eq!((record.kind, record.progress), (None, None));
        let record = set("state=blocked:kind=other:progress=-1");
        assert_eq!((record.kind, record.progress), (None, None));
        // No `=`, an empty key, a key with capitals, a value outside the value set, an unknown key.
        let record = set("junk:=x:App=a:app=a b:colour=red: state = working ");
        assert_eq!(record.state, ReportedState::Working);
        assert_eq!(record.app, None);
        // Whitespace around keys and values is trimmed; a pair with a byte outside the sets, or a
        // malformed key however long, is skipped.
        let record = set("state=working:\tapp = x\t:app\u{e9}=y:KKKKKKKKKKKKKKKKKKKKK=1");
        assert_eq!(record.app.as_deref(), Some("x"));
        assert_eq!(set("state=working:progress=+5").progress, None);
        assert_eq!(set("state=working:progress=050").progress, Some(50));
        // The last value of a key given twice counts.
        assert_eq!(set("state=idle:state=done").state, ReportedState::Done);
        // Padding is optional.
        let unpadded = b64("ab").trim_end_matches('=').to_string();
        assert_eq!(
            set(&format!("state=idle:msg={unpadded}")).msg.as_deref(),
            Some("ab")
        );
    }

    #[test]
    fn a_report_breaking_a_limit_is_discarded_whole() {
        let long_key = format!("state=idle:{}=1", "k".repeat(17));
        let long_app = format!("state=idle:app={}", "a".repeat(33));
        let long_msg = format!("state=idle:msg={}", b64(&"m".repeat(2049)));
        let long_title = format!("state=idle:title={}", b64(&"t".repeat(193)));
        let long_id = format!("state=idle:id={}", vec!["a".repeat(32); 5].join("/"));
        let long_segment = format!("state=idle:id={}", "s".repeat(33));
        let deep_id = format!("state=idle:id={}", ["a"; 9].join("/"));
        for body in [
            long_key,
            long_app,
            long_msg,
            long_title,
            long_id,
            long_segment,
            deep_id,
            "state=idle:id=a//b".into(),
            "state=idle:msg=a".into(),
            "state=idle:msg=YQ===".into(),
            format!("state=idle:msg={}", b64("line\nbreak")),
            format!("state=idle:title={}", b64("bell\x07")),
        ] {
            assert_eq!(parse(body.as_bytes()), None, "{body:?}");
        }
        // At the limits, it is kept.
        assert!(parse(format!("state=idle:{}=1", "k".repeat(16)).as_bytes()).is_some());
        assert!(parse(format!("state=idle:msg={}", b64(&"m".repeat(2048))).as_bytes()).is_some());
        assert!(parse(format!("state=idle:id={}", ["a"; 8].join("/")).as_bytes()).is_some());
        // A pair whose value leaves the value set is only skipped: the report goes to the root.
        assert_eq!(parse(b"state=idle:id=a b"), parse(b"state=idle"));
    }

    #[test]
    fn each_report_replaces_its_record_and_clear_removes_children() {
        let mut status = ProgramStatus::default();
        assert!(status.apply(parse(b"state=working:progress=10:app=x").unwrap()));
        assert!(!status.apply(parse(b"state=working").unwrap()));
        assert_eq!(
            status.record("").unwrap().app,
            None,
            "nothing is kept from before"
        );
        assert!(!status.apply(parse(b"state=done:id=a").unwrap()));
        status.apply(parse(b"state=done:id=a/b").unwrap());
        status.apply(parse(b"state=done:id=ab").unwrap());
        assert!(!status.apply(parse(b"state=clear:id=a").unwrap()));
        assert_eq!(status.len(), 2, "the root and `ab` stay");
        assert!(status.apply(parse(b"state=idle").unwrap()));
        assert_eq!(status.root(), Some(ReportedState::Idle));
        assert!(status.apply(parse(b"state=clear").unwrap()));
        assert_eq!((status.root(), status.len()), (None, 0));
    }

    #[test]
    fn a_terminal_keeps_at_most_256_records_dropping_the_least_recently_updated() {
        let mut status = ProgramStatus::default();
        status.apply(parse(b"state=working").unwrap());
        for at in 0..MOST_RECORDS {
            status.apply(parse(format!("state=idle:id=r{at}").as_bytes()).unwrap());
        }
        assert_eq!(status.len(), MOST_RECORDS);
        assert_eq!(
            status.root(),
            None,
            "the root was the least recently updated"
        );
        status.apply(parse(b"state=idle:id=r0").unwrap());
        status.apply(parse(b"state=idle:id=extra").unwrap());
        assert!(status.record("r0").is_some());
        assert!(status.record("r1").is_none());
    }

    #[test]
    fn a_prompt_or_an_exit_forgets_working_and_blocked_records_only() {
        let mut status = ProgramStatus::default();
        status.apply(parse(b"state=working").unwrap());
        status.apply(parse(b"state=blocked:id=a").unwrap());
        status.apply(parse(b"state=done:id=b").unwrap());
        status.apply(parse(b"state=error:id=c").unwrap());
        status.apply(parse(b"state=idle:id=d").unwrap());
        assert!(status.end_volatile());
        assert_eq!(status.root(), None);
        assert!(status.record("a").is_none());
        for kept in ["b", "c", "d"] {
            assert!(status.record(kept).is_some(), "{kept}");
        }
        assert!(!status.end_volatile(), "nothing more to forget");

        let mut status = ProgramStatus::default();
        status.apply(parse(b"state=done").unwrap());
        assert!(!status.end_volatile());
        assert_eq!(status.root(), Some(ReportedState::Done));
    }

    /// The bodies of the reports [`ProgramStatus::reports`] wrote, in order.
    fn bodies(reports: &str) -> Vec<&str> {
        reports
            .split_terminator("\x1b\\")
            .map(|report| report.strip_prefix("\x1b]7501;").expect("a status report"))
            .collect()
    }

    #[test]
    fn the_reports_of_the_records_rebuild_them_in_the_order_they_were_updated() {
        let mut status = ProgramStatus::default();
        status.apply(parse(b"state=done:id=b").unwrap());
        let body = format!(
            "state=blocked:kind=question:progress=40:app=my.tool:title={}:msg={}",
            b64("Build"),
            b64("Which target? \u{e9}")
        );
        status.apply(parse(body.as_bytes()).unwrap());
        status.apply(parse(b"state=working:id=a/b:progress=0:app=").unwrap());
        status.apply(parse(b"state=error:id=b").unwrap());
        let reports = status.reports();
        assert!(
            reports.len() < LONGEST_REPORT,
            "each report within the limit"
        );

        let mut rebuilt = ProgramStatus::default();
        for body in bodies(&reports) {
            rebuilt.apply(parse(body.as_bytes()).expect("a report that is kept"));
        }
        assert_eq!(rebuilt.len(), 3);
        for id in ["", "a/b", "b"] {
            assert_eq!(rebuilt.record(id), status.record(id), "{id:?}");
        }
        assert_eq!(
            bodies(&rebuilt.reports())[0],
            "state=blocked:kind=question:progress=40:app=my.tool:title=QnVpbGQ=:msg=V2hpY2ggdGFyZ2V0PyDDqQ==",
            "the least recently updated first"
        );
        assert_eq!(rebuilt.reports(), reports);
        assert!(ProgramStatus::default().reports().is_empty());
    }

    #[test]
    fn a_reset_forgets_every_record() {
        let mut status = ProgramStatus::default();
        assert!(!status.reset());
        status.apply(parse(b"state=working").unwrap());
        assert!(status.reset());
        assert_eq!(status.root(), None);
    }
}
