# Grok Build activity title captures

Captured on 2026-10-26 from Grok Build 1.0.70 running interactively in a PTY against a local stand-in for its
inference API, with command hooks logging every event. Only complete OSC title sequences are retained, in the order
they were sent; screen output is excluded.

- `grok_title_cancel_before_output.bin`: a turn whose request was never answered, from its first running frame
  (`⠋ - Waiting for response… - grok`, the spinner turning) to the title without a spinner sent 16 ms after Ctrl-C was
  pressed. No hook fired for the cancel: no `StopCancelled`, no `Stop`; the `idle_prompt` notification came about a
  minute later.
- `grok_title_turn_end.bin`: the near miss — a turn that ended normally. Its `Stop` hook (`reason: "end_turn"`) was
  posted about 150 ms before the title lost its spinner.
- `grok_title_tool.bin`: a turn running a shell command (`sleep`), whose title keeps its spinner for the whole run.
