# Remote Access over SSH Development Plan

## Problem and approach

The owner wants to reach their daemon from another device: the macOS or Windows application controlling a daemon on
a Linux server (the server form, see "Two deployment forms" in `docs/architecture.md`), or another machine or a phone
reaching the daemon of their desktop form. The daemon is a remote shell by nature — it starts agents, types into them
and lists directories — so the connection needs authentication and encryption.

SSH already provides both, and the owner already has it to their server. The client application therefore builds an
SSH tunnel itself, to the daemon on the remote machine's loopback, and the daemon never listens beyond loopback.
Setting a device up is made easy with a QR code carrying the connection details, in the spirit of a remote-desktop
tool's pairing, without a network layer or a hosted service of Octoboard's own.

One user, any number of their devices. Whoever can log in over SSH as the daemon's Unix user can already control the
daemon, so that login is the credential: a device is let in by its SSH key and revoked by removing it.

## Key design decisions

- **The daemon stays on loopback**, in both forms. There is no external listening surface, no TLS and no
  daemon-issued per-device credential: SSH authenticates and encrypts.
- **The application builds the tunnel.** It connects over SSH, obtains the remote daemon's address and key by running
  an `octoboardd` command, as the daemon's user, that prints them in a form a program can read, forwards a local port
  to it, and runs the UI against that.
- **First login with the user's own SSH credentials** (a password or a key they already have). The application then
  makes a key pair of its own for that device, installs the public key for the remote user, as `ssh-copy-id` does,
  and connects with it from then on.
- **The host key is pinned**: the QR code carries the remote host key's fingerprint, and the application refuses a
  host whose key does not match it.
- **Revoking a device** is removing its SSH key; a connection already open lasts until it drops.
- **The QR code carries connection details only**: host, port, user and host key fingerprint, never a secret. It is
  shown in the Linux terminal by `octoboardd` and in the desktop application, with the same details as text for a
  device without a camera.
- **The desktop form can be a host too.** The application checks whether the computer's SSH server is on (macOS
  Remote Login, Windows OpenSSH Server) and guides the owner to turn it on.
- **Clients.** The macOS and Windows applications connect through this plan. A browser cannot build an SSH tunnel; it
  reaches a daemon through the owner's own SSH port forward, outside this plan. A phone connects the same way as the
  applications once a mobile client exists. Reaching a daemon without SSH, through a relay as a remote-desktop tool
  does, is a separate plan.

## Milestones

- [01 Connecting to a remote daemon over SSH](01-remote-daemon-over-ssh.md) — needs the daemon running on Linux and
  the `octoboardd` command printing its address and key
- [02 Device keys](02-device-keys.md) — needs 01
- [03 QR code and setup guide](03-qr-code-and-setup-guide.md) — needs 01
- [04 The desktop form as a host](04-desktop-form-as-host.md) — needs 03

## Open

- Whether the application can start a remote daemon that is not running, or only tells the owner it is not.
- Whether the application shows one daemon at a time (its own or a remote one) or several side by side.
- How a device's installed key is listed and removed from within Octoboard, beyond removing it on the remote machine.
- Settings' Permissions section (`permissionsPane`) shows the client application's macOS privacy grants, which reach
  agents only while the daemon is the application's local child: whether to hide it, or say it does not apply, when
  the daemon is remote.
- Which mobile client a phone uses; no mobile client exists yet.
- How a host typed in by hand, with no fingerprint, is trusted on first connection.
- Where the application keeps passwords and private keys.
