import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  checkStage,
  conptyMismatches,
  filesEndingWith,
  isMicrosoftFile,
  removeInstallers,
  signConfigArgs,
  signerOf,
  signingMode,
  signInvocation,
  splitCommandLine,
  windowsReleaseFiles,
} from "./windows-release.mjs";

function temporaryDirectory(run) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "windows-release-"));
  try {
    return run(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("the signing mode is none unless set, and an unknown one or a command mode without a command is refused", () => {
  assert.equal(signingMode({}), "none");
  assert.equal(signingMode({ WINDOWS_SIGNING: "" }), "none");
  assert.equal(signingMode({ WINDOWS_SIGNING: "signpath" }), "signpath");
  assert.equal(signingMode({ WINDOWS_SIGNING: "command", WINDOWS_SIGN_COMMAND: "sign %1" }), "command");
  assert.throws(() => signingMode({ WINDOWS_SIGNING: "pfx" }), /one of none, command, signpath/);
  assert.throws(() => signingMode({ WINDOWS_SIGNING: "command" }), /WINDOWS_SIGN_COMMAND/);
  assert.throws(() => signingMode({ WINDOWS_SIGNING: "command", WINDOWS_SIGN_COMMAND: "  " }), /WINDOWS_SIGN_COMMAND/);
});

test("stages belong to the signpath mode, which needs one, and its later stages the signed files", () => {
  assert.equal(checkStage("none", undefined, undefined), null);
  assert.equal(checkStage("command", undefined, undefined), null);
  assert.throws(() => checkStage("none", "build", undefined), /belong to WINDOWS_SIGNING=signpath/);
  assert.throws(() => checkStage("signpath", undefined, undefined), /needs --stage/);
  assert.throws(() => checkStage("signpath", "sign", undefined), /needs --stage/);
  assert.equal(checkStage("signpath", "build", undefined), "build");
  assert.throws(() => checkStage("signpath", "build", "dir"), /takes no --signed/);
  assert.throws(() => checkStage("signpath", "bundle", undefined), /needs --signed/);
  assert.equal(checkStage("signpath", "finish", "dir"), "finish");
});

test("only the command mode adds a sign command to the build, and not the command itself", () => {
  assert.deepEqual(signConfigArgs("none", "sign-file.mjs"), []);
  assert.deepEqual(signConfigArgs("signpath", "sign-file.mjs"), []);
  const [flag, json] = signConfigArgs("command", "C:\\scripts\\sign-file.mjs");
  assert.equal(flag, "--config");
  assert.deepEqual(JSON.parse(json), {
    bundle: { windows: { signCommand: { cmd: process.execPath, args: ["C:\\scripts\\sign-file.mjs", "%1"] } } },
  });
});

test("a command line splits at whitespace, a quoted part staying whole", () => {
  assert.deepEqual(splitCommandLine('sign --name "My App" /f  %1'), ["sign", "--name", "My App", "/f", "%1"]);
  assert.deepEqual(splitCommandLine('tool --opt="a b" ""'), ["tool", "--opt=a b", ""]);
  assert.deepEqual(splitCommandLine("   "), []);
  assert.throws(() => splitCommandLine('tool "open'), /unclosed/);
});

test("the sign command is run with the file's path in place of %1, and Microsoft's files are left alone", () => {
  const file = "C:\\build\\Octoboard.exe";
  assert.deepEqual(signInvocation('signer --file %1 --description "Octoboard app"', file), {
    cmd: "signer",
    args: ["--file", file, "--description", "Octoboard app"],
  });
  assert.deepEqual(signInvocation("signer /f=%1", file).args, [`/f=${file}`]);
  assert.equal(signInvocation("signer %1", "C:\\x\\conpty.dll"), null);
  assert.equal(signInvocation("signer %1", "C:\\x\\x64\\OpenConsole.exe"), null);
  assert.throws(() => signInvocation("signer", file), /needs %1/);
  assert.throws(() => signInvocation("", file), /needs %1/);
  assert.equal(isMicrosoftFile("C:\\x\\OPENCONSOLE.EXE"), true);
  assert.equal(isMicrosoftFile("C:\\x\\octoboardd.exe"), false);
});

test("the signer is the first certificate of the chain signtool lists", () => {
  const report = [
    "Verifying: Octoboard.exe",
    "Signature Index: 0 (Primary Signature)",
    "Signing Certificate Chain:",
    "    Issued to: Root CA",
    "        Issued to: Octoboard Foundation",
  ].join("\n");
  assert.equal(signerOf(report), "Root CA");
  assert.equal(signerOf("SignTool Error: No signature found."), null);
});

test("files are found by the end of their path whatever the case and separators", () => {
  const listing = ["C:\\t\\$INSTDIR\\Conpty.dll", "C:\\t\\$INSTDIR\\x64\\OpenConsole.exe", "C:\\t\\$INSTDIR\\arm64\\OpenConsole.exe"];
  assert.deepEqual(filesEndingWith(listing, "x64/OpenConsole.exe"), [listing[1]]);
  assert.deepEqual(filesEndingWith(listing, "conpty.dll"), [listing[0]]);
  assert.deepEqual(filesEndingWith(listing, "Octoboard.exe"), []);
});

test("ConPTY files that differ from the package's, or are missing, are reported", () =>
  temporaryDirectory((dir) => {
    const files = { "conpty.dll": "dll", "x64/OpenConsole.exe": "x64", "arm64/OpenConsole.exe": "arm" };
    for (const root of ["fetched", "extracted"]) {
      for (const [name, content] of Object.entries(files)) {
        mkdirSync(path.dirname(path.join(dir, root, name)), { recursive: true });
        writeFileSync(path.join(dir, root, name), content);
      }
    }
    const listing = () => Object.keys(files).map((name) => path.join(dir, "extracted", name));
    assert.deepEqual(conptyMismatches(listing(), path.join(dir, "fetched")), []);
    writeFileSync(path.join(dir, "extracted", "x64", "OpenConsole.exe"), "signed by us");
    assert.deepEqual(conptyMismatches(listing(), path.join(dir, "fetched")), ["x64/OpenConsole.exe"]);
    assert.deepEqual(conptyMismatches(listing().slice(0, 2), path.join(dir, "fetched")), ["x64/OpenConsole.exe", "arm64/OpenConsole.exe"]);
  }));

test("earlier installers and their signatures are removed, other files are not", () =>
  temporaryDirectory((dir) => {
    for (const name of ["a_1.0.0_x64-setup.exe", "a_1.0.0_x64-setup.exe.sig", "notes.txt"]) writeFileSync(path.join(dir, name), "x");
    removeInstallers(dir);
    assert.deepEqual(readdirSync(dir), ["notes.txt"]);
    removeInstallers(path.join(dir, "missing"));
  }));

test("the Windows build's files are the installer named for the version and a signature not older than it", () =>
  temporaryDirectory((dir) => {
    const installer = path.join(dir, "Octoboard_0.2.0_x64-setup.exe");
    assert.throws(() => windowsReleaseFiles(dir, "Octoboard", "0.2.0"), /does not exist/);
    writeFileSync(installer, "setup");
    assert.throws(() => windowsReleaseFiles(dir, "Octoboard", "0.2.0"), /\.sig does not exist/);
    writeFileSync(`${installer}.sig`, "sig");
    assert.deepEqual(windowsReleaseFiles(dir, "Octoboard", "0.2.0"), { installer, signature: `${installer}.sig` });
    assert.throws(() => windowsReleaseFiles(dir, "Octoboard", "0.3.0"), /does not exist/);
    const now = Date.now() / 1000;
    utimesSync(installer, now, now);
    utimesSync(`${installer}.sig`, now - 3, now - 3);
    assert.doesNotThrow(() => windowsReleaseFiles(dir, "Octoboard", "0.2.0"));
    utimesSync(`${installer}.sig`, now - 60, now - 60);
    assert.throws(() => windowsReleaseFiles(dir, "Octoboard", "0.2.0"), /older than/);
  }));
