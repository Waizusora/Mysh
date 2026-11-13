# Architecture

## Overview

Mysh is a standalone native command-line shell implemented in Rust. Its implementation is independent of Bash, Zsh, BusyBox, and other shell interpreters.

The initial architecture is intentionally small. It separates input tokenization, command dispatch, built-in operations, and external process execution so that later shell-language features can be added without coupling them to filesystem commands. The long-term compatibility baseline is the POSIX shell command language, followed by a dedicated FreeBSD port; platform-specific behavior should remain isolated from the portable shell core.

## Compatibility Contract

Mysh targets the behavior specified for a POSIX-conforming `sh`, rather than cloning a particular shell implementation.

- The shell lexer, parser, expansion phases, execution semantics, and built-ins belong to Mysh and are implemented in Rust.
- Mysh interprets scripts itself. It must not invoke the host's `/bin/sh` as a hidden interpreter or require Bash, Zsh, or BusyBox to parse or execute shell language.
- The operating system remains responsible for process and filesystem services. External commands are separate executables resolved through the environment and platform rules.
- POSIX shell-language compatibility is a different scope from bundling every POSIX command-line utility. Utility coverage must be specified and tested separately.
- `dash`, BusyBox `ash`, and FreeBSD `sh` are useful independent comparison implementations for compatibility testing; none is a runtime dependency or the normative specification.

The current implementation is not yet POSIX-conformant. Conformance must be established through implementation work and systematic tests, not inferred from the use of familiar command names or syntax.

## Runtime Flow

1. The entry point selects interactive, standard-input, or `-c` command-string mode.
2. The shell reads one command line.
3. The lexer creates shell words with ordered fragments and retains whether each fragment came from unquoted, single-quoted, double-quoted, or escaped input.
4. The parser identifies unquoted operators. Quoted or escaped operator-looking text remains part of an ordinary word.
5. The expansion phase currently performs only basic environment-variable substitution on expandable fragments. Single-quoted and escaped fragments are preserved literally.
6. The dispatcher checks whether the command is a built-in.
7. Built-ins execute in the current process. This is required for stateful operations such as `cd`.
8. Non-built-in commands are launched through Rust's process API. The inherited environment is used for executable lookup.
9. Normal child exit codes are preserved. On Unix-like targets, signal termination is represented as 128 plus the signal number.
10. The shell records the resulting exit status.

## Modules

| Module | Responsibility |
| --- | --- |
| `main.rs` | Process entry point and command-line options |
| `lib.rs` | Public library module declarations |
| `lexer.rs` | Shell-word tokenization and quote/escape metadata |
| `shell.rs` | Input loop, preliminary parsing, expansion, command dispatch, history, and status |
| `builtins/mod.rs` | Built-in command implementations and filesystem safety checks |

## Portability Strategy

The shell language and shared command behavior should follow POSIX where applicable. Implementations must distinguish portable semantics from operating-system-specific facilities such as process creation, signal handling, terminal control, and filesystem behavior. Keep those facilities behind narrow platform abstractions so that Android and Linux remain supported while a later FreeBSD port can supply its own integration.

The committed BSD target is FreeBSD only. Support for OpenBSD or NetBSD is not currently planned. POSIX is the compatibility target for the shell core; it does not imply that every POSIX utility is bundled or that full conformance has already been achieved.

## Android Targets

| ABI | Rust target triple |
| --- | --- |
| ARMv8-A / arm64-v8a | `aarch64-linux-android` |
| ARMv7-A / armeabi-v7a | `armv7-linux-androideabi` |
| x86-64 / x86_64 | `x86_64-linux-android` |

Android executables are linked directly as standalone ELF binaries against Android's Bionic C library. CI uses the NDK LLVM toolchain's API-level Clang linker wrappers and Cargo's Rust Android target configuration. The build does not produce an Android shared library, JNI module, or APK.

## Current Language Boundary

The lexer currently supports unquoted words, single quotes, double quotes, backslash escaping, adjacent quoted and unquoted fragments within one word, empty quoted words, and comments beginning with an unquoted `#` at a token boundary. It retains fragment metadata so later phases can distinguish expandable text from literal text.

Basic parameter substitution for environment-variable names is now performed only in unquoted and double-quoted fragments. Single-quoted and backslash-escaped text is not expanded. This is an improvement to quoting semantics, not a complete POSIX expansion implementation.

The current implementation includes sequential, buffered pipelines and basic `<`, `>`, `>>`, and `2>` redirection. These supported operators are tokenized even when adjacent to words (for example, `echo hi|cat` or `echo hi>file`). The lexer does not yet recognize the complete POSIX operator set or shell grammar. Pipelines are not concurrent. POSIX parameter expansion forms, field splitting, pathname expansion, command substitution, arithmetic expansion, control-flow syntax, here-documents, and job control are not implemented.

## Process and Filesystem Model

Built-ins execute in the Mysh process. External commands execute as child processes and inherit the current working directory and environment. Pipeline stages currently exchange buffered byte vectors sequentially rather than connected concurrent pipes; standard input is passed to external commands and the `cat` built-in when a prior stage or input redirection supplies data.

Filesystem built-ins validate operand counts and supported options before performing mutations. Recursive copy rejects symbolic links and special filesystem entries instead of following them implicitly, and prevents a destination from being created inside the source tree. Recursive removal refuses the current working directory, its ancestors, and filesystem roots. Multi-source copy and move operations require an existing destination directory. Move operations use the platform rename primitive and do not emulate cross-filesystem copies.

Mysh does not elevate privileges. Android sandboxing, SELinux policy, filesystem permissions, executable mapping restrictions, and linker compatibility remain enforced by the operating system.
