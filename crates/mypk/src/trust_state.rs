//! Persistent anti-rollback state for verified repository metadata.

use crate::trust::{VerifiedRepositoryIndex, VerifiedRootMetadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_STATE_SIZE: u64 = 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustStateError(String);

impl std::fmt::Display for TrustStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TrustStateError {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustState {
    format: String,
    format_version: u32,
    repositories: BTreeMap<String, RepositoryVersions>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryVersions {
    root: Option<AcceptedMetadata>,
    index: Option<AcceptedMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedMetadata {
    version: u64,
    sha256: String,
}

impl TrustState {
    /// Load state from a regular file. A missing file creates an empty state.
    pub fn load(path: &Path) -> Result<Self, TrustStateError> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default_state());
            }
            Err(error) => return Err(TrustStateError(format!("cannot inspect trust state: {error}"))),
        };
        if !metadata.file_type().is_file() {
            return Err(TrustStateError("trust state must be a regular file".into()));
        }
        if metadata.len() > MAX_STATE_SIZE {
            return Err(TrustStateError("trust state exceeds 1 MiB".into()));
        }
        let bytes = fs::read(path)
            .map_err(|error| TrustStateError(format!("cannot read trust state: {error}")))?;
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|error| TrustStateError(format!("invalid trust state: {error}")))?;
        if state.format != "MYPK-TRUST-STATE" || state.format_version != 1 {
            return Err(TrustStateError("unsupported trust-state format".into()));
        }
        Ok(state)
    }

    /// Return the minimum root metadata version required for a repository.
    pub fn minimum_root_version(&self, repository_id: &str) -> u64 {
        self.repositories
            .get(repository_id)
            .and_then(|versions| versions.root.as_ref())
            .map_or(0, |accepted| accepted.version)
    }

    /// Return the minimum repository index version required for a repository.
    pub fn minimum_index_version(&self, repository_id: &str) -> u64 {
        self.repositories
            .get(repository_id)
            .and_then(|versions| versions.index.as_ref())
            .map_or(0, |accepted| accepted.version)
    }

    /// Persist a root document only after its signature and policy checks succeeded.
    ///
    /// A lower version is rejected. Reusing a version with different bytes is also
    /// rejected to detect signed metadata equivocation.
    pub fn accept_root(
        &mut self,
        verified: &VerifiedRootMetadata,
        metadata_bytes: &[u8],
        path: &Path,
    ) -> Result<(), TrustStateError> {
        let mut candidate = self.clone();
        let versions = candidate.repositories.entry(verified.repository_id.clone()).or_default();
        let accepted = accepted_metadata(verified.version, metadata_bytes);
        check_advance(versions.root.as_ref(), &accepted, "root")?;
        versions.root = Some(accepted);
        candidate.persist(path)?;
        *self = candidate;
        Ok(())
    }

    /// Persist a repository index only after signature and digest verification.
    pub fn accept_index(
        &mut self,
        verified: &VerifiedRepositoryIndex,
        metadata_bytes: &[u8],
        path: &Path,
    ) -> Result<(), TrustStateError> {
        let mut candidate = self.clone();
        let versions = candidate.repositories.entry(verified.repository_id.clone()).or_default();
        let accepted = accepted_metadata(verified.version, metadata_bytes);
        check_advance(versions.index.as_ref(), &accepted, "repository index")?;
        versions.index = Some(accepted);
        candidate.persist(path)?;
        *self = candidate;
        Ok(())
    }

    /// Atomically write the state file with restrictive permissions.
    pub fn persist(&self, path: &Path) -> Result<(), TrustStateError> {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path.file_name()
            .ok_or_else(|| TrustStateError("trust-state path has no file name".into()))?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(TrustStateError("trust state must be a regular file".into()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(TrustStateError(format!("cannot inspect trust state: {error}"))),
        }

        let mut state = self.clone();
        state.format = "MYPK-TRUST-STATE".into();
        state.format_version = 1;
        let bytes = serde_json::to_vec(&state)
            .map_err(|error| TrustStateError(format!("cannot encode trust state: {error}")))?;
        if bytes.len() as u64 > MAX_STATE_SIZE {
            return Err(TrustStateError("trust state exceeds 1 MiB".into()));
        }

        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = file_name.to_os_string();
        temporary_name.push(format!(".tmp.{}.{}", std::process::id(), sequence));
        let temporary_path: PathBuf = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary_path)
            .map_err(|error| TrustStateError(format!("cannot create temporary trust state: {error}")))?;
        let write_result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary_path, path)?;
            if let Ok(directory) = fs::File::open(parent) {
                directory.sync_all()?;
            }
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(TrustStateError(format!("cannot persist trust state: {error}")));
        }
        Ok(())
    }

    fn default_state() -> Self {
        Self {
            format: "MYPK-TRUST-STATE".into(),
            format_version: 1,
            repositories: BTreeMap::new(),
        }
    }
}

fn accepted_metadata(version: u64, bytes: &[u8]) -> AcceptedMetadata {
    let digest = Sha256::digest(bytes);
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    AcceptedMetadata { version, sha256 }
}

fn check_advance(
    previous: Option<&AcceptedMetadata>,
    next: &AcceptedMetadata,
    label: &str,
) -> Result<(), TrustStateError> {
    if let Some(previous) = previous {
        if next.version < previous.version {
            return Err(TrustStateError(format!("{label} rollback detected")));
        }
        if next.version == previous.version && next.sha256 != previous.sha256 {
            return Err(TrustStateError(format!("{label} version equivocation detected")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_path() -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("mypk-trust-state-{}-{nonce}.json", std::process::id()))
    }

    #[test]
    fn persists_and_reloads_version_floors() {
        let path = temporary_path();
        let mut state = TrustState::default_state();
        let mut candidate = state.clone();
        candidate.repositories.insert(
            "official".into(),
            RepositoryVersions {
                root: Some(accepted_metadata(4, b"root-v4")),
                index: Some(accepted_metadata(9, b"index-v9")),
            },
        );
        candidate.persist(&path).unwrap();
        state = TrustState::load(&path).unwrap();
        assert_eq!(state.minimum_root_version("official"), 4);
        assert_eq!(state.minimum_index_version("official"), 9);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_rollback_and_same_version_equivocation() {
        let previous = accepted_metadata(5, b"accepted");
        assert!(check_advance(Some(&previous), &accepted_metadata(4, b"older"), "index").is_err());
        assert!(check_advance(Some(&previous), &accepted_metadata(5, b"different"), "index").is_err());
        assert!(check_advance(Some(&previous), &accepted_metadata(6, b"newer"), "index").is_ok());
        assert!(check_advance(Some(&previous), &previous, "index").is_ok());
    }

    #[test]
    fn rejects_symlink_state_file() {
        let path = temporary_path();
        let target = temporary_path();
        fs::write(&target, b"{}").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert!(TrustState::load(&path).is_err());
            assert!(TrustState::default_state().persist(&path).is_err());
            fs::remove_file(&path).unwrap();
        }
        fs::remove_file(target).unwrap();
    }
}
