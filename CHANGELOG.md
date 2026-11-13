# Changelog

All notable changes to Mysh are documented in this file.

The format follows Keep a Changelog. Version numbers follow Semantic Versioning.

## [0.1.0] - Unreleased

### Added

- Milestone 2 filesystem enhancements: hidden-file listing, recursive directory copy, multiple-source copy and move, combined removal options, and the `--` option terminator.
- Filesystem safeguards against recursive self-copy and removal of the current working directory, its ancestors, or filesystem roots.
- Unix child-process signal termination status mapping.

### Added

- Initial Rust project structure and Android ABI build targets.
- Interactive command loop and command-line entry point.
- Command lexer with single quotes, double quotes, and backslash escaping.
- Built-in filesystem and session commands.
- External executable lookup through `PATH`.
- Unit tests, integration tests, and GitHub Actions CI.
- Architecture and command reference documentation.
