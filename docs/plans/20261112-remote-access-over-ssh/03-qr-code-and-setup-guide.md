# 03 QR code and setup guide

> Goal: the owner sets a device up from the connection details shown on the remote machine, scanned or typed.
> Completion criteria: `octoboardd` prints a QR code and the same details as text in the Linux terminal; the details
> typed into the macOS or Windows application fill a remote daemon entry with its pinned fingerprint and connect; the
> setup guide covers a Linux server end to end.

## Technical design

- [ ] The connection details: host, SSH port, user and host key fingerprint, with no secret, in a form both a QR code
      and text carry.
- [ ] An `octoboardd` command printing them as a QR code and as text.
- [ ] Filling a remote daemon entry from the details in the application.
- [ ] A user-facing setup guide.

## Notes for the developer

- **Development notes**: choosing the host to put in the details is the awkward part when the machine has several
  interfaces or is reached by a name the machine does not know itself; the owner may need to correct it.
- **Reference docs**: `README.md`.
