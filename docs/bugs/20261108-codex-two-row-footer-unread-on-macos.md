> Severity: Moderate

# Codex 0.166.0's empty composer is not recognised on macOS

## Symptom

On macOS, Octoboard reads no composer at all off a Codex 0.166.0 session whose composer is empty, because Codex draws its
default two-row footer there and the composer reader accepts that layout only on a Windows host.

## Reproduction steps

1. On macOS, with Codex 0.166.0 (`codex-cli 0.166.0`, the standalone install), start Codex in a git repository with a
   scratch `HOME` and `CODEX_HOME` whose `auth.json` is a symlink to the real login (the isolation in
   `docs/memory/isolating-agent-cli-probes.md`), in a 120×40 terminal and again in a 44×40 one. Send nothing to the model:
   answer the folder-trust screen with Enter, wait for the composer, then type `draft line one` without Enter.
2. Or replay the recorded output instead: `20261108-codex-two-row-footer-unread-on-macos/codex-0.166.0-macos-120x40.bin`
   and `…-44x40.bin` beside this ticket. Feed the first 12769 bytes (120 columns) or 6009 bytes (44 columns) to
   `crate::terminal::snapshot::TerminalSnapshot::new(40, <columns>)`, take `visible_text_undimmed()`, and pass it to
   `crate::session::input_box::input_box_rows(AgentId::CODEX, …)`: the empty composer. The whole files (12922 and 6162
   bytes) end with the draft typed.
3. The screens those prefixes give (blank rows dropped):

   ```
   120 columns, empty composer
     >_ OpenAI Codex
   ›
     GPT-6.1-Sol default · /private/tmp/claude-501/-Volumes-2TB-Document-Code-App-octoboard/0abc3b84-04ad-4915-a847-08ab2b…
     ← for agents · ? for shortcuts

   44 columns, empty composer
     >_ OpenAI Codex
   ›
     GPT-6.1-Sol default · /private/tmp/claude…
     ← for agents · ? for shortcuts
   ```

   With the draft typed, the hint row goes blank and only the status row is left below the composer.

## Expected vs. actual

- Expected: an empty composer reads as empty (`Some([""])`), so ordinary terminal delivery may write a message during
  the draft-protection grace period; `docs/product/message-delivery.md`, "When separate delivery cannot be used,
  delivery takes priority after three seconds": during the grace period delivery "requires a recognised empty composer".
- Actual: the reader returns `None` for the empty composer at both widths; with the draft typed it returns
  `Some(["draft line one"])`.

## Environment

- macOS (Darwin 25.2.0), Codex `codex-cli 0.166.0` (`~/.codex/packages/standalone/releases/0.166.0-aarch64-apple-darwin`),
  default configuration (scratch `config.toml` holding only the trust entry), `TERM=xterm-256color`, not over SSH.
- Octoboard branch `feat/windows-port` at 122c7b186 (the reader in `apps/daemon/src/adapter/codex/screen.rs`).
- The same captures replayed on a Windows host's reader would be read: the two-row layout is accepted there.

## Scope of impact

- Every Codex 0.166.0 session on a macOS (and presumably Linux) daemon host, whenever its composer is empty.
- Workaround: none needed for delivery itself; each automatic message waits out the three-second grace period and is then
  written anyway. What is lost is the check that the composer is empty.

## Leads

- Verified: `composer` in `apps/daemon/src/adapter/codex/screen.rs` reads a two-row footer (status row, then hint row)
  only where the host's path style is Windows; on Unix the hint row `← for agents · ? for shortcuts` is taken as a
  one-row footer, which it is not, so the result is `None`. Existing tests pin the Unix two-row layout as `None`
  (`codex_s_footer_ends_the_composer_in_the_shapes_of_the_host`).
- Verified: the status row's directory is cut with `…` at both widths; the 120-column one names an absolute path, not `~/…`.
- Inferred, from Codex 0.166.0's source (tag `rust-v0.166.0`, `codex-rs/tui/src/bottom_pane/footer.rs`): while a turn
  runs and the composer holds a draft, the hint row reads `tab to queue message`, so a busy draft is unrecognised on macOS
  too, and after the grace period a message is written into that draft. Not captured on macOS.
- Inferred: once the thread has a name the status row ends with ` · <thread name>`, and a working directory under the
  home directory is shown as `~/…`; the Unix reader rejects both, so a draft row above such a status row would not be
  read either. Not captured on macOS.
- Inferred: the macOS captures of Codex 0.163.0 in `apps/daemon/testdata/codex-composer.md` show a one-row footer, so
  this may be new with 0.166.0, or with how those captures were taken.

## Acceptance criteria

- [ ] The two recorded captures, replayed as in step 2, read the empty composer as `Some([""])` at 120 and at 44 columns.
- [ ] The full captures (draft typed) still read `Some(["draft line one"])`.
- [ ] A Unix two-row footer with `tab to queue message` under the status row reads the draft above it.
- [ ] A Unix status row with a `~/…` directory and with a thread name is read.
- [ ] Unknown footer shapes still read as no composer on macOS, and the Windows readings are unchanged.
