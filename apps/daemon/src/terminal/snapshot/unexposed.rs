//! The state of a parsed terminal that `alacritty_terminal` keeps to itself and a first paint, or
//! a saved screen, has to restore or say it does not.

use alacritty_terminal::vte::ansi::{
    CharsetIndex, CursorShape, CursorStyle, Handler, NamedPrivateMode, PrivateMode,
    TabulationClearMode,
};

/// How deep a program may push titles, as in alacritty_terminal.
const TITLE_STACK_MAX_DEPTH: usize = 4096;

/// The scroll region, the cursor style a program chose, the window title and the titles pushed, the
/// character set in use, whether the tab stops were changed, the character a repeat (REP) draws
/// again, and what the alternate screen went through, of a parsed terminal. Kept by parsing the
/// same output a second time and applying it as `alacritty_terminal`'s own handler, or its parser,
/// does; every other sequence is a default no-op of [`Handler`].
pub(super) struct Unexposed {
    lines: usize,
    /// The scroll region as rows `start..end` from the top, when it is not the whole screen.
    pub(super) region: Option<(usize, usize)>,
    pub(super) cursor_style: Option<CursorStyle>,
    pub(super) title: Option<String>,
    /// The titles pushed (`CSI 22 t`), the last pushed last.
    pub(super) titles: Vec<Option<String>>,
    /// The character set the output is drawn in (SI, SO and their kin).
    pub(super) active_charset: CharsetIndex,
    /// A tab stop was set or cleared since the last reset. Where is not kept: this handler does
    /// not know the cursor's column.
    pub(super) tabs_changed: bool,
    /// The character last drawn, which a repeat draws again. The parser keeps it across a reset.
    pub(super) preceding: Option<char>,
    /// The alternate screen is shown.
    alternate: bool,
    /// The alternate screen was left (`CSI ? 1049 l`) since the owner last cleared this; a reset
    /// does not clear it.
    pub(super) left_alternate: bool,
    /// A cursor was saved (DECSC) while the alternate screen was shown, since the last reset: the
    /// alternate screen keeps it, hidden, for a restore (DECRC) once it is shown again.
    pub(super) alternate_saved_cursor: bool,
}

impl Unexposed {
    pub(super) fn new(lines: usize) -> Self {
        Self {
            lines,
            region: None,
            cursor_style: None,
            title: None,
            titles: Vec::new(),
            active_charset: CharsetIndex::G0,
            tabs_changed: false,
            preceding: None,
            alternate: false,
            left_alternate: false,
            alternate_saved_cursor: false,
        }
    }

    /// A resize to `lines` rows resets the scroll region, as it does in `alacritty_terminal`
    /// whenever the size changes.
    pub(super) fn resize(&mut self, lines: usize, changed: bool) {
        self.lines = lines;
        if changed {
            self.region = None;
        }
    }

    /// The cursor style a program chose, made from the default one where it had chosen none, as
    /// `alacritty_terminal` makes it when a program changes only its shape or its blinking.
    fn chosen_style(&mut self) -> &mut CursorStyle {
        self.cursor_style.get_or_insert_with(CursorStyle::default)
    }
}

impl Handler for Unexposed {
    fn input(&mut self, c: char) {
        self.preceding = Some(c);
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.active_charset = index;
    }

    fn set_horizontal_tabstop(&mut self) {
        self.tabs_changed = true;
    }

    fn clear_tabs(&mut self, _mode: TabulationClearMode) {
        self.tabs_changed = true;
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        let bottom = bottom.unwrap_or(self.lines);
        if top >= bottom {
            return;
        }
        let start = top.saturating_sub(1).min(self.lines);
        let end = bottom.min(self.lines);
        self.region = (start, end).ne(&(0, self.lines)).then_some((start, end));
    }

    fn set_cursor_style(&mut self, style: Option<CursorStyle>) {
        self.cursor_style = style;
    }

    fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.chosen_style().shape = shape;
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Named(NamedPrivateMode::BlinkingCursor) => {
                self.chosen_style().blinking = true;
            }
            // Switching the column mode (DECCOLM) resets the scroll region.
            PrivateMode::Named(NamedPrivateMode::ColumnMode) => self.region = None,
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor) => {
                self.alternate = true;
            }
            _ => {}
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Named(NamedPrivateMode::BlinkingCursor) => {
                self.chosen_style().blinking = false;
            }
            PrivateMode::Named(NamedPrivateMode::ColumnMode) => self.region = None,
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor) => {
                self.left_alternate |= self.alternate;
                self.alternate = false;
            }
            _ => {}
        }
    }

    fn save_cursor_position(&mut self) {
        self.alternate_saved_cursor |= self.alternate;
    }

    fn report_private_mode(&mut self, mode: PrivateMode) {
        // Asking whether the cursor blinks fixes the default style as the chosen one.
        if mode == PrivateMode::Named(NamedPrivateMode::BlinkingCursor) {
            self.chosen_style();
        }
    }

    fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    fn push_title(&mut self) {
        if self.titles.len() >= TITLE_STACK_MAX_DEPTH {
            self.titles.remove(0);
        }
        self.titles.push(self.title.clone());
    }

    fn pop_title(&mut self) {
        if let Some(title) = self.titles.pop() {
            self.title = title;
        }
    }

    fn reset_state(&mut self) {
        let (preceding, left_alternate) = (self.preceding, self.left_alternate);
        *self = Self::new(self.lines);
        self.preceding = preceding;
        self.left_alternate = left_alternate;
    }
}
