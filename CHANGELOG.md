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
