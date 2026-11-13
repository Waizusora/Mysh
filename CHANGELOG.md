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
