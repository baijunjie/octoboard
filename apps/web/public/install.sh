#!/bin/sh
# Installs octoboardd, the Octoboard daemon on its own (the server form), on Linux, for the current user, without root:
#
#   curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh
#   curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --service [--port N]
#   curl --proto '=https' --tlsv1.2 -fsSL https://octoboard.dev/install.sh | sh -s -- --uninstall
#
# It reads the newest version from the release manifest of the chosen channel, downloads the archive for this
# machine's architecture, checks its SHA-256 and its signature, and replaces ~/.local/bin/octoboardd in one rename
# (keeping the replaced file as octoboardd.previous, for `octoboardd update --rollback`). A daemon that runs is then
# restarted into the new file where that interrupts no session (`octoboardd restart --if-safe`); otherwise it keeps
# running on the old file until `octoboardd restart`. Run it again to upgrade. With --service it also installs and
# starts a systemd user unit that starts the daemon at boot.
#
# With --uninstall it installs nothing and removes what an install put on the system: the systemd user unit, lingering
# where the install turned it on (which it notes in ~/.octoboard/install.lingering), octoboardd, octoboardd.previous and
# the hidden .octoboardd.* files an update or a rollback leaves beside them. Lingering stays on where another service
# of the user's is enabled. It leaves ~/.octoboard, the daemon's own data, and says how to remove it.
#
# Uninstalling is octoboardd's own command, `octoboardd uninstall`, which needs no download: --uninstall here only
# finds the installed octoboardd and hands over to it, with --yes, --force and --keep-changes passed on. That command
# first clears what Octoboard's features have in effect on this system outside ~/.octoboard, which only octoboardd
# can clear. Only for an octoboardd from before that command does this script remove the installation itself, as it
# always has (the part marked as the fallback for older builds below), and such a build clears nothing first.
#
# The signature is checked with `minisign` where it is installed, else with the octoboardd being upgraded, which has
# the release key built in (`octoboardd verify-archive`). Where neither can (a first install without minisign) the
# script says so in a warning and goes on with the SHA-256 alone, or, with --require-signature, stops.
#
# Options:
#   --service             Install the systemd user unit octoboardd.service, enable it and start it.
#   --port N              The port the unit's daemon itself listens on (with --service; default: one the system assigns).
#   --beta                Take the newest beta instead of the newest stable release.
#   --stable              Take the newest stable release even when the installed beta is newer.
#   --allow-downgrade     Install the channel's newest release even when it is older than the installed one.
#   --require-signature   Stop instead of installing when the signature cannot be checked.
#   --bin-dir DIR         Install into DIR instead of ~/.local/bin.
#   --uninstall           Remove octoboardd and what the install set up, instead of installing, by handing over to
#                         `octoboardd uninstall` (with --bin-dir where the install had it; no option of an install).
#   --yes                 With --uninstall: uninstall without asking (passed on to `octoboardd uninstall`).
#   --force               With --uninstall: stop the daemon whatever work that interrupts (passed on).
#   --keep-changes        With --uninstall: do not clear what Octoboard's features have in effect outside
#                         ~/.octoboard first (passed on).
#   -h, --help            Print this help.
#
# Without --beta or --stable, the channel is stable, unless the installed build is a prerelease newer than the stable
# release: then it is beta, so that a beta is not offered an older stable release.
#
# OCTOBOARD_MANIFEST_BASE replaces the address the manifests are read from, for a mirror or a test (and only then is
# a file:// address downloaded from), and OCTOBOARD_UPDATE_PUBKEY the key minisign checks a signature with, for a
# test; with it set, the installed octoboardd, which knows only the release key, is not asked.
set -eu

manifest_base_default="https://github.com/baijunjie/octoboard/releases/download/updater"
# The public half of the update key the release signs every archive with (the desktop application's updater trusts
# the same key).
update_pubkey="RWSVMIwih++1gX8lYHjtWtYI6FzRxgcQVkKLHTqLgNw0lz27touOK8GB"
# The file of the daemon's data directory that says this script turned lingering on, holding the user's name; with it
# --uninstall turns lingering off again, and without it leaves lingering as it is.
linger_record="install.lingering"

say() { printf '%s\n' "$*"; }
warn() { printf 'octoboardd install: warning: %s\n' "$*" >&2; }
fail() {
  printf 'octoboardd install: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat << USAGE
Installs octoboardd, the Octoboard daemon for a Linux server, into ~/.local/bin.

Usage: install.sh [--service [--port N]] [--beta | --stable] [--allow-downgrade] [--require-signature]
                  [--bin-dir DIR]
       install.sh --uninstall [--bin-dir DIR] [--yes] [--force] [--keep-changes]

  --service             Install the systemd user unit octoboardd.service, enable it and start it.
  --port N              The port the service's daemon itself listens on (default: one the system assigns).
  --beta                Install the newest beta instead of the newest stable release.
  --stable              Install the newest stable release even when the installed beta is newer.
  --allow-downgrade     Install the newest release even when it is older than the installed one.
  --require-signature   Stop when the download's signature cannot be checked (minisign is not installed and
                        no installed octoboardd can check it), instead of going on with its SHA-256 alone.
  --bin-dir DIR         Install into DIR instead of ~/.local/bin.
  --uninstall           Remove octoboardd, octoboardd.previous and what an update left beside them, the systemd
                        user unit, and lingering where the install turned it on and no other service of
                        yours is enabled, instead of installing. It leaves ~/.octoboard, the daemon's data.
                        Takes --bin-dir where the install had it, and no option of an install. It hands over
                        to the installed octoboardd's own command, \`octoboardd uninstall\`, which you can run
                        directly instead: that first clears what Octoboard's features have in effect on this
                        system outside ~/.octoboard, then stops the daemon and removes the installation.
  --yes                 With --uninstall: uninstall without asking first.
  --force               With --uninstall: stop the daemon whatever work that interrupts.
  --keep-changes        With --uninstall: do not clear first what Octoboard's features have in effect outside
                        ~/.octoboard; it stays in place.
  -h, --help            Print this help.

Without --beta or --stable, a beta newer than the stable release is upgraded to the newest beta. Run it again to
upgrade.
USAGE
}

# Whether the manifests come from somewhere else than the release (a mirror or a test), the one case where a file://
# address is downloaded from.
local_files_allowed() { [ -n "${OCTOBOARD_MANIFEST_BASE:-}" ]; }

# Downloads $1 to the file $2, with curl or wget, over https only, redirects included (and from a file:// address
# where local_files_allowed). wget gets --https-only where it has it (GNU wget, not BusyBox's), which may keep it to
# https only for the links it follows itself, not for redirects; with wget, the checks of what it downloads are what
# hold.
fetch() {
  if command -v curl > /dev/null 2>&1; then
    if local_files_allowed; then protocols='=https,file'; else protocols='=https'; fi
    curl -fsSL --proto "$protocols" --proto-redir '=https' --tlsv1.2 -o "$2" "$1"
  elif command -v wget > /dev/null 2>&1; then
    if wget --help 2>&1 | grep -q -- --https-only; then
      wget -q --https-only -O "$2" "$1"
    else
      wget -q -O "$2" "$1"
    fi
  else
    fail "neither curl nor wget is installed."
  fi
}

# Only https addresses are downloaded from, whichever downloader runs, and file:// ones where local_files_allowed.
check_address() {
  case "$1" in
    https://*) ;;
    file://*) local_files_allowed || fail "$1 is not an https address; nothing was downloaded from it." ;;
    *) fail "$1 is not an https address; nothing was downloaded from it." ;;
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

# Prints -1, 0 or 1 as the version $1 is older than, the same as or newer than the version $2, as semantic versioning
# orders them (numbers as numbers, a prerelease before its release, build metadata ignored), as the daemon's own
# comparison does; nothing when either is not a version.
compare_versions() {
  awk -v a="$1" -v b="$2" '
    function valid(v) { return v ~ /^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/ }
    function identifiers(x, y,   xn, yn) {
      xn = (x ~ /^[0-9]+$/); yn = (y ~ /^[0-9]+$/)
      if (xn && yn) return (x + 0 < y + 0) ? -1 : (x + 0 > y + 0)
      if (xn) return -1
      if (yn) return 1
      return (x < y) ? -1 : (x > y)
    }
    function compare(a, b,   ap, bp, i, ac, bc, na, nb, r) {
      sub(/\+.*/, "", a); sub(/\+.*/, "", b)
      ap = ""; bp = ""
      i = index(a, "-"); if (i) { ap = substr(a, i + 1); a = substr(a, 1, i - 1) }
      i = index(b, "-"); if (i) { bp = substr(b, i + 1); b = substr(b, 1, i - 1) }
      split(a, ac, "."); split(b, bc, ".")
      for (i = 1; i <= 3; i++) { r = identifiers(ac[i] + 0 "", bc[i] + 0 ""); if (r) return r }
      if (ap == "" && bp == "") return 0
      if (ap == "") return 1
      if (bp == "") return -1
      na = split(ap, ac, "."); nb = split(bp, bc, ".")
      for (i = 1; i <= na && i <= nb; i++) { r = identifiers(ac[i], bc[i]); if (r) return r }
      return (na < nb) ? -1 : (na > nb)
    }
    BEGIN { if (valid(a) && valid(b)) print compare(a, b) }
  '
}

# Prints the version of the octoboardd at $1 (the first word of `octoboardd --version` that is one), nothing when
# there is none there or it is a build that does not say.
read_installed_version() {
  [ -f "$1" ] && [ -x "$1" ] || return 0
  found=$("$1" --version 2> /dev/null | awk 'NR == 1 { for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+\.[0-9]+\.[0-9]/) { print $i; exit } }') || found=""
  case "$found" in
    [0-9]*.*.*) printf '%s\n' "$found" ;;
  esac
}

# Checks the signature of the archive $1, which is saved under the name the release gave it, against the manifest's
# signature in the file $2 (the base64 of a minisign signature file): with minisign where it is installed, else with
# the installed octoboardd where it can, else not at all, with a warning (or, with --require-signature, a refusal).
# Either way the signature must name the archive's own file, so that an older release's archive, which the same key
# signed, is not taken for this one.
verify_signature() {
  archive_name=${1##*/}
  if command -v minisign > /dev/null 2>&1; then
    base64 -d < "$2" > "$tmp/signature.minisig" 2> /dev/null || fail "the signature in $manifest_url is not valid."
    verified=$(minisign -V -P "${OCTOBOARD_UPDATE_PUBKEY:-$update_pubkey}" -m "$1" -x "$tmp/signature.minisig") \
      || fail "the download's signature does not verify; nothing was installed."
    tab=$(printf '\t')
    trusted=$(printf '%s\n' "$verified" | sed -n 's/^Trusted comment: //p')
    case "$tab$trusted$tab" in
      *"${tab}file:$archive_name$tab"*) ;;
      *) fail "the signature is not of $archive_name; nothing was installed." ;;
    esac
    say "Signature verified."
  elif [ -z "${OCTOBOARD_UPDATE_PUBKEY:-}" ] && [ -f "$bin_dir/octoboardd" ] && [ -x "$bin_dir/octoboardd" ] \
    && "$bin_dir/octoboardd" verify-archive --help > /dev/null 2>&1; then
    "$bin_dir/octoboardd" verify-archive "$1" "$2" \
      || fail "the download's signature does not verify (checked by the installed octoboardd); nothing was installed."
    say "Signature verified (by the installed octoboardd)."
  elif [ "$require_signature" = true ]; then
    fail "the download's signature cannot be checked: minisign is not installed, and no installed octoboardd can check it; nothing was installed. Install minisign and run this again."
  else
    warn "the download's signature was not checked, only its SHA-256 from the release manifest: minisign is not installed. Install minisign and run this again to have it checked (--require-signature refuses to install without the check)."
  fi
}

# Reads the manifest of the channel $1 into $tmp/manifest.json, setting channel, manifest_url and version.
read_manifest() {
  channel=$1
  manifest_url="${manifest_base%/}/$channel-linux.json"
  check_address "$manifest_url"
  fetch "$manifest_url" "$tmp/manifest.json" || fail "could not read $manifest_url."
  version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$tmp/manifest.json" | head -n 1)
  case "$version" in
    "" | *[!0-9A-Za-z.+-]*) fail "$manifest_url names no version." ;;
  esac
}

main() {
  service=false
  port=""
  port_given=false
  channel=""
  allow_downgrade=false
  require_signature=false
  bin_dir="${HOME:?HOME is not set}/.local/bin"
  bin_dir_given=false
  uninstall=false
  # The options `octoboardd uninstall` takes, passed on to it as they were given.
  uninstall_options=""
  # The last option given that only an install takes, which --uninstall refuses: every option but the ones that go
  # with --uninstall, listed here and in its refusal, so that an option added later is an install's unless it is
  # added here.
  install_option=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --uninstall | --bin-dir | --bin-dir=* | --yes | --force | --keep-changes | --dry-run | -h | --help) ;;
      *) install_option=${1%%=*} ;;
    esac
    case "$1" in
      --uninstall) uninstall=true ;;
      # `octoboardd uninstall --dry-run` is the command's own; this script does nothing it could only show.
      --dry-run) fail "this script has no --dry-run. To see what uninstalling would do, run the installed octoboardd's own \`octoboardd uninstall --dry-run\`." ;;
      --yes | --force | --keep-changes) uninstall_options="$uninstall_options $1" ;;
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
      --stable) channel=stable ;;
      --allow-downgrade) allow_downgrade=true ;;
      --require-signature) require_signature=true ;;
      --bin-dir)
        [ $# -ge 2 ] || fail "--bin-dir needs a directory."
        bin_dir=$2
        bin_dir_given=true
        shift
        ;;
      --bin-dir=*)
        bin_dir=${1#--bin-dir=}
        bin_dir_given=true
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) fail "unknown option $1 (see --help)." ;;
    esac
    shift
  done
  if [ "$uninstall" = true ] && [ -n "$install_option" ]; then
    fail "--uninstall removes what is installed and takes no option of an install ($install_option); only --bin-dir, --yes, --force and --keep-changes go with it."
  fi
  if [ -n "$uninstall_options" ] && [ "$uninstall" = false ]; then
    fail "${uninstall_options# } goes with --uninstall."
  fi
  if [ "$port_given" = true ]; then
    [ "$service" = true ] || fail "--port is the port of the service, so it goes with --service."
    case "$port" in
      "" | *[!0-9]*) fail "--port $port is not a port number." ;;
    esac
    { [ "$port" -ge 1 ] && [ "$port" -le 65535 ]; } || fail "--port $port is not a port number."
  fi
  # Run as root (`sudo sh`) or with another user's home (sudo keeping HOME), the files would belong to someone else
  # than the user the daemon runs as, who could then neither update nor replace them.
  uid=$(id -u)
  if [ "$uid" = 0 ] && [ "$bin_dir_given" = false ]; then
    fail "this installs octoboardd for the user who runs it, and it is running as root. Run it as the user the daemon is to run as, without sudo (or pass --bin-dir to install for root on purpose)."
  fi
  home_owner=$(ls -ndL "$HOME" 2> /dev/null | awk '{ print $3 }')
  [ "$home_owner" = "$uid" ] \
    || fail "$HOME is not the home of the user running this (uid $uid), as under sudo. Run it as the user the daemon is to run as, without sudo."
  case "$bin_dir" in
    /*) ;;
    *) bin_dir="$(pwd)/$bin_dir" ;;
  esac
  while [ "${bin_dir%/}" != "$bin_dir" ] && [ "$bin_dir" != / ]; do bin_dir=${bin_dir%/}; done
  if [ "$uninstall" = true ]; then
    [ "$(uname -s)" = Linux ] || fail "octoboardd is installed this way on Linux only, so there is nothing of it to remove here; this is $(uname -s)."
    uninstall
    return 0
  fi
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
  staged=""
  trap 'rm -rf "$tmp"; [ -z "$staged" ] || rm -f "$staged"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  installed_version=$(read_installed_version "$bin_dir/octoboardd")
  manifest_base=${OCTOBOARD_MANIFEST_BASE:-$manifest_base_default}
  if [ -n "$channel" ]; then
    read_manifest "$channel"
  else
    read_manifest stable
    # A beta installed on its own channel, where the stable release is older than it: its newest beta is the upgrade,
    # and the stable release a downgrade. A prerelease the stable manifest itself announces (as it does until the first
    # stable release) is no reason to leave stable.
    case "${installed_version%%+*}" in
      *-*)
        if [ "$(compare_versions "$version" "$installed_version")" = -1 ]; then
          say "The installed octoboardd $installed_version is a beta newer than the stable release $version, so this installs the newest beta (--stable takes the stable release)."
          read_manifest beta
        fi
        ;;
    esac
  fi
  url=$(platform_field "linux-$arch" url "$tmp/manifest.json")
  sha256=$(platform_field "linux-$arch" sha256 "$tmp/manifest.json")
  signature=$(platform_field "linux-$arch" signature "$tmp/manifest.json")
  [ -n "$url" ] && [ -n "$sha256" ] || fail "$manifest_url has no linux-$arch build of $version."
  [ -n "$signature" ] || fail "$manifest_url has no signature for the linux-$arch build."
  if [ -n "$installed_version" ] && [ "$(compare_versions "$version" "$installed_version")" = -1 ] \
    && [ "$allow_downgrade" = false ]; then
    fail "octoboardd $installed_version is installed, which is newer than $version, the newest $channel release; nothing was installed. A daemon may not open what a newer one has stored; pass --allow-downgrade to install $version anyway."
  fi

  check_address "$url"
  say "Downloading octoboardd $version for $arch..."
  # Saved under the name the release gave it, which the signature names.
  archive="$tmp/octoboardd_${version}_linux_$arch.tar.gz"
  fetch "$url" "$archive" || fail "could not download $url."
  [ "$(sha256_of "$archive")" = "$sha256" ] || fail "the download does not match its SHA-256; nothing was installed."
  printf '%s' "$signature" > "$tmp/signature"
  verify_signature "$archive" "$tmp/signature"
  mkdir "$tmp/unpacked"
  tar -xzf "$archive" -C "$tmp/unpacked" \
    || fail "could not unpack the archive; nothing was installed. If tar says it cannot run gzip, install gzip and run this again."
  unpacked="$tmp/unpacked/octoboardd_${version}_linux_$arch"
  [ -f "$unpacked/octoboardd" ] || fail "the archive does not hold octoboardd."

  # A rename within the directory replaces the file in one step: nothing ever sees half a binary, and a running daemon
  # keeps the file it started from. The replaced file is kept as octoboardd.previous by a second link, as the daemon's
  # own update keeps it (a copy where the file system has no hard links), so that `octoboardd update --rollback` puts
  # back the build this replaced; a reinstall of the same build keeps the previous one there is. It is done under the
  # daemon's update lock, where flock is installed and the data directory exists, so that it does not run into the
  # daemon's own update.
  mkdir -p "$bin_dir"
  binary="$bin_dir/octoboardd"
  take_update_lock
  staged="$bin_dir/.octoboardd.install.$$"
  cp "$unpacked/octoboardd" "$staged"
  chmod 755 "$staged"
  if [ -f "$binary" ] && ! cmp -s "$binary" "$staged"; then
    if ! ln -f "$binary" "$bin_dir/octoboardd.previous" 2> /dev/null; then
      kept="$bin_dir/.octoboardd.previous.$$"
      { cp -p "$binary" "$kept" && mv -f "$kept" "$bin_dir/octoboardd.previous"; } || {
        rm -f "$kept"
        fail "could not keep $binary as $bin_dir/octoboardd.previous; nothing was installed."
      }
    fi
  fi
  mv -f "$staged" "$binary"
  staged=""
  say "Installed octoboardd $version to $binary."

  # The command the next steps are run with: the name where it finds this file on the PATH, else the full path.
  if [ "$(command -v octoboardd 2> /dev/null || true)" = "$binary" ]; then run_as=octoboardd; else run_as=$binary; fi

  running=false
  if "$binary" status > /dev/null 2>&1; then running=true; fi

  if [ "$service" = true ]; then
    install_service
  elif [ "$running" = true ]; then
    restart_if_safe "A daemon is running from the file this replaced. Run \`$run_as restart\` to move it to $version; its sessions keep running." || :
  else
    say "Start it with \`$run_as start\`, then run \`$run_as address\`: it prints the address to open and the SSH forward to open it through."
  fi

  case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) say "$bin_dir is not on your PATH; add it (for example in ~/.profile) to run octoboardd by name." ;;
  esac
}

# Restarts the daemon that runs into the build just installed, where that interrupts no session: `restart --if-safe`,
# which says otherwise what the restart would interrupt, naming no command to run next. The update lock is let go
# first: the daemon's next daemon falls back to the build that ran, where the new one does not start, only while no
# other process holds it. Where the restart was not made (declined as one that would interrupt work, status 4,
# refused, not come back, or not asked, a build without --if-safe), says $1, the advice to restart by hand, and
# returns 1.
restart_if_safe() {
  exec 9>&-
  if "$binary" restart --if-safe < /dev/null; then return 0; fi
  say "$1"
  return 1
}

# Takes the update lock the daemon and `octoboardd update` hold while they replace the executable
# (<data directory>/update.lock, flock(2)), until `restart_if_safe` lets it go before a restart, or the run ends.
# Without flock, or before the daemon has made its data directory (so that it has never run), it is not taken ("The install script" in
# docs/product/installing-the-server-form.md says not to run the two together then).
take_update_lock() {
  data_dir="$HOME/.octoboard"
  command -v flock > /dev/null 2>&1 && [ -d "$data_dir" ] || return 0
  # A redirection that fails ends the shell at `exec` (a special built-in) with the shell's own message, so the file is
  # tried first with `true`, which only fails.
  { true >> "$data_dir/update.lock"; } 2> /dev/null || fail "could not open $data_dir/update.lock to take the update lock; nothing was installed."
  exec 9>> "$data_dir/update.lock"
  flock -n 9 || fail "an update is under way already (in the running octoboardd or in \`octoboardd update\`); nothing was installed. Run this again once it has ended."
}

install_service() {
  unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  unit="$unit_dir/octoboardd.service"
  [ -f "$unpacked/octoboardd.service" ] || fail "the archive does not hold octoboardd.service."
  mkdir -p "$unit_dir"
  exec_start="ExecStart=$binary --service-managed"
  if [ "$port_given" = true ]; then exec_start="ExecStart=$binary --port $port --service-managed"; fi
  sed "s|^ExecStart=.*|$exec_start|" "$unpacked/octoboardd.service" > "$tmp/octoboardd.service"
  grep -qxF "$exec_start" "$tmp/octoboardd.service" || fail "the archive's octoboardd.service has no ExecStart line."
  changed=false
  if [ -f "$unit" ] && ! cmp -s "$tmp/octoboardd.service" "$unit"; then
    changed=true
    say "Replaced $unit with the unit of $version; any changes you made to it are gone."
  fi
  mv -f "$tmp/octoboardd.service" "$unit"
  systemctl --user daemon-reload || fail "systemctl --user could not reach your user's service manager (log in over SSH rather than with su, so that a user session exists)."
  systemctl --user enable octoboardd.service > /dev/null 2>&1 || fail "could not enable octoboardd.service."
  if systemctl --user is-active --quiet octoboardd.service; then
    if [ "$changed" = true ]; then
      restart_if_safe "The service is running from the file and the unit this replaced. Run \`$run_as restart\` to move it to $version and to the new unit; its sessions keep running." || :
    else
      restart_if_safe "The service is running from the file this replaced. Run \`$run_as restart\` to move it to $version; its sessions keep running." || :
    fi
  elif [ "$running" = true ]; then
    say "Installed and enabled the service, but did not start it: a daemon started outside it is running. Stop that one (\`$run_as stop\`, adding \`--force\` if it says it is running work) and run \`systemctl --user start octoboardd\`."
  else
    systemctl --user start octoboardd.service || fail "the service did not start; see \`journalctl --user -u octoboardd\`."
    say "Started the service. Run \`$run_as address\`: it prints the address to open and the SSH forward to open it through."
  fi
  # Lingering keeps the user's services running after logout and starts them at boot. polkit lets users turn it on for
  # themselves by default, so it is tried here (never asking for a password) and only advised when that is refused.
  user=$(id -un)
  if [ "$(loginctl show-user "$user" --property=Linger --value 2> /dev/null || true)" != yes ]; then
    if loginctl --no-ask-password enable-linger "$user" > /dev/null 2>&1; then
      record_lingering "$user"
      say "Turned lingering on for $user, so the service starts at boot and keeps running after you log out."
    else
      say "Your user services stop when you log out and do not start at boot until lingering is on. Turn it on once with \`loginctl enable-linger $user\` (it may need sudo)."
    fi
  fi
}

# Notes that this run turned lingering on for the user $1, in the daemon's data directory (made as the daemon makes it,
# for the user alone, where the daemon has not made it yet).
record_lingering() {
  data_dir="$HOME/.octoboard"
  if { [ -d "$data_dir" ] || (umask 077 && mkdir -p "$data_dir") 2> /dev/null; } \
    && { printf '%s\n' "$1" > "$data_dir/$linger_record"; } 2> /dev/null; then
    return 0
  fi
  warn "could not note in $data_dir/$linger_record that this turned lingering on, so --uninstall will leave it on."
}

# Uninstalls what an install into $bin_dir put on the system. The installed octoboardd does it where it knows how
# (`octoboardd uninstall`, asked for its help, which a build from before the command refuses): this hands over to it
# and ends with its exit status. Standard input is the terminal where there is one, so that the command can ask
# although this script was piped in; with none, the command needs --yes, and says so.
#
# The rest is the fallback for older builds, kept as it was: it removes what an install into $bin_dir put on the system, in the order that leaves nothing running from a file that is
# gone, and only once everything that could stop it half way has been looked at: the unit runs this executable, the
# files are the running user's and their directory can be written. Then a daemon that runs is stopped by its own
# command (where that does not stop it, nothing is removed), the unit is disabled and removed, lingering is turned off
# where the install's note says it turned it on and no other service of the user's is enabled, and the files in
# $bin_dir go last. The daemon's data directory is left, and named.
uninstall() {
  binary="$bin_dir/octoboardd"
  if [ -f "$binary" ] && [ -x "$binary" ] && "$binary" uninstall --help > /dev/null 2>&1; then
    # shellcheck disable=SC2086 # The options are words of this script's own, split on purpose.
    if [ ! -t 0 ] && [ -t 1 ] && (: < /dev/tty) 2> /dev/null; then
      exec "$binary" uninstall $uninstall_options < /dev/tty
    fi
    # shellcheck disable=SC2086
    exec "$binary" uninstall $uninstall_options
  fi
  # The fallback for older builds, from here to the end of the function.
  if [ -f "$binary" ] || [ -L "$binary" ]; then
    warn "this octoboardd is from before \`octoboardd uninstall\`, so this script removes it itself, and nothing is cleared first. An older build may have left an account sharing its agent's default configuration: entries of that account's directory are then links into the default account's directory (~/.claude, say), which can be deleted by hand. --yes, --force and --keep-changes have no effect here."
  fi
  unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  unit="$unit_dir/octoboardd.service"
  data_dir="$HOME/.octoboard"
  has_unit=false
  if [ -f "$unit" ] || [ -L "$unit" ]; then has_unit=true; fi
  has_files=false
  for file in "$binary" "$bin_dir/octoboardd.previous" "$bin_dir"/.octoboardd.*; do
    if [ -f "$file" ] || [ -L "$file" ]; then has_files=true; fi
  done
  if [ "$has_unit" = false ] && [ "$has_files" = false ] && [ ! -f "$data_dir/$linger_record" ]; then
    say "octoboardd is not installed in $bin_dir and has no service; nothing was removed. If it was installed with --bin-dir, pass the same --bin-dir."
    say_data_left
    return 0
  fi
  # The unit this script writes runs the installed file by its full path; one copied by hand from the archive runs
  # it by %h, the user's home. A unit that runs another file is the unit of an install into another directory, or one
  # the user wrote, and neither is this run's to remove.
  if [ "$has_unit" = true ]; then
    unit_program=$(sed -n 's/^ExecStart=//p' "$unit" 2> /dev/null | head -n 1)
    unit_program=${unit_program%% *}
    case "$unit_program" in
      %h/*) unit_program="$HOME/${unit_program#%h/}" ;;
    esac
    [ "$unit_program" = "$binary" ] \
      || fail "$unit runs ${unit_program:-nothing this script can read}, not $binary; nothing was removed. If octoboardd was installed into another directory, pass that directory with --bin-dir. If the unit is one you wrote yourself, it is not this script's to remove: remove it yourself, or run this with the directory its ExecStart names."
  fi
  # Run as another user than the one who installed (under sudo with --bin-dir), this would remove that user's files
  # and leave their unit and their daemon, which are looked for under this user's home.
  for file in "$binary" "$bin_dir/octoboardd.previous" "$bin_dir"/.octoboardd.*; do
    if [ -f "$file" ] || [ -L "$file" ]; then
      [ "$(ls -nd "$file" 2> /dev/null | awk '{ print $3 }')" = "$uid" ] \
        || fail "$file does not belong to the user running this (uid $uid); nothing was removed. Run this as the user who installed octoboardd, without sudo."
    fi
  done
  if [ "$has_files" = true ] && [ ! -w "$bin_dir" ]; then
    fail "$bin_dir cannot be written by the user running this, so the files in it could not be removed; nothing was removed."
  fi

  if [ -f "$binary" ] && [ -x "$binary" ] && "$binary" status > /dev/null 2>&1; then
    "$binary" stop < /dev/null \
      || fail "\`$binary stop\` did not stop the daemon (see what it said above); nothing was removed. Where it named work that stopping would interrupt, \`$binary stop --force\` stops it all the same and ends its sessions as interrupted; then run this again."
    say "Stopped the daemon."
  fi

  if [ "$has_unit" = true ]; then
    if command -v systemctl > /dev/null 2>&1 && systemctl --user disable --now octoboardd.service > /dev/null 2>&1; then
      rm -f "$unit"
      systemctl --user daemon-reload > /dev/null 2>&1 || :
      systemctl --user reset-failed octoboardd.service > /dev/null 2>&1 || :
      say "Stopped and disabled the service, and removed $unit."
    else
      # No service manager to ask (no user session, or no systemd any more): what enabling made is a link among the
      # units the default target wants, removed here with the unit itself.
      rm -f "$unit" "$unit_dir/default.target.wants/octoboardd.service"
      warn "systemctl --user could not reach your user's service manager, so $unit and the link that enabled it were removed without it. If the service still runs, it stops when you log out, or with \`systemctl --user stop octoboardd\`."
    fi
  fi

  if [ -f "$data_dir/$linger_record" ]; then
    user=$(id -un)
    # The note says the install turned lingering on, not that nothing else needs it now: a service the user enabled
    # since would stop starting at boot without it. Enabled is a link among the units a target wants, requires or
    # upholds, in the user's unit directory or, for a unit a generator makes (a podman quadlet), in the generators'
    # directories of the user's runtime directory; a unit's name may begin with a dot. `systemctl list-unit-files`
    # is not asked: it also names what the distribution enables for every user, which would keep lingering on for all.
    other_units=""
    generators="${XDG_RUNTIME_DIR:-/run/user/$uid}/systemd"
    for dir in "$unit_dir" "$generators/generator" "$generators/generator.early" "$generators/generator.late"; do
      for kind in wants requires upholds; do
        for link in "$dir"/*."$kind"/* "$dir"/*."$kind"/.[!.]*; do
          if [ -e "$link" ] || [ -L "$link" ]; then
            [ "${link##*/}" = octoboardd.service ] || other_units="$other_units ${link##*/}"
          fi
        done
      done
    done
    if [ -n "$other_units" ]; then
      say "The install turned lingering on for $user. It was left on, since other services of yours are enabled and may need it:$other_units. To turn it off all the same: \`loginctl disable-linger $user\` (it may need sudo)."
    elif command -v loginctl > /dev/null 2>&1 && loginctl --no-ask-password disable-linger "$user" > /dev/null 2>&1; then
      rm -f "$data_dir/$linger_record"
      say "Turned lingering off for $user, which the install had turned on."
    else
      say "The install turned lingering on for $user, and it could not be turned off here. Turn it off with \`loginctl disable-linger $user\` (it may need sudo)."
    fi
  fi

  for file in "$binary" "$bin_dir/octoboardd.previous" "$bin_dir"/.octoboardd.*; do
    if [ -f "$file" ] || [ -L "$file" ]; then
      rm -f "$file" || fail "could not remove $file."
    fi
  done
  if [ "$has_files" = true ]; then
    say "Removed octoboardd from $bin_dir."
  fi
  say_data_left
}

# Says that the daemon's data directory stays, and how to remove it, where there is one.
say_data_left() {
  data_dir="$HOME/.octoboard"
  [ -d "$data_dir" ] || return 0
  say "$data_dir, which holds the daemon's own data (your consoles, session records, settings and logs), was left as it is. To remove it as well: rm -rf $data_dir"
}

main "$@"
