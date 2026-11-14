# 08 Connection details and setup guide

> Goal: the owner sets a device up from the connection details shown on the remote machine, pasted or typed, with a
> QR code of the same details ready for a mobile client.
> Completion criteria: `octoboardd` prints the details as text and as a QR code in the Linux terminal; the text,
> pasted into the macOS or Windows application, fills a remote daemon entry with its pinned fingerprint and the path
> of `octoboardd` and connects; the QR code decodes to the same text; the setup guide covers a Linux server end to
> end, removing a device's key included.

## Technical design

- [ ] The connection details: host, SSH port, user, host key fingerprint and the path of `octoboardd`, with no
      secret, in one text form that a QR code carries unchanged.
- [ ] An `octoboardd` command printing them as text and as a QR code.
- [ ] Filling a remote daemon entry from the pasted text in the application.
- [ ] A user-facing setup guide.

## Notes for the developer

- **Development notes**: choosing the host to put in the details is the awkward part when the machine has several
  interfaces or is reached by a name the machine does not know itself; the owner may need to correct it. No client
  scans the QR code yet.
- **Reference docs**: the repository's root `README.md`, `docs/product/installing-the-server-form.md`.
