//! Passphrase encryption for API keys synced to the cloud server:
//! Argon2id derives a key from the passphrase, XChaCha20-Poly1305 encrypts.
//! The server only ever stores the resulting envelope.

use argon2::Argon2;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::error::{AppResult, invalid};

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u8,
    salt: String,
    nonce: String,
    data: String,
}

fn random<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| invalid(format!("No secure random source: {e}")))?;
    Ok(bytes)
}

fn derive_key(passphrase: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| invalid(format!("Key derivation failed: {e}")))?;
    Ok(key)
}

pub fn encrypt(passphrase: &str, plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let salt = random::<16>()?;
    let nonce = random::<24>()?;
    let key = derive_key(passphrase, &salt)?;
    let data = XChaCha20Poly1305::new(Key::from_slice(&key))
        .encrypt(XNonce::from_slice(&nonce), plaintext)
        .map_err(|_| invalid("Encryption failed"))?;
    let envelope = Envelope { v: 1, salt: B64.encode(salt), nonce: B64.encode(nonce), data: B64.encode(data) };
    Ok(serde_json::to_vec(&envelope)?)
}

pub fn decrypt(passphrase: &str, blob: &[u8]) -> AppResult<Vec<u8>> {
    let corrupted = || invalid("The synced API keys are corrupted");
    let envelope: Envelope = serde_json::from_slice(blob).map_err(|_| corrupted())?;
    let decode = |s: &str| B64.decode(s).map_err(|_| corrupted());
    let (salt, nonce, data) = (decode(&envelope.salt)?, decode(&envelope.nonce)?, decode(&envelope.data)?);
    if nonce.len() != 24 {
        return Err(corrupted());
    }
    let key = derive_key(passphrase, &salt)?;
    XChaCha20Poly1305::new(Key::from_slice(&key))
        .decrypt(XNonce::from_slice(&nonce), data.as_ref())
        .map_err(|_| invalid("Wrong passphrase: the synced API keys could not be decrypted"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_passphrase() {
        let blob = encrypt("correct horse", b"{\"api_key_openai\":\"sk-1\"}").unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("sk-1"));
        assert_eq!(decrypt("correct horse", &blob).unwrap(), b"{\"api_key_openai\":\"sk-1\"}");
        assert!(decrypt("wrong", &blob).is_err());
        assert!(decrypt("correct horse", b"not json").is_err());
        // Fresh salt and nonce every time.
        assert_ne!(encrypt("p", b"x").unwrap(), encrypt("p", b"x").unwrap());
    }
}
