# Mysh

Mysh (My Shell) is an independent, native command-line shell written in Rust for Android. It is distributed as a standalone executable and is not a wrapper around Bash, Zsh, BusyBox, or another shell.

## Project status

The project is progressing through its native shell implementation milestones. Milestone 1 established the command loop, lexer, built-in commands, external process execution, tests, and Android build automation. Milestone 2 strengthens filesystem command semantics and child-process status handling. The official compatibility strategy is POSIX-first: establish portable shell behavior before adding operating-system-specific support.

## Compatibility principles

- **POSIX is the shell-language compatibility baseline.** Mysh aims to implement the portable command language and behavior expected from a POSIX-conforming `sh`.
- **Mysh is an independent implementation.** Its lexer, parser, expansion engine, execution model, and built-ins are implemented in Rust. It does not delegate shell-script interpretation to the host's `/bin/sh`, Bash, Zsh, or BusyBox.
- **The host supplies operating-system services.** Mysh may use platform APIs and launch external executables available on the host; this does not make another shell its interpreter.
- **Compatibility is not yet complete.** The current implementation is partial and must not be described as POSIX-conformant until the required behavior has been implemented and validated.

POSIX shell compatibility is distinct from implementing every POSIX utility. Mysh's shell-language target does not by itself promise to bundle the complete POSIX utility suite.

## Platform roadmap

Mysh follows this platform order:

1. **POSIX compatibility:** implement and test the portable shell-language and utility behavior that forms the project's compatibility baseline.
2. **Android:** complete and maintain the three Android executable targets listed below.
3. **Linux:** validate the shared implementation on mainstream Linux environments.
4. **FreeBSD:** add and test a dedicated FreeBSD target after the POSIX foundation is stable.

FreeBSD is the only BSD operating system currently planned as an explicit target. OpenBSD and NetBSD are not part of the committed roadmap. POSIX compatibility is a behavioral baseline, not a claim that every POSIX utility or shell feature is already implemented.

## Supported Android build targets

| Android ABI | Rust target |
| --- | --- |
| ARMv8-A (arm64-v8a) | `aarch64-linux-android` |
| ARMv7-A (armeabi-v7a) | `armv7-linux-androideabi` |
| x86-64 (x86_64) | `x86_64-linux-android` |

The Android NDK is required to produce Android executables. Host builds are supported for development and testing.

## Current features

- Interactive and non-interactive command input.
- Quoted arguments and backslash escaping.
- Built-in commands: `cd`, `pwd`, `ls`, `mkdir`, `rmdir`, `touch`, `cp`, `mv`, `rm`, `cat`, `echo`, `clear`, `history`, `help`, and `exit`.
- Hidden-file listing with `ls -a`.
- Recursive directory copying and multiple-source copy/move operations.
- Combined `rm` options and consistent `--` option termination across filesystem commands.
- Safeguards against deleting the current working directory, its ancestors, or filesystem roots.
- Conventional child-process signal status mapping on Unix-like targets.
- External executable lookup through `PATH`.
- Command exit-status reporting.
- Basic sequential pipelines with buffered input between stages.
- Basic input, output, append, and standard-error redirection.
- Preliminary environment-variable substitution.
- Unit and integration tests.

The current pipeline and redirection implementation is intentionally limited and is not yet POSIX-conformant. Variable expansion currently occurs after tokenization and does not preserve single-quote semantics. Concurrent pipeline execution, field splitting, pathname expansion, command substitution, control-flow syntax, job control, and persistent history are not implemented.

## Build on a host

Install the stable Rust toolchain, then run:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The resulting host executable is located at `target/release/mysh`.

## Build for Android

Install the Android SDK and NDK. Add the Rust standard-library targets required by the Android ABIs:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
```

Set the NDK toolchain location and build each standalone executable. The linker names below select Android API level 21:

```sh
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
NDK_TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"

CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_TOOLCHAIN/aarch64-linux-android21-clang" \
  cargo build --target aarch64-linux-android --release

CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$NDK_TOOLCHAIN/armv7a-linux-androideabi21-clang" \
  cargo build --target armv7-linux-androideabi --release

CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$NDK_TOOLCHAIN/x86_64-linux-android21-clang" \
  cargo build --target x86_64-linux-android --release
```

Each executable is emitted under its Rust target directory in `target/<target-triple>/release/mysh`. The exact installation and execution location is subject to Android's application sandbox and executable-file policies.

## Usage

Start an interactive session:

```sh
./mysh
```

Run a command from a script or another process:

```sh
printf 'pwd\\necho "hello from Mysh"\\nexit\\n' | ./mysh
```

External commands are resolved using the current `PATH`. Mysh does not provide root access or bypass Android application sandbox restrictions.

## Documentation

- [Architecture](docs/architecture.md)
- [Command reference](docs/command-reference.md)
- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)

## License

Mysh is licensed under the BSD 3-Clause License. See [LICENSE](LICENSE) for the complete license text.
