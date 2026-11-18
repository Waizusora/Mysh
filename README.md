# Mysh

Mysh is a standalone native command-line shell written in Rust. It implements its own lexer, parser, expansion, and shell interpreter; it is not a wrapper around Bash, Zsh, or BusyBox.

## Status

Mysh is under active development and is **not yet POSIX-conformant**. The current source provides the shell foundation, a partial POSIX grammar and expansion engine, core built-ins, and external process execution. See the [roadmap](docs/roadmap.md) for development phases.

## Current features

- Interactive mode, standard-input mode, and `mysh -c 'command'`.
- Lexer with single/double quotes, escaping, comments, and quote metadata.
- Shell variables, exported environment, assignments, positional parameters, and `$?`.
- Parameter expansion, command substitution (`$(...)` and backticks), arithmetic expansion, tilde expansion, IFS field splitting, and pathname expansion within a partial implementation.
- Compound commands: `if/elif/else`, `case`, `for`, `while`, `until`, brace groups, and subshells.
- Shell functions, `return`, and `shift`.
- Command lists `;`, `&&`, `||`; concurrent kernel-pipe execution for pipelines made entirely of simple external commands; sequential buffered fallback for pipelines involving built-ins, functions, compound commands, or unresolved external commands; basic redirection `<`, `>`, `>>`, and `2>`.
- Built-ins: `:`, `.`, `cd`, `export`, `unset`, `pwd`, `true`, `false`, `eval`, `exit`, `break`, `continue`, `return`, and `shift`.
- Executable lookup through `PATH`, child-process exit status, and Unix signal-status mapping.
- External utility integration through uutils Coreutils in the Android runtime.

See the [Command Reference](docs/command-reference.md) for supported behavior and limitations. An operator recognized by the lexer is not necessarily supported by the parser or executor.

## Important limitations

- POSIX grammar, expansions, built-ins, and interactive-shell behavior are incomplete.
- Only all-external simple-command pipelines use concurrent OS pipes. Pipelines requiring in-process built-ins, functions, compound commands, or unresolved external commands use sequential buffered fallback; compound stages do not yet receive input from the previous stage.
- Background execution and job control are not implemented.
- Here-documents, descriptor duplication/closure, and advanced redirections are not supported.
- Pattern matching and locale behavior are partial.

## Build targets

| Platform | Rust target |
| --- | --- |
| Android ARM64 | `aarch64-linux-android` |
| Android x86-64 | `x86_64-linux-android` |
| Linux musl x86-64 | `x86_64-unknown-linux-musl` |
| Linux musl ARM64 | `aarch64-unknown-linux-musl` |

Android builds are standalone ELF executables linked against Bionic. CI builds an Android runtime containing `bin/mysh` and `bin/coreutils` for each ABI. The runtime archive is not an APK; the consuming Android application must extract it into its private files directory and set `MYSH_PREFIX` to that directory.

## Build and test (host)

Requires stable Rust:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

The host executable is `target/release/mysh`.

## Usage

```sh
./mysh
./mysh -c 'echo hello'
printf 'pwd\\nexit\\n' | ./mysh
```

CLI options: `--help`, `--version`, and `--platform`. The `--platform` option prints `{hostname}-{c-library}-{arch}`.

## Documentation

- [Architecture](docs/architecture.md)
- [Command reference](docs/command-reference.md)
- [Roadmap](docs/roadmap.md)
- [Documentation index](docs/INDEX.md)
- [Wicofe integration](docs/WICOFE_INTEGRATION.md)
- [Changelog](CHANGELOG.md)

## License

BSD 3-Clause. See [LICENSE](LICENSE).

## Wicofe-wide principles

Wicofe is an independent Unix-like, POSIX-compatible operating system and software ecosystem, designed as an alternative to established and widely adopted software ecosystems and technologies, including GNU. Native components must use Wicofe interfaces and must not require Linux libc or GNU compatibility layers. Prefer permissive licenses; avoid GPL/AGPL, and consider LGPL only when no suitable permissive alternative exists. Reuse or fork upstream software when implementing it from scratch would be disproportionately difficult. See the [canonical project principles](https://github.com/Waizusora/Wicofe-Docs/blob/main/PROJECT_PRINCIPLES.md).
