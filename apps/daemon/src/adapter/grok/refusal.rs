//! What Grok prints when it refuses an option on its command line, read for the reduced launch
//! that follows an early exit (`AgentProvider::reduced_plan`).
//!
//! Grok parses its command line before it draws anything, and refuses an option it does not know
//! by printing `error: unexpected argument '<option>' found`, a tip and its usage, and exiting with
//! status 2 within a fifth of a second (1.0.89 and 1.0.92, measured in a PTY; the captures and the
//! near-misses beside them are described in `apps/daemon/testdata/grok-early-exit-captures.md`).
//! A value it does not take for a known option is refused on a line of another shape
//! (`error: invalid value '<value>' for '<option> <VALUE>'`), which names no option to leave out,
//! and an unreadable configuration ends it as fast with `Error: Failed to load config: …`, which
//! is no option refusal at all.

/// The option the refusal line in `output`, the early exit's plain text, names; `None` where the
/// output holds no such line. An option given as `--name=value` is named without its value.
pub(super) fn refused_option(output: &str) -> Option<&str> {
    output.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("error: unexpected argument '")?;
        let (argument, _) = rest.split_once("' found")?;
        Some(argument.split_once('=').map_or(argument, |(name, _)| name))
    })
}
