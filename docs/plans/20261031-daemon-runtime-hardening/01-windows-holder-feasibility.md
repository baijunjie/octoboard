# 01 Windows holder feasibility

> Goal: settle by testing on Windows whether a holder can keep a session's programs running while the daemon is gone,
> and how an update replaces the files that running holders and helpers keep open.
> Completion criteria:
> - On a Windows 11 machine, a process detached from the daemon creates a pseudoconsole and starts an agent on it.
>   The daemon is then stopped normally, and killed outright in a second run. In both runs the agent keeps running and
>   still takes input and gives output through that process. Each run is done once with the bundled console host and
>   once with the system's.
> - An update is installed while an old holder, its console host and an MCP bridge run, and hooks and the git gate
>   are called during and after the install. The installer replaces every file it ships, nothing running is killed,
>   and the old processes keep working.
> - The outcome of both is recorded in `docs/architecture.md`'s Windows notes. If either fails, the record says what
>   failed and why, and Windows takes the matching fallback in the overview.

## Technical design

- [ ] The detach: how a holder leaves the daemon's hidden console and the daemon's jobs, so that the daemon ending,
      its console closing or its jobs closing does not end the holder or the programs on the holder's consoles.
      This includes the case where the daemon itself runs inside a kill-on-close job.
- [ ] Console lifetime under a holder: a pseudoconsole created by the holder outlives the daemon, with both the
      bundled host and the system's host.
- [ ] Holder death: what the programs on a holder's consoles see when the holder itself is killed. This is expected to
      end them, as closing a console does. It sets the limit of crash survival on Windows.
- [ ] Files in use during an update. These run from the installation folder today: the daemon binary, which the MCP
      bridge `octoboardd mcp`, the hooks and the git gate already are and the holder mode will be, `OpenConsole.exe`
      and `conpty.dll`. How the NSIS installer behaves when they are open, and whether running holders and helpers
      from a per-build copy outside the installation folder (see the overview) is enough for it.

## Implementation plan

- [ ] A throwaway test harness, outside the shipped code, that creates a detached holder, a pseudoconsole and an
      agent. Run the lifetime cases on the Windows test machine.
- [ ] Install an update over running processes and observe the installer.
- [ ] Record the outcomes and the chosen detach and file-location approaches.

## Notes for the developer

- **Reusable capabilities**: the daemon's ConPTY backend and the bundled console host lookup; the named job per launch
  without kill-on-close, and its reopening by name after a restart.
- **Development notes**:
  - The daemon runs with a hidden console of its own, and its group leaders share it. A launch never asks to break away
    from an outer job, because that fails where the outer job does not allow it.
  - Closing a pseudoconsole sends `CTRL_CLOSE_EVENT` to every process attached to it, and the bundled host's close
    blocks while output is unread.
  - The panic and abort hook of the daemon's process layer kills every PTY process when the daemon crashes. The
    harness must not inherit it, and milestone 02 decides where it goes.
- **Reference docs**: `docs/architecture.md` ("Running on more than one operating system" and its Windows notes),
  `docs/structure/daemon.md`, `docs/product/in-app-updates.md` ("Downloading and installing").
