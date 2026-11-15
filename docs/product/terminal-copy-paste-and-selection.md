# Copy, paste and selection in the session terminal

The editing menu of the session terminal (see `docs/product/session-terminal.md`), selecting its text with the mouse,
and the text a program running in it copies.

## The editing menu

With a session selected that is not archived, right-clicking its terminal opens an editing menu in both the desktop app
and a browser. On macOS, Control-click opens it too. With keyboard focus in the terminal, the context-menu key or
`Shift+F10` opens the same menu; neither key is sent to the agent. Opening it preserves the terminal's selection, and
closing it returns keyboard focus to the terminal. There is no separate menu button. Copy, Paste and Select all form
one group, with no divider between them.

- **Open in file manager** shows a file path link in the file manager on the machine the daemon runs on (see "File
  paths" in `docs/product/terminal-links.md`). It is offered only when the menu was opened with the pointer — a
  right-click, or a Mac's Control-click — on a file path the terminal shows as a link, and then comes first, set apart
  from the items below by a separator. A menu opened from the keyboard, or with the pointer anywhere else, does not
  have it.
- **Copy** copies the terminal text selected when the menu opened or, with nothing selected in the terminal, the text a
  program last copied, while that is still on offer (see "Text a program copies" below). It is disabled with neither.
  When it is disabled while the running program has mouse reporting on, it says under its name how to make a
  selection anyway (see "Selecting with the mouse" below): "Option-drag to select" on macOS, "Shift-drag to select"
  elsewhere. macOS here is what the terminal itself treats as macOS, a browser platform of `Macintosh`, `MacIntel`,
  `MacPPC` or `Mac68K`, so an iPhone or iPad gets the Shift-drag hint. Without mouse reporting it is disabled with no
  reason given.
  Where the platform has no clipboard, or it refuses, the text is copied through the browser's selection copy instead.
- **Paste** reads text from the clipboard and sends it as terminal input, respecting the running program's bracketed
  paste mode, as Cmd+V / Ctrl+V does. A clipboard holding no text but an image pastes the path of a file the image is
  saved as (see `docs/product/pasting-images.md`); one holding neither is pasted as an empty paste rather than refused.
  It is disabled without clipboard reading support, saying under its name that the clipboard is unavailable, and while
  the selected session cannot accept input, with no reason given (the terminal itself shows why, by its reconnecting
  spinner, its disconnected notice or the cover of a process that cannot be reached).
- **Select all** selects the terminal's contents, including its available scrollback, for copying.

The clipboard is read only when Paste is chosen, so merely opening the menu does not request clipboard access. If
clipboard access completes after the user switches sessions or the original session can no longer accept input, the
paste is discarded. It is never redirected to another session. A failed clipboard read, or a copy that neither way
allows, shows an error alert.

## Selecting with the mouse

A drag selects terminal text, except while the running program has mouse reporting on: then a plain drag goes to the
program, which may highlight and copy the text itself (Claude Code's fullscreen view does), but makes no selection of
the terminal's own. **Option-drag on macOS, and Shift-drag on other platforms, selects even under mouse reporting.** On
macOS an Option-drag therefore always makes an ordinary selection, never a column (block) one; on other platforms an
Alt-drag on a screen without mouse reporting selects a column.

Moving the pointer away to right-click keeps a selection made under mouse reporting: while the terminal holds a
selection, pointer motion with no button held is not reported to a program that follows all motion, since the report (in
the SGR encodings Claude Code uses) would clear the selection, so that program sees no hover until the selection is
gone. Typing, and a click or the wheel reported to the program, may still clear it.

The selection also survives the program re-asserting its modes, as Claude Code does whenever input reaches it after a
few idle seconds (a focus report as the window comes back to the front, say): output that only sets modes, leaving mouse
reporting as it was, leaves the selection where it was, unless a key, a paste, a press that cannot open the editing
menu, or a drag or the wheel reported to the program reached the terminal while that output was being parsed. A
re-assertion in the middle of a forced drag (Option-drag, or Shift-drag elsewhere) still ends the drag, keeping what it
had selected so far. The program does clear the selection when re-asserted mouse modes come in the same output as
anything else, such as text it redraws, when it switches to another mouse reporting mode, and when it switches between
the main and the alternate screen.

## Text a program copies

A program running in the terminal can put text on the clipboard with the OSC 52 escape sequence, as Claude Code does
with the text selected in its fullscreen view. Whichever selection the request names, the text goes to the one system
clipboard. Only writes are honoured:

- a request to read the clipboard is never answered, and a request to clear it is ignored;
- a request in output written before the session was attached is ignored: whatever opens each attach (the first paint
  of a running program, or the paint of an exited shell terminal's last screen), the saved output of a session without
  a process, and output still waiting to be drawn when the screen is reset, as on a switch to another session;
- where the platform has no clipboard nothing is put on it; the text is still offered to Copy in the editing menu, which
  falls back to the browser's selection copy.

A write the clipboard fails shows the same error alert as a failed Copy, with its clipboard remedy (see "Recovery and
unexpected errors" in `docs/product/error-alerts.md`). A browser's refusal of a copy the user did not just make (no
recent gesture, or the page not focused) stays silent, so in a browser that refuses every such copy (Safari) a program's
copy can fail without notice; Copy in the editing menu, a gesture of the user's, then puts it there.

## The menu offers what a program copied

The text of the latest honoured write stays on offer to the editing menu's Copy while the terminal has no selection of
its own; a selection in the terminal comes first. It is offered until the user turns back to the program or the screen
moves on: a key typed to the program (`Control-C` included), a paste, a press in the terminal other than a right-click
or, on macOS, a `Control`-click (a click, a drag, an Option-click), a reset of the screen (another session selected, a
reconnect, a fresh paint sent to a client that fell behind the output) or the terminal closing. Moving the pointer and
the right-click that opens the menu leave it on offer, as does the wheel while the program has mouse reporting on; on an
alternate screen without mouse reporting the wheel sends the program arrow keys, which drop it like any other key. The
program's next copy replaces it.
