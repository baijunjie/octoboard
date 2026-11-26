# Pi captures

Measured on Pi 1.5.2 (`@earendil-works/pi-coding-agent`, Node 22, macOS) in a scratch `HOME` and agent directory
(`PI_CODING_AGENT_DIR`), with the project in `/private/tmp/pi-probe/project` holding `.pi/settings.json`. What each
capture shows is described in `docs/agent-cli-reference.md`, section "Pi".

- `pi_trust_screen.bin`: the terminal output of a launch in that folder, a 100x30 PTY, up to the first complete
  drawing of the project-trust confirmation and its immediate full redraw, with the cursor on `Trust`.
- `pi_trust_after_down.bin`: the output after one Down: only the two rows the cursor leaves and enters are redrawn,
  the cursor now on `Trust parent folder`.
- `pi_trust_dismissed.bin`: the output after Enter: the confirmation erased, then Pi's start-up screen. It is cut
  before the start-up warning that names the machine's Pi install path. The two lines about downloading `fd` and
  `ripgrep` are from the scratch agent directory having none yet.
- `pi_models.txt`: `pi --list-models` stdout for an account whose `models.json` holds two providers of three models, one
  of which thinks and takes images (the trailing spaces are as printed).
- `pi_models_signed_out.txt`: `pi --list-models` stdout for an account with no credential: the notice, then two doc
  paths, which are rewritten from the machine's Pi install path to `/usr/lib/node_modules/...`. It exits with status 0.

The input box, from one run on 2026-11-23 of Pi 1.5.2 on a 100x30 PTY in `/private/tmp/pi-probe-20/project`, with a
local stand-in answering as the provider of the scratch `models.json` and holding the reply open
(`pi --session-id <id> --model standin/m1 -- 'SLOW one'`). The run's output is cut where each step had been drawn, so
the files are fed one after another, in this order:

- `pi_box_working.bin`: from the launch to the turn running: Pi's `idle` and `working` reports, the empty box under
  the rule that reads `Working`.
- `pi_box_steered.bin`: `steer two` and `steer three` each pasted and entered: two `Steering:` rows above the box.
- `pi_box_cancelled.bin`: Esc: `Operation aborted`, the `idle` report, and both messages back in the box with an empty
  row between them.
- `pi_box_ctrl_u_1.bin` to `pi_box_ctrl_u_3.bin`: one Ctrl-U each: `steer three` gone, then a line break a press.
- `pi_box_draft.bin`: `my draft` typed after what was left, `steer two`.
- `pi_box_model_picker.bin`: a Ctrl-U that empties the box, then `/model` and Enter: the model selector drawn between
  rules where the box was.

A second run of the same kind on the same day, Pi 1.5.2 with `PI_SKIP_VERSION_CHECK=1` (so no update notice is drawn),
the same stand-in and launch, cut the same way:

- `pi_box_drafted_working.bin`: from the launch to the turn running, with Pi's `idle` and `working` reports.
- `pi_box_drafted_steered.bin`: `steer two` and `steer three` each pasted and entered.
- `pi_box_drafted_draft_typed.bin`: `my draft` typed into the box while the turn still ran.
- `pi_box_drafted_cancelled.bin`: Esc: the `idle` report, and in the box `steer two`, an empty row, `steer three`, an
  empty row, `my draft`.

A warning about an invalid `models.json` provider goes to stderr in 1.5.2 (`Warning: errors loading models.json:` and
a line naming the provider), and stdout then holds the table alone.

What Octoboard's extension forwards as Pi settles, from runs on 2026-11-23 of Pi 1.5.2 against the same kind of
stand-in:

- `pi_settled_hooks.jsonl`: `agent_settled` payloads the session's hook script was given, one a line. The first two
  are from a session named `Fix - the build` with `/name`: after a turn the stand-in answered (Pi's `done` report
  carried the session name), and after a request it refused with HTTP 400 (the `error` report carried the first line
  of the same error). The third is from an unnamed session, after a turn whose answer was empty.

From runs on 2026-11-23 of Pi 1.5.2 against the same kind of stand-in, its model declared to take images, with the
project in `/private/tmp/nap25/proj` and a second directory, `/private/tmp/nap25/other`, standing for another project:

- `pi_prompt_hooks.jsonl`: the `message_start` payloads Octoboard's extension forwarded in one run, one a line: two
  prompts that each made the stand-in call `write` or `edit`, a message of one pasted image path alone (an empty
  prompt), one of two image paths and `two images` pasted one after another, then `SLOW first` and `steer me`,
  entered while the first ran and taken into it.
- `pi_guard_payloads.jsonl`: what the extension asked the session's guard script in that run, one a line: a `write`
  of `/private/tmp/nap25/other/x5.txt`, a `write` of `local.txt` in the project, and an `edit` of
  `~/../other/y.txt`, each path as the extension resolved it.

From a run on 2026-11-23 of Pi 1.5.2 in a 60x30 PTY, launched with `--model standin/m1 --thinking off`,
`PI_SKIP_VERSION_CHECK=1` and `PI_OFFLINE=1`, a scratch configuration directory and the project in
`/private/tmp/nap25b/proj`, no model request made:

- `pi_box_image_paths_60.bin`: the bytes from the launch until after the last paste: two image paths,
  `/private/tmp/nap25/home/.octoboard/run/0199sess/pasted/paste-1795441000000-0.png` twice, and `text after`, each
  pasted on its own and none sent: each path wraps onto a second row of the box, and the text follows the second path
  with nothing between.


### Records and delivery, Pi 1.5.2

- `pi_branched_records.jsonl` is Pi's session file from a scratch configuration on macOS. A local model stand-in
  replied to an opening prompt and another turn. The capture extension navigated back to the first reply and a new
  prompt made Pi append a different branch containing a successful write to a scratch file and a final reply.
  The production history and feed readers replay this file. The final reply reports 179 tokens.
- The records milestone's other probes are recorded in `docs/agent-cli-reference.md`, "Pi". They exercised the
  production extension with a stand-in channel helper and tested device authorization without completing a new login.

### Exits as Pi starts, Pi 1.5.2

Measured on 2026-11-26 on macOS, Pi 1.5.2 on Node 22, in a 100x30 PTY with a scratch `HOME` and
`PI_CODING_AGENT_DIR` under `/private/tmp/pi-m10`, `PI_SKIP_VERSION_CHECK=1` and no credential. Each run's whole
output, from the launch to the exit; each exited with status 1 within 0.3 seconds, before switching to the alternate
screen or drawing anything.

- `pi_unknown_option_1.5.2.bin`: `pi --no-such-option`: the line `Error: Unknown option: --no-such-option` in red.
  The same bytes came for `pi --no-such-option value`.
- `pi_extension_throws_1.5.2.bin`: `pi --extension /private/tmp/pi-m10/throws.js`, whose default export throws
  `probe: thrown while loading`: `Error: Failed to load extension "<path>": Failed to load extension: <message>` in
  red, then `Hint: Start without extensions using "pi -ne".` in yellow.
- `pi_session_not_found_1.5.2.bin`, the near-miss: `pi --session <id>` for an id nothing matches: the line
  `No session found matching '<id>'` in red, a fast exit that is no option refusal.
