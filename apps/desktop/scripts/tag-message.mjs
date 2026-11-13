#!/usr/bin/env node
// Reads a release tag's message from standard input and prints one part of it, for `.github/release-notes.sh`:
// `notes` (the message without its `Platforms:` line) or `platforms` (the selection, such as `macos,linux`).
import { readFileSync } from "node:fs";
import { readTagMessage } from "./release-manifest.mjs";

const part = process.argv[2];
if (process.argv.length !== 3 || !["notes", "platforms"].includes(part)) {
  throw new Error("Usage: tag-message.mjs notes|platforms < message");
}
const { notes, platforms } = readTagMessage(readFileSync(0, "utf8"));
console.log(part === "notes" ? notes : platforms.join(","));
