# 02 Owner access link

> Goal: the owner, logged in to the server as the daemon's user, can get the address to open in their browser, key
> included, whether the daemon was started by hand or by systemd. Completion criteria: for a daemon started with `start`
> and for one run by the example systemd unit, including the successor of a planned restart under that unit, the owner's
> command prints an address that the daemon accepts through an SSH port forward; another Unix user on the server cannot
> obtain the key through that command or its channel; the key appears nowhere in the journal, the owner record, the
> daemon's arguments or its environment.

## Technical design

- [ ] A channel only the daemon's own Unix user can read, through which a running daemon hands its current key to a
      command of the same user.
- [ ] An owner command that finds the running daemon (as `status` does) and prints the browser address with the key.
- [ ] The example unit's key-delivery TODO resolved: a service-run daemon, its planned-restart successor included,
      is reachable through the command while its standard output stays discarded.

## Notes for the developer

- **Reusable capabilities**: the owner record and single-instance lookup the background commands use; the key check
  every route already applies.
- **Development notes**: `start` relays the handshake line, key included, to the owner's terminal and keeps doing so;
  `status` prints the address alone today. The daemon's own arguments and environment must not gain the key. Its
  output stays stable enough for a program, run over SSH, to read the port and key.
- **Reference docs**: `apps/daemon/PROTOCOL.md` ("Who may connect", "The key"),
  `docs/product/application-lifecycle.md` ("Running as a systemd user service").
