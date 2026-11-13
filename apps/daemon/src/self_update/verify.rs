//! What makes a downloaded archive the release it claims to be: its SHA-256 equals the manifest's,
//! its minisign signature is the release key's, and the file the signature names is this release's
//! archive for this machine. The last is what stops an older release, which the key signed too,
//! from being passed off as the new one.
//!
//! The release key is the update key of the desktop application: the public key built in below is
//! the key line of `plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json` (a test
//! holds the two together). The manifest's `signature` is the content of the `.sig` file `tauri
//! signer sign` writes, which is the base64 of the minisign signature file.

use std::io::Read;
use std::path::Path;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

/// The release key's public key, as the key line of a minisign public key file.
pub const PUBLIC_KEY: &str = "RWSVMIwih++1gX8lYHjtWtYI6FzRxgcQVkKLHTqLgNw0lz27touOK8GB";

/// The release key as a key.
pub fn release_key() -> PublicKey {
    PublicKey::from_base64(PUBLIC_KEY).expect("the built-in release key is a minisign public key")
}

/// The signature `text` carries: the content of a `.sig` file written by `tauri signer sign` (the
/// base64 of a minisign signature file), or the minisign signature file itself.
pub fn decode_signature(text: &str) -> Result<Signature, String> {
    let text = text.trim();
    let plain = if text.starts_with("untrusted comment:") {
        text.to_owned()
    } else {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|_| "the signature is not base64".to_owned())?;
        String::from_utf8(bytes).map_err(|_| "the signature is not text".to_owned())?
    };
    Signature::decode(&plain).map_err(|err| format!("the signature is not readable: {err}"))
}

/// The file the signature's trusted comment names (`timestamp:…\tfile:<name>`), `None` when it
/// names none.
fn signed_file(signature: &Signature) -> Option<&str> {
    signature
        .trusted_comment()
        .split('\t')
        .find_map(|part| part.strip_prefix("file:"))
}

/// Checks the archive at `archive` against `sha256` (lowercase hex) and `signature` made with
/// `key`, and that the signature names `expected_name`. The error says which check failed, in
/// English, for the failure's detail. Blocking: it reads the archive once.
pub fn verify_archive(
    archive: &Path,
    sha256: &str,
    signature: &str,
    expected_name: &str,
    key: &PublicKey,
) -> Result<(), String> {
    let signature = decode_signature(signature)?;
    match signed_file(&signature) {
        Some(name) if name == expected_name => {}
        Some(name) => {
            return Err(format!(
                "the signature is of `{name}`, not of `{expected_name}`"
            ))
        }
        None => return Err("the signature names no file".to_owned()),
    }
    let mut stream = key
        .verify_stream(&signature)
        .map_err(|err| format!("the signature does not fit the release key: {err}"))?;
    let mut hash = Sha256::new();
    let mut file = std::fs::File::open(archive)
        .map_err(|err| format!("the archive could not be read back: {err}"))?;
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut chunk)
            .map_err(|err| format!("the archive could not be read back: {err}"))?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
        stream.update(&chunk[..read]);
    }
    let found: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if found != sha256 {
        return Err(format!(
            "the archive's SHA-256 is {found}, not the {sha256} the manifest names"
        ));
    }
    stream
        .finalize()
        .map_err(|_| "the archive's signature is not the release key's".to_owned())
}
