# The terminal of interrupted and archived sessions

What the session terminal (see `docs/product/session-terminal.md`) shows for a session that has no process: the card
offering to resume or reopen it, an archived session's history in its place, and an interrupted session's saved
terminal preview.

## Resuming on selection, and the card

- Selecting an interrupted session resumes it rather than attaching, unless it is shown as switching its account (see
  "While a session is switching" in `docs/product/switching-session-accounts.md`). Two ways of reaching one only show
  it, with its card, and never resume it: moving with Back or Forward (see "Moving back and forward" in
  `docs/product/navigation-history.md`), and Next or Previous console session (`Ctrl+Tab` / `Ctrl+Shift+Tab` by default)
  in a console session's focus mode (see "The switch strip" in `docs/product/focus-mode.md`). While a session has no
  process and no resume of it is under way, the terminal shows a card at its bottom centre, raised over it by a large
  shadow with no outline (see "The window chrome and the content panel" in `docs/product/window-layout.md`): for an
  interrupted session it reads "Not running" and offers "Resume"; for an archived one it says the session is archived
  and offers "Reopen". The card is hidden while the session is being resumed or is shown as switching its account.
  Assistive technology is told the card's sentence once, as the card appears or its sentence changes; its button is not
  part of what is said, and is reached as any other control. When the card appears or its sentence changes while a
  dialog is open over the window — the confirmation that selecting an interrupted session can ask first (see "Starting a
  session on an account whose last change is incomplete" in `docs/product/sharing-the-default-configuration.md`), for
  one — the sentence is told about 1.5 seconds after the dialog closes, after what the return of keyboard focus says. A
  sentence already told before a dialog opened is not told again when it closes.

## An archived session

- **An archived session shows its history, not its terminal**: a read-only list of what was said in it, read from the
  agent's own record (see `docs/product/session-history.md`). Its terminal cannot be reached: nothing typed or pasted
  goes anywhere, and typing does not reopen it. Reopening it — the card's "Reopen", or the other ways in "Archiving,
  interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md` — is the way back to its terminal.

## The saved terminal preview

- **An interrupted session shows its own saved terminal preview**, behind that card, read-only. A session interrupted
  while it is shown has its live screen replaced by that preview; reopening the view or loading it on another client
  shows the same saved content. Showing a different interrupted session shows its own preview, or a blank terminal if it
  has none. Clicking, selecting, scrolling or focusing the terminal does not resume it, and nothing typed reaches the
  stopped process.
- **Archiving preserves the screen before the agent is asked to exit.** The same rule applies when switching accounts or
  stopping the daemon normally: cleanup, a clear screen or leaving an agent's full-screen interface cannot replace the
  captured preview. An agent that exits or crashes on its own instead leaves its final screen, so content it clears
  before exiting need not survive. These rules are shared by every supported agent. An archived session's preview is
  kept, but its history is what is shown.
- The preview contains the active terminal screen, retaining text colours and styles. With the normal screen active, it
  includes up to **10,000 rows of scrollback**. An agent's alternate full-screen interface has no scrollback: its
  preview contains only that visible screen, without the hidden normal screen's history. The preview does not depend
  on how much output the agent wrote, and wraps to the reader's terminal width. It is **not a full conversation record**:
  content an agent already overwrote inside its full-screen interface is not reconstructed. The agent's own stored
  conversation is separate.
- Each process end replaces the previous preview; starting a new process discards it. While a resume is under way, the
  loading state covers it, so it is what shows when a resume fails before starting a process. A successful resume
  redraws from the new process's first paint rather than appending to the preview. Older saved output is displayed as
  recorded, including any clearing it contains; it is not converted into a recovered screen. A session whose process
  ended before Octoboard kept output, or that was lost with a daemon killed outright and not taken over from a process
  holder (see "What a daemon start takes over" in `docs/product/process-holders.md`), shows a blank terminal, even if
  an earlier process left some; one whose screen a planned stop saved before its holder was lost shows that screen
  instead (see "The screen of a terminal handed over" in `docs/product/process-holders.md`). Deleting the session's
  record deletes its saved preview too (see `docs/product/files-octoboard-owns.md`).
- **A process that exits as it starts does not interrupt the session** when Octoboard starts the agent again with less
  of its additions (see "When the agent exits as it starts" in `docs/product/launching-without-octoboards-additions.md`): the session stays
  *starting*, shows no card, and the next process redraws the terminal. Only when the last of those launches exits as
  it starts too is the session interrupted, its preview that process's final screen, with the agent's own message on
  it.
