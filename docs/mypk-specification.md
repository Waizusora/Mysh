# MYPK — My Package Keeper

## Identity and Scope

MYPK is the independent Rust package manager for Mysh. It produces a separate mypk executable and uses the native .mypk package format. It is not a wrapper around APT, APK, DNF, or another package manager.

- Executable: mypk
- Package extension: .mypk
- Package format: version 1
- Installation prefix: /data/data/com.waizusora.terminal/files/

## Command Interface

| Canonical command | Purpose | Alias |
| --- | --- | --- |
| mypk sync | Synchronize repository indexes | mypk update |
| mypk upgrade | Upgrade installed packages | — |
| mypk find | Search package metadata | mypk search |
| mypk install | Install packages and dependencies | — |
| mypk delete | Delete installed packages | mypk remove |
| mypk reinstall | Reinstall a package | — |
| mypk list | List installed packages | — |
| mypk info | Display package metadata and state | — |
| mypk clean | Remove cached archives and temporary files | — |
| mypk clear | Remove unused automatically installed dependencies | — |

clean removes cache data only. clear implements autoremove, preserving explicitly installed and protected base packages. It must show a removal plan and require confirmation.

## Runtime and ABI

Every package declares its CPU architecture and userspace runtime. Supported identifiers are aarch64, armv7, x86_64 and bionic, musl, glibc. The installer must reject incompatible architecture or runtime combinations. Listing a runtime identifier does not mean that runtime is already installed.

## Package Format Version 1

A .mypk file is a TAR archive containing:

- manifest.json: UTF-8 package metadata.
- checksums.txt: SHA-256 checksums for the manifest and every regular payload file.
- signature.ed25519: detached 64-byte Ed25519 signature over the exact checksums.txt bytes.
- payload/<relative-path>: package files installed beneath the configured prefix.

Only regular files and directories under payload/ are accepted. Symbolic links, hard links, devices, FIFOs, sparse files, and other TAR entry types are rejected. Absolute paths, traversal components, duplicate paths, backslashes, and members outside the defined layout are rejected. The reader does not extract files to disk.

### Manifest schema

The version 1 JSON manifest has exactly these fields:

| Field | Type |
| --- | --- |
| format | string; literal MYPK |
| format_version | integer; 1 |
| name | lowercase package identifier |
| version | string |
| architecture | aarch64, armv7, or x86_64 |
| runtime | bionic, musl, or glibc |
| dependencies | array of name and version objects |
| description | string |
| license | string |

Unknown fields are rejected. Package names are limited to 128 lowercase ASCII letters, digits, plus signs, periods, underscores, and hyphens, beginning with a lowercase letter. Self-dependencies and duplicate dependencies are rejected.

### Checksums

Each non-empty checksums.txt line has this format:

    <64-character SHA-256 hex digest><two spaces><archive-relative path>

Paths are manifest.json and payload/<relative-path>. Every manifest and payload file must have exactly one checksum. Missing, duplicate, malformed, or extra entries are rejected. The signature member is excluded from the checksum list.

### Trusted publisher keys

Trusted publisher public keys are stored as UTF-8 text files beneath `etc/mypk/keys/`. Each non-comment line has exactly two whitespace-separated fields: a key ID (1–64 ASCII letters, digits, periods, underscores, or hyphens) and a 32-byte Ed25519 public key encoded as 64 hexadecimal characters. Key IDs must be unique within a keyring. Private signing keys must never be distributed with packages or stored in the client trusted-key directory. MYPK loads only regular `.pub` files, rejects symbolic links, limits each key file to 64 KiB and the directory to 256 key files, and rejects duplicate key IDs across files. `TrustedKeyring::load_dir` implements this bounded loading. An empty directory is not a trust configuration and all signature checks fail.

The package signature is the standard Ed25519 signature over the exact byte sequence in `checksums.txt` (PureEdDSA Ed25519 as specified by RFC 8032, not Ed25519ph). The verifier tries the configured trusted keys and accepts only a strict cryptographic verification match. An empty keyring, unknown signer, malformed key, malformed signature, or modified checksum list is a verification failure. The matching key ID is returned for audit/logging. Key rotation is performed by distributing the replacement public key through an already trusted channel, verifying its fingerprint out of band, adding it before removing the retired key, and publishing a transition signed by the currently trusted key. The Mysh release operator owns the official signing key; independent repository operators own separate keys. End users do not generate signing keys merely to install packages.

### Resource limits

- Manifest: 1 MiB.
- Checksum list: 4 MiB.
- TAR entries: 20,000.
- Individual payload file: 128 MiB.
- Total payload: 512 MiB.

## Signature Verification Boundary

The archive reader validates signature length and SHA-256 checksums, and exposes `PackageArchive::verify_signature` to authenticate the exact checksum-list bytes against a `TrustedKeyring`. Package installation must call this method successfully before applying payload files. The current CLI does not yet perform package installation.


## Repository Trust Framework Version 1

Repository trust metadata is verified in two stages. The initial root public key is provisioned by an authenticated bootstrap process and is never learned from an unauthenticated repository response.

### Root metadata

The root document is UTF-8 JSON with exactly these fields:

| Field | Type |
| --- | --- |
| format | `MYPK-ROOT` |
| format_version | integer; 1 |
| repository_id | 1–128 ASCII letters, digits, periods, underscores, or hyphens |
| version | monotonically increasing unsigned integer |
| issued_at | Unix timestamp in seconds |
| expires_at | Unix timestamp in seconds; exclusive upper bound |
| keys | array of repository key records |
| revoked_key_ids | array of revoked key IDs |

Each repository key record contains exactly `key_id`, `public_key` (64 hexadecimal characters), `not_before`, and `expires_at`. Key validity uses the half-open interval `[not_before, expires_at)`. A key listed in `revoked_key_ids` is excluded regardless of its validity interval. Duplicate key IDs, duplicate revocations, unknown JSON fields, invalid time windows, empty active key sets, and oversized metadata are rejected.

The root signature is a detached text record containing the root signer key ID, one space, and a 128-character hexadecimal Ed25519 signature. The signed message is the exact root JSON byte sequence prefixed by the domain separator `MYSH-MYPK-ROOT-METADATA-V1\0`. The root signature must verify against the locally bootstrapped root keyring.

### Repository index metadata

The repository index metadata JSON has exactly these fields: `format` (`MYPK-INDEX`), `format_version` (1), `repository_id`, `version`, `issued_at`, `expires_at`, `package_index_sha256`, and `package_index_size`. The repository ID must match the verified root metadata. Its detached signature uses the same text encoding and signs the exact metadata bytes prefixed by `MYSH-MYPK-REPOSITORY-INDEX-V1\0`. The signer must be an active, non-revoked repository key authorized by the verified root metadata.

The referenced package-index bytes must match both the declared byte size and SHA-256 digest. Root metadata and repository index metadata are bounded to 1 MiB. Clients persist the highest accepted root and index versions and pass those values as minimum versions to reject rollback. A client must not reset these version floors as part of ordinary cache cleanup.

### Expiration, revocation, and rotation

Expiration is enforced by the client using the current Unix time. Root metadata, repository index metadata, and repository signing keys are rejected at or after their exclusive expiration timestamp. A key is revoked by publishing its ID in a newer root document signed by an already trusted root key. A revoked key is rejected immediately after the client successfully verifies and accepts that root document. Offline clients cannot learn about a newly published revocation until they refresh trusted metadata; short metadata lifetimes bound this exposure.

Routine repository-key rotation is performed by adding a new public key to root metadata, accepting it only after the root signature is verified, and then removing or revoking the old key in a later root version. The root private key should be kept offline. Root-key rotation itself requires an authenticated root-update procedure and is not implemented by this version. Private keys are publisher-side secrets and must never be distributed to clients or included in repository metadata.

### Implementation boundary

Implemented in the Rust library: strict root and index metadata parsing, exact-byte Ed25519 verification with domain separation, repository identity binding, key validity windows, revocation filtering, expiry checks, package-index size and SHA-256 validation, and caller-supplied minimum-version rollback checks.

Not yet implemented: network synchronization, persistent version floors, secure clock handling, root-key rotation, threshold signatures, and integration into the `mypk sync` or `mypk install` command paths. Until those operations are integrated, this library API is a verification primitive and does not by itself make repository synchronization secure.

## Repository and State

Repository configuration is stored at etc/mypk/repositories.conf and trusted keys at etc/mypk/keys/. Repository indexes must be signed and verified before package selection. Repository downloads require authenticated TLS and bounded redirects, sizes, and decompression.

Installed state will record package version, architecture, runtime, installation reason, and owned files. Explicit user packages must be distinguished from automatically installed dependencies. Database writes and package file changes must be transactional and recoverable.

## Security Requirements

- Verify repository metadata and package signatures before installation.
- Verify SHA-256 checksums before applying payload files.
- Reject traversal, absolute paths, duplicate members, and unsafe TAR entry types.
- Enforce archive, file-count, and expanded-size limits.
- Keep operations within the configured prefix.
- Never execute package scripts in format version 1.
- Preserve explicitly installed packages during autoremove.

## Implementation Status

Implemented: strict JSON manifest parsing, metadata validation, bounded TAR reading, path and entry validation, required-member validation, SHA-256 verification, Ed25519 trusted-key parsing, bounded public-key directory loading, strict signature verification, and unit tests.

Not implemented: repository indexes, dependency resolution, installed-state database, transaction engine, and filesystem installation.
