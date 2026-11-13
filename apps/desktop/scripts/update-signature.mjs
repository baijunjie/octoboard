// Checks an update signature against the update key's public key before it is published, with Node's built-in modules
// only (the publishing installs nothing). A signature is the content of the `.sig` file `tauri signer sign` writes: the
// base64 of a minisign signature file, whose signature line is `ED` (Ed25519 over the BLAKE2b-512 of the file), the key
// id and the signature, and whose global signature covers that signature and the trusted comment
// (`timestamp:…\tfile:<name>\tversion:<version>`). The public key is `plugins.updater.pubkey` of `tauri.conf.json`: the
// base64 of a minisign public key file, `Ed`, the key id and the Ed25519 key. The in-app updater and the daemon's
// self-update check the same, so a signature made with another key would fail every client; it fails here instead.
import { createHash, createPublicKey, verify } from "node:crypto";
import { closeSync, openSync, readSync } from "node:fs";

// The DER prefix of an Ed25519 SubjectPublicKeyInfo, before the 32 key bytes.
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

// The lines of a minisign file given as the base64 Tauri uses, or as the file itself.
function minisignLines(text, what) {
  let plain = text.trim();
  if (!plain.startsWith("untrusted comment:")) plain = Buffer.from(plain, "base64").toString("utf8");
  const lines = plain.split(/\r?\n/).filter((line) => line !== "");
  if (lines.length === 0 || !lines[0].startsWith("untrusted comment:")) {
    throw new Error(`The ${what} is not a minisign file.`);
  }
  return lines;
}

// The key id and the Ed25519 key of a minisign public key (`plugins.updater.pubkey`, or the key file itself).
export function parsePublicKey(text) {
  const lines = minisignLines(text, "public key");
  const bytes = Buffer.from(lines[1] ?? "", "base64");
  if (bytes.length !== 42 || bytes.subarray(0, 2).toString("latin1") !== "Ed") {
    throw new Error("The public key is not a minisign Ed25519 key.");
  }
  return {
    keyId: bytes.subarray(2, 10),
    key: createPublicKey({ key: Buffer.concat([ED25519_SPKI_PREFIX, bytes.subarray(10)]), format: "der", type: "spki" }),
  };
}

// The parts of a minisign signature: its algorithm (`ED` prehashed, `Ed` legacy), key id, signature, trusted comment
// and global signature.
export function parseSignature(text) {
  const lines = minisignLines(text, "signature");
  const bytes = Buffer.from(lines[1] ?? "", "base64");
  const trusted = lines[2] ?? "";
  const global = Buffer.from(lines[3] ?? "", "base64");
  if (bytes.length !== 74 || !trusted.startsWith("trusted comment: ") || global.length !== 64) {
    throw new Error("The signature is not a minisign signature.");
  }
  return {
    algorithm: bytes.subarray(0, 2).toString("latin1"),
    keyId: bytes.subarray(2, 10),
    signature: bytes.subarray(10),
    trustedComment: trusted.slice("trusted comment: ".length),
    globalSignature: global,
  };
}

// The `key:value` fields of a trusted comment, separated by tabs; of a key given twice, the first, as the daemon's
// self-update reads it.
export function trustedFields(comment) {
  const fields = {};
  for (const part of comment.split("\t")) {
    const colon = part.indexOf(":");
    if (colon > 0 && !Object.hasOwn(fields, part.slice(0, colon))) fields[part.slice(0, colon)] = part.slice(colon + 1);
  }
  return fields;
}

// The BLAKE2b-512 of a file, read in chunks: the macOS update archive is large.
function blake2bOfFile(file) {
  const hash = createHash("blake2b512");
  const fd = openSync(file, "r");
  try {
    const chunk = Buffer.alloc(1024 * 1024);
    for (let read; (read = readSync(fd, chunk, 0, chunk.length, null)) > 0; ) hash.update(chunk.subarray(0, read));
  } finally {
    closeSync(fd);
  }
  return hash.digest();
}

// Throws unless `signatureText` is a signature of `file` by `publicKeyText`, whose trusted comment names the file
// `name` and the version `version`. The error says which check failed.
export function verifyUpdateSignature({ signatureText, file, publicKeyText, name, version }) {
  const publicKey = parsePublicKey(publicKeyText);
  const signature = parseSignature(signatureText);
  const which = `The signature of ${name}`;
  if (!signature.keyId.equals(publicKey.keyId)) {
    throw new Error(
      `${which} was made with the key ${keyIdHex(signature.keyId)}, and the update key in tauri.conf.json is ` +
        `${keyIdHex(publicKey.keyId)}: every client would refuse it. Sign the update again with the update key.`,
    );
  }
  if (signature.algorithm !== "ED") {
    throw new Error(`${which} is of the algorithm "${signature.algorithm}", not the prehashed "ED" Tauri writes.`);
  }
  if (!verify(null, blake2bOfFile(file), publicKey.key, signature.signature)) {
    throw new Error(`${which} does not match the file ${file}: it was made for other content or with another key.`);
  }
  const signedComment = Buffer.concat([signature.signature, Buffer.from(signature.trustedComment, "utf8")]);
  if (!verify(null, signedComment, publicKey.key, signature.globalSignature)) {
    throw new Error(`${which} has a trusted comment its global signature does not cover.`);
  }
  const fields = trustedFields(signature.trustedComment);
  if (fields.file !== name) throw new Error(`${which} names the file "${fields.file ?? ""}", not ${name}.`);
  if (fields.version !== version) {
    throw new Error(`${which} is bound to the version "${fields.version ?? ""}", not ${version}.`);
  }
}

// A key id the way minisign prints it: the eight bytes in reverse order, as uppercase hex.
function keyIdHex(keyId) {
  return Buffer.from(keyId).reverse().toString("hex").toUpperCase();
}
