// The Linux release's pieces, which `release-linux.mjs` builds with, `sign-update.mjs` signs and `publish-release.mjs`
// checks: where the archives go, the Rust target of each architecture, the check that a binary is statically linked,
// the `.sha256` file beside each archive and the check of the files a publish takes. Kept apart from the scripts so
// `linux-release.test.mjs` can check them.
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { repoRoot } from "./bundle.mjs";
import { LINUX_ARCHES, linuxArchiveName } from "./release-manifest.mjs";

// Where `release-linux.mjs` writes each architecture's archive, `.sha256` and (after `sign-update`) `.sig`.
export const linuxReleaseDir = path.join(repoRoot, "target", "linux-release");

// The Rust target an architecture builds for: musl, linked statically, so one binary runs on any distribution
// whatever its C library.
export function linuxTarget(arch) {
  if (!LINUX_ARCHES.includes(arch)) throw new Error(`"${arch}" is not a Linux release architecture (${LINUX_ARCHES.join(", ")}).`);
  return `${arch}-unknown-linux-musl`;
}

// The release architecture of a Node `process.arch`, undefined for one that has no release.
export function linuxArchOf(nodeArch) {
  return { x64: "x86_64", arm64: "aarch64" }[nodeArch];
}

// What an ELF file says about how it is linked: its machine (`x86_64`, `aarch64` or the raw number) and whether it
// names a program interpreter (`PT_INTERP`), the dynamic loader a dynamically linked executable needs. A static
// musl binary names none. Reads 64-bit little-endian ELF only, the two release architectures; anything else throws.
export function elfLinkage(buffer) {
  if (buffer.length < 64 || buffer.readUInt32BE(0) !== 0x7f454c46) throw new Error("not an ELF file");
  if (buffer[4] !== 2 || buffer[5] !== 1) throw new Error("not a 64-bit little-endian ELF file");
  const machineNumber = buffer.readUInt16LE(18);
  const machine = { 62: "x86_64", 183: "aarch64" }[machineNumber] ?? String(machineNumber);
  const phoff = Number(buffer.readBigUInt64LE(32));
  const phentsize = buffer.readUInt16LE(54);
  const phnum = buffer.readUInt16LE(56);
  if (phoff + phentsize * phnum > buffer.length) throw new Error("ELF program headers run past the end of the file");
  let interpreter = false;
  for (let i = 0; i < phnum; i += 1) {
    if (buffer.readUInt32LE(phoff + i * phentsize) === 3) interpreter = true;
  }
  return { machine, interpreter };
}

// Throws unless `file` is a statically linked executable for `arch`.
export function assertStaticBinary(file, arch) {
  const { machine, interpreter } = elfLinkage(readFileSync(file));
  if (machine !== arch) throw new Error(`${file} is built for ${machine}, not ${arch}.`);
  if (interpreter) throw new Error(`${file} is dynamically linked (it names a program interpreter); the release needs a static binary.`);
}

export function sha256Of(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

// The content of an archive's `.sha256` file, in the format `sha256sum -c` checks: the digest, two spaces and the
// archive's name.
export function sha256Line(digest, name) {
  return `${digest}  ${name}\n`;
}

// The digest a `.sha256` file's content gives for `name`; throws when it is not one line naming that file.
export function parseSha256Line(text, name) {
  const match = /^([0-9a-f]{64}) [ *](.+)\n?$/.exec(text);
  if (!match || match[2] !== name) throw new Error(`not a SHA-256 line for ${name}`);
  return match[1];
}

// The archives `release-linux.mjs` left in `dir` for `version`, by architecture, for `sign-update` to sign.
export function builtLinuxArchives(dir, version) {
  if (!existsSync(dir)) return [];
  const present = new Set(readdirSync(dir));
  return LINUX_ARCHES.filter((arch) => present.has(linuxArchiveName(version, arch))).map((arch) => ({
    arch,
    archive: path.join(dir, linuxArchiveName(version, arch)),
  }));
}

// The files a Linux publish takes from `dir`: for every architecture, the archive, its `.sha256`, which must match the
// archive, and its `.sig`, which must be no older than the archive. Returns them with the archive's digest.
export function linuxReleaseFiles(dir, version) {
  return LINUX_ARCHES.map((arch) => {
    const name = linuxArchiveName(version, arch);
    const archive = path.join(dir, name);
    const sha256File = `${archive}.sha256`;
    const signature = `${archive}.sig`;
    for (const file of [archive, sha256File, signature]) {
      if (!existsSync(file)) {
        throw new Error(`${file} does not exist. The Linux build of ${version} makes ${name}, its .sha256 and, once signed, its .sig, for each of ${LINUX_ARCHES.join(" and ")}.`);
      }
    }
    const sha256 = sha256Of(archive);
    let recorded;
    try {
      recorded = parseSha256Line(readFileSync(sha256File, "utf8"), name);
    } catch (error) {
      throw new Error(`${sha256File} is not valid (${error.message}). Build the archive again.`, { cause: error });
    }
    if (recorded !== sha256) throw new Error(`${sha256File} does not match ${name}. Build the archive again.`);
    if (statSync(signature).mtimeMs + 5000 < statSync(archive).mtimeMs) {
      throw new Error(`${signature} is older than ${archive}, so it is left over from an earlier build. Sign the update again.`);
    }
    return { arch, archive, sha256File, signature, sha256 };
  });
}
