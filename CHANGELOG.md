# Changelog

All notable changes to Mysh are documented in this file.

The format follows Keep a Changelog. Version numbers follow Semantic Versioning.

## [0.1.0] - Unreleased

### Roadmap

- Define POSIX shell behavior as the primary compatibility baseline while keeping the interpreter implementation independent in Rust.
- Explicitly exclude runtime dependence on the host `/bin/sh`, Bash, Zsh, or BusyBox for parsing and interpreting Mysh scripts.
- Treat `dash`, BusyBox `ash`, and FreeBSD `sh` as compatibility references rather than implementation dependencies.
- Plan Linux validation after the Android foundation, followed by a dedicated FreeBSD port; other BSD systems are not currently in scope.

### Added

- Initial MYPK (My Package Keeper) Rust workspace crate and standalone mypk executable.
- Canonical MYPK commands: sync, upgrade, find, install, delete, reinstall, list, info, clean, and clear.
- Compatibility aliases: update for sync, search for find, and remove for delete.
- MYPK command-resolution unit tests and help/version output.
- Initial MYPK package-format, repository, runtime, dependency, transaction, and security specification.

- Explicit external-command resolution through `PATH`, including ordered directory search, explicit slash-containing paths, executable-permission checks, and command-not-found (127) versus not-executable (126) status handling.
- Unit tests for PATH lookup, search order, explicit paths, missing commands, and non-executable files.

- Token recognition for supported pipeline and redirection operators adjacent to words, including compact forms such as `echo text|cat` and `echo text>file`.
- Regression tests for compact pipeline and output-redirection syntax.

- Quote-aware shell-word lexer that preserves ordered quoted, unquoted, and escaped fragments.
- Variable expansion now respects single quotes and escaped dollar signs while retaining expansion inside double quotes.
- Regression tests for quote metadata, adjacent quoted fragments, quoted operators, and quote-aware variable expansion.

- Pipeline input delivery to the `cat` built-in, including multi-stage built-in pipelines.
- Regression tests for `echo | cat` and `echo | cat | cat`.
- Documentation of the current partial pipeline, redirection, and variable-substitution behavior and its POSIX limitations.

- Milestone 2 filesystem enhancements: hidden-file listing, recursive directory copy, multiple-source copy and move, combined removal options, and the `--` option terminator.
- Filesystem safeguards against recursive self-copy and removal of the current working directory, its ancestors, or filesystem roots.
- Unix child-process signal termination status mapping.
- Regression coverage for refusing removal of working-directory ancestors and filesystem roots, and for rejecting symbolic links during recursive copy.
- Consistent `--` option termination for `rmdir`, `touch`, and `cat`.
- Explicit rejection of unsupported leading options by `mkdir`, `rmdir`, `touch`, and `cat`.
- Standard-input handling for the `cat -` operand.

- Initial Rust project structure and Android ABI build targets.
- Interactive command loop and command-line entry point.
- Command lexer with single quotes, double quotes, and backslash escaping.
- Built-in filesystem and session commands.
- External executable lookup through `PATH`.
- Unit tests, integration tests, and GitHub Actions CI.
- Architecture and command reference documentation.

## Unreleased

### Changed

- Expanded shell lexical recognition for POSIX compound-command delimiters, case terminators, descriptor redirections, and here-document operators.
- Made the parser reject recognized but unsupported shell operators and compound-command forms explicitly instead of passing them through as ordinary command arguments.
- Added lexer and parser regression tests for operator boundaries, descriptor numbers, quoting, and unsupported syntax rejection.

### Documentation

- Added a seven-phase Mysh maturity roadmap covering the native shell foundation, POSIX grammar, expansion, execution semantics, built-ins, interactive job control, and conformance and release stabilization.
- Linked the roadmap from the README and architecture documentation, with explicit phase statuses and testable exit criteria.

- Added a shell-managed environment map initialized from inherited process variables.
- Added `export` and `unset` built-ins and passed the current environment to external child processes.
- Added regression coverage for exported variable expansion and removal.

- Added separate shell-variable and exported-variable state, persistent assignment-only commands, and temporary command-prefix assignments.
- Added startup PATH priority for `$MYSH_PREFIX/bin` when the directory exists.
- Added regression tests for variable assignment, temporary command assignments, and prefix PATH ordering.
- Isolated variable state, export attributes, and working-directory changes between sequentially executed pipeline stages.
- Applied temporary command-prefix assignments within pipeline stages without leaking them into the parent shell.
- Added `$?` and `${?}` expansion for the last command status.
- Preserved assignments preceding the implemented POSIX special built-ins `export`, `unset`, and `exit`.
- Added `$?` and `${?}` expansion regression coverage and external-child environment inheritance tests.
- Added sequential `;` command lists and left-associative `&&` / `||` short-circuit execution with regression tests.

- Added initial single-level, single-line `if` / `then` / optional `else` / `fi` compound command execution, selected using the condition's exit status.
- Added regression tests for successful and failing conditions and malformed compound-command syntax.
- Documented current limitations: nested `if` statements and multiline compound commands are not supported.

- Extended `if` compound-command handling with ordered `elif` conditions and optional `else` branches.
- Extended compound-branch execution to support nested `if` statements alongside sequential commands and `&&` / `||` command lists.
- Added multiline input buffering for `if` compound commands, including nested blocks and an interactive continuation prompt.
- Added regression coverage for multiline `if` execution, continuation-depth detection, and mixed commands around nested blocks.
- Added context-sensitive recognition of `if`, `then`, `elif`, `else`, and `fi` at command boundaries, preserving keyword-looking ordinary arguments and quoted or escaped words.
- Added incremental input classification for complete, incomplete, and structurally invalid input, including recoverable unterminated quotes and unmatched closing `fi` detection.
- Other POSIX compound commands and the complete POSIX compound-command grammar remain unsupported.
