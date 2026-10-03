# T1 checklist: agent TUIs rendering inside xterm.js / WKWebView

This walkthrough has already been carried out; its results are recorded under the T1 entry of
`docs/plans/20261001-octoboard-mvp/00-technical-validation.md`, not here. The boxes below are left
unchecked on purpose — this file is a template for re-running the walkthrough, not a record that
it was skipped.

T1 is whether Claude Code's, Grok's, and Codex's TUIs render and behave correctly when driven
through `xterm.js` inside the Tauri (WKWebView) shell — full screen, mouse, keyboard shortcuts,
CJK input methods, window resizing. None of that can be settled by a script; it needs a human
driving the real app. This is the walkthrough.

## Launching the prototype

Run these from the repository root. Steps 1-3 are one-time setup; after that only step 4 is needed.

1. Install the UI's dependencies once: `npm --prefix prototype/ui install`.
2. Build the daemon: `cargo build --release --manifest-path prototype/daemon/Cargo.toml`.
3. Put the daemon where Tauri expects the sidecar, under the per-platform name it requires
   (`-aarch64-apple-darwin` on Apple silicon, `-x86_64-apple-darwin` on Intel):

   ```sh
   mkdir -p prototype/tauri/src-tauri/binaries
   cp prototype/daemon/target/release/obd-proto \
      prototype/tauri/src-tauri/binaries/obd-proto-aarch64-apple-darwin
   ```

4. Run the app: `cd prototype/tauri && cargo tauri dev` (install the CLI first if needed:
   `cargo install tauri-cli --version 2.11.5 --locked`). The Vite dev server is started for you by
   `beforeDevCommand`; if the console sits on *"Waiting for your frontend dev server to start on
   http://localhost:5173/"* and never proceeds, that hook is missing from
   `prototype/tauri/src-tauri/tauri.conf.json` — the alternative is to run `npm --prefix prototype/ui run dev`
   in a second terminal and leave it running.

   For the packaged build instead, `cargo tauri build` and then open
   `prototype/tauri/src-tauri/target/release/bundle/macos/obd-proto-shell.app`. It is unsigned, so
   macOS will warn on first open.

5. The app window opens with the control strip at the top and the terminal pane below. Pick an
   agent, type an absolute `cwd` for a real project (not empty — Claude Code's workspace-trust
   dialog, see below, is keyed on the directory), and click **Start session**.

### The dialogs that look like a hang

Both of these are real, expected, first-launch behavior — not the app being stuck. If a session
looks frozen right after starting, check for these before assuming something is broken.

- **Claude Code workspace trust.** The first time Claude Code runs in a directory, it shows a
  one-time trust prompt. **Its default-highlighted option is "No, exit"** — pressing Enter
  without first moving the selection kills the session immediately. Use the arrow keys to select
  "Yes, proceed" before pressing Enter.
- **Claude Code MCP approval.** If the project's `.mcp.json` lists a server Claude Code has not
  approved yet, it raises a second blocking modal before the session becomes usable. Approve it
  to continue.
- **Grok first launch.** Grok shows a privacy opt-in and then a welcome screen the first time it
  runs. Click or key through both before expecting a normal session.

## Checklist

Check off each box by trying the thing and writing what actually happened (not "looked fine" —
write what you saw, e.g. "cursor stayed one row above the prompt after resize").

### Per agent (claude / grok / codex) — repeat this whole block three times

- [ ] **Full-screen rendering.** The TUI's alternate-screen UI (Claude Code's box-drawing
      prompt, Grok's/Codex's equivalent) fills the terminal pane edge-to-edge with no stray
      scrollback line left above or below it.
- [ ] **Mouse.** Click to place the cursor inside the TUI's input area, if the TUI supports
      click-to-position. Scroll with the trackpad/wheel inside the pane and confirm it scrolls
      the TUI's own history (or does nothing destructive) rather than scrolling xterm.js's own
      buffer out of sync with what the TUI thinks is on screen.
- [ ] **Keyboard shortcuts.** Try the TUI's own shortcuts (e.g. Claude Code's `Ctrl+C` to
      interrupt, `Ctrl+R` or similar history/search keys if it has one, `Esc` to cancel). Confirm
      each does what it does in a real terminal, not something xterm.js or the OS intercepts
      instead (e.g. `Cmd+K` clearing the macOS Terminal would be the wrong comparison — check
      specifically that WKWebView doesn't swallow it for its own browser shortcut first).
- [ ] **CJK input method.** Switch to a Japanese or Chinese input method, type a sentence that
      requires IME composition (conversion candidates, not just romaji passthrough), and confirm
      the composition window appears in the right place and the finished text lands correctly in
      the TUI's input line, not duplicated, dropped, or shown only after a delay.
- [ ] **Window resizing.** Resize the Tauri window (drag an edge, then try maximize/restore).
      Confirm the TUI reflows to the new size (not just xterm.js's own grid — the agent CLI's own
      rendering should redraw at the new width/height) and that typing immediately after a resize
      lands in the right place rather than at a stale cursor position.

### Once, not per agent

- [ ] **Reconnect.** While a session is mid-output, click the terminal's "Disconnect" button,
      then "Reconnect". Confirm the pane picks back up without visibly corrupted/garbled
      characters at the seam (see `prototype/bench/replay.mjs` for the byte-level version of this
      check — this box is about what it *looks like*, not the byte count).
- [ ] **Switching sessions.** Start two sessions (different agents or the same one twice) and
      click between their chips in the control strip. Confirm the terminal pane shows the
      selected session's own content, not a mix of both or a stale screen from the other one.

## Recording results

For each unchecked or surprising box, write down: which agent, what you did, what you expected,
what actually happened. That write-up — not a checked box — is what settles T1.
