# Opening a project terminal

**Terminal** under **Open in** in a project's action menu opens the system terminal at that project's directory,
on the machine the daemon runs on — not on the machine a browser client happens to be open on. It is offered for every project,
including one that is not a git repository and one whose git status has not arrived yet. That menu is on the
project's row, on the focused project's header, and on a project's heading in a console session's focus mode; a
right-click opens it too (see "Project rows" in `docs/product/sidebar.md`). Choosing it closes the menu at once.
Nothing is toasted when the terminal has been opened. A failure is an error alert worded from the refusal below.

The terminal is not part of Octoboard's window. The request finishes once the terminal has been asked to open, not
when the user closes it, and quitting Octoboard or the daemon does not close it.

## Which terminal opens

The working directory is the project's directory.

On macOS, Octoboard asks Terminal.app to open that directory, through `/usr/bin/open -a Terminal --` followed by the
directory. Terminal.app starts a shell there. Octoboard does not choose whether that is a new window or a tab; that
is Terminal.app's own. The request finishes when `open` returns, which is when Terminal.app has accepted the
directory. If `open` has not returned within 15 seconds, the request fails.

On Linux there is no single system terminal. The programs below are looked up in the user's login-shell `PATH`, not
in whatever the daemon process itself can see. The first one that is found is started, and it is given the directory:

- `gnome-terminal`, with `--working-directory`
- `konsole`, with `--workdir`
- `xfce4-terminal`, with `--working-directory`
- `mate-terminal`, with `--working-directory`
- `kitty`, with `--directory`
- `alacritty`, with `--working-directory`
- `wezterm`, with `start --cwd`
- `foot`, with `--working-directory`

When none of those is found, `x-terminal-emulator` is used if it is found, and otherwise `xterm`. Those two are not
given the directory as an argument: they inherit it as their own working directory. The request finishes once that
process has been started, not when its window is on screen.

Any other operating system has no system terminal Octoboard opens.

## Refusals

The open is refused with `terminal_unavailable`, and the error alert includes the reason, when:

- the project directory does not exist, or is not a directory;
- the project path is not valid Unicode;
- the operating system is not macOS or Linux;
- on Linux, none of the programs in "Which terminal opens" is found;
- on Linux, the login shell's environment could not be read — this is not reported as a missing emulator;
- the launch itself fails, including, on macOS, `open` not returning within 15 seconds.

A project that no longer exists is refused with `unknown_project`. Nothing is opened in either case.
