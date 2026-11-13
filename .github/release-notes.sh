#!/bin/sh
# Prints the release notes of the version tag $1, the message of the annotated tag without its signature and its
# `Platforms:` line, after checking that the tag names the application's version, the one in apps/desktop/package.json.
# With `--platforms` as $2 it prints the platforms the tag selects instead (`macos,windows,linux` unless the message ends
# with a `Platforms:` line; see apps/desktop/scripts/README.md's "Releasing by tag").
set -eu
tag=$1
part=${2:-}

# A checkout may hold the tag as a lightweight ref; the annotated tag object carries the message.
git fetch --quiet --force origin "refs/tags/$tag:refs/tags/$tag"
version=$(node -p 'require("./apps/desktop/package.json").version')
if [ "$tag" != "desktop-v$version" ]; then
  echo "::error::The tag $tag does not match the version $version in apps/desktop/package.json. Tag desktop-v$version instead." >&2
  exit 1
fi
if [ "$(git cat-file -t "refs/tags/$tag")" != "tag" ]; then
  echo "::error::$tag is a lightweight tag. Tag with \`git tag -a $tag\`, whose message is the release notes." >&2
  exit 1
fi
# `%(contents)` keeps the message's lines as they are (`%(contents:subject)` would join the first paragraph into one
# line) and ends with the signature of a signed tag, which is cut off.
message=$(git tag -l --format='%(contents)' "$tag" | sed '/^-----BEGIN [A-Z ]*SIGNATURE-----$/,$d')
notes=$(printf '%s\n' "$message" | node apps/desktop/scripts/tag-message.mjs notes)
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "::error::The message of $tag is empty, and it is the release notes." >&2
  exit 1
fi
if [ "$part" = "--platforms" ]; then
  printf '%s\n' "$message" | node apps/desktop/scripts/tag-message.mjs platforms
else
  printf '%s\n' "$notes"
fi
