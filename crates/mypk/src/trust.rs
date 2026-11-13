//! Signed repository trust metadata and repository index verification.

use crate::signature::{SignatureError, TrustedKeyring};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const ROOT_DOMAIN: &[u8] = b"MYSH-MYPK-ROOT-METADATA-V1\0";
const INDEX_DOMAIN: &[u8] = b"MYSH-MYPK-REPOSITORY-INDEX-V1\0";
const MAX_METADATA_SIZE: usize = 1024 * 1024;
const MAX_KEYS: usize = 256;
const MAX_REVOKED_KEYS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustError(String);

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TrustError {}
impl From<SignatureError> for TrustError {
    fn from(value: SignatureError) -> Self { Self(value.to_string()) }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootMetadata {
    format: String,
    format_version: u32,
    repository_id: String,
    version: u64,
    issued_at: u64,
    expires_at: u64,
    keys: Vec<RepositoryKey>,
    revoked_key_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryKey {
    key_id: String,
    public_key: String,
    not_before: u64,
    expires_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryIndex {
    format: String,
    format_version: u32,
    repository_id: String,
    version: u64,
    issued_at: u64,
    expires_at: u64,
    package_index_sha256: String,
    package_index_size: u64,
}

#[derive(Debug, Clone)]
pub struct VerifiedRootMetadata {
    pub repository_id: String,
    pub version: u64,
    pub expires_at: u64,
    pub active_key_ids: Vec<String>,
    pub revoked_key_ids: Vec<String>,
    keyring: TrustedKeyring,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRepositoryIndex {
    pub repository_id: String,
    pub version: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub package_index_sha256: String,
    pub package_index_size: u64,
    pub signer_key_id: String,
}

/// Verify root metadata using keys provisioned by an authenticated bootstrap channel.
///
/// Signatures use the exact metadata bytes prefixed by the MYPK root domain separator.
pub fn verify_root_metadata(
    bootstrap_keys: &TrustedKeyring,
    metadata_bytes: &[u8],
    detached_signature: &str,
    now: u64,
    minimum_version: u64,
) -> Result<VerifiedRootMetadata, TrustError> {
    check_metadata_size(metadata_bytes)?;
    let metadata: RootMetadata = serde_json::from_slice(metadata_bytes)
        .map_err(|e| TrustError(format!("invalid root metadata: {e}")))?;
    if metadata.format != "MYPK-ROOT" || metadata.format_version != 1 {
        return Err(TrustError("unsupported root metadata format".into()));
    }
    validate_repository_id(&metadata.repository_id)?;
    validate_time_window(metadata.issued_at, metadata.expires_at, now, "root metadata")?;
    if metadata.version < minimum_version {
        return Err(TrustError("root metadata rollback detected".into()));
    }
    if metadata.keys.is_empty() || metadata.keys.len() > MAX_KEYS {
        return Err(TrustError("root metadata key count is outside permitted limits".into()));
    }
    if metadata.revoked_key_ids.len() > MAX_REVOKED_KEYS {
        return Err(TrustError("root metadata revocation list exceeds its limit".into()));
    }

    verify_detached(
        bootstrap_keys,
        ROOT_DOMAIN,
        metadata_bytes,
        detached_signature,
    )?;
    let revoked: BTreeSet<String> = metadata.revoked_key_ids.iter().cloned().collect();
    if revoked.len() != metadata.revoked_key_ids.len() {
        return Err(TrustError("duplicate key ID in revocation list".into()));
    }
    for id in &revoked {
        validate_key_id(id)?;
    }

    let mut seen = BTreeSet::new();
    let mut key_lines = String::new();
    let mut active_key_ids = Vec::new();
    for key in &metadata.keys {
        validate_key_id(&key.key_id)?;
        if !seen.insert(key.key_id.clone()) {
            return Err(TrustError(format!("duplicate repository key ID: {}", key.key_id)));
        }
        if key.not_before >= key.expires_at {
            return Err(TrustError(format!("invalid validity interval for key {}", key.key_id)));
        }
        if revoked.contains(&key.key_id) {
            continue;
        }
        if now >= key.not_before && now < key.expires_at {
            if key.public_key.len() != 64 || !key.public_key.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(TrustError(format!("invalid public key encoding for {}", key.key_id)));
            }
            key_lines.push_str(&key.key_id);
            key_lines.push(' ');
            key_lines.push_str(&key.public_key);
            key_lines.push('\n');
            active_key_ids.push(key.key_id.clone());
        }
    }
    if active_key_ids.is_empty() {
        return Err(TrustError("root metadata contains no active repository signing keys".into()));
    }
    let keyring = TrustedKeyring::parse(&key_lines)?;
    active_key_ids.sort();
    let mut revoked_key_ids = revoked.into_iter().collect::<Vec<_>>();
    revoked_key_ids.sort();
    Ok(VerifiedRootMetadata {
        repository_id: metadata.repository_id,
        version: metadata.version,
        expires_at: metadata.expires_at,
        active_key_ids,
        revoked_key_ids,
        keyring,
    })
}

/// Verify repository index metadata against a previously verified root document.
///
/// The index signature covers the exact JSON bytes with a domain separator. The
/// referenced package-index bytes must match both the declared size and SHA-256.
pub fn verify_repository_index(
    root: &VerifiedRootMetadata,
    index_bytes: &[u8],
    detached_signature: &str,
    package_index_bytes: &[u8],
    now: u64,
    minimum_version: u64,
) -> Result<VerifiedRepositoryIndex, TrustError> {
    check_metadata_size(index_bytes)?;
    let index: RepositoryIndex = serde_json::from_slice(index_bytes)
        .map_err(|e| TrustError(format!("invalid repository index metadata: {e}")))?;
    if index.format != "MYPK-INDEX" || index.format_version != 1 {
        return Err(TrustError("unsupported repository index format".into()));
    }
    validate_repository_id(&index.repository_id)?;
    if index.repository_id != root.repository_id {
        return Err(TrustError("repository identity does not match trusted root metadata".into()));
    }
    validate_time_window(index.issued_at, index.expires_at, now, "repository index")?;
    if index.version < minimum_version {
        return Err(TrustError("repository index rollback detected".into()));
    }
    let signer_key_id = verify_detached(
        &root.keyring,
        INDEX_DOMAIN,
        index_bytes,
        detached_signature,
    )?;
    if package_index_bytes.len() as u64 != index.package_index_size {
        return Err(TrustError("package index size mismatch".into()));
    }
    if index.package_index_sha256.len() != 64
        || !index.package_index_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(TrustError("invalid package index SHA-256 encoding".into()));
    }
    let digest = Sha256::digest(package_index_bytes);
    let actual = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    if !actual.eq_ignore_ascii_case(&index.package_index_sha256) {
        return Err(TrustError("package index SHA-256 mismatch".into()));
    }

    Ok(VerifiedRepositoryIndex {
        repository_id: index.repository_id,
        version: index.version,
        issued_at: index.issued_at,
        expires_at: index.expires_at,
        package_index_sha256: index.package_index_sha256.to_ascii_lowercase(),
        package_index_size: index.package_index_size,
        signer_key_id,
    })
}

fn verify_detached(
    keyring: &TrustedKeyring,
    domain: &[u8],
    payload: &[u8],
    detached_signature: &str,
) -> Result<String, TrustError> {
    let mut fields = detached_signature.split_whitespace();
    let declared_id = fields.next().ok_or_else(|| TrustError("missing signature key ID".into()))?;
    let encoded = fields.next().ok_or_else(|| TrustError("missing detached signature".into()))?;
    if fields.next().is_some() {
        return Err(TrustError("invalid detached signature record".into()));
    }
    validate_key_id(declared_id)?;
    if encoded.len() != 128 || !encoded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(TrustError("Ed25519 signature must be 128 hexadecimal characters".into()));
    }
    let signature = decode_hex_64(encoded)?;
    let mut message = Vec::with_capacity(domain.len() + payload.len());
    message.extend_from_slice(domain);
    message.extend_from_slice(payload);
    let verified_id = keyring.verify(&message, &signature)?;
    if verified_id != declared_id {
        return Err(TrustError("signature key ID does not match the verifying key".into()));
    }
    Ok(verified_id)
}

fn decode_hex_64(value: &str) -> Result<[u8; 64], TrustError> {
    let mut bytes = [0_u8; 64];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| TrustError("invalid signature encoding".into()))?;
    }
    Ok(bytes)
}

fn validate_key_id(value: &str) -> Result<(), TrustError> {
    if value.is_empty() || value.len() > 64
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(TrustError("invalid key ID".into()));
    }
    Ok(())
}

fn validate_repository_id(value: &str) -> Result<(), TrustError> {
    if value.is_empty() || value.len() > 128
        || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(TrustError("invalid repository ID".into()));
    }
    Ok(())
}

fn validate_time_window(issued_at: u64, expires_at: u64, now: u64, label: &str) -> Result<(), TrustError> {
    if issued_at >= expires_at || now < issued_at || now >= expires_at {
        return Err(TrustError(format!("{label} is not currently valid")));
    }
    Ok(())
}

fn check_metadata_size(bytes: &[u8]) -> Result<(), TrustError> {
    if bytes.is_empty() || bytes.len() > MAX_METADATA_SIZE {
        return Err(TrustError("metadata is empty or exceeds 1 MiB".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn public_hex(key: &SigningKey) -> String {
        key.verifying_key().to_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    fn sign(key: &SigningKey, domain: &[u8], payload: &[u8], id: &str) -> String {
        let mut message = domain.to_vec();
        message.extend_from_slice(payload);
        let signature = key.sign(&message);
        let encoded = signature.to_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
        format!("{id} {encoded}")
    }

    #[test]
    fn verifies_root_and_repository_index_and_package_index_digest() {
        let root_signer = SigningKey::from_bytes(&[21_u8; 32]);
        let repo_signer = SigningKey::from_bytes(&[22_u8; 32]);
        let bootstrap = TrustedKeyring::parse(&format!("root {}\n", public_hex(&root_signer))).unwrap();
        let root = format!(
            r#"{{"format":"MYPK-ROOT","format_version":1,"repository_id":"official","version":2,"issued_at":100,"expires_at":1000,"keys":[{{"key_id":"repo-1","public_key":"{}","not_before":100,"expires_at":900}}],"revoked_key_ids":[]}}"#,
            public_hex(&repo_signer)
        );
        let root_sig = sign(&root_signer, ROOT_DOMAIN, root.as_bytes(), "root");
        let verified_root = verify_root_metadata(&bootstrap, root.as_bytes(), &root_sig, 200, 1).unwrap();

        let packages = b"package-index-content";
        let digest = Sha256::digest(packages).iter().map(|b| format!("{b:02x}")).collect::<String>();
        let index = format!(
            r#"{{"format":"MYPK-INDEX","format_version":1,"repository_id":"official","version":5,"issued_at":150,"expires_at":800,"package_index_sha256":"{}","package_index_size":{}}}"#,
            digest, packages.len()
        );
        let index_sig = sign(&repo_signer, INDEX_DOMAIN, index.as_bytes(), "repo-1");
        let verified = verify_repository_index(&verified_root, index.as_bytes(), &index_sig, packages, 200, 4).unwrap();
        assert_eq!(verified.signer_key_id, "repo-1");
        assert_eq!(verified.version, 5);
    }

    #[test]
    fn rejects_revoked_repository_key() {
        let root_signer = SigningKey::from_bytes(&[31_u8; 32]);
        let repo_signer = SigningKey::from_bytes(&[32_u8; 32]);
        let bootstrap = TrustedKeyring::parse(&format!("root {}\n", public_hex(&root_signer))).unwrap();
        let root = format!(
            r#"{{"format":"MYPK-ROOT","format_version":1,"repository_id":"official","version":2,"issued_at":100,"expires_at":1000,"keys":[{{"key_id":"repo-1","public_key":"{}","not_before":100,"expires_at":900}}],"revoked_key_ids":["repo-1"]}}"#,
            public_hex(&repo_signer)
        );
        let signature = sign(&root_signer, ROOT_DOMAIN, root.as_bytes(), "root");
        assert!(verify_root_metadata(&bootstrap, root.as_bytes(), &signature, 200, 1).is_err());
    }

    #[test]
    fn rejects_expired_root_metadata_and_rollback() {
        let signer = SigningKey::from_bytes(&[41_u8; 32]);
        let bootstrap = TrustedKeyring::parse(&format!("root {}\n", public_hex(&signer))).unwrap();
        let root = format!(
            r#"{{"format":"MYPK-ROOT","format_version":1,"repository_id":"official","version":2,"issued_at":100,"expires_at":200,"keys":[{{"key_id":"repo","public_key":"{}","not_before":100,"expires_at":900}}],"revoked_key_ids":[]}}"#,
            public_hex(&SigningKey::from_bytes(&[42_u8; 32]))
        );
        let signature = sign(&signer, ROOT_DOMAIN, root.as_bytes(), "root");
        assert!(verify_root_metadata(&bootstrap, root.as_bytes(), &signature, 200, 1).is_err());
        assert!(verify_root_metadata(&bootstrap, root.as_bytes(), &signature, 150, 3).is_err());
    }

    #[test]
    fn rejects_tampered_metadata_and_package_index() {
        let root_signer = SigningKey::from_bytes(&[51_u8; 32]);
        let repo_signer = SigningKey::from_bytes(&[52_u8; 32]);
        let bootstrap = TrustedKeyring::parse(&format!("root {}\n", public_hex(&root_signer))).unwrap();
        let root = format!(
            r#"{{"format":"MYPK-ROOT","format_version":1,"repository_id":"official","version":1,"issued_at":100,"expires_at":1000,"keys":[{{"key_id":"repo","public_key":"{}","not_before":100,"expires_at":900}}],"revoked_key_ids":[]}}"#,
            public_hex(&repo_signer)
        );
        let root_sig = sign(&root_signer, ROOT_DOMAIN, root.as_bytes(), "root");
        let verified_root = verify_root_metadata(&bootstrap, root.as_bytes(), &root_sig, 200, 1).unwrap();
        let packages = b"original";
        let digest = Sha256::digest(packages).iter().map(|b| format!("{b:02x}")).collect::<String>();
        let index = format!(
            r#"{{"format":"MYPK-INDEX","format_version":1,"repository_id":"official","version":1,"issued_at":150,"expires_at":800,"package_index_sha256":"{}","package_index_size":{}}}"#,
            digest, packages.len()
        );
        let signature = sign(&repo_signer, INDEX_DOMAIN, index.as_bytes(), "repo");
        let mut tampered = index.as_bytes().to_vec();
        tampered.push(b' ');
        assert!(verify_repository_index(&verified_root, &tampered, &signature, packages, 200, 1).is_err());
        assert!(verify_repository_index(&verified_root, index.as_bytes(), &signature, b"tampered", 200, 1).is_err());
    }
}
