import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  assertStaticBinary,
  builtLinuxArchives,
  elfLinkage,
  linuxArchOf,
  linuxReleaseFiles,
  linuxTarget,
  parseSha256Line,
  sha256Line,
  sha256Of,
} from "./linux-release.mjs";
import { linuxArchiveName } from "./release-manifest.mjs";

function temporaryDirectory(run) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "linux-release-"));
  try {
    return run(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const EM_X86_64 = 62;
const EM_AARCH64 = 183;
const PT_LOAD = 1;
const PT_INTERP = 3;

// A synthetic 64-bit ELF header followed by program headers of the given types; the rest of the file is not needed.
function elf({ machine = EM_X86_64, types = [PT_LOAD], bits = 2, endian = 1, phoff = 64, truncate = 0 } = {}) {
  const phentsize = 56;
  const buffer = Buffer.alloc(phoff + phentsize * types.length);
  buffer.writeUInt32BE(0x7f454c46, 0);
  buffer[4] = bits;
  buffer[5] = endian;
  buffer[6] = 1;
  buffer.writeUInt16LE(2, 16);
  buffer.writeUInt16LE(machine, 18);
  buffer.writeBigUInt64LE(BigInt(phoff), 32);
  buffer.writeUInt16LE(64, 52);
  buffer.writeUInt16LE(phentsize, 54);
  buffer.writeUInt16LE(types.length, 56);
  types.forEach((type, i) => buffer.writeUInt32LE(type, phoff + i * phentsize));
  return truncate > 0 ? buffer.subarray(0, buffer.length - truncate) : buffer;
}

test("each release architecture builds for its musl target, and no other does", () => {
  assert.equal(linuxTarget("x86_64"), "x86_64-unknown-linux-musl");
  assert.equal(linuxTarget("aarch64"), "aarch64-unknown-linux-musl");
  for (const arch of ["arm64", "x64", "armv7", "", undefined]) assert.throws(() => linuxTarget(arch), /not a Linux release architecture/, String(arch));
});

test("a Node architecture maps to its release architecture, and one without a release to nothing", () => {
  assert.equal(linuxArchOf("x64"), "x86_64");
  assert.equal(linuxArchOf("arm64"), "aarch64");
  for (const arch of ["arm", "ia32", "riscv64", "x86_64", undefined]) assert.equal(linuxArchOf(arch), undefined, String(arch));
});

test("an ELF file's machine and whether it names a program interpreter are read from its headers", () => {
  assert.deepEqual(elfLinkage(elf({ machine: EM_X86_64, types: [PT_LOAD, PT_LOAD] })), { machine: "x86_64", interpreter: false });
  assert.deepEqual(elfLinkage(elf({ machine: EM_AARCH64, types: [PT_LOAD] })), { machine: "aarch64", interpreter: false });
  assert.deepEqual(elfLinkage(elf({ machine: EM_X86_64, types: [PT_LOAD, PT_INTERP, PT_LOAD] })), { machine: "x86_64", interpreter: true });
  assert.deepEqual(elfLinkage(elf({ machine: EM_AARCH64, types: [PT_INTERP] })), { machine: "aarch64", interpreter: true });
  assert.deepEqual(elfLinkage(elf({ machine: 40, types: [] })), { machine: "40", interpreter: false });
});

test("a file that is not a 64-bit little-endian ELF, or whose program headers run past its end, is refused", () => {
  assert.throws(() => elfLinkage(Buffer.from("#!/bin/sh\necho hi\n")), /not an ELF file/);
  assert.throws(() => elfLinkage(Buffer.alloc(0)), /not an ELF file/);
  assert.throws(() => elfLinkage(elf().subarray(0, 32)), /not an ELF file/);
  assert.throws(() => elfLinkage(elf({ bits: 1 })), /not a 64-bit little-endian ELF file/);
  assert.throws(() => elfLinkage(elf({ endian: 2 })), /not a 64-bit little-endian ELF file/);
  assert.throws(() => elfLinkage(elf({ types: [PT_LOAD, PT_LOAD], truncate: 56 })), /program headers run past the end/);
  assert.throws(() => elfLinkage(elf({ phoff: 1000, truncate: 500 })), /program headers run past the end/);
});

test("a binary is accepted only when it is static and built for the architecture", () => {
  temporaryDirectory((dir) => {
    const write = (name, buffer) => {
      const file = path.join(dir, name);
      writeFileSync(file, buffer);
      return file;
    };
    const x86Static = write("x86-static", elf({ machine: EM_X86_64 }));
    const armStatic = write("arm-static", elf({ machine: EM_AARCH64 }));
    const x86Dynamic = write("x86-dynamic", elf({ machine: EM_X86_64, types: [PT_LOAD, PT_INTERP] }));
    assert.doesNotThrow(() => assertStaticBinary(x86Static, "x86_64"));
    assert.doesNotThrow(() => assertStaticBinary(armStatic, "aarch64"));
    assert.throws(() => assertStaticBinary(armStatic, "x86_64"), /built for aarch64, not x86_64/);
    assert.throws(() => assertStaticBinary(x86Static, "aarch64"), /built for x86_64, not aarch64/);
    assert.throws(() => assertStaticBinary(x86Dynamic, "x86_64"), /dynamically linked/);
    assert.throws(() => assertStaticBinary(write("script", "#!/bin/sh\n"), "x86_64"), /not an ELF file/);
  });
});

test("a file's SHA-256 is its content's digest in lowercase hex", () => {
  temporaryDirectory((dir) => {
    const file = path.join(dir, "a.bin");
    writeFileSync(file, "hello");
    assert.equal(sha256Of(file), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
  });
});

test("the .sha256 line is what sha256sum prints: the digest, two spaces and the name, ending in a newline", () => {
  const digest = "a".repeat(64);
  assert.equal(sha256Line(digest, "octoboardd_0.2.0_linux_x86_64.tar.gz"), `${digest}  octoboardd_0.2.0_linux_x86_64.tar.gz\n`);
});

test("a .sha256 line gives its digest for the file it names, and nothing else does", () => {
  const digest = "0123456789abcdef".repeat(4);
  const name = "octoboardd_0.2.0_linux_x86_64.tar.gz";
  assert.equal(parseSha256Line(sha256Line(digest, name), name), digest);
  assert.equal(parseSha256Line(`${digest}  ${name}`, name), digest);
  assert.equal(parseSha256Line(`${digest} *${name}\n`, name), digest);
  assert.throws(() => parseSha256Line(sha256Line(digest, "other.tar.gz"), name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line(sha256Line(digest, `sub/${name}`), name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line(`${digest.slice(1)}  ${name}\n`, name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line(`${digest.toUpperCase()}  ${name}\n`, name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line(`${digest}${name}\n`, name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line(`${digest}  ${name}\n${digest}  ${name}\n`, name), /not a SHA-256 line/);
  assert.throws(() => parseSha256Line("", name), /not a SHA-256 line/);
});

test("the archives built for a version are the ones present, and none when the directory is missing", () => {
  temporaryDirectory((dir) => {
    assert.deepEqual(builtLinuxArchives(path.join(dir, "missing"), "0.2.0"), []);
    assert.deepEqual(builtLinuxArchives(dir, "0.2.0"), []);
    writeFileSync(path.join(dir, linuxArchiveName("0.2.0", "aarch64")), "a");
    writeFileSync(path.join(dir, `${linuxArchiveName("0.2.0", "aarch64")}.sha256`), "x");
    writeFileSync(path.join(dir, linuxArchiveName("0.1.0", "x86_64")), "old");
    assert.deepEqual(builtLinuxArchives(dir, "0.2.0"), [{ arch: "aarch64", archive: path.join(dir, linuxArchiveName("0.2.0", "aarch64")) }]);
    writeFileSync(path.join(dir, linuxArchiveName("0.2.0", "x86_64")), "x");
    assert.deepEqual(
      builtLinuxArchives(dir, "0.2.0").map((built) => built.arch),
      ["x86_64", "aarch64"],
    );
  });
});

// Writes both architectures' archive, .sha256 and .sig for `version`; `options` tell what to spoil.
function writeRelease(dir, version, options = {}) {
  const made = {};
  for (const arch of ["x86_64", "aarch64"]) {
    const name = linuxArchiveName(version, arch);
    const archive = path.join(dir, name);
    const content = `archive ${arch}`;
    writeFileSync(archive, content);
    const digest = createHash("sha256").update(content).digest("hex");
    writeFileSync(`${archive}.sha256`, options.sha256?.[arch] ?? `${digest}  ${name}\n`);
    writeFileSync(`${archive}.sig`, `signature ${arch}`);
    made[arch] = { archive, digest };
  }
  return made;
}

test("a Linux publish takes both architectures' archive, .sha256 and .sig, with the archive's digest", () => {
  temporaryDirectory((dir) => {
    const made = writeRelease(dir, "0.2.0");
    const files = linuxReleaseFiles(dir, "0.2.0");
    assert.deepEqual(
      files.map((file) => file.arch),
      ["x86_64", "aarch64"],
    );
    for (const file of files) {
      assert.deepEqual(file, {
        arch: file.arch,
        archive: made[file.arch].archive,
        sha256File: `${made[file.arch].archive}.sha256`,
        signature: `${made[file.arch].archive}.sig`,
        sha256: made[file.arch].digest,
      });
    }
  });
});

test("a Linux publish is refused when a file is missing, naming it", () => {
  for (const [arch, extension] of [
    ["x86_64", ""],
    ["aarch64", ".sha256"],
    ["x86_64", ".sig"],
  ]) {
    temporaryDirectory((dir) => {
      writeRelease(dir, "0.2.0");
      const missing = path.join(dir, `${linuxArchiveName("0.2.0", arch)}${extension}`);
      rmSync(missing);
      assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), (error) => error.message.includes(missing) && /does not exist/.test(error.message), `${arch}${extension}`);
    });
  }
  temporaryDirectory((dir) => {
    writeRelease(dir, "0.1.0");
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), /does not exist/);
  });
});

test("a Linux publish is refused when the .sha256 is not valid or does not match its archive", () => {
  temporaryDirectory((dir) => {
    writeRelease(dir, "0.2.0", { sha256: { aarch64: "not a digest line\n" } });
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), /octoboardd_0.2.0_linux_aarch64.tar.gz.sha256 is not valid/);
  });
  temporaryDirectory((dir) => {
    const name = linuxArchiveName("0.2.0", "x86_64");
    writeRelease(dir, "0.2.0", { sha256: { x86_64: sha256Line("f".repeat(64), "someone-else.tar.gz") } });
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), new RegExp(`${name}.sha256 is not valid`));
  });
  temporaryDirectory((dir) => {
    const name = linuxArchiveName("0.2.0", "x86_64");
    writeRelease(dir, "0.2.0", { sha256: { x86_64: sha256Line("f".repeat(64), name) } });
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), new RegExp(`${name}.sha256 does not match ${name}`));
  });
});

test("a Linux publish is refused when a .sig is older than its archive by more than five seconds, and takes one within that", () => {
  temporaryDirectory((dir) => {
    const made = writeRelease(dir, "0.2.0");
    const archiveTime = new Date("2026-11-01T12:00:00Z");
    for (const arch of ["x86_64", "aarch64"]) utimesSync(made[arch].archive, archiveTime, archiveTime);
    const signTime = (arch, secondsAfterArchive) => {
      const time = new Date(archiveTime.getTime() + secondsAfterArchive * 1000);
      utimesSync(`${made[arch].archive}.sig`, time, time);
    };
    signTime("x86_64", 60);
    signTime("aarch64", -4);
    assert.equal(linuxReleaseFiles(dir, "0.2.0").length, 2);
    signTime("aarch64", -60);
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), (error) => error.message.includes("aarch64.tar.gz.sig is older than") && /Sign the update again/.test(error.message));
    signTime("aarch64", 0);
    signTime("x86_64", -3600);
    assert.throws(() => linuxReleaseFiles(dir, "0.2.0"), /x86_64.tar.gz.sig is older than/);
  });
});
