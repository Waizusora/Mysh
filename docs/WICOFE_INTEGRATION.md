# Mysh on Wicofe — Integration Checklist

- **Status:** Planned integration target; not implemented or validated
- **Component owner:** Mysh
- **OS interface owners:** Wicofe-ABI and Wicofe-Kernel
- **Runtime owner:** Wicofe-Libc and/or Wicofe-Lib, depending on execution target

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
- A userspace runtime capable of supporting the selected Rust target and required `std` functionality, or a separately approved runtime strategy.
- A stable syscall ABI and compatible libc/system-library integration where required.

Rust `std` support is not implied by a Rust kernel. It requires an explicit target/runtime port with operating-system services implemented.

## 3. Integration sequence

1. Review the Wicofe userspace ABI and target triple/toolchain strategy.
2. Establish the required Rust target and `std` support path.
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
- [Wicofe-Libc](https://github.com/Waizusora/Wicofe-Libc): C runtime if required by dependencies or integration.
- [Wicofe-Lib](https://github.com/Waizusora/Wicofe-Lib): Rust userspace wrappers and shared APIs.
- [Wicofe-SDK](https://github.com/Waizusora/Wicofe-SDK): target, sysroot, compiler, and linker integration.
- [Wicofe-Tests](https://github.com/Waizusora/Wicofe-Tests): cross-repository shell integration and PTY tests.

## 5. Acceptance criteria

Wicofe support must not be declared complete until:
- The exact Wicofe target and toolchain are reproducible.
- Mysh builds and links against the declared runtime.
- Script execution, external command lookup, pipelines, and redirection pass tests.
- Interactive input, terminal control, signals, and PTY behavior pass tests.
- Exit status and process cleanup match the documented OS contract.
- Tests distinguish implemented behavior from unsupported shell features.

**Current status:** planning only. Mysh is not yet a validated Wicofe shell.
