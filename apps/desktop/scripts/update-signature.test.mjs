import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, randomBytes, sign } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { parsePublicKey, parseSignature, trustedFields, verifyUpdateSignature } from "./update-signature.mjs";

const root = mkdtempSync(path.join(os.tmpdir(), "update-signature-"));
after(() => rmSync(root, { recursive: true, force: true }));

let fileCount = 0;
function fileWith(content) {
  const file = path.join(root, `update-${fileCount++}.bin`);
  writeFileSync(file, content);
  return file;
}

const base64 = (text) => Buffer.from(text, "utf8").toString("base64");

// A minisign key pair made with Node's crypto, and signatures built by the minisign file format.
function makeKey(keyId = randomBytes(8)) {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const publicKeyFile = `untrusted comment: minisign public key\n${Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64")}\n`;
  return {
    keyId,
    publicKeyFile,
    publicKeyText: base64(publicKeyFile),
    // `signed` is what the signature line signs; the prehashed BLAKE2b-512 of `content` by default.
    sign({
      content,
      name = "app.tar.gz",
      version = "1.2.3",
      algorithm = "ED",
      trustedComment = `timestamp:1700000000\tfile:${name}\tversion:${version}`,
      signedComment = trustedComment,
      signer = privateKey,
      globalSigner = signer,
      signed = algorithm === "ED" ? createHash("blake2b512").update(content).digest() : content,
    }) {
      const signature = sign(null, signed, signer);
      const global = sign(null, Buffer.concat([signature, Buffer.from(signedComment, "utf8")]), globalSigner);
      const line = Buffer.concat([Buffer.from(algorithm), keyId, signature]).toString("base64");
      return (
        `untrusted comment: signature from tauri secret key\n${line}\n` +
        `trusted comment: ${trustedComment}\n${global.toString("base64")}\n`
      );
    },
  };
}

const content = Buffer.from("the update archive");
const args = (key, signatureFile, extra = {}) => ({
  signatureText: base64(signatureFile),
  file: fileWith(content),
  publicKeyText: key.publicKeyText,
  name: "app.tar.gz",
  version: "1.2.3",
  ...extra,
});

test("a prehashed signature of the file by the key, naming the file and version, is accepted", () => {
  const key = makeKey();
  assert.doesNotThrow(() => verifyUpdateSignature(args(key, key.sign({ content }))));
});

test("the signature and the public key are accepted as the base64 Tauri writes or as the minisign text", () => {
  const key = makeKey();
  const signatureFile = key.sign({ content });
  assert.doesNotThrow(() =>
    verifyUpdateSignature({ ...args(key, signatureFile), signatureText: signatureFile, publicKeyText: key.publicKeyFile }),
  );
  assert.doesNotThrow(() => verifyUpdateSignature({ ...args(key, signatureFile), signatureText: `${base64(signatureFile)}\n` }));
  assert.doesNotThrow(() => verifyUpdateSignature({ ...args(key, signatureFile), signatureText: signatureFile.replaceAll("\n", "\r\n") }));
});

test("a file larger than the read chunk is verified over all of its bytes", () => {
  const key = makeKey();
  const big = randomBytes(3 * 1024 * 1024 + 17);
  const signatureFile = key.sign({ content: big });
  assert.doesNotThrow(() => verifyUpdateSignature({ ...args(key, signatureFile), file: fileWith(big) }));
  const changed = Buffer.from(big);
  changed[changed.length - 1] ^= 1;
  assert.throws(() => verifyUpdateSignature({ ...args(key, signatureFile), file: fileWith(changed) }), /does not match the file/);
});

test("a signature made with another key is refused, naming both key ids as minisign prints them", () => {
  const key = makeKey(Buffer.from("0123456789abcdef", "hex"));
  const other = makeKey(Buffer.from("1122334455667788", "hex"));
  assert.throws(
    () => verifyUpdateSignature(args(key, other.sign({ content }))),
    (error) => error.message.includes("8877665544332211") && error.message.includes("EFCDAB8967452301"),
  );
});

test("a signature carrying the key id of the key but made with another private key is refused", () => {
  const key = makeKey();
  const impostor = makeKey(key.keyId);
  assert.throws(() => verifyUpdateSignature(args(key, impostor.sign({ content }))), /does not match the file/);
});

test("a file with other content than the signed one is refused", () => {
  const key = makeKey();
  assert.throws(
    () => verifyUpdateSignature(args(key, key.sign({ content }), { file: fileWith("another archive") })),
    /does not match the file/,
  );
});

test("a trusted comment its global signature does not cover is refused", () => {
  const key = makeKey();
  const tampered = key.sign({ content, signedComment: "timestamp:1700000000\tfile:app.tar.gz\tversion:0.0.1" });
  assert.throws(() => verifyUpdateSignature(args(key, tampered)), /trusted comment/);
});

test("a global signature made with another private key is refused", () => {
  const key = makeKey();
  const forged = key.sign({ content, globalSigner: generateKeyPairSync("ed25519").privateKey });
  assert.throws(() => verifyUpdateSignature(args(key, forged)), /trusted comment/);
});

test("a signature made for another file name is refused even when it is validly signed", () => {
  const key = makeKey();
  assert.throws(() => verifyUpdateSignature(args(key, key.sign({ content, name: "other.tar.gz" }))), /other\.tar\.gz/);
});

test("a signature bound to another version is refused even when it is validly signed", () => {
  const key = makeKey();
  assert.throws(() => verifyUpdateSignature(args(key, key.sign({ content, version: "1.2.2" }))), /1\.2\.2/);
});

test("a trusted comment with no file or no version is refused", () => {
  const key = makeKey();
  assert.throws(
    () => verifyUpdateSignature(args(key, key.sign({ content, trustedComment: "timestamp:1700000000\tversion:1.2.3" }))),
    /file/,
  );
  assert.throws(
    () => verifyUpdateSignature(args(key, key.sign({ content, trustedComment: "timestamp:1700000000\tfile:app.tar.gz" }))),
    /version/,
  );
  assert.throws(() => verifyUpdateSignature(args(key, key.sign({ content, trustedComment: "" }))), /file/);
});

test("the legacy non-prehashed algorithm is refused", () => {
  const key = makeKey();
  assert.throws(() => verifyUpdateSignature(args(key, key.sign({ content, algorithm: "Ed" }))), /"Ed"/);
});

test("text that is not a minisign signature is refused", () => {
  const key = makeKey();
  for (const signatureText of ["", "not a signature", base64("hello\nworld\n"), base64("untrusted comment: x\nAAAA\n")]) {
    assert.throws(() => verifyUpdateSignature({ ...args(key, ""), signatureText }), /not a minisign signature|not a minisign file/);
  }
});

test("a public key that is not a minisign Ed25519 key is refused", () => {
  const key = makeKey();
  const signatureFile = key.sign({ content });
  const legacyId = Buffer.concat([Buffer.from("ED"), key.keyId, Buffer.alloc(32)]).toString("base64");
  for (const publicKeyText of [
    "",
    "not a key",
    base64("untrusted comment: minisign public key\nAAAA\n"),
    base64(`untrusted comment: minisign public key\n${legacyId}\n`),
  ]) {
    assert.throws(() => verifyUpdateSignature({ ...args(key, signatureFile), publicKeyText }), /public key|minisign/);
  }
});

test("parsePublicKey reads the key id and key from the base64 or the plain text", () => {
  const key = makeKey();
  for (const text of [key.publicKeyText, key.publicKeyFile]) {
    const parsed = parsePublicKey(text);
    assert.deepEqual(parsed.keyId, key.keyId);
    assert.equal(parsed.key.asymmetricKeyType, "ed25519");
  }
  assert.throws(() => parsePublicKey("garbage"), /minisign/);
});

test("parseSignature splits a signature into its parts", () => {
  const key = makeKey();
  const parsed = parseSignature(base64(key.sign({ content, trustedComment: "timestamp:1\tfile:a\tversion:2" })));
  assert.equal(parsed.algorithm, "ED");
  assert.deepEqual(parsed.keyId, key.keyId);
  assert.equal(parsed.signature.length, 64);
  assert.equal(parsed.globalSignature.length, 64);
  assert.equal(parsed.trustedComment, "timestamp:1\tfile:a\tversion:2");
  assert.throws(() => parseSignature("garbage"), /minisign/);
});

test("trustedFields reads the tab-separated key:value fields, keeping colons inside a value", () => {
  assert.deepEqual(trustedFields("timestamp:1700000000\tfile:app.tar.gz\tversion:1.2.3"), {
    timestamp: "1700000000",
    file: "app.tar.gz",
    version: "1.2.3",
  });
  assert.deepEqual(trustedFields("file:a:b\tnovalue\t"), { file: "a:b" });
  assert.deepEqual(trustedFields(""), {});
});

test("trustedFields keeps the first of a key given twice, as the daemon's self-update does", () => {
  assert.deepEqual(trustedFields("file:first.tar.gz\tfile:second.tar.gz"), { file: "first.tar.gz" });
});

// What `tauri signer sign --app-version 1.2.3` and `tauri signer generate` wrote, to prove the compatibility with
// Tauri.
const TAURI_PUBLIC_KEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEVCMzYwNTJENTkzNjZEMTQKUldRVWJUWlpMUVUyNjRHbHhQcXZPSkplcGFVQmJkRlo3dWpPQ1dZK2NDRzNJOFdvTGZuU1VrQjEK";
const TAURI_SIGNATURE =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRVWJUWlpMUVUyNjdVZHV3d3NRekRyL01VdU1YMHA3eGZlWTZFNkg1NVAwekhVakU3ZURSU0t1RTlidHI5SDdrQW5pbUlUVVRXbWxvNWk2cU9iN0d0dVZIRENhWGRWT2dNPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NTY4MzE2CWZpbGU6cGF5bG9hZC5iaW4JdmVyc2lvbjoxLjIuMwp1a2JNUklvWCs2dWwxOHh3N3h3UEtheUE0NGtDekp4M3VnT01WSFlFUGM2Y0xDcGJiSWgwazVwbVpLVXVyamZEZzMrRFh4N2FwZzRxRGZYbCs5RzFCZz09Cg==";
const TAURI_CONTENT = "octoboard update fixture\n";

test("a signature written by the Tauri CLI is accepted, and refused for another file, name, version or key", () => {
  const tauri = {
    signatureText: TAURI_SIGNATURE,
    file: fileWith(TAURI_CONTENT),
    publicKeyText: TAURI_PUBLIC_KEY,
    name: "payload.bin",
    version: "1.2.3",
  };
  assert.doesNotThrow(() => verifyUpdateSignature(tauri));
  assert.throws(() => verifyUpdateSignature({ ...tauri, file: fileWith(`${TAURI_CONTENT}x`) }), /does not match the file/);
  assert.throws(() => verifyUpdateSignature({ ...tauri, name: "other.bin" }), /other\.bin/);
  assert.throws(() => verifyUpdateSignature({ ...tauri, version: "1.2.4" }), /1\.2\.4/);
  assert.throws(
    () => verifyUpdateSignature({ ...tauri, publicKeyText: makeKey().publicKeyText }),
    /EB36052D59366D14/,
  );
});
