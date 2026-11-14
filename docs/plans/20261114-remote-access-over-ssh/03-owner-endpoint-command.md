# 03 The owner endpoint command

> Goal: a program run on the daemon's machine as the daemon's user — over SSH, say — reaches the daemon's owner
> endpoint through `octoboardd` alone, and learns from it what it needs to use that daemon.
> Completion criteria: on Linux and macOS, a mode of `octoboardd` connects to the running daemon's owner socket and
> carries the bytes both ways between it and its own standard input and output until either side closes, so that an
> HTTP request and a WebSocket made through `ssh <host> octoboardd <mode>` are answered as they are on the socket;
> with no daemon running, with a daemon that has no owner endpoint (one that serves no browser, or whose remote
> access is off) and with an endpoint that cannot be reached, it exits with a status of its own for each and the
> reason on its error output; `octoboardd address` can print its answer in a form a program reads, with the owner
> key, the protocol generation and the release version of the running daemon; no key is on any command line.

## Technical design

- [ ] A proxy mode of `octoboardd`: it finds the running daemon as the other commands do, connects to its owner
      endpoint and copies bytes in both directions, writing nothing else to its standard output.
- [ ] Its exit statuses tell apart no daemon running, a daemon without an owner endpoint, and an endpoint that
      cannot be reached.
- [ ] A machine-readable form of `octoboardd address`'s answer: the owner key, the protocol generation the daemon
      speaks and its release version.

## Notes for the developer

- **Reusable capabilities**: how the commands find the daemon, waiting through a planned restart; the key check
  `address` makes on the socket; the commands' exit statuses (3 for no daemon running).
- **Development notes**: the owner endpoint on Windows, a named pipe, comes with milestone 09; until then the mode
  says on Windows that the daemon has no owner endpoint. Against the desktop application's daemon both are checked
  in milestone 09, since that daemon has an owner endpoint only once milestone 01 is in. The mode is a byte pipe and
  reads no key: the client sends the owner key in its own requests, as a browser does.
- **Reference docs**: `docs/product/daemon-command-line.md`, `docs/product/server-form.md` ("The browser socket"),
  `apps/daemon/PROTOCOL.md` ("Versions and compatibility", "Who may connect"), `docs/structure/daemon.md`.
