#!/usr/bin/env node
// `pnpm release`: the distributable build of the host platform, which the one command line runs everywhere.
// `release-macos.mjs` builds the universal, signed and notarized macOS bundle, `release-windows.mjs` the x64
// Windows installer and `release-linux.mjs` the daemon's archive for the Linux host's architecture; each reads the same
// `--no-sign-update` flag, and the Windows and Linux ones a few of their own.
const script = { darwin: "./release-macos.mjs", win32: "./release-windows.mjs", linux: "./release-linux.mjs" }[process.platform];
if (!script) throw new Error(`pnpm release builds on macOS, Windows and Linux; this is ${process.platform}.`);
await import(script);
