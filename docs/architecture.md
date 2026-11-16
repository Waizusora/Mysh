# Architecture

## Overview

Mysh is a standalone native shell implemented primarily with the Rust standard library. Mysh itself implements tokenization, parsing, expansion, command dispatch, and shell interpretation; another shell is not used as a hidden interpreter.

The language target is the POSIX Shell Command Language. The current implementation is partial and is not POSIX-conformant. Bash, Zsh, and other shell extensions are not compatibility targets.

## Runtime flow

1. main.rs handles --help, --version, --platform, and -c. Without an option, input is read from standard input. Interactive mode is selected when stdin is a terminal.
2. Shell manages input, last status, variables, export attributes, functions, positional parameters, and loop-control state.
3. Input is classified as complete, incomplete, or invalid to support compound-command continuation.
4. The lexer produces shell words with ordered fragments and single-quoted, double-quoted, unquoted, or escaped metadata.
5. The parser builds command-list, pipeline, and typed compound-command nodes.
6. Expansion handles parameters, command substitution, arithmetic, tilde, field splitting, and pathname expansion within the implemented subset.
7. Built-ins and functions execute in the shell. External commands are resolved through PATH and launched as child processes through Rust's process API.
8. The resulting status is stored for subsequent commands and the $? parameter.

## Source modules

| Path | Responsibility |
| --- | --- |
| src/main.rs | CLI entry point and input-mode selection |
| src/lib.rs | Module declarations and public Shell API |
| src/lexer.rs | Word/operator tokenization and quote/escape metadata |
| src/parser.rs | Input classification, command lists, pipelines, and compound-command AST |
| src/shell.rs | Input loop, expansion, variables, functions, built-in dispatch, pipelines, and process execution |
| src/builtins/mod.rs | Built-ins executed inside the shell process |
| src/command.rs | Executable and shell-script lookup by explicit path or PATH |
| src/platform.rs | Hostname, C-library family, and runtime identifier |

## Shell state and command execution

- Shell variables and exported variables are tracked separately. Assignment-only commands update shell state; command-prefix assignments are temporary for that invocation.
- External children receive exported variables.
- MYSH_PREFIX/bin is prepended to PATH when MYSH_PREFIX is set and that directory exists.
- State-changing built-ins such as cd, export, and unset run in the Mysh process.
- Functions run in the current shell with temporary positional parameters; the caller's values are restored when the function returns.
- Subshells use copied shell state and restore the parent working directory after execution.
- Command substitution runs in an isolated copy of shell state and removes trailing newlines from its output.
- Executables are searched in PATH order; names containing a slash are treated as explicit paths. Command-not-found returns status 127; a found but non-executable command returns 126.
- Normal child exit codes are preserved. On Unix, termination by signal N is represented as 128 + N.

## Implemented language subset

The parser and executor currently provide initial support for:

- Command lists ;, &&, and ||, and pipeline negation with !.
- Concurrent kernel-pipe execution for pipelines made entirely of simple external commands, with concurrent draining of each child stderr and the final stdout stream. Pipelines requiring in-process built-ins, functions, compound commands, or unresolved external commands use a sequential buffered fallback.
- if/elif/else, case, for, while, until, brace groups, and subshells.
- Function definitions using name() compound-command syntax.
- Basic input, output, append, and standard-error redirection: <, >, >>, and 2>.
- Parameter expansion, including positional/special parameters, default/alternate/assignment/required-value forms, and prefix/suffix pattern removal.
- $(...) and backquoted command substitution.
- Integer arithmetic expansion with a subset of operators and assignments.
- Leading tilde expansion, IFS field splitting, and pathname expansion.

See the [Command Reference](command-reference.md) for the supported behavior and limitations of each syntax. The lexer may recognize POSIX operators that the parser or executor does not support; lexical recognition is not execution support.

## Known execution limitations

- Concurrent OS pipes are currently limited to pipelines made entirely of simple external commands. Mixed/builtin/function/compound pipelines use the buffered fallback; compound stages do not yet receive input from the preceding stage. Full POSIX pipeline process, signal, and job-control semantics remain incomplete.
- Background execution and job control are not implemented. The parser retains the & separator, but the executor rejects it.
- Here-documents, descriptor duplication/closure, >|, <>, and advanced redirections are not supported.
- Compound-command grammar is not accepted in every position permitted by POSIX.
- Parameter-operator word parsing, expansion-order interactions, quote-sensitive case patterns, locale collation, and some pathname edge cases remain incomplete.
- POSIX built-ins, trap/signal semantics, interactive line editing, persistent history, and job control remain incomplete.

## Portability

The implementation prefers Rust std; FFI is used narrowly when a platform API is required. For example, Android may deny procfs hostname reads, so platform.rs provides a libc gethostname() fallback.

| Platform | Rust target | C library |
| --- | --- | --- |
| Android ARM64 | aarch64-linux-android | Bionic |
| Android x86-64 | x86_64-linux-android | Bionic |
| Linux x86-64 | x86_64-unknown-linux-musl | musl |
| Linux ARM64 | aarch64-unknown-linux-musl | musl |

Android produces standalone ELF executables, not a JNI shared library or APK. The Android runtime archive contains bin/mysh and bin/coreutils; the consuming application is responsible for extracting it into its private directory and setting MYSH_PREFIX.

## External utilities

Mysh implements the shell language and shell built-ins. Utility commands are external executables. Android CI builds pinned uutils Coreutils with a selected feature set; utilities outside that set are not automatically available. Mysh does not duplicate utility commands as shell built-ins.

## Wicofe

Wicofe is a planned integration target and has not been implemented or validated. A port requires a compatible ABI, process model, filesystem, terminal/PTY, toolchain, and Rust userspace runtime. Cross-repository contracts are tracked in [Wicofe integration checklist](WICOFE_INTEGRATION.md).
