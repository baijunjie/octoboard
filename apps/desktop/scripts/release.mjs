#!/usr/bin/env node
// `pnpm release`: the distributable build of the host platform, which the one command line runs everywhere.
// `release-macos.mjs` builds the universal, signed and notarized macOS bundle and `release-windows.mjs` the x64
// Windows installer; each reads the same `--no-sign-update` flag, and the Windows one a few of its own.
const script = { darwin: "./release-macos.mjs", win32: "./release-windows.mjs" }[process.platform];
if (!script) throw new Error(`pnpm release builds on macOS and Windows; this is ${process.platform}.`);
await import(script);
