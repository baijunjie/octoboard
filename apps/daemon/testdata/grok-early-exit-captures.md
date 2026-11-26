# Grok Build early-exit captures

Captured on 2026-11-26 from Grok Build 1.0.92 (`b06756fd6256`), run in a 120×40 PTY from an empty working directory
with an empty `HOME` and `GROK_HOME` of their own, no key typed, every byte the process wrote kept as it came. No
prompt was sent and no model was reached: each run ended in Grok's command-line parser or its configuration load.
1.0.89 (`28be5bb1bf65`) printed the same refusal and exited with the same status for the unknown option.

- `grok_1.0.92_unknown_option.bin`: `grok --octoboard-unknown-option`, an option no release takes. Exit status 2
  within 0.2 s, before anything is drawn: `error: unexpected argument '--octoboard-unknown-option' found`, a tip to
  pass it after `--`, the usage line and a pointer to `--help`. A release that dropped an option Octoboard passes
  would refuse it on this line, which is what Grok's reduced launch reads (`adapter/grok/refusal.rs`).
- `grok_1.0.92_unknown_option_like_no_leader.bin`: the near-miss for `--no-leader`. `grok --no-leaderx` is refused on
  the same line, and its tip names `--no-leader` (`a similar argument exists: '--no-leader'`), so only the `error:`
  line may be read for the option refused.
- `grok_1.0.92_invalid_option_value.bin`: `grok --permission-mode octoboardMode`, a known option given a value it does
  not take. Exit status 2, before anything is drawn, on a line of another shape: `error: invalid value
  'octoboardMode' for '--permission-mode <MODE>'` and the possible values (`default`, `acceptEdits`, `auto`,
  `dontAsk`, `bypassPermissions`, `plan`). It names no option to leave out.
- `grok_1.0.92_config_unreadable.bin`: the near-miss of a fast exit that is no option refusal. `grok --no-leader`
  against a home whose `config.toml` is not TOML exits with status 1 in 0.3 s with `Error: Failed to load config:
  <path> cannot be read or parsed: TOML parse error at line 1, column 5: unclosed table, expected `]``. The home was
  `/private/tmp/gp/grokhome` for this capture.

Taken in the same runs, not kept as captures: `--no-leader --version`, `--session-id not-a-uuid --version`,
`--reasoning-effort octoboard --version` and `--rules x --version` each printed `grok 1.0.92 (b06756fd6256)` and exited
with status 0, so 1.0.92 takes `--no-leader` (as 1.0.89 does), and checks neither the session id's shape nor the
reasoning effort's value while parsing.
