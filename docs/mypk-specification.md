# MYPK — My Package Keeper

## Identity and Scope

MYPK (My Package Keeper) is the independent package-management system for the Mysh environment. It is implemented as a separate Rust crate and produces a separate executable named mypk. It is not a wrapper around APT, APK, DNF, or another package manager.

- Executable: mypk
- Package extension: .mypk
- Initial package format version: 1
- Implementation language: Rust
- Installation prefix: /data/data/com.waizusora.terminal/files/

Mysh interprets commands and scripts. MYPK manages package repositories, metadata, archives, dependencies, installed files, and package transactions.

## Command Interface

All command names are lowercase.

| Canonical command | Purpose | Compatibility alias |
| --- | --- | --- |
| mypk sync | Synchronize repository indexes | mypk update |
| mypk upgrade | Upgrade installed packages | — |
| mypk find | Search package metadata | mypk search |
| mypk install | Install packages and dependencies | — |
| mypk delete | Delete installed packages | mypk remove |
| mypk reinstall | Reinstall an installed package | — |
| mypk list | List installed packages | — |
| mypk info | Display package metadata and installation state | — |
| mypk clean | Remove cached archives and temporary files | — |
| mypk clear | Remove unused automatically installed dependencies | — |

Aliases resolve to the same internal handler as their canonical command. Canonical names are used in documentation and help output.

### clean and clear

clean removes package-manager cache data and abandoned temporary files. It must not remove installed package files or installed-package records.

clear implements autoremove behavior. It identifies packages marked as automatically installed that are no longer required by any installed package. It must preserve packages explicitly installed by the user and protected base packages. The initial implementation must display a removal plan and require confirmation before changing the installed system.

## Runtime and ABI Model

Every package must declare its CPU architecture and userspace runtime ABI. Matching CPU architecture alone is insufficient.

Recognized runtime identifiers:

- bionic: Android native userspace.
- musl: Linux userspace linked against musl.
- glibc: Linux userspace linked against GNU libc.

The installer must reject packages whose architecture, runtime ABI, or declared dependencies are not supported by the active environment. Listing an identifier in the format does not mean that runtime is already implemented or available.

The MYPK executable must be built for the same Android ABI matrix as Mysh: ARM64 (aarch64-linux-android), ARMv7 (armv7-linux-androideabi), and x86-64 (x86_64-linux-android).

## Package Format Version 1

A .mypk file is a TAR archive with a MYPK-defined member layout. The initial layout contains:

- manifest.json: UTF-8 package metadata.
- checksums.txt: SHA-256 checksums for the manifest and every payload member.
- signature.ed25519: detached Ed25519 signature over the exact bytes of checksums.txt.
- payload/<relative-path>: files installed beneath the configured prefix.

The archive is a transport container, not permission to extract arbitrary paths. The installer must validate every member before writing any file. Absolute paths, parent-directory traversal, duplicate paths, unsupported entry types, and paths escaping the installation prefix must be rejected. Symbolic-link and hard-link entries require explicit policy and validation; extraction must never follow an untrusted link.

The signature must be checked against a trusted repository key before installation. A checksum detects content changes only when the checksum source is trusted; it does not replace signature verification. Unknown signing keys and invalid signatures must fail closed.

### Manifest fields

The version 1 manifest is JSON and must include:

| Field | Meaning |
| --- | --- |
| format | Literal identifier MYPK |
| format_version | Integer package format version |
| name | Canonical package name |
| version | Package version |
| architecture | Target CPU architecture |
| runtime | Required userspace runtime ABI |
| dependencies | Dependency names and version constraints |
| description | Short package description |
| license | Package license identifier |

The manifest schema is versioned. Unknown required fields and unsupported format versions must be rejected rather than guessed.

## Repository Design

A repository publishes a signed package index and package archives. The index contains package names, versions, architectures, runtime identifiers, dependency constraints, archive sizes, checksums, and signing metadata. Index signatures are verified against configured trusted keys before package selection.

Repository configuration:
etc/mypk/repositories.conf

Trusted repository keys:
etc/mypk/keys/

Repository URLs must use authenticated TLS. Redirects, archive sizes, index sizes, and decompressed data sizes must be bounded to reduce downgrade and resource-exhaustion risks.

## Filesystem Layout

All paths are relative to the configured Mysh prefix:

    bin/mypk
    etc/mypk/config
    etc/mypk/repositories.conf
    etc/mypk/keys/
    lib/mypk/
    var/cache/mypk/packages/
    var/lib/mypk/installed.db
    var/lib/mypk/package-files.db
    var/lib/mypk/history.db
    share/mypk/

MYPK must not require root privileges or bypass Android application sandbox restrictions.

## Installed Package State

The installed-package database must record package name, version, architecture, runtime ABI, installation reason, and installation state. Installation reason distinguishes:

- explicit: requested directly by the user.
- automatic: installed only to satisfy another package dependency.

A file ownership index records which package owns each installed path. Conflicting file ownership must be rejected unless a future replacement policy explicitly permits it.

The database format is an implementation detail and must be versioned with a migration strategy. A partially written database must never be treated as a completed transaction.

## Dependency Resolution

Dependencies are resolved before modifying the installed prefix. The resolver must:

1. Select candidates compatible with the active architecture and runtime.
2. Enforce declared version constraints.
3. Detect missing dependencies and dependency cycles.
4. Detect file ownership conflicts.
5. Produce a complete install, upgrade, delete, or autoremove plan.
6. Refuse to proceed when the plan cannot be satisfied safely.

Packages explicitly requested by the user are marked explicit. Dependencies added only to satisfy the plan are marked automatic. An automatic package later requested directly must be promoted to explicit.

## Transaction and Recovery Requirements

Package operations must be planned and validated before installation or deletion begins. Downloads and extraction occur in a staging area. Every archive is verified before its payload is applied. The manager records enough transaction state to detect and recover from an interrupted operation.

The implementation must not report success until filesystem changes and installed-package state are committed consistently. On failure, it must restore the previous state where possible and report any recovery action that could not be completed. Package scripts are excluded from format version 1; packages must not execute arbitrary installation or removal hooks.

## Security Requirements

- Verify repository metadata and package signatures before installation.
- Verify all SHA-256 checksums before applying payload files.
- Reject path traversal, absolute paths, duplicate archive members, and unsafe archive entry types.
- Enforce archive, file-count, and expanded-size limits.
- Avoid following symbolic links while validating or writing package paths.
- Prevent untrusted packages from overwriting MYPK state, trust keys, or protected files.
- Keep package operations within the configured prefix.
- Never run package payloads or scripts during installation.
- Preserve explicitly installed packages during autoremove.

## Implementation Status

The initial Rust crate provides a separate executable, canonical command parsing, compatibility aliases, help/version output, and unit tests for command resolution. Package operations are not functional until repository access, archive parsing, verification, dependency resolution, transactions, and installed-state management are implemented.
