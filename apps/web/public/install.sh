#!/bin/sh
# Installs octoboardd, the Octoboard daemon on its own (the server form), on Linux, for the current user, without root:
#
#   curl -fsSL https://octoboard.dev/install.sh | sh
#   curl -fsSL https://octoboard.dev/install.sh | sh -s -- --service [--port N]
#
# It reads the newest version from the release manifest of the chosen channel, downloads the archive for this
# machine's architecture, checks its SHA-256 (and its signature when `minisign` is installed) and replaces
# ~/.local/bin/octoboardd in one rename, so a running daemon keeps running on the old file until it restarts. Run it
# again to upgrade. With --service it also installs and starts a systemd user unit that starts the daemon at boot.
#
# Options:
#   --service         Install the systemd user unit octoboardd.service, enable it and start it.
#   --port N          The port the unit's daemon listens on (with --service; default: default_port, set below).
#   --beta            Take the newest beta instead of the newest stable release.
#   --bin-dir DIR     Install into DIR instead of ~/.local/bin.
#   -h, --help        Print this help.
#
# OCTOBOARD_MANIFEST_BASE replaces the address the manifests are read from, for a mirror or a test, and
# OCTOBOARD_UPDATE_PUBKEY the key a signature is checked with, for a test.
set -eu

manifest_base_default="https://github.com/baijunjie/octoboard/releases/download/updater"
# The port the service listens on without --port, the same as `octoboardd start`'s (`defaultServerPort` in the
# repository's config/app.json).
default_port=3369
# The public half of the update key the release signs every archive with (the desktop application's updater trusts
# the same key).
update_pubkey="RWSVMIwih++1gX8lYHjtWtYI6FzRxgcQVkKLHTqLgNw0lz27touOK8GB"

say() { printf '%s\n' "$*"; }
fail() {
  printf 'octoboardd install: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat << USAGE
Installs octoboardd, the Octoboard daemon for a Linux server, into ~/.local/bin.

Usage: install.sh [--service [--port N]] [--beta] [--bin-dir DIR]

  --service       Install the systemd user unit octoboardd.service, enable it and start it.
  --port N        The port the service's daemon listens on (default $default_port).
  --beta          Install the newest beta instead of the newest stable release.
  --bin-dir DIR   Install into DIR instead of ~/.local/bin.
  -h, --help      Print this help.

Run it again to upgrade.
USAGE
}

# Downloads $1 to the file $2, with curl or wget.
fetch() {
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL --proto '=https,file' -o "$2" "$1"
  elif command -v wget > /dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    fail "neither curl nor wget is installed."
  fi
}

# Only these schemes are downloaded from, whichever downloader runs: curl is told so, wget is not.
https_or_file() {
  case "$1" in
    https://* | file://*) ;;
    *) fail "$1 is neither an https nor a file address; nothing was downloaded from it." ;;
  esac
}

sha256_of() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

# Prints the value of the string field $2 in the entry of the platform key $1 of the manifest file $3, which
# `publish-release.mjs` writes one field per line.
platform_field() {
  awk -v key="\"$1\"" -v field="\"$2\"" '
    index($0, key ":") { inside = 1; next }
    inside && /}/ { exit }
    inside && index($0, field ":") {
      sub(/^[^:]*:[[:space:]]*"/, ""); sub(/".*$/, ""); print; exit
    }
  ' "$3"
}

main() {
  service=false
  port=""
  port_given=false
  channel=stable
  bin_dir="${HOME:?HOME is not set}/.local/bin"
  while [ $# -gt 0 ]; do
    case "$1" in
      --service) service=true ;;
      --port)
        [ $# -ge 2 ] || fail "--port needs a port number."
        port=$2
        port_given=true
        shift
        ;;
      --port=*)
        port=${1#--port=}
        port_given=true
        ;;
      --beta) channel=beta ;;
      --bin-dir)
        [ $# -ge 2 ] || fail "--bin-dir needs a directory."
        bin_dir=$2
        shift
        ;;
      --bin-dir=*) bin_dir=${1#--bin-dir=} ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) fail "unknown option $1 (see --help)." ;;
    esac
    shift
  done
  if [ "$port_given" = true ]; then
    [ "$service" = true ] || fail "--port is the port of the service, so it goes with --service."
    case "$port" in
      "" | *[!0-9]*) fail "--port $port is not a port number." ;;
    esac
    { [ "$port" -ge 1 ] && [ "$port" -le 65535 ]; } || fail "--port $port is not a port number."
  fi
  port=${port:-$default_port}
  case "$bin_dir" in
    /*) ;;
    *) bin_dir="$(pwd)/$bin_dir" ;;
  esac
  if [ "$service" = true ]; then
    command -v systemctl > /dev/null 2>&1 || fail "--service installs a systemd user unit, and systemctl is not installed."
    # The path goes into the unit's ExecStart line as it is, so nothing systemd or the sed that writes it interprets.
    case "$bin_dir" in
      *[[:space:]%\\\&\|\$\;\'\"\`]*) fail "--service needs an install directory without spaces or any of % \\ & | \$ ; ' \" \`; $bin_dir has one." ;;
    esac
  fi
  command -v sha256sum > /dev/null 2>&1 || command -v shasum > /dev/null 2>&1 \
    || fail "neither sha256sum nor shasum is installed, and the download is checked with one of them."

  [ "$(uname -s)" = Linux ] || fail "octoboardd is installed this way on Linux only; this is $(uname -s). On macOS and Windows, install the desktop application from https://octoboard.dev."
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) fail "there is no octoboardd build for $(uname -m); builds exist for x86_64 and aarch64." ;;
  esac

  tmp=$(mktemp -d "${TMPDIR:-/tmp}/octoboardd-install.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  manifest_base=${OCTOBOARD_MANIFEST_BASE:-$manifest_base_default}
  manifest_url="${manifest_base%/}/$channel-linux.json"
  https_or_file "$manifest_url"
  fetch "$manifest_url" "$tmp/manifest.json" || fail "could not read $manifest_url."
  version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$tmp/manifest.json" | head -n 1)
  case "$version" in
    "" | *[!0-9A-Za-z.+-]*) fail "$manifest_url names no version." ;;
  esac
  url=$(platform_field "linux-$arch" url "$tmp/manifest.json")
  sha256=$(platform_field "linux-$arch" sha256 "$tmp/manifest.json")
  signature=$(platform_field "linux-$arch" signature "$tmp/manifest.json")
  [ -n "$url" ] && [ -n "$sha256" ] || fail "$manifest_url has no linux-$arch build of $version."

  https_or_file "$url"
  say "Downloading octoboardd $version for $arch..."
  fetch "$url" "$tmp/octoboardd.tar.gz" || fail "could not download $url."
  [ "$(sha256_of "$tmp/octoboardd.tar.gz")" = "$sha256" ] || fail "the download does not match its SHA-256; nothing was installed."
  if command -v minisign > /dev/null 2>&1; then
    [ -n "$signature" ] || fail "$manifest_url has no signature for the linux-$arch build."
    printf '%s' "$signature" | base64 -d > "$tmp/octoboardd.tar.gz.minisig" 2> /dev/null || fail "the signature in $manifest_url is not valid."
    minisign -Vq -P "${OCTOBOARD_UPDATE_PUBKEY:-$update_pubkey}" -m "$tmp/octoboardd.tar.gz" -x "$tmp/octoboardd.tar.gz.minisig" \
      || fail "the download's signature does not verify; nothing was installed."
    say "Signature verified."
  fi
  mkdir "$tmp/unpacked"
  tar -xzf "$tmp/octoboardd.tar.gz" -C "$tmp/unpacked"
  unpacked="$tmp/unpacked/octoboardd_${version}_linux_$arch"
  [ -f "$unpacked/octoboardd" ] || fail "the archive does not hold octoboardd."

  # A rename within the directory replaces the file in one step: nothing ever sees half a binary, and a running daemon
  # keeps the file it started from.
  mkdir -p "$bin_dir"
  staged="$bin_dir/.octoboardd.install.$$"
  cp "$unpacked/octoboardd" "$staged"
  chmod 755 "$staged"
  mv -f "$staged" "$bin_dir/octoboardd"
  binary="$bin_dir/octoboardd"
  say "Installed octoboardd $version to $binary."

  running=false
  if "$binary" status > /dev/null 2>&1; then running=true; fi

  if [ "$service" = true ]; then
    install_service
  elif [ "$running" = true ]; then
    say "A daemon is running from the file this replaced. Run \`octoboardd restart\` to move it to $version; its sessions keep running."
  else
    say "Start it with \`octoboardd start\` (port $port; \`--port N\` picks another), then open the address \`octoboardd address\` prints through an SSH forward (ssh -L $port:127.0.0.1:$port <this server>)."
  fi

  case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) say "$bin_dir is not on your PATH; add it (for example in ~/.profile) to run octoboardd by name." ;;
  esac
}

install_service() {
  unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  unit="$unit_dir/octoboardd.service"
  [ -f "$unpacked/octoboardd.service" ] || fail "the archive does not hold octoboardd.service."
  mkdir -p "$unit_dir"
  exec_start="ExecStart=$binary --port $port --service-managed"
  sed "s|^ExecStart=.*|$exec_start|" "$unpacked/octoboardd.service" > "$tmp/octoboardd.service"
  grep -qxF "$exec_start" "$tmp/octoboardd.service" || fail "the archive's octoboardd.service has no ExecStart line."
  changed=false
  if [ -f "$unit" ] && ! cmp -s "$tmp/octoboardd.service" "$unit"; then
    changed=true
    say "Replaced $unit with the unit of $version for port $port; any changes you made to it are gone."
  fi
  mv -f "$tmp/octoboardd.service" "$unit"
  systemctl --user daemon-reload || fail "systemctl --user could not reach your user's service manager (log in over SSH rather than with su, so that a user session exists)."
  systemctl --user enable octoboardd.service > /dev/null 2>&1 || fail "could not enable octoboardd.service."
  if systemctl --user is-active --quiet octoboardd.service; then
    if [ "$changed" = true ]; then
      say "The service is running from the file and the unit this replaced. Run \`octoboardd restart\` to move it to $version and to the new unit, port $port included; its sessions keep running. Then open the address \`octoboardd address\` prints."
    else
      say "The service is running from the file this replaced. Run \`octoboardd restart\` to move it to $version; its sessions keep running."
    fi
  elif [ "$running" = true ]; then
    say "Installed and enabled the service, but did not start it: a daemon started outside it is running. Stop that one (\`octoboardd stop\`) and run \`systemctl --user start octoboardd\`."
  else
    systemctl --user start octoboardd.service || fail "the service did not start; see \`journalctl --user -u octoboardd\`."
    say "Started the service on port $port. Open the address \`octoboardd address\` prints through an SSH forward (ssh -L $port:127.0.0.1:$port <this server>)."
  fi
  user=$(id -un)
  if [ "$(loginctl show-user "$user" --property=Linger --value 2> /dev/null || true)" != yes ]; then
    say "Your user services stop when you log out and do not start at boot until lingering is on. Turn it on once with \`loginctl enable-linger $user\` (it may need sudo)."
  fi
}

main "$@"
