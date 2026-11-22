# Links and files in a remote window

A remote window of the desktop application shows a daemon on another machine (see "The remote window" in
`docs/product/remote-daemons.md`), whose loopback addresses and files are that machine's, not this computer's. This
document covers how such a window opens an address a program on that machine gives for itself, and a file of that
machine, on this computer. Neither applies to the main window, to a remote daemon opened in the browser, or to a
browser.

## Loopback addresses of the remote machine

An address a program on the remote machine gives for itself — `http://localhost:3000/`, as a development server prints
— names the remote machine's loopback, which this computer's browser cannot reach as written. **A remote window
forwards it**: opening such an address listens on a loopback port of this computer, carries every connection made there
over the window's SSH connection to the address's port on the remote machine's loopback, and opens the address in this
computer's browser at the port it listens on.

- **Which addresses**: an `http` or `https` address whose host is `localhost` or a name ending in `.localhost`,
  `127.0.0.1`, `[::1]`, or the any-address `0.0.0.0` or `[::]` a server prints for every interface, compared without
  regard to case and with a trailing dot ignored; with no port written, port 80 or 443. Every address the window hands
  to the browser goes by this rule, wherever it is shown: a plain-text address or an OSC 8 hyperlink in a terminal, a
  link in a Markdown document, in a session's history or on a report page. An address on any other host — the rest of
  `127.0.0.0/8`, a name that merely resolves to the loopback, the remote machine's own name — opens as written. So does
  every address in the main window, in a remote daemon opened in the browser, and in any browser.
- **Where on the remote machine**: `127.0.0.1` and `[::1]` are reached as written, each on its own address family. A
  name or an any-address is probed at `127.0.0.1` and at `::1` at once, and the first to answer is the one the forward
  uses for its connections, so a server listening on either family alone is found whatever the remote machine's system;
  the later connections through that forward try that one first, and the other where it refuses one.
- **Which port on this computer**: the remote port's own number where nothing on this computer answers at it and it can
  be listened on, so that an address the page builds for itself (a sign-in's return address, an absolute link) still
  leads back to it; otherwise a port the system assigns. The address is opened as written but for that port, and for an
  any-address host, which becomes `localhost`.
- **One forward for each remote port**: a later address at the same port reuses it, whatever its host, scheme or path.
  Each remote window has its own, so the same port of two remote machines takes two ports here, the first one opened
  keeping the number.
- **Before the browser opens, the remote port is tried once.** Where nothing listens on it — which includes the remote
  SSH server taking the connection and closing it again within about half a second, unanswered, as the Windows one
  does for a port nothing listens on — an error alert says so, naming the port and the daemon, and nothing opens.
  Where the remote machine's SSH server does not allow forwarding, the alert says that, and that the address can still
  be opened on that computer. Where the SSH connection is down, it says the connection to the daemon is down, and
  nothing waits for the reconnection: the window reconnects by itself, and the address opens when it is opened again
  once the connection is back. Which of these carry a remedy is in "Recovery and unexpected errors" in
  `docs/product/error-alerts.md`.
- **A forward lasts until it is stopped or its window closes.** It is let go at Stop (see "Forwarded ports on the rail"
  below), when the window closes, at Disconnect and when the application quits, and is not brought back at the next
  launch: the next opening makes it again. While one holds a port, that port on this computer is the remote machine's: a
  program started here that wants it finds it taken, and this computer's own `localhost` address at it leads to the
  remote machine, from every window and every program.
- **Who can use a forward**: it listens on this computer's loopback alone, `127.0.0.1` and `::1`, never on another
  interface. Any program on this computer, another user's included, can connect to it while it is held, and reaches
  what listens on the remote port, as with a forward made with `ssh -L`. Only a port whose address the owner opened is
  forwarded, and only to the remote machine's own loopback: no address makes the application connect from there to
  another machine. A user name and password written in an address are passed to the browser with it but are not kept
  with the forward or listed.

### Forwarded ports on the rail

While a remote window holds at least one forward, its rail has a **Forwarded ports** button, directly below Processes
(see "The rail" in `docs/product/window-layout.md`): a cable glyph with the number of forwards in a badge placed and
drawn as the Processes button's, reading 99+ beyond 99. It is there only while a forward is held. It has no tooltip, and
its name for assistive technology says how many ports are forwarded and that pressing it shows them. The main window
and a browser never have it.

Its popover opens and closes as the Processes popover does (see "Opening and closing the popover" in
`docs/product/processes-on-the-rail.md`): a mouse resting on the button floats it in, a tap, **Enter** or **Space**
opens it with keyboard focus on its first row, and it closes by itself when the last forward goes, focus then moving to
the rail's next control. It starts with a **Forwarded ports** heading and a line saying that addresses on the daemon's
own loopback opened from this window are reached through these ports of this computer. Under it is one row for each
forward, in the order they were made:

- **the address on this computer**, its host and port (`localhost:3000`), read left to right in every language, and
  beneath it, smaller and muted, **the remote port and the daemon's name** ("Port 3000 on build-server");
- **pressing the row opens the address last opened through the forward again**, in this computer's browser, by the same
  path as opening it from the window, so the remote port is tried first and a failure raises the same alert. The row
  carries the external-link glyph, and that address in full, as it was written but without a user name or password, is
  its tooltip;
- **Stop**, a text button in the danger colour at the row's end, named for assistive technology with the port ("Stop
  forwarding port 3000"). It stops the forward at once and asks nothing: the port on this computer is let go and the
  connections passing through it are cut, so a page of it left open in the browser stops answering. Opening the address
  from the window again makes a new forward. A Stop that fails raises an error alert saying the port could not be
  stopped; a forward stopped from the keyboard hands focus to the next row's Stop, or the previous one's for the last
  row.

The list is at most 70% of the window's height, and scrolls and fades at its edges beyond that. Rows come and go with
the collection motion (see "Items moving within a collection" in `docs/product/collection-motion.md`).

## Files opened on this computer

A remote window opens nothing on the remote machine, so a file is opened here from a copy: **Download and open**, in the
file viewer (see "Download and open" in `docs/product/file-viewer.md`), copies the file to this computer and opens the
copy with this computer's default application for it.

- **Which files**: only a live file the viewer itself may read — a file of the project or of one of its worktrees as it
  is on disk, or one a terminal's path link leads to (see "File paths" in `docs/product/terminal-links.md`) — by the
  same rules, which the remote daemon checks again at every part it sends. A committed version of a file, a deleted
  file and a folder cannot be copied.
- **How it travels**: through the window's own connection to its daemon, in parts of at most 1 MiB, one at a time.
  There is no limit on the file's size. A file that changes while it is copied ends the copy with an error rather than
  leaving a file put together from two versions; so does a connection lost on the way.
- **Where the copy is kept**: in the application's local data directory, which stays on this computer, in
  `remote-files/copies/` (see "The desktop
  application's own files" in `docs/product/files-octoboard-owns.md`), each copy in a new folder of its own and under
  the file's own name, so two files of the same name, or the same file copied twice, never meet. A character this computer's file names cannot hold
  is replaced, and a name too long is cut, keeping a short extension. The folders and the copies are readable by the
  user alone.
- **How long it is kept**: a copy stays while the application runs, so that whatever opened it can go on reading it, and
  every copy is removed when the application next starts. One that was cancelled or failed is removed at once, and so
  is one not yet opened when its window closes. A change made to a copy stays in the copy: nothing is sent back to the
  remote machine.
- **What is never copied**: what this computer could launch rather than show, judged by this computer's system, not the
  remote machine's. A file whose name, after its last dot — a leading one included, as in `.bat` — has one of the
  extensions "File paths" in `docs/product/terminal-links.md` lists for every system, or, on a Windows computer, one of
  those it lists for Windows, or that has an executable bit on the remote machine, is refused before anything is copied;
  Download and open is not offered for it, nor for a file the remote machine's rules call launchable for a reason other
  than its name, such as one inside an application bundle. A name on the Windows list alone is copied and opened on a
  Mac, where such a file is plain data. Every copy is written without an executable bit and marked as downloaded
  for the system — quarantined on macOS, marked as coming from the Internet zone on Windows — so the system's own checks
  apply to whatever opens it.
