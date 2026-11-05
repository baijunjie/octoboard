#!/usr/bin/env node
// What `tauri build` calls for each file it signs in `WINDOWS_SIGNING=command` mode (`bundle.windows.signCommand`, set
// by `release-windows.mjs` through `--config`): `node sign-file.mjs <file>`. It runs the command line in
// `WINDOWS_SIGN_COMMAND` on the file, except for Microsoft's ConPTY files, which keep Microsoft's signature.
import { execFileSync } from "node:child_process";
import { signInvocation } from "./windows-release.mjs";

const [file, ...extra] = process.argv.slice(2);
if (!file || extra.length > 0) throw new Error("Usage: node sign-file.mjs <file>");
const invocation = signInvocation(process.env.WINDOWS_SIGN_COMMAND ?? "", file);
if (invocation === null) {
  console.log(`not signing ${file}: Microsoft's file keeps Microsoft's signature.`);
} else {
  console.log(`signing ${file}`);
  execFileSync(invocation.cmd, invocation.args, { stdio: "inherit" });
}
