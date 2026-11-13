# Mysh

Mysh (My Shell) is an independent, native command-line shell written in Rust for Android. It is distributed as a standalone executable and is not a wrapper around Bash, Zsh, BusyBox, or another shell.

## Project status

The project is in its initial implementation phase. Milestone 1 establishes the interactive command loop, command lexer, built-in filesystem utilities, basic external program execution, tests, and Android build automation.

## Supported targets

| Android ABI | Rust target |
| --- | --- |
| ARMv8-A (arm64-v8a) | `aarch64-linux-android` |
| ARMv7-A (armeabi-v7a) | `armv7-linux-androideabi` |
| x86-64 (x86_64) | `x86_64-linux-android` |

The Android NDK is required to produce Android executables. Host builds are supported for development and testing.

## Milestone 1 features

- Interactive and non-interactive command input.
- Quoted arguments and backslash escaping.
- Built-in commands: `cd`, `pwd`, `ls`, `mkdir`, `rmdir`, `touch`, `cp`, `mv`, `rm`, `cat`, `echo`, `clear`, `history`, `help`, and `exit`.
- External executable lookup through `PATH`.
- Command exit-status reporting.
- Unit and integration tests.

Pipelines, shell expansions, redirections, job control, scripting syntax, and persistent history are not part of Milestone 1.

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
