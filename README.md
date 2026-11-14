# Mysh

Mysh (My Shell) is an independent, native command-line shell written in Rust for Android. It is distributed as a standalone executable and is not a wrapper around Bash, Zsh, BusyBox, or another shell.

## Project status

The project is progressing toward a mature POSIX shell. The native execution foundation is established, but the shell-language implementation remains partial and is not yet POSIX-conformant. The seven-phase maturity plan, phase statuses, and exit criteria are maintained in the [development roadmap](docs/roadmap.md).

## Compatibility principles

- **POSIX is the shell-language compatibility baseline.** Mysh aims to implement the portable command language and behavior expected from a POSIX-conforming `sh`.
- **Mysh is an independent implementation.** Its lexer, parser, expansion engine, execution model, and built-ins are implemented in Rust. It does not delegate shell-script interpretation to the host's `/bin/sh`, Bash, Zsh, or BusyBox.
- **The host supplies operating-system services.** Mysh may use platform APIs and launch external executables available on the host; this does not make another shell its interpreter.
- **Compatibility is not yet complete.** The current implementation is partial and must not be described as POSIX-conformant until the required behavior has been implemented and validated.

POSIX shell compatibility is distinct from implementing every POSIX utility. Mysh's shell-language target does not by itself promise to bundle the complete POSIX utility suite.

## MYPK — My Package Keeper

MYPK is the independent Rust package manager for the Mysh environment. It is built as a separate mypk executable and uses the native .mypk package format. Its commands, aliases, package format, repository model, runtime identifiers, and security requirements are specified in [the MYPK specification](docs/mypk-specification.md).

Canonical commands are sync, upgrade, find, install, delete, reinstall, list, info, clean, and clear. Compatibility aliases are update for sync, search for find, and remove for delete. MYPK includes strict version 1 manifest parsing, bounded TAR archive reading, path and entry validation, and SHA-256 payload integrity checks. Repository operations, signature authentication, dependency resolution, and installation are not yet implemented.

## Platform roadmap

Mysh follows this platform order:

1. **POSIX compatibility:** implement and test the portable shell-language and utility behavior that forms the project's compatibility baseline.
2. **Android:** complete and maintain the three Android executables per ABI listed below.
3. **Linux:** validate the shared implementation on mainstream Linux environments.
4. **FreeBSD:** add and test a dedicated FreeBSD target after the POSIX foundation is stable.

FreeBSD is the only BSD operating system currently planned as an explicit target. OpenBSD and NetBSD are not part of the committed roadmap. POSIX compatibility is a behavioral baseline, not a claim that every POSIX utility or shell feature is already implemented.

## Supported Android build targets

| Android ABI | Rust target |
| --- | --- |
| ARMv8-A (arm64-v8a) | `aarch64-linux-android` |
| x86-64 (x86_64) | `x86_64-linux-android` |

The Android NDK is required to produce Android executables. Host builds are supported for development and testing.

## Current features

- Interactive and non-interactive command input.
- Quoted arguments and backslash escaping.
- Implemented shell built-ins include `:`, `.`, `cd`, `export`, `unset`, `pwd`, `true`, `false`, `eval`, `exit`, `break`, and `continue`.
- External utilities are provided by the pinned uutils Coreutils Android multicall build where supported; they are not duplicated as Mysh built-ins.
- Conventional child-process signal status mapping on Unix-like targets.
- Explicit external executable lookup through `PATH`, including ordered search, explicit paths, and conventional 126/127 error statuses.
- Command exit-status reporting.
- Basic sequential pipelines with buffered input between stages; supported operators may be adjacent to words.
- Basic input, output, append, and standard-error redirection.
- Shell-managed environment variables initialized from the inherited process environment.
- Persistent shell assignments (`NAME=value`) and temporary command-prefix assignments (`NAME=value command`).
- Separate shell-variable and exported-variable tracking; external commands inherit exported values only.
- `export` and `unset` built-ins.
- Automatic priority for `$MYSH_PREFIX/bin` in command lookup when that directory exists.
- Quote-aware preliminary environment-variable substitution (disabled in single-quoted and escaped fragments).
- Initial `if` / `then` / `elif` / `else` / `fi` compound-command support, including nested and multiline forms.
- Context-sensitive recognition of implemented `if` compound reserved words and incremental input classification.
- Unit and integration tests.

The current pipeline and redirection implementation is intentionally limited and is not yet POSIX-conformant. Basic IFS field splitting is now applied to unquoted parameter-expansion results while preserving literal and quoted text, with default, empty, whitespace, and non-whitespace IFS handling covered by focused regression tests. Unquoted `$@` now retains positional-parameter boundaries before field splitting, while unquoted `$*` joins parameters using the first IFS character before splitting. Initial pathname expansion for `*`, `?`, and bracket expressions is implemented across path components; bracket patterns support ranges and `!` negation, unmatched patterns remain literal, and hidden names require an explicit leading dot. Per-character quote metadata is preserved through field splitting, so quoted or escaped wildcard characters remain literal while unquoted wildcard fragments and unquoted variable results can participate in pathname expansion. Other POSIX pathname edge cases remain incomplete. Complete POSIX positional expansion semantics, parameter operators, command substitution, and arithmetic expansion remain incomplete. Concurrent pipeline execution, the remaining POSIX control-flow grammar, job control, and persistent history are also not implemented.

## Build on a host

Install the stable Rust toolchain, then run:

```sh
cargo fmt --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p my-pkg-keeper --release
cargo build --release
```

The resulting host executable is located at `target/release/mysh`.

## Build for Android

Install the Android SDK and NDK. Add the Rust standard-library targets required by the Android ABIs:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
```

Set the NDK toolchain location and build each standalone executable. The linker names below select Android API level 21:

```sh
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
NDK_TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"

CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_TOOLCHAIN/aarch64-linux-android21-clang" \
  cargo build --target aarch64-linux-android --release

CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$NDK_TOOLCHAIN/x86_64-linux-android21-clang" \
  cargo build --target x86_64-linux-android --release
```

Each executable is emitted under its Rust target directory in `target/<target-triple>/release/mysh`. The exact installation and execution location is subject to Android's application sandbox and executable-file policies.

The resulting executables are built for ARM64 and x86-64 only. CI also builds uutils Coreutils and MYPK for each ABI and publishes a runtime archive named `mysh-runtime-<abi>.tar.gz`. The archive contains `bin/mysh`, `bin/coreutils`, and `bin/mypk`.

Extract the archive into the Android application's private files directory (`<filesDir>`). Set `MYSH_PREFIX` to `<filesDir>` in the environment used to launch Mysh. Mysh then prepends `<MYSH_PREFIX>/bin` to `PATH`. The archive is not an APK; the Android application is responsible for extraction and must comply with platform executable-file policies.


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
- [Development roadmap](docs/roadmap.md)
- [Command reference](docs/command-reference.md)
- [MYPK specification](docs/mypk-specification.md)
- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)

## License

Mysh is licensed under the BSD 3-Clause License. See [LICENSE](LICENSE) for the complete license text.
