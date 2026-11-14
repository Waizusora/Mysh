# Architecture

## Overview

Mysh is a standalone native command-line shell implemented in Rust. Its implementation is independent of Bash, Zsh, BusyBox, and other shell interpreters.

The initial architecture is intentionally small. It separates input tokenization, command dispatch, built-in operations, and external process execution so that later shell-language features can be added without coupling them to filesystem commands. The long-term compatibility baseline is the POSIX shell command language, followed by a dedicated FreeBSD port; platform-specific behavior should remain isolated from the portable shell core.

## Development Roadmap

The maturity phases, current phase status, scope, and completion criteria are maintained in [the development roadmap](roadmap.md). The roadmap separates shell-language conformance from external utility coverage and platform expansion. A phase is complete only after its behavior is implemented, documented, and covered by automated tests.

## MYPK Component Boundary

MYPK (My Package Keeper) is a separate Rust crate and executable in the same Cargo workspace. It has its own command-line entry point and package-management responsibilities. It does not implement shell parsing and does not depend on Mysh's command interpreter. Mysh can launch mypk through normal external-command lookup when the executable is installed in PATH.

MYPK owns repository synchronization, package metadata, archive parsing, signature and checksum verification, dependency resolution, installed-file ownership, package transactions, and package history. Its detailed contract is maintained in [the MYPK specification](mypk-specification.md).

## Compatibility Contract

Mysh targets the behavior specified for a POSIX-conforming `sh`, rather than cloning a particular shell implementation.

- The shell lexer, parser, expansion phases, execution semantics, and built-ins belong to Mysh and are implemented in Rust.
- Mysh interprets scripts itself. It must not invoke the host's `/bin/sh` as a hidden interpreter or require Bash, Zsh, or BusyBox to parse or execute shell language.
- The operating system remains responsible for process and filesystem services. External commands are separate executables resolved through the environment and platform rules.
- POSIX shell-language compatibility is a different scope from bundling every POSIX command-line utility. Utility coverage must be specified and tested separately.
- `dash`, BusyBox `ash`, and FreeBSD `sh` are useful independent comparison implementations for compatibility testing; none is a runtime dependency or the normative specification.

The current implementation is not yet POSIX-conformant. Conformance must be established through implementation work and systematic tests, not inferred from the use of familiar command names or syntax. The implemented special built-ins currently include `export`, `unset`, and `exit`; assignments preceding these commands persist in the current shell environment. This is only a partial implementation of POSIX special-builtin semantics.

## Runtime Flow

1. The entry point selects interactive, standard-input, or `-c` command-string mode.
2. The shell reads input lines and classifies buffered input as complete, incomplete, or invalid. Open quotes, unmatched `if`/loop compounds, open brace groups, and open subshells request continuation; unmatched closing compound delimiters and invalid reserved-word ordering are rejected. Interactive continuation input uses a secondary prompt.
3. The lexer creates shell words with ordered fragments and retains whether each fragment came from unquoted, single-quoted, double-quoted, or escaped input. It recognizes the additional POSIX operator spellings for compound commands, case terminators, and descriptor redirections; the parser rejects operator forms whose grammar or execution semantics are not implemented. The parser helper recognizes reserved words only at command boundaries, including immediately after `{` and `(` openers, so argument text such as `echo if` does not alter compound-command structure.
4. The parser identifies unquoted command-list and pipeline operators and produces a typed command-list/pipeline representation whose stages are either simple commands or supported brace-group/subshell nodes; redirection parsing remains in the shell execution layer. Quoted or escaped operator-looking text remains part of an ordinary word. Sequential command lists and left-associative `&&` / `||` short-circuit lists are supported. The `if` evaluator supports `if`, `then`, repeated `elif` clauses, optional `else`, and `fi`. The parser builds typed `IfStatement`, `LoopStatement`, and `ForStatement` nodes, plus recursive brace-group and subshell nodes. Their bodies use a recursive `CompoundList` containing `Simple`, `If`, `Loop`, `For`, `BraceGroup`, or `Subshell` command nodes. Brace groups execute in the current shell environment. Subshell execution copies shell variables and export attributes, resets loop-control context, executes the nested list, restores the parent working directory, and propagates the child status without propagating child variable changes or an `exit` request to the parent. Parenthesis and brace-group delimiters are classified incrementally, including nested brace groups; quoted brace characters do not affect delimiter depth. Conditions remain simple command lists. This remains a partial compound-command grammar: compound commands are not yet accepted in every POSIX grammar position.
5. The expansion phase currently performs only basic environment-variable substitution on expandable fragments. Single-quoted and escaped fragments are preserved literally.
6. The dispatcher checks whether the command is a built-in.
7. Built-ins execute in the current process. This is required for stateful operations such as `cd`, `export`, and `unset`.
8. Mysh maintains separate shell-variable and exported-variable state initialized from the process environment. Parameter substitution and `PATH` lookup use shell variables; external children receive only exported values. Assignment-only commands persist shell variables, while command-prefix assignments are temporary and exported for that command. If `MYSH_PREFIX` is set and its `bin/` directory exists, that directory is prepended to the shell's `PATH`.
9. Non-built-in commands are resolved by Mysh: names without `/` are searched in the shell process's `PATH` entries in order, while commands containing `/` are treated as explicit paths. Empty `PATH` components refer to the current directory. The selected executable is launched through Rust's process API; Mysh does not invoke another shell.
10. Normal child exit codes are preserved. On Unix-like targets, signal termination is represented as 128 plus the signal number.
11. The shell records the resulting exit status.

## Modules

| Module | Responsibility |
| --- | --- |
| `main.rs` | Process entry point and command-line options |
| `lib.rs` | Public library module declarations |
| `lexer.rs` | Shell-word tokenization and quote/escape metadata |
| `parser.rs` | Incremental input classification, reserved-word recognition, command-list, pipeline, brace-group, subshell, `if`, `while`/`until`, and `for` AST construction, and nested compound-boundary matching |
| `shell.rs` | Input loop, preliminary parsing, expansion, command dispatch, history, and status |
| `builtins/mod.rs` | Built-in command implementations and filesystem safety checks |
| `command.rs` | Explicit external-command resolution and `PATH` search |
| `builtins/mod.rs` | Built-in commands, including environment mutation |

## Portability Strategy

The shell language and shared command behavior should follow POSIX where applicable. Implementations must distinguish portable semantics from operating-system-specific facilities such as process creation, signal handling, terminal control, and filesystem behavior. Keep those facilities behind narrow platform abstractions so that Android and Linux remain supported while a later FreeBSD port can supply its own integration.

The committed BSD target is FreeBSD only. Support for OpenBSD or NetBSD is not currently planned. POSIX is the compatibility target for the shell core; it does not imply that every POSIX utility is bundled or that full conformance has already been achieved.

## Android Targets

| ABI | Rust target triple |
| --- | --- |
| ARMv8-A / arm64-v8a | `aarch64-linux-android` |
| x86-64 / x86_64 | `x86_64-linux-android` |

Android executables are linked directly as standalone ELF binaries against Android's Bionic C library. CI uses the NDK LLVM toolchain's API-level Clang linker wrappers and Cargo's Rust Android target configuration. The build does not produce an Android shared library, JNI module, or APK.

## Current Language Boundary

The lexer currently supports unquoted words, single quotes, double quotes, backslash escaping, adjacent quoted and unquoted fragments within one word, empty quoted words, and comments beginning with an unquoted `#` at a token boundary. It retains fragment metadata so later phases can distinguish expandable text from literal text.

The shell maintains an environment map initialized from inherited process variables. `export NAME=value` changes the shell environment, and `unset NAME` removes a variable. These changes affect later expansion, `PATH` lookup, and the environment passed to external children. Assignment-only commands persist shell variables without exporting them. Command-prefix assignments are temporary and are exported only for the command invocation. The implementation does not yet cover all POSIX assignment and special-builtin rules. Basic parameter substitution for environment-variable names and the last-exit-status parameter (`$?` and `${?}`) is performed only in unquoted and double-quoted fragments. Single-quoted and backslash-escaped text is not expanded. Other POSIX special and positional parameters, parameter operators, field splitting, and pathname expansion remain incomplete.

The current implementation includes sequential, buffered pipelines with simple-command, brace-group, and subshell stages, plus basic `<`, `>`, `>>`, and `2>` redirection. These supported operators are tokenized even when adjacent to words (for example, `echo hi|cat` or `echo hi>file`). The lexer recognizes a broader set of POSIX operator spellings, but token recognition does not imply grammar or execution support. Unsupported operators are rejected explicitly; the complete POSIX shell grammar remains unimplemented. Pipelines are not concurrent. POSIX parameter expansion forms, field splitting, pathname expansion, command substitution, full arithmetic expansion, `case` compounds, functions, here-documents, and job control are not implemented. The implemented compound-command subset includes brace groups, subshells, `if`/`elif`/`else`, `while`, `until`, and `for` with an explicit word list. A `for name; do ...; done` form currently iterates zero times because positional parameters are not implemented. `break` and `continue` support positive numeric levels across nested loops. The parser now creates typed `IfStatement` and `LoopStatement` nodes for the outer structures; branch bodies are still retained as token sequences and dispatched by the existing compound-branch executor. Compound commands are not accepted in every POSIX grammar position. Input continuation distinguishes recoverable unterminated quotes and open `if`/loop, brace-group, and subshell delimiters from unmatched closing delimiters; this is an incremental parser foundation, not a complete POSIX grammar.

## Compound-command limitations

Compound-command support currently includes brace groups, subshells, `if`/`elif`/`else`, `while`, and `until`. Loop conditions are simple command lists; loop bodies support nested loops and `if` statements. `break` and `continue` support optional positive loop levels and nested-loop propagation. The parser represents `if`/`elif`/`else` and `while`/`until` as typed nodes whose conditions and bodies contain recursive `CompoundList` entries. A compound list can contain simple commands, nested brace groups and subshells, if statements, case statements, and loops, with sequence and conditional connectors preserved. Brace groups execute in the current shell environment and currently require a complete single input unit. Subshells use copied shell state and restore the parent working directory after execution. The executor consumes this AST recursively. This is still a limited compound-command grammar: `case` and functions are not implemented, and compound commands are not accepted in every POSIX grammar position.

## Process and Filesystem Model

Built-ins execute in the Mysh process. External commands execute as child processes and inherit the current working directory and environment. Mysh resolves command names against `PATH` before spawning them and distinguishes command-not-found status 127 from a found but non-executable command status 126. Pipeline stages currently exchange buffered byte vectors sequentially rather than connected concurrent pipes. A multi-stage pipeline may contain a brace-group or subshell AST stage; that stage executes in a copied shell state and its captured output is forwarded to the next stage. Pipeline input is not yet injected into commands inside a compound stage. Each stage runs with a snapshot of shell variables, exported attributes, and the working directory; stage-local assignments and built-in state changes are restored before the next stage. Explicit subshell commands use a copied shell state and restore the parent working directory after execution. Command-prefix assignments are applied temporarily and exported to external commands within that stage. Standard input is passed to external commands and the `cat` built-in when a prior stage or input redirection supplies data.

Filesystem built-ins validate operand counts and supported options before performing mutations. Recursive copy rejects symbolic links and special filesystem entries instead of following them implicitly, and prevents a destination from being created inside the source tree. Recursive removal refuses the current working directory, its ancestors, and filesystem roots. Multi-source copy and move operations require an existing destination directory. Move operations use the platform rename primitive and do not emulate cross-filesystem copies.

Mysh does not elevate privileges. Android sandboxing, SELinux policy, filesystem permissions, executable mapping restrictions, and linker compatibility remain enforced by the operating system.
## Adopted Shell and Utility Architecture

The project target is a standalone POSIX shell implementation conforming to the Shell Command Language in POSIX.1-2024. Mysh is the shell interpreter; it is not a wrapper around Bash, dash, BusyBox ash, or another shell.

The language boundary is deliberately limited to POSIX shell. Bash, KornShell, Zsh, GNU-shell, and BSD-shell language extensions are not project targets. Compatibility tests may use other shells as comparison implementations, but their extensions must not become Mysh features.

### Responsibilities

- **Mysh shell engine:** tokenization, grammar, parsing, expansions, command search and dispatch, command execution, pipelines, redirections, subshell environments, shell variables, positional parameters, traps, and the shell execution environment.
- **Mysh shell built-ins:** implement only commands that POSIX defines as shell built-ins or that must execute in the shell process to preserve shell state and control. Implement the POSIX special-built-in semantics separately from regular built-ins.
- **External utilities:** use uutils Coreutils for utilities that uutils provides. Do not retain a Mysh implementation of a utility merely to duplicate an available uutils command. Resolve these commands as external executables through the normal command-search path.
- **Utility gaps:** uutils Coreutils is not the complete POSIX.1 utility set. Utilities absent from uutils must be evaluated individually against the POSIX utility specification and selected or implemented as separate external utilities; they must not be added as Bash-like shell features.
- **MYPK:** remains a separate package-management component and is not part of the shell language implementation.

### Command ownership policy

A command belongs to the shell when POSIX specifies it as a shell built-in, or when its semantics require direct access to the current shell execution environment. A command provided by uutils belongs to the external utility layer unless POSIX shell semantics require a built-in implementation. The shell's command-search and special-built-in precedence rules must follow POSIX rather than a project-specific generic lookup order.

The presence of a familiar command name in the current source tree does not establish POSIX conformance. The legacy filesystem utility implementations have been removed from the Mysh built-in module; those command names now go through external command resolution. Host tests exercise external utility behavior, while a dedicated test verifies uutils multicall dispatch and precedence.

### Conformance and migration

POSIX.1-2024 is the normative language and behavior reference. The implementation must be tested against the specification, including lexical and grammar rules, expansion order, special parameters, field splitting, pathname expansion, redirections, command search, exit statuses, and interactive-shell requirements. Passing tests against another shell is useful evidence but does not replace the normative specification.

The current source tree is a partial implementation and is not to be described as POSIX-conformant until the relevant conformance tests pass. Integration of uutils must separately validate the selected release, its license and transitive dependencies, Android Bionic compatibility, and both supported Android ABIs (aarch64-linux-android and x86_64-linux-android). A successful build for one Android ABI does not establish support for the other.

The Android build integration pins uutils Coreutils 0.12.0 at commit `dc1efd89948a9ca4c78c3a4b9a6ac891019a8c69` and builds with default features disabled using the explicit `feat_common_core`, selected utility features, and `feat_require_unix_core`. This intentionally excludes `stdbuf`, whose preload-library implementation is not compatible with Android Bionic. The Android build excludes uutils' `hostname` feature because its set-hostname implementation references `sethostname`, which is not exported by Android Bionic. CI applies a narrow source compatibility cast in uutils `uptime` at its existing API boundary, using the `uucore::libc::time_t` re-export. For each supported ABI, CI builds `mysh`, `coreutils`, and `mypk`, then packages them as `bin/mysh`, `bin/coreutils`, and `bin/mypk` in `mysh-runtime-<abi>.tar.gz`. The archive is intended to be extracted into the Android application's private files directory at `<filesDir>`; the application must set `MYSH_PREFIX` to `<filesDir>` before launching Mysh. This is a runtime payload, not an APK or an automatic installation into an application sandbox. When a command name is in the pinned Android uutils utility set and the multicall binary is available, Mysh prefers it and invokes it as `coreutils <utility> ...`. Unknown command names are not forwarded to the multicall binary and retain command-not-found status 127.

No Bash compatibility layer, Bash-specific syntax, GNU Coreutils dependency, or BSD shell extension is to be introduced as part of this architecture.
