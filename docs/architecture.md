# Architecture

## Overview

Mysh is a standalone native command-line shell implemented in Rust. Its implementation is independent of Bash, Zsh, BusyBox, and other shell interpreters.

The initial architecture is intentionally small. It separates input tokenization, command dispatch, built-in operations, and external process execution so that later shell-language features can be added without coupling them to filesystem commands.

## Runtime Flow

1. The entry point selects interactive, standard-input, or `-c` command-string mode.
2. The shell reads one command line.
3. The lexer converts the line into arguments while applying quote and escape rules.
4. The dispatcher checks whether the command is a built-in.
5. Built-ins execute in the current process. This is required for stateful operations such as `cd`.
6. Non-built-in commands are launched through Rust's process API and resolved using the inherited `PATH`.
7. The shell records the resulting exit status.

## Modules

| Module | Responsibility |
| --- | --- |
| `main.rs` | Process entry point and command-line options |
| `lib.rs` | Public library module declarations |
| `lexer.rs` | Quote-aware command-line tokenization |
| `shell.rs` | Input loop, command dispatch, history, and status |
| `builtins/mod.rs` | Built-in command implementations |

## Android Targets

| ABI | Rust target triple |
| --- | --- |
| ARMv8-A / arm64-v8a | `aarch64-linux-android` |
| ARMv7-A / armeabi-v7a | `armv7-linux-androideabi` |
| x86-64 / x86_64 | `x86_64-linux-android` |

Android executables are linked directly as standalone ELF binaries against Android's Bionic C library. CI uses the NDK LLVM toolchain's API-level Clang linker wrappers and Cargo's Rust Android target configuration. The build does not produce an Android shared library, JNI module, or APK.

## Current Language Boundary

Milestone 1 supports command names, whitespace-separated arguments, single-quoted strings, double-quoted strings, backslash escaping, and comments beginning with an unquoted `#` at the start of a token.

Pipelines, redirections, command substitution, variable expansion, glob expansion, control-flow syntax, and job control are intentionally outside the current implementation.

## Process and Filesystem Model

Built-ins execute in the Mysh process. External commands execute as child processes and inherit the current working directory, environment, and standard file descriptors.

Mysh does not elevate privileges. Android sandboxing, SELinux policy, filesystem permissions, executable mapping restrictions, and linker compatibility remain enforced by the operating system.
