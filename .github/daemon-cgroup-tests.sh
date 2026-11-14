#!/bin/sh
# Runs the daemon's test suite with a cgroup v2 hierarchy it may write in and OCTOBOARD_REQUIRE_CGROUP=1, so that the
# tests that take a cgroup from apps/daemon/src/test_support/cgroups.rs run instead of returning early: a plain
# runner's cgroup is not the test user's, and there they would pass without running.
#
# It runs as root in a container started with `--privileged --cgroupns=private --init`, in the checkout, and runs the
# suite as a normal user (some tests fail as root) whose uid, `TEST_UID`, owns the checkout. Every process of the
# namespace's root cgroup moves to a group of its own, a group owned by that user takes this shell, and the suite
# starts from it, so the groups the tests make land below a cgroup the user owns. Afterwards no group may be left
# there: one would be a launch a test never ended.
set -eu

cg=/sys/fs/cgroup
uid=${TEST_UID:?TEST_UID names the uid that owns the checkout}

# A mountinfo line is: ID, parent ID, major:minor, root, mount point, options, any number of optional fields
# (none in a container whose mounts are private), the separator `-`, then the filesystem type.
if ! grep -Eq "^([^ ]+ ){4}$cg [^ ]+( [^ ]+)* - cgroup2 " /proc/self/mountinfo; then
  echo "::error::$cg is not a cgroup v2 mount in this container."
  exit 1
fi
# A privileged container may still get the hierarchy read-only.
mount -o remount,rw "$cg"
mkdir -p "$cg/init" "$cg/tests"
while read -r pid; do
  echo "$pid" > "$cg/init/cgroup.procs" 2> /dev/null || true
done < "$cg/cgroup.procs"

useradd --create-home --uid "$uid" tester
chown -R tester "$cg/tests"
echo $$ > "$cg/tests/cgroup.procs"

# The toolchain `rust-toolchain.toml` pins, installed by root into the image's shared RUSTUP_HOME, which the test user
# only reads. The test user's cargo home is its own.
rustup toolchain install
umask 022
env HOME=/home/tester CARGO_HOME=/home/tester/.cargo OCTOBOARD_REQUIRE_CGROUP=1 \
  setpriv --reuid=tester --regid="$(id -g tester)" --init-groups \
  cargo test --locked -p octoboardd

left=$(find "$cg/tests" -mindepth 1 -type d)
if [ -n "$left" ]; then
  echo "::error::The tests left these cgroups behind:"
  echo "$left"
  exit 1
fi
