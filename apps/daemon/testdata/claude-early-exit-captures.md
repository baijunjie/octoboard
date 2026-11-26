# Claude Code early-exit captures

Taken on Claude Code 2.1.332 (native install, macOS) on 2026-11-26, each the raw bytes the CLI wrote on a pseudoterminal
(100 by 30, `TERM=xterm-256color`) in an empty working directory, with `CLAUDE_CONFIG_DIR` set to an empty directory.
Neither run drew anything or wrote into the configuration directory, and neither reached the network.

- `claude_early_exit_unknown_option_2.1.332.bin`: `claude --session-id <uuid> --octoboard-unknown-option --effort
  high`. `error: unknown option '--octoboard-unknown-option'`, then the cursor shown again (`ESC [?25h`); status 1,
  about 0.07 s after the start. The refusal names the first option it does not know, before any option after it is
  read.
- `claude_early_exit_settings_not_found_2.1.332.bin`, the near miss: `claude --settings
  /nonexistent/octoboard-probe.json --effort high`. `Error: Settings file not found: <path>` in red, then the cursor
  shown again; status 1, about 0.06 s after the start. A fast exit before drawing that is no option refusal.

`adapter/claude/mod.rs` replays both: neither gives a reduced plan, so the relaunch after either is bare.
