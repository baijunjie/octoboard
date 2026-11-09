//! Answering the program's queries from the parsed terminal: which of them it answers and how,
//! and the display facts its colour and size answers are given with; and, once asked, keeping the
//! status the program reports through OSC 7501 out of what the clients are given. See
//! [`TerminalSnapshot::take_in`].

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::vte::ansi::NamedColor;

use super::{Request, TerminalSnapshot};
use crate::terminal::display::DisplayFacts;
use crate::terminal::program_status::{self, ProgramStatus, ReportedState};
use crate::terminal::queries::{Piece, Query, QueryScanner};

/// The private mode a program sets to be told when the colour scheme changes (`CSI ? 997 ; 1 n`
/// for dark, `2 n` for light), which the parsed terminal does not keep.
const COLOR_SCHEME_UPDATES: u16 = 2031;

/// The report of whether `display`'s colour scheme is dark or light, which answers `CSI ? 996 n`
/// and tells a program subscribed to [`COLOR_SCHEME_UPDATES`] of a change.
fn color_scheme_report(display: &DisplayFacts) -> String {
    let scheme = if display.dark() { 1 } else { 2 };
    format!("\x1b[?997;{scheme}n")
}

/// Whether the parsed terminal's answer to a mode query (DECRQM) is the one to give: the modes it
/// keeps as the clients' terminals keep them. Another mode's query, one it would call unknown or
/// answer for a mode a client keeps differently (the cursor's blinking, which only a client's
/// terminal draws, and synchronised output, which only a client's terminal holds a frame for),
/// passes through for a client to answer.
fn emulator_reports_mode(private: bool, mode: u16) -> bool {
    if !private {
        return matches!(mode, 4 | 20);
    }
    matches!(
        mode,
        1 | 6 | 7 | 25 | 1000 | 1002 | 1003 | 1004 | 1006 | 1049 | 2004
    )
}

/// The daemon answering the program's queries, and keeping its status reports once asked; see
/// [`TerminalSnapshot::answer_queries`] and [`TerminalSnapshot::keep_program_status`].
pub(super) struct QueryAnswering {
    scanner: QueryScanner,
    /// The host answers the device attributes, status and cursor reports itself, so those pass
    /// through.
    host_answers_reports: bool,
    display: DisplayFacts,
    /// The program has set [`COLOR_SCHEME_UPDATES`].
    color_scheme_updates: bool,
    /// The records of the program's status reports, kept once
    /// [`TerminalSnapshot::keep_program_status`] has asked for them.
    program_status: Option<ProgramStatus>,
}

/// What [`TerminalSnapshot::take_in`] made of a chunk of output.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TakenIn {
    /// The output for the clients: the chunk without the queries answered, and without a
    /// sequence its end cut short, which comes with the next chunk.
    pub delivered: Vec<u8>,
    /// The answers, in order, for the program's input.
    pub answers: Vec<u8>,
    /// The queries the chunk asked, in order, each with whether it was answered.
    pub asked: Vec<(Query, bool)>,
    /// The state the program's root status record moved to, each time the chunk moved it (a status
    /// report, a prompt start, a reset), in order; `None` where the record went. Empty unless
    /// [`TerminalSnapshot::keep_program_status`] has asked for the reports.
    pub program_status: Vec<Option<ReportedState>>,
}

impl TakenIn {
    /// Adds what `other`, taken in right after this, made of its chunk.
    pub fn append(&mut self, other: TakenIn) {
        self.delivered.extend(other.delivered);
        self.answers.extend(other.answers);
        self.asked.extend(other.asked);
        self.program_status.extend(other.program_status);
    }
}

impl TerminalSnapshot {
    /// Has [`Self::take_in`] answer the program's queries the parsed terminal can answer, and
    /// take them out of what the clients are given; the device attributes, status and cursor
    /// reports are left to the clients where `host_answers_reports` (the host has answered them
    /// before the output reaches the daemon, and a second answer would be read as input).
    pub fn answer_queries(&mut self, host_answers_reports: bool) {
        self.answering.get_or_insert_with(|| QueryAnswering {
            scanner: QueryScanner::default(),
            host_answers_reports,
            display: DisplayFacts::default(),
            color_scheme_updates: false,
            program_status: None,
        });
    }

    /// Has [`Self::take_in`] keep the program's status reports (OSC 7501) and take them out of
    /// what the clients are given, and answer the protocol's support query, which it otherwise
    /// passes through to the clients, none of which answers it. Answered whether or not the host
    /// answers the device attributes, status and cursor reports itself, since no host answers
    /// this one. Needs [`Self::answer_queries`] first.
    pub fn keep_program_status(&mut self) {
        if let Some(answering) = self.answering.as_mut() {
            answering
                .program_status
                .get_or_insert_with(ProgramStatus::default);
        }
    }

    /// Forgets the `working` and `blocked` records, as the end of the program's process does
    /// (see [`ProgramStatus::end_volatile`]). What that does to the root record is not reported:
    /// the end of the process settles its status.
    pub fn end_program_status(&mut self) {
        if let Some(status) = self.program_status_mut() {
            status.end_volatile();
        }
    }

    /// The records kept once [`Self::keep_program_status`] has asked for them.
    fn program_status_mut(&mut self) -> Option<&mut ProgramStatus> {
        self.answering
            .as_mut()
            .and_then(|answering| answering.program_status.as_mut())
    }

    /// Writes to `saved` what takes a fresh terminal that answers queries and keeps status reports
    /// to the state of this one's answering: the program's status records as reports, the least
    /// recently updated first, and the setting of [`COLOR_SCHEME_UPDATES`] where the program made
    /// it. A RIS before them forgets both, as it does here.
    pub(super) fn write_answering(&self, saved: &mut Vec<u8>) {
        let Some(answering) = self.answering.as_ref() else {
            return;
        };
        if let Some(status) = &answering.program_status {
            saved.extend_from_slice(status.reports().as_bytes());
        }
        if answering.color_scheme_updates {
            saved.extend_from_slice(format!("\x1b[?{COLOR_SCHEME_UPDATES}h").as_bytes());
        }
    }

    /// What the query scanner holds back: the start of a sequence that may still be a query,
    /// which neither parser has taken in yet.
    pub(super) fn held_back(&self) -> &[u8] {
        match &self.answering {
            Some(answering) => answering.scanner.held(),
            None => &[],
        }
    }

    /// The program's status records as [`ProgramStatus::reports`] writes them, for comparing two
    /// terminals' records.
    #[cfg(test)]
    pub(crate) fn program_status_reports(&self) -> String {
        self.answering
            .as_ref()
            .and_then(|answering| answering.program_status.as_ref())
            .map(ProgramStatus::reports)
            .unwrap_or_default()
    }

    /// Takes a chunk of the program's output in as [`Self::process`] does, answering the queries
    /// in it once [`Self::answer_queries`] has asked for that. A query is answered from the parsed
    /// terminal as it stands at that point of the output, with the colours and cell size of the
    /// display (see [`Self::set_display`]), and left out of what is delivered; a query it cannot
    /// answer is delivered for a client to answer.
    pub fn take_in(&mut self, bytes: &[u8]) -> TakenIn {
        self.take_in_allowing(bytes, |_| true)
    }

    /// [`Self::take_in`], answering or delivering only the queries `allowed` admits: one it does
    /// not admit is parsed and dropped, neither answered nor delivered, and not recorded as
    /// asked. Output that only looks like a query (status reports, prompt starts, private
    /// modes set) is not a query and is taken in as ever.
    pub fn take_in_allowing(
        &mut self,
        bytes: &[u8],
        mut allowed: impl FnMut(&Query) -> bool,
    ) -> TakenIn {
        let Some(pieces) = self
            .answering
            .as_mut()
            .map(|answering| answering.scanner.scan(bytes))
        else {
            self.process(bytes);
            return TakenIn {
                delivered: bytes.to_vec(),
                ..TakenIn::default()
            };
        };
        let mut taken = TakenIn::default();
        for piece in pieces {
            let (query, sequence) = match piece {
                Piece::Output(output) => {
                    // A reset (RIS) turns every mode off, the one the parsed terminal does not
                    // keep too.
                    if output.windows(2).any(|pair| pair == b"\x1bc") {
                        if let Some(answering) = self.answering.as_mut() {
                            answering.color_scheme_updates = false;
                            // And forgets the program's status records.
                            if let Some(status) = answering.program_status.as_mut() {
                                if status.reset() {
                                    taken.program_status.push(None);
                                }
                            }
                        }
                    }
                    self.process(&output);
                    taken.delivered.extend_from_slice(&output);
                    continue;
                }
                Piece::Sequence(query, sequence) => (query, sequence),
            };
            if let Query::PrivateModes { set, modes } = &query {
                if modes.contains(&COLOR_SCHEME_UPDATES) {
                    if let Some(answering) = self.answering.as_mut() {
                        answering.color_scheme_updates = *set;
                    }
                }
                self.process(&sequence);
                taken.delivered.extend_from_slice(&sequence);
                continue;
            }
            if query == Query::PromptStart {
                // The prompt is the clients' to draw; it only ends what running programs said.
                if let Some(status) = self.program_status_mut() {
                    if status.end_volatile() {
                        taken.program_status.push(status.root());
                    }
                }
                self.process(&sequence);
                taken.delivered.extend_from_slice(&sequence);
                continue;
            }
            if let Query::ProgramStatusReport(body) = &query {
                if let Some(status) = self.program_status_mut() {
                    let moved =
                        program_status::parse(body).is_some_and(|report| status.apply(report));
                    if moved {
                        taken.program_status.push(status.root());
                    }
                    continue;
                }
                self.process(&sequence);
                taken.delivered.extend_from_slice(&sequence);
                continue;
            }
            let requests = self.parse_query(&sequence);
            if !allowed(&query) {
                continue;
            }
            let answer = self.answer(&query, requests);
            taken.asked.push((query, answer.is_some()));
            match answer {
                Some(answer) => taken.answers.extend_from_slice(&answer),
                None => taken.delivered.extend_from_slice(&sequence),
            }
        }
        taken
    }

    /// Parses a query's sequence, with what the parsed terminal asks as it does.
    fn parse_query(&mut self, sequence: &[u8]) -> Vec<Request> {
        *self.events.requests() = Some(Vec::new());
        self.unended.advance(sequence);
        self.unexposed_parser.advance(&mut self.unexposed, sequence);
        let watching_normal = self.watches_normal();
        self.parser.advance(&mut self.terminal, sequence);
        if watching_normal {
            self.note_normal();
        }
        self.events.requests().take().unwrap_or_default()
    }

    /// The answer to `query`, given what the parsed terminal asked as it parsed it; `None` when
    /// the query is the clients' to answer.
    fn answer(&self, query: &Query, requests: Vec<Request>) -> Option<Vec<u8>> {
        let answering = self.answering.as_ref()?;
        let display = answering.display;
        let own = match *query {
            Query::DeviceAttributes
            | Query::SecondaryAttributes
            | Query::Status
            | Query::CursorPosition
                if answering.host_answers_reports =>
            {
                return None
            }
            Query::Mode {
                private: true,
                mode: COLOR_SCHEME_UPDATES,
            } => {
                let state = if answering.color_scheme_updates { 1 } else { 2 };
                Some(format!("\x1b[?{COLOR_SCHEME_UPDATES};{state}$y"))
            }
            Query::Mode { private, mode } if !emulator_reports_mode(private, mode) => return None,
            Query::ColorScheme => Some(color_scheme_report(&display)),
            Query::CellPixels => Some(format!("\x1b[6;{};{}t", display.cell.1, display.cell.0)),
            Query::ProgramStatusSupport { st } => {
                // Unsupported where nothing keeps the reports: none of the clients answers it.
                answering.program_status.as_ref()?;
                let end = if st { "\x1b\\" } else { "\x07" };
                Some(format!("\x1b]{}{end}", program_status::SUPPORT_ANSWER))
            }
            Query::PrivateModes { .. } => return None,
            _ => None,
        };
        if let Some(own) = own {
            return Some(own.into_bytes());
        }
        let mut answer = String::new();
        for request in requests {
            match request {
                Request::Text(text) => answer.push_str(&text),
                Request::Color(index, write) => {
                    // A colour the program set itself is the one it reads back.
                    let reported = match index.checked_sub(NamedColor::Foreground as usize) {
                        Some(0) => display.foreground,
                        Some(1) => display.background,
                        _ => display.cursor,
                    };
                    answer.push_str(&write(self.terminal.colors()[index].unwrap_or(reported)));
                }
                Request::Area(write) => answer.push_str(&write(WindowSize {
                    num_lines: self.terminal.screen_lines() as u16,
                    num_cols: self.terminal.columns() as u16,
                    cell_width: display.cell.0,
                    cell_height: display.cell.1,
                })),
            }
        }
        (!answer.is_empty()).then(|| answer.into_bytes())
    }

    /// Sets the display the program's colour and size queries are answered with. Returns what to
    /// tell the program: that the colour scheme changed, where the colours did and the program
    /// subscribed to such changes ([`COLOR_SCHEME_UPDATES`]), and nothing otherwise.
    pub fn set_display(&mut self, display: DisplayFacts) -> Vec<u8> {
        let Some(answering) = self.answering.as_mut() else {
            return Vec::new();
        };
        let changed = answering.display.colors_differ(&display);
        answering.display = display;
        if !changed || !answering.color_scheme_updates {
            return Vec::new();
        }
        color_scheme_report(&display).into_bytes()
    }
}
