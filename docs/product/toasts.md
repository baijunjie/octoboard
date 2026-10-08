# Toasts

Failures and notices that need no answer are shown as **toasts**; which ones are, and how they name the session they
are about, is in "Losing the daemon connection" in `docs/product/application-lifecycle.md`.

- **Always at the window's bottom right** (bottom left under a right-to-left language), floating over whatever is there,
  on the main screen and on the screen shown while connecting to the daemon alike, whether or not Settings or another
  dialog is open. They never move the layout; the stack rides above the connection banner while that is shown. The
  newest is at the front.
- A toast about a particular session is titled with where that session is — the project it runs in, or the console
  whose console session it is — with the message under it; any other toast is just the message. An error is marked
  as one; a notice is not.
- **A toast dismisses itself**: a notice after about 5 seconds, an error after about 8. The countdown pauses while
  the pointer is over the toasts or keyboard focus is inside them. Each toast also has a close button.
- **An identical toast** — the same kind, message and session — arriving while one is still shown does not stack a
  second copy: the existing one is replaced by a fresh one at the front, its countdown started over.
- **A toast's text can be selected and copied.** A press on a toast still leaves keyboard focus where it was, so
  typing goes on reaching the terminal: a press on the close button or on the toast around its text never takes focus,
  and a plain click on the text, which takes it for a moment, hands it straight back. A drag that leaves text
  selected keeps focus in the toast so `Cmd+C` copies it, and the copy hands focus back. Focus goes back to whatever
  had it before, or to the terminal when that is gone.
- The toasts are reached from the keyboard with `F6` and `Shift+F6`: while at least one is shown they are the last
  stop of the window's region cycle, landing on the newest toast (see "Moving focus between regions with F6" in
  `docs/product/window-layout.md`).
