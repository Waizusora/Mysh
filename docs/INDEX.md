# Mysh Documentation Index

This repository owns the Mysh shell implementation and its platform-specific runtime behavior. Mysh remains an independently maintained shell project; Wicofe support is an integration target, not the scope of the entire repository.

## Current project documentation
- [Architecture](architecture.md)
- [Development roadmap](roadmap.md)
- [Command reference](command-reference.md)
- [Changelog](../CHANGELOG.md)
- [Security policy](../SECURITY.md)

## Wicofe integration
- [Wicofe porting and integration checklist](WICOFE_INTEGRATION.md)

## Ownership boundary
Mysh owns shell-language parsing, expansion, execution policy, built-ins, and shell CLI behavior. Wicofe owns the kernel syscall ABI, process/terminal primitives, libc/runtime, and OS integration. A successful Android/Linux build does not imply Wicofe compatibility.

- [Wicofe repository documentation map](https://github.com/Waizusora/Wicofe-Docs/blob/main/REPOSITORY_DOCUMENTATION_MAP.md)
