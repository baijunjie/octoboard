//! Switches on what the test-only provider declares, for the tests of what the generic code does
//! with a provider's declaration: its trust screen (a screen that waits for bracketed paste,
//! `crate::adapter::test_provider::PASTE_WAITING_SCREEN`, say), its interrupt key, a screen reader
//! of its own (an input box read off the screen as drawn, say), hooks of its own, what its status
//! reports say beside its status integration, the done gap its reports declare, a private server,
//! an extension channel, a launch without its status integration, the wait for its tool listing
//! and its launch's further notices. The test-only provider is the one whose screen reader and
//! interpreter claim nothing, so a test of generic code that wants a declaration puts it there
//! rather than naming a registered provider. Each switch holds on the thread that took it, as the
//! variants of `crate::providers::test_provider` do: work a test hands to another thread sees the
//! plain provider.

use std::cell::Cell;
use std::marker::PhantomData;
use std::thread::LocalKey;
use std::time::Duration;

use crate::protocol::Notice;

use crate::hooks::HookInterpreter;
use crate::session::screen::{Interrupt, ScreenReader};
use crate::session::trust::Screen;
use crate::session::ReportedSignals;

/// What a test has the provider answer for its launch's further notices.
type FurtherNotices = fn() -> Vec<Notice>;

thread_local! {
    static TRUST_SCREEN: Cell<Option<&'static Screen>> = const { Cell::new(None) };
    static INTERRUPT: Cell<Option<&'static Interrupt>> = const { Cell::new(None) };
    static HOOKS: Cell<Option<&'static dyn HookInterpreter>> = const { Cell::new(None) };
    static SCREEN: Cell<Option<&'static dyn ScreenReader>> = const { Cell::new(None) };
    static REPORTED_SIGNALS: Cell<Option<ReportedSignals>> = const { Cell::new(None) };
    static REPORTED_DONE_GAP: Cell<Option<Duration>> = const { Cell::new(None) };
    static PRIVATE_SERVER: Cell<Option<()>> = const { Cell::new(None) };
    static EXTENSION_CHANNEL: Cell<Option<()>> = const { Cell::new(None) };
    static PROMPT_UNREADABLE_AFTER_INPUT: Cell<Option<()>> = const { Cell::new(None) };
    static LAUNCH_WITHOUT_STATUS_INTEGRATION: Cell<Option<()>> = const { Cell::new(None) };
    static TOOL_LISTING_WAIT: Cell<Option<Duration>> = const { Cell::new(None) };
    static FURTHER_NOTICES: Cell<Option<FurtherNotices>> = const { Cell::new(None) };
}

/// What each `declaring_*` switch of this module returns; what the thread declared before
/// (nothing, when none) is back when it drops. It stays on the thread that took it.
pub(crate) struct Declared<T: Copy + 'static> {
    cell: &'static LocalKey<Cell<Option<T>>>,
    previous: Option<T>,
    _not_send: PhantomData<*const ()>,
}

impl<T: Copy + 'static> Declared<T> {
    fn new(cell: &'static LocalKey<Cell<Option<T>>>, value: T) -> Self {
        Self {
            cell,
            previous: cell.with(|held| held.replace(Some(value))),
            _not_send: PhantomData,
        }
    }
}

impl<T: Copy + 'static> Drop for Declared<T> {
    fn drop(&mut self) {
        self.cell.with(|held| held.set(self.previous));
    }
}

/// The trust screen a test has the test-only provider declare on this thread
/// ([`declaring_trust_screen`]), as its screen reader reads it when a watch is made.
pub(crate) fn declared_trust_screen() -> Option<&'static Screen> {
    TRUST_SCREEN.with(Cell::get)
}

/// The test-only provider's trust screen is `screen`, for the sessions this thread makes while the
/// guard lives. The default is the screen of a provider nothing has been measured of.
pub(crate) fn declaring_trust_screen(screen: &'static Screen) -> Declared<&'static Screen> {
    Declared::new(&TRUST_SCREEN, screen)
}

/// The interrupt a test has the test-only provider declare on this thread
/// ([`declaring_interrupt`]).
pub(crate) fn declared_interrupt() -> Option<&'static Interrupt> {
    INTERRUPT.with(Cell::get)
}

/// The test-only provider interrupts a turn as `interrupt` says, for this thread while the guard
/// lives. By default it declares no key.
pub(crate) fn declaring_interrupt(interrupt: &'static Interrupt) -> Declared<&'static Interrupt> {
    Declared::new(&INTERRUPT, interrupt)
}

/// The screen reader a test has the test-only provider read its screens with on this thread
/// ([`declaring_screen`]).
pub(crate) fn declared_screen() -> Option<&'static dyn ScreenReader> {
    SCREEN.with(Cell::get)
}

/// The test-only provider's screens are read by `reader`, whole, for this thread while the guard
/// lives: what the reader leaves at its defaults is not taken from
/// `crate::adapter::test_provider::TestScreen`. Another thread, such as one a test draws the
/// agent's output from, still reads with that one, which finds no box on a screen of the test's
/// own drawing.
pub(crate) fn declaring_screen(
    reader: &'static dyn ScreenReader,
) -> Declared<&'static dyn ScreenReader> {
    Declared::new(&SCREEN, reader)
}

/// The hook interpreter a test has the test-only provider declare on this thread
/// ([`declaring_hooks`]).
pub(crate) fn declared_hooks() -> Option<&'static dyn HookInterpreter> {
    HOOKS.with(Cell::get)
}

/// The test-only provider reads its hooks as `interpreter` does, for this thread while the guard
/// lives. By default its interpreter claims nothing.
pub(crate) fn declaring_hooks(
    interpreter: &'static dyn HookInterpreter,
) -> Declared<&'static dyn HookInterpreter> {
    Declared::new(&HOOKS, interpreter)
}

/// What a test has the test-only provider declare its status reports say beside a status
/// integration of its own ([`declaring_reported_signals`]).
pub(crate) fn declared_reported_signals() -> Option<ReportedSignals> {
    REPORTED_SIGNALS.with(Cell::get)
}

/// The test-only provider has a status integration of its own, and its status reports say
/// `signals` beside it, for the sessions this thread makes while the guard lives. By default it has
/// none, and its status comes from its reports.
pub(crate) fn declaring_reported_signals(signals: ReportedSignals) -> Declared<ReportedSignals> {
    Declared::new(&REPORTED_SIGNALS, signals)
}

/// The gap a test has the test-only provider declare its `done` reports may be inside a turn
/// ([`declaring_reported_done_gap`]).
pub(crate) fn declared_reported_done_gap() -> Option<Duration> {
    REPORTED_DONE_GAP.with(Cell::get)
}

/// A `done` the test-only provider reports straight after `working` may be a gap inside the turn
/// lasting up to `bound` (`AgentProvider::reported_done_gap`), for the sessions this thread makes
/// while the guard lives. By default a `done` ends the turn. Under test the window a held `done`
/// waits in does not run on `bound`: it ends on the test's word
/// (`LiveSession::end_reported_done_gaps`).
pub(crate) fn declaring_reported_done_gap(bound: Duration) -> Declared<Duration> {
    Declared::new(&REPORTED_DONE_GAP, bound)
}

/// Whether a test has the test-only provider's launches start a server of its own on this thread
/// ([`declaring_private_server`]).
pub(crate) fn declares_private_server() -> bool {
    PRIVATE_SERVER.with(Cell::get).is_some()
}

/// The test-only provider's launch plans carry a server its messages go through
/// (`crate::adapter::test_provider::server`), for the plans this thread makes while the guard
/// lives. By default it has none.
#[cfg(unix)]
pub(crate) fn declaring_private_server() -> Declared<()> {
    Declared::new(&PRIVATE_SERVER, ())
}

/// How long a test has the test-only provider's launches wait for their MCP server to have
/// answered a listing of its tools, on this thread ([`declaring_tool_listing_wait`]).
pub(crate) fn declared_tool_listing_wait() -> Option<Duration> {
    TOOL_LISTING_WAIT.with(Cell::get)
}

/// The launches this thread makes of the test-only provider hold what is written into them for
/// `bound`, or until their MCP server has answered a listing of its tools
/// (`AgentProvider::awaits_tool_listing`); a stand-in session made meanwhile waits the same. By
/// default nothing waits.
pub(crate) fn declaring_tool_listing_wait(bound: Duration) -> Declared<Duration> {
    Declared::new(&TOOL_LISTING_WAIT, bound)
}

/// What else a test has the test-only provider tell the user about a launch, on this thread
/// ([`declaring_further_launch_notices`]).
pub(crate) fn declared_further_launch_notices() -> Vec<Notice> {
    FURTHER_NOTICES
        .with(Cell::get)
        .map_or_else(Vec::new, |notices| notices())
}

/// The launches this thread makes of the test-only provider tell the user what `notices` gives,
/// beside their plan's own notice (`AgentProvider::further_launch_notices`). By default a launch
/// has nothing more to tell.
pub(crate) fn declaring_further_launch_notices(
    notices: FurtherNotices,
) -> Declared<FurtherNotices> {
    Declared::new(&FURTHER_NOTICES, notices)
}

/// Whether a test has the test-only provider's launches go without its status integration on this
/// thread ([`declaring_launch_without_status_integration`]).
pub(crate) fn declares_launch_without_status_integration() -> bool {
    LAUNCH_WITHOUT_STATUS_INTEGRATION.with(Cell::get).is_some()
}

/// The launches this thread makes of the test-only provider go without the status integration it
/// declares ([`declaring_reported_signals`]), as a launch whose hooks the user switched off does:
/// the provider answers so for the launch (`AgentProvider::launch_has_status_integration`) and a
/// stand-in session made meanwhile takes its status from its reports. By default a launch has
/// what the provider has.
pub(crate) fn declaring_launch_without_status_integration() -> Declared<()> {
    Declared::new(&LAUNCH_WITHOUT_STATUS_INTEGRATION, ())
}

/// Whether a test has the test-only provider's prompts read as unreadable once a key went into
/// them on this thread ([`declaring_prompt_unreadable_after_input`]).
pub(crate) fn declares_prompt_unreadable_after_input() -> bool {
    PROMPT_UNREADABLE_AFTER_INPUT.with(Cell::get).is_some()
}

/// The test-only provider's interaction reader declares that a prompt cannot be read once a key
/// went into it (`InteractionReader::prompt_unreadable_after_input`), for the watches this thread
/// runs while the guard lives. By default it can be.
pub(crate) fn declaring_prompt_unreadable_after_input() -> Declared<()> {
    Declared::new(&PROMPT_UNREADABLE_AFTER_INPUT, ())
}

/// Whether a test has the test-only provider declare an extension channel on this thread
/// ([`declaring_extension_channel`]).
pub(crate) fn declares_extension_channel() -> bool {
    EXTENSION_CHANNEL.with(Cell::get).is_some()
}

/// The test-only provider's agent runs an extension that opens the session's channel
/// (`crate::adapter::test_provider::extension`), for the launches this thread plans while the
/// guard lives. By default it runs none.
pub(crate) fn declaring_extension_channel() -> Declared<()> {
    Declared::new(&EXTENSION_CHANNEL, ())
}
