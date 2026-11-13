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
  Remote Login, Windows OpenSSH Server) and guides the owner to turn it on. Everything about reaching the desktop
  daemon from elsewhere lives in one Remote access section of Settings, which each milestone adds to.
- **A browser reaches the desktop daemon through the owner's own SSH port forward** (`ssh -L`), so the desktop
  application's daemon can be used from a browser on another machine without the server form, as the server form's
  daemon already is. This is opt-in: an Allow browser access setting, off by default and confirmed explicitly when
  turned on, makes the daemon serve the UI the application bundles, accept the persistent owner key and listen at a
  fixed port, still on loopback only. It needs nothing of the tunnel the applications build, only the owner key that
  already exists, so it comes first. Quitting the desktop application still stops its daemon, so remote use needs the
  application kept running.
- **Clients.** The macOS and Windows applications connect through the tunnel they build themselves. A browser cannot
  build an SSH tunnel; it reaches a daemon through the owner's own SSH port forward, which milestone 01 makes possible
  for the desktop daemon. A phone connects the same way as the
  applications once a mobile client exists. Reaching a daemon without SSH, through a relay as a remote-desktop tool
  does, is a separate plan.

## Milestones

- [01 Browser access to the desktop daemon](01-browser-access-to-the-desktop-daemon.md) — needs the owner key, which
  exists
- [02 Connecting to a remote daemon over SSH](02-remote-daemon-over-ssh.md) — needs the daemon running on Linux and
  the `octoboardd` command printing its address and key
- [03 Device keys](03-device-keys.md) — needs 02
- [04 QR code and setup guide](04-qr-code-and-setup-guide.md) — needs 02
- [05 The desktop form as a host](05-desktop-form-as-host.md) — needs 01 and 04

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
- How the desktop daemon serves the UI: the application bundles its UI inside its executable rather than as separate
  files, so serving "the application's own files" means either the application writing its UI out for the daemon,
  which costs no download size but needs the setting known before the daemon starts, or the daemon carrying a
  compressed copy of the UI, which adds a few megabytes to every download.
- Whether turning browser access on or off, and changing its port, take effect at once or need the daemon restarted;
  and whether turning it off, or rotating the key, also drops browser pages already connected.
- The default port, and what Settings shows when that port is already in use.
- How the server form's page for a refused key words its way back (today: run `octoboardd address` on the server) when
  the daemon is the desktop application's.
- Whether the Remote access section shows anything in a browser opened on the desktop daemon.
