# Mysh on Wicofe — Integration Checklist

- **Status:** Planned integration target; not implemented or validated
- **Component owner:** Mysh
- **OS interface owners:** Wicofe-ABI and Wicofe-Kernel
- **Runtime owner:** Native Wicofe Rust platform/runtime; Wicofe-Lib may provide safe shared wrappers. Wlibc is a separate C runtime and is not a mandatory Mysh dependency.

## 1. Scope

Mysh remains a standalone shell project with its own supported-platform roadmap. This document describes the additional work required before Mysh can be used as the default Wicofe shell. It does not change the compatibility or release claims of the standalone project.

## 2. Required platform prerequisites

Before building or running Mysh on Wicofe, the OS must provide and document:

- A supported executable format and loader.
- Process creation, execution, wait, exit status, and signal/interruption semantics.
- File descriptors, pipes, redirection, and descriptor inheritance.
- A terminal/TTY interface, process groups, and PTY support for interactive sessions.
- Environment and argument passing.
- Filesystem path, permissions, and executable lookup semantics.
- A Wicofe-native Rust target and platform implementation capable of supporting the selected `std` functionality without a Linux libc backend.
- A stable Wicofe syscall ABI and native Rust system-interface integration. Linux libc, GNU libc, musl-for-Linux, and GNU compatibility layers must not be required.

Rust `std` support is not implied by a Rust kernel. It requires an explicit target/runtime port with operating-system services implemented.

## 3. Integration sequence

1. Review the Wicofe userspace ABI and native target/toolchain strategy.
2. Establish the Wicofe Rust target and `std` platform implementation without relying on Linux libc.
3. Build Mysh for Wicofe without changing its Android/Linux/FreeBSD platform behavior.
4. Validate non-interactive execution, argument/environment passing, and exit status.
5. Validate pipelines, redirections, file descriptors, and process lifecycle.
6. Validate interactive TTY behavior, terminal modes, signals, and PTY lifecycle.
7. Validate shell startup, PATH, working directory, and user environment.
8. Run the Mysh test suite on the Wicofe target and add Wicofe-specific integration tests.
9. Only after reproducible validation, consider Mysh for the default Wicofe image.


## 4. Required cross-repository contracts

- [Wicofe-ABI](https://github.com/Waizusora/Wicofe-ABI): syscall and userspace-visible structures.
- [Wicofe-Kernel](https://github.com/Waizusora/Wicofe-Kernel): process, terminal, VFS, and syscall implementation.
- Wicofe native Rust platform/runtime: OS services and `std` integration for native Rust applications.
- [Wicofe-Lib](https://github.com/Waizusora/Wicofe-Lib): optional safe Rust wrappers and shared APIs.
- [Wlibc](https://github.com/Waizusora/WLibc): separate C runtime; not a required dependency for native Rust Mysh.
- [Wicofe-SDK](https://github.com/Waizusora/Wicofe-SDK): target, sysroot, compiler, and linker integration.
- [Wicofe-Tests](https://github.com/Waizusora/Wicofe-Tests): cross-repository shell integration and PTY tests.

## 5. Acceptance criteria

Wicofe support must not be declared complete until:
- The exact native Wicofe target and toolchain are reproducible.
- Mysh builds and links without Linux libc or GNU compatibility dependencies.
- Script execution, external command lookup, pipelines, and redirection pass tests.
- Interactive input, terminal control, signals, and PTY behavior pass tests.
- Exit status and process cleanup match the documented OS contract.
- Tests distinguish implemented behavior from unsupported shell features.

**Current status:** planning only. Mysh is not yet a validated Wicofe shell.
