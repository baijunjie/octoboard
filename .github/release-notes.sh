#!/bin/sh
# Prints the release notes of the version tag $1, the message of the annotated tag without its signature, after
# checking that the tag names the version in tauri.conf.json.
set -eu
tag=$1

# A checkout may hold the tag as a lightweight ref; the annotated tag object carries the message.
git fetch --quiet --force origin "refs/tags/$tag:refs/tags/$tag"
version=$(node -p 'require("./apps/desktop/src-tauri/tauri.conf.json").version')
if [ "$tag" != "v$version" ]; then
  echo "::error::The tag $tag does not match the version $version in tauri.conf.json. Tag v$version instead." >&2
  exit 1
fi
if [ "$(git cat-file -t "refs/tags/$tag")" != "tag" ]; then
  echo "::error::$tag is a lightweight tag. Tag with \`git tag -a $tag\`, whose message is the release notes." >&2
  exit 1
fi
# `%(contents)` keeps the message's lines as they are (`%(contents:subject)` would join the first paragraph into one
# line) and ends with the signature of a signed tag, which is cut off.
notes=$(git tag -l --format='%(contents)' "$tag" | sed '/^-----BEGIN [A-Z ]*SIGNATURE-----$/,$d')
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "::error::The message of $tag is empty, and it is the release notes." >&2
  exit 1
fi
printf '%s\n' "$notes"
