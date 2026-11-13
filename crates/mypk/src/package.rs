//! MYPK version 1 manifest and archive validation.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path};
use tar::{Archive, EntryType};

const MAX_MANIFEST: u64 = 1_048_576;
const MAX_CHECKSUMS: u64 = 4_194_304;
const MAX_ENTRIES: usize = 20_000;
const MAX_FILE: u64 = 134_217_728;
const MAX_TOTAL: u64 = 536_870_912;
pub const SIGNATURE_SIZE: usize = 64;

/// A package dependency and its required version constraint.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub name: String,
    pub version: String,
}

/// Required metadata for a version 1 MYPK package.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub format: String,
    pub format_version: u32,
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub runtime: String,
    pub dependencies: Vec<Dependency>,
    pub description: String,
    pub license: String,
}

impl PackageManifest {
    /// Parse and validate a UTF-8 JSON manifest.
    pub fn parse(bytes: &[u8]) -> Result<Self, PackageError> {
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err(PackageError::new("manifest exceeds 1 MiB"));
        }
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|e| PackageError::new(format!("invalid manifest JSON: {e}")))?;
        value.validate()?;
        Ok(value)
    }

    /// Validate package identity, runtime, architecture, and dependencies.
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.format != "MYPK" || self.format_version != 1 {
            return Err(PackageError::new("unsupported MYPK format identifier or version"));
        }
        package_name(&self.name)?;
        nonempty("version", &self.version)?;
        nonempty("description", &self.description)?;
        nonempty("license", &self.license)?;
        if !matches!(self.architecture.as_str(), "aarch64" | "armv7" | "x86_64") {
            return Err(PackageError::new("unsupported package architecture"));
        }
        if !matches!(self.runtime.as_str(), "bionic" | "musl" | "glibc") {
            return Err(PackageError::new("unsupported package runtime"));
        }
        let mut names = BTreeSet::new();
        for dependency in &self.dependencies {
            package_name(&dependency.name)?;
            nonempty("dependency version", &dependency.version)?;
            if dependency.name == self.name || !names.insert(&dependency.name) {
                return Err(PackageError::new("self-dependency or duplicate dependency"));
            }
        }
        Ok(())
    }
}

/// Fully parsed package data. No filesystem extraction is performed.
#[derive(Debug, Clone)]
pub struct PackageArchive {
    pub manifest: PackageManifest,
    pub payload: BTreeMap<String, Vec<u8>>,
    pub signature: [u8; SIGNATURE_SIZE],
    pub checksums: Vec<u8>,
}

impl PackageArchive {
    /// Verify the detached signature over the exact checksum-list bytes.
    pub fn verify_signature(&self, keyring: &crate::signature::TrustedKeyring) -> Result<String, crate::signature::SignatureError> {
        keyring.verify(&self.checksums, &self.signature)
    }

    /// Read a TAR archive, reject unsafe entries, and verify SHA-256 checksums.
    ///
    /// This does not authenticate the publisher. Verify the detached Ed25519
    /// signature over checksums using a trusted key before installation.
    pub fn read<R: Read>(reader: R) -> Result<Self, PackageError> {
        let mut archive = Archive::new(reader);
        let mut seen = BTreeSet::new();
        let mut manifest = None;
        let mut checksums = None;
        let mut signature = None;
        let mut payload = BTreeMap::new();
        let mut total = 0_u64;
        let mut count = 0_usize;

        for item in archive.entries().map_err(|e| PackageError::new(format!("TAR read error: {e}")))? {
            count += 1;
            if count > MAX_ENTRIES {
                return Err(PackageError::new("TAR entry-count limit exceeded"));
            }
            let mut entry = item.map_err(|e| PackageError::new(format!("invalid TAR entry: {e}")))?;
            let path = entry.path().map_err(|e| PackageError::new(format!("invalid TAR path: {e}")))?.into_owned();
            let name = path.to_str().ok_or_else(|| PackageError::new("archive path is not UTF-8"))?.to_owned();
            safe_path(&path, &name)?;
            if !seen.insert(name.clone()) {
                return Err(PackageError::new(format!("duplicate archive member: {name}")));
            }

            let kind = entry.header().entry_type();
            if kind == EntryType::Directory {
                if !name.starts_with("payload/") {
                    return Err(PackageError::new("directories are only allowed under payload/"));
                }
                continue;
            }
            if kind != EntryType::Regular {
                return Err(PackageError::new(format!("unsupported TAR entry type: {name}")));
            }

            let size = entry.header().size().map_err(|e| PackageError::new(format!("invalid TAR size: {e}")))?;
            match name.as_str() {
                "manifest.json" => {
                    if size > MAX_MANIFEST { return Err(PackageError::new("manifest exceeds 1 MiB")); }
                    manifest = Some(read_member(&mut entry, size)?);
                }
                "checksums.txt" => {
                    if size > MAX_CHECKSUMS { return Err(PackageError::new("checksums exceed 4 MiB")); }
                    checksums = Some(read_member(&mut entry, size)?);
                }
                "signature.ed25519" => {
                    if size != SIGNATURE_SIZE as u64 { return Err(PackageError::new("Ed25519 signature must be 64 bytes")); }
                    signature = Some(read_member(&mut entry, size)?);
                }
                _ if name.starts_with("payload/") => {
                    let relative = name.strip_prefix("payload/").unwrap_or_default();
                    if relative.is_empty() { return Err(PackageError::new("empty payload path")); }
                    if size > MAX_FILE { return Err(PackageError::new(format!("payload file exceeds 128 MiB: {relative}"))); }
                    total = total.checked_add(size).ok_or_else(|| PackageError::new("payload size overflow"))?;
                    if total > MAX_TOTAL { return Err(PackageError::new("total payload exceeds 512 MiB")); }
                    payload.insert(relative.to_owned(), read_member(&mut entry, size)?);
                }
                _ => return Err(PackageError::new(format!("unexpected archive member: {name}"))),
            }
        }

        let manifest_bytes = manifest.ok_or_else(|| PackageError::new("missing manifest.json"))?;
        let checksum_bytes = checksums.ok_or_else(|| PackageError::new("missing checksums.txt"))?;
        let signature_bytes = signature.ok_or_else(|| PackageError::new("missing signature.ed25519"))?;
        let manifest = PackageManifest::parse(&manifest_bytes)?;
        let mut signature = [0_u8; SIGNATURE_SIZE];
        signature.copy_from_slice(&signature_bytes);
        verify_checksums(&checksum_bytes, &manifest_bytes, &payload)?;
        Ok(Self { manifest, payload, signature, checksums: checksum_bytes })
    }
}

/// Package format validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageError(String);

impl PackageError {
    fn new(message: impl Into<String>) -> Self { Self(message.into()) }
}
impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for PackageError {}

fn nonempty(field: &str, value: &str) -> Result<(), PackageError> {
    if value.trim().is_empty() { return Err(PackageError::new(format!("{field} must not be empty"))); }
    Ok(())
}

fn package_name(value: &str) -> Result<(), PackageError> {
    let valid = !value.is_empty() && value.len() <= 128
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+._-".contains(&b));
    if !valid { return Err(PackageError::new(format!("invalid package name: {value}"))); }
    Ok(())
}

fn safe_path(path: &Path, text: &str) -> Result<(), PackageError> {
    if text.is_empty() || text.starts_with('/') || text.contains('\\') {
        return Err(PackageError::new(format!("unsafe archive path: {text}")));
    }
    let normalized = text.strip_suffix('/').unwrap_or(text);
    if normalized.is_empty() || normalized.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(PackageError::new(format!("unsafe archive path: {text}")));
    }
    for part in path.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err(PackageError::new(format!("unsafe archive path: {text}")));
        }
    }
    Ok(())
}

fn read_member<R: Read>(reader: &mut R, size: u64) -> Result<Vec<u8>, PackageError> {
    let capacity = usize::try_from(size).map_err(|_| PackageError::new("member too large for platform"))?;
    let mut bytes = Vec::with_capacity(capacity);
    reader.take(size.saturating_add(1)).read_to_end(&mut bytes)
        .map_err(|e| PackageError::new(format!("cannot read archive member: {e}")))?;
    if bytes.len() as u64 != size { return Err(PackageError::new("TAR member size mismatch")); }
    Ok(bytes)
}

fn verify_checksums(
    raw: &[u8],
    manifest: &[u8],
    payload: &BTreeMap<String, Vec<u8>>,
) -> Result<(), PackageError> {
    let text = std::str::from_utf8(raw).map_err(|_| PackageError::new("checksums.txt must be UTF-8"))?;
    let mut declared = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() { continue; }
        let (digest, path) = line.split_once("  ")
            .ok_or_else(|| PackageError::new(format!("invalid checksum line {}", index + 1)))?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PackageError::new(format!("invalid SHA-256 on line {}", index + 1)));
        }
        if path != "manifest.json" && !path.starts_with("payload/") {
            return Err(PackageError::new(format!("invalid checksum path on line {}", index + 1)));
        }
        if declared.insert(path.to_owned(), digest.to_ascii_lowercase()).is_some() {
            return Err(PackageError::new(format!("duplicate checksum path: {path}")));
        }
    }
    if declared.len() != payload.len() + 1 {
        return Err(PackageError::new("checksum list must cover exactly the manifest and all payload files"));
    }
    check_one(&declared, "manifest.json", manifest)?;
    for (path, bytes) in payload { check_one(&declared, &format!("payload/{path}"), bytes)?; }
    Ok(())
}

fn check_one(declared: &BTreeMap<String, String>, path: &str, bytes: &[u8]) -> Result<(), PackageError> {
    let expected = declared.get(path).ok_or_else(|| PackageError::new(format!("missing checksum: {path}")))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if &actual != expected { return Err(PackageError::new(format!("SHA-256 mismatch: {path}"))); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tar::{Builder, Header};

    fn manifest() -> Vec<u8> {
        br#"{"format":"MYPK","format_version":1,"name":"hello","version":"1.0.0","architecture":"aarch64","runtime":"bionic","dependencies":[],"description":"Example package","license":"BSD-3-Clause"}"#.to_vec()
    }
    fn append(builder: &mut Builder<Vec<u8>>, path: &str, bytes: &[u8]) {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, bytes).unwrap();
    }
    fn archive(path: &str, bytes: &[u8]) -> Vec<u8> {
        let meta = manifest();
        let sums = format!("{:x}  manifest.json\n{:x}  payload/{path}\n", Sha256::digest(&meta), Sha256::digest(bytes));
        let mut builder = Builder::new(Vec::new());
        append(&mut builder, "manifest.json", &meta);
        append(&mut builder, "checksums.txt", sums.as_bytes());
        append(&mut builder, "signature.ed25519", &[9; SIGNATURE_SIZE]);
        append(&mut builder, &format!("payload/{path}"), bytes);
        builder.into_inner().unwrap()
    }

    #[test] fn parses_valid_manifest() { assert_eq!(PackageManifest::parse(&manifest()).unwrap().name, "hello"); }
    #[test] fn rejects_unknown_manifest_fields() {
        let bad = String::from_utf8(manifest()).unwrap().replace("}", ",\"unknown\":true}");
        assert!(PackageManifest::parse(bad.as_bytes()).is_err());
    }
    #[test] fn rejects_unsupported_version() {
        let bad = String::from_utf8(manifest()).unwrap().replace("\"format_version\":1", "\"format_version\":2");
        assert!(PackageManifest::parse(bad.as_bytes()).is_err());
    }
    #[test] fn rejects_invalid_package_name() {
        let bad = String::from_utf8(manifest()).unwrap().replace("\"hello\"", "\"../bad\"");
        assert!(PackageManifest::parse(bad.as_bytes()).is_err());
    }
    #[test] fn reads_archive_and_checks_hashes() {
        let value = PackageArchive::read(Cursor::new(archive("bin/hello", b"hello"))).unwrap();
        assert_eq!(value.payload["bin/hello"], b"hello");
        assert_eq!(value.signature, [9; SIGNATURE_SIZE]);
    }
    #[test] fn rejects_payload_hash_mismatch() {
        let mut bytes = archive("bin/hello", b"hello");
        let pos = bytes.windows(5).position(|w| w == b"hello").unwrap();
        bytes[pos] = b'X';
        assert!(PackageArchive::read(Cursor::new(bytes)).is_err());
    }
    #[test] fn rejects_missing_required_members() {
        let mut builder = Builder::new(Vec::new());
        append(&mut builder, "manifest.json", &manifest());
        assert!(PackageArchive::read(Cursor::new(builder.into_inner().unwrap())).is_err());
    }
    #[test] fn rejects_unsafe_paths() {
        assert!(safe_path(Path::new("../escape"), "../escape").is_err());
        assert!(safe_path(Path::new("/absolute"), "/absolute").is_err());
        assert!(safe_path(Path::new("payload/../escape"), "payload/../escape").is_err());
    }
}
