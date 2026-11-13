//! Trusted Ed25519 public-key parsing and detached signature verification.

use ed25519_dalek::{Signature, VerifyingKey};
use std::collections::BTreeSet;

/// A trusted publisher key identified by a local key ID.
#[derive(Debug, Clone)]
struct TrustedKey {
    id: String,
    key: VerifyingKey,
}

/// A parsed collection of trusted publisher keys.
#[derive(Debug, Clone, Default)]
pub struct TrustedKeyring {
    keys: Vec<TrustedKey>,
}

/// Signature verification error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureError(String);

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SignatureError {}

impl TrustedKeyring {
    /// Parse lines in the form "<key-id> <64-character-hex-public-key>".
    /// Empty lines and lines beginning with '#' are ignored.
    pub fn parse(input: &str) -> Result<Self, SignatureError> {
        let mut keys = Vec::new();
        let mut ids = BTreeSet::new();

        for (index, raw) in input.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let id = fields.next().ok_or_else(|| SignatureError(format!("missing key ID on line {}", index + 1)))?;
            let encoded = fields.next().ok_or_else(|| SignatureError(format!("missing public key on line {}", index + 1)))?;
            if fields.next().is_some() || !valid_key_id(id) {
                return Err(SignatureError(format!("invalid trusted-key entry on line {}", index + 1)));
            }
            if !ids.insert(id.to_owned()) {
                return Err(SignatureError(format!("duplicate trusted key ID: {id}")));
            }
            let bytes = decode_hex(encoded).ok_or_else(|| SignatureError(format!("invalid Ed25519 public key on line {}", index + 1)))?;
            let raw_key: [u8; 32] = bytes.try_into().map_err(|_| SignatureError(format!("Ed25519 public key must be 32 bytes on line {}", index + 1)))?;
            let key = VerifyingKey::from_bytes(&raw_key)
                .map_err(|_| SignatureError(format!("invalid Ed25519 public key on line {}", index + 1)))?;
            keys.push(TrustedKey { id: id.to_owned(), key });
        }
        Ok(Self { keys })
    }

    /// Verify a 64-byte Ed25519 signature and return the matching trusted key ID.
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<String, SignatureError> {
        let signature = Signature::from_bytes(signature);
        for trusted in &self.keys {
            if trusted.key.verify_strict(message, &signature).is_ok() {
                return Ok(trusted.id.clone());
            }
        }
        Err(SignatureError("signature does not match any trusted publisher key".into()))
    }

    /// Return the number of trusted publisher keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Return whether the keyring contains no trusted keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

fn valid_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_signature_with_trusted_key() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let message = b"canonical checksums";
        let signature = signing.sign(message);
        let public = signing.verifying_key().to_bytes();
        let encoded = public.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let keyring = TrustedKeyring::parse(&format!("release-2026 {encoded}\n")).unwrap();
        assert_eq!(keyring.verify(message, &signature.to_bytes()).unwrap(), "release-2026");
    }

    #[test]
    fn rejects_modified_message() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing.sign(b"original");
        let encoded = signing.verifying_key().to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let keyring = TrustedKeyring::parse(&format!("release {encoded}")).unwrap();
        assert!(keyring.verify(b"modified", &signature.to_bytes()).is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        let signer = SigningKey::from_bytes(&[7_u8; 32]);
        let other = SigningKey::from_bytes(&[8_u8; 32]);
        let signature = signer.sign(b"message");
        let encoded = other.verifying_key().to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let keyring = TrustedKeyring::parse(&format!("other {encoded}")).unwrap();
        assert!(keyring.verify(b"message", &signature.to_bytes()).is_err());
    }

    #[test]
    fn rejects_malformed_and_duplicate_key_entries() {
        assert!(TrustedKeyring::parse("bad only-one-field").is_err());
        let signing = SigningKey::from_bytes(&[9_u8; 32]);
        let encoded = signing.verifying_key().to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        assert!(TrustedKeyring::parse(&format!("same {encoded}\nsame {encoded}")).is_err());
        assert!(TrustedKeyring::parse("key zzzz").is_err());
    }
}
