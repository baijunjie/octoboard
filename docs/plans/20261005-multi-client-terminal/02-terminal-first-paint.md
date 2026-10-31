# 02 Terminal first paint

> Goal: attaching to a running terminal shows the program's exact current screen, however much output it has written.
> Completion criteria:
> - A session has written well over the ring buffer's size, including alternate-screen and mode changes. A newly
>   attached client then shows the same screen as one that has been attached throughout.
> - Output written while the attach is being set up appears exactly once.

## Technical design

- [ ] A first paint, produced from the daemon's parsed screen:
  - It starts by resetting the client's terminal modes and clearing it.
  - Then it adds up to a requested number of history lines (normal screen only), pushed into the client's scrollback.
  - Then it enters the alternate screen if the program is on it.
  - Then it adds the active screen with its modes, cursor and title.
- [ ] An attach request field asking for the number of history lines.
- [ ] Producing and queueing the paint happens at the same point as appending live output for that terminal, so live
      output always follows the paint in order.
- [ ] The replay header describes the paint. The paint holds no program queries, so all of it is settled output.
- [ ] Where the client's size is applied after what it receives on attaching, it is applied after the paint.

## Implementation plan

- [ ] Produce the paint from the existing parsed screen first, and compare it with the ring replay on recorded output.
- [ ] Switch attaching to a running process over to the paint, then remove the replay and its mode-restoring prefix
      there.

## Notes for the developer

- **Reusable capabilities**:
  - The daemon already parses every terminal's output continuously.
  - The frozen-preview snapshot already turns the parsed screen into styled text.
  - The terminal stream's replay header tells the client where live output starts.
- **Development notes**:
  - Clients keep answering the live output's queries until milestone 04 replaces that.
  - An exited shell terminal's attach is open (see the overview).
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("terminal stream"), `docs/product/session-terminal.md`,
  `docs/product/shell-terminals.md`.
