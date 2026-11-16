# Writing daemon tests for Windows

Every daemon test also runs on native Windows (the `windows-check` job of `.github/workflows/quality.yml`), which the
Mac never runs; these are the ways a test that passes on the Mac fails there.

## A path a test reads back is compared in its canonical form, and a path it only names is a host path

Applies to daemon tests that assert on a path or an environment the code under test or another process reports, and to
tests that name an absolute path without touching it.

- Resolve with `hostfs::canonical`, never `std::fs::canonicalize` (the platform scan refusing `canonicalize` exempts
  test code): on Windows the latter gives the verbatim `\\?\C:\...` form, which nothing the daemon or `git` reports
  carries.
- Compare a path a process reports (a working directory, a path `git` prints) by its canonical path, not its text:
  Windows reports a working directory as the process spelled it, and the CI runner's `TEMP` is in its 8.3 short form
  (`C:\Users\RUNNER~1\...`), so a scratch directory's own path and a report under it differ in spelling.
- A Unix-spelled absolute path the test only names (`/srv/repo`) is not absolute on Windows; build it with
  `test_support::host_path`.
- On Windows look a variable up in a captured environment without regard to case: a process's environment names `PATH`
  as `Path`.

## A name Windows `git` cannot hold goes into a fixture through a commit, on every host

Applies to daemon tests of paths a repository can hold but Windows cannot create (`a:b`, `CON`). Do not gate such a test
to Unix: a repository made elsewhere brings these names in its commits, and the daemon reads them on Windows too. But
Windows `git` refuses `a:b` in the index always (it reads it as a drive) and `CON` unless `core.protectNTFS` is off, so
`git add` or `update-index --cacheinfo` fails there before the daemon is reached. Build the commit with
`test_support::commit_holding` or `tree_holding` (`git mktree` checks a name for nothing but a `/`); in a Windows
checkout such a name is then a staged deletion. Stage `CON` only with `-c core.protectNTFS=false`, as a Windows user
would have to.

## A stand-in on a Windows console is not a Unix terminal

Applies to daemon tests that write into a stand-in session or a console helper and read back what reached it. A Windows
console echoes only what its program reads, and its host redraws that echo: line breaks come out as cursor moves, a
paste's markers and caret notation not at all, and what scrolls out between two frames is lost. Input that arrives
before the program has set its console raw is taken as typed keys and loses the paste markers.

- On Windows use a native stand-in (`stand_in_launch_echoing`, `stand_in_running_steps`, `idle_stand_in`), not a Git
  Bash script (`stand_in_launch` is Unix only): they read raw, echo as a Unix terminal would, and are handed back only
  once they say they read raw. A fixture of your own does the same: set the console raw, then signal ready, and the test
  writes nothing before it sees that signal.
- End a line written to a program that reads its console a line at a time with `\r`, not `\n`: a Windows console ends
  an edited line only at Enter and takes a line feed as a character of the line. A Unix terminal turns `\r` into a line
  feed, so `\r` works on both.
- Where the terminal hands a paste over as keys, as on Windows, the daemon writes Codex's Enter only once its input box
  shows the paste (`session::paste_apart`); a Codex stand-in that draws no box leaves the message Unsent, and everything
  queued behind it. Use `stand_in_launch_echoing`, which draws the box for such an agent, or the test provider
  (`AgentId::TEST`) where the agent does not matter.
- Git Bash's `printf` reaches the console in pieces some milliseconds apart, so a screen a fixture redraws can be seen
  half drawn. Where a test depends on a screen arriving whole, draw it with `test_support::console_writer` (one console
  write), started before the window the production code times, not inside it.

## A committed file a test reads arrives with CRLF line ends on Windows

Applies to daemon and desktop shell tests (the same job tests `-p octoboard`) that read a committed text file, by
`include_str!` or from disk. GitHub's `windows-latest` runners, like Git for Windows' default install, check out with
`core.autocrlf` on, so the file arrives with CRLF line ends, and a match across a line end fails there although every
line matches; the Mac never shows it.

- A file a test compares byte for byte with what code renders, as `protocol::bindings` and `protocol::schema` do with
  the generated protocol files: name it in `.gitattributes` with `text eol=lf`; do not loosen the comparison.
- A source file a test searches for text spanning a line end (`"#[cfg(test)]\nmod tests"`, `"\n}\n"`): turn `\r\n`
  into `\n` once it is read, or match line by line.
