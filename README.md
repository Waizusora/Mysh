# Mysh

Mysh (My Shell) is an independent, native command-line shell written in Rust for Android. It is distributed as a standalone executable and is not a wrapper around Bash, Zsh, BusyBox, or another shell.

## Project status

The project is progressing through its native shell implementation milestones. Milestone 1 established the command loop, lexer, built-in commands, external process execution, tests, and Android build automation. Milestone 2 strengthens filesystem command semantics and child-process status handling. The official compatibility strategy is POSIX-first: establish portable shell behavior before adding operating-system-specific support.

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
- Unit and integration tests.

Pipelines, shell expansions, redirections, job control, scripting syntax, and persistent history are not implemented yet.

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
