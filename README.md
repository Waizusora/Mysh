# Mysh

Mysh adalah shell command-line native yang ditulis dengan Rust. Mysh menjalankan lexer, parser, ekspansi, dan interpretasi shell sendiri; bukan wrapper Bash, Zsh, atau BusyBox.

## Status

Mysh masih dalam pengembangan dan **belum POSIX-conformant**. Source saat ini menyediakan fondasi shell, subset grammar dan ekspansi POSIX, built-in inti, serta eksekusi proses eksternal. Lihat [roadmap](docs/roadmap.md) untuk tahapan pengembangan.

## Fitur saat ini

- Mode interaktif, input standar, dan `mysh -c 'command'`.
- Lexer dengan single/double quotes, escaping, komentar, dan metadata kutipan.
- Shell variables, exported environment, assignments, positional parameters, dan status `$?`.
- Ekspansi parameter, command substitution (`$(...)` dan backticks), arithmetic, tilde, IFS field splitting, dan pathname expansion dalam cakupan parsial.
- Compound commands: `if/elif/else`, `case`, `for`, `while`, `until`, brace groups, dan subshell.
- Fungsi shell; `return` dan `shift`.
- Command list `;`, `&&`, `||`; pipeline sequential-buffered; redirection dasar `<`, `>`, `>>`, dan `2>`.
- Built-ins: `:`, `.`, `cd`, `export`, `unset`, `pwd`, `true`, `false`, `eval`, `exit`, `break`, `continue`, `return`, dan `shift`.
- Resolusi executable melalui `PATH`, exit status child process, dan pemetaan signal status pada Unix.
- Integrasi utilitas eksternal melalui uutils Coreutils pada runtime Android.

Rincian perilaku dan batas tiap fitur ada di [Command Reference](docs/command-reference.md). Operator yang dikenali lexer belum tentu didukung parser atau executor.

## Batasan penting

- Grammar, ekspansi, built-in, dan perilaku interaktif POSIX belum lengkap.
- Pipeline masih sequential dan buffered, bukan concurrent OS pipes.
- Background execution dan job control belum tersedia.
- Here-documents, descriptor duplication/closure, dan redirection lanjutan belum didukung.
- Pattern matching dan locale behavior masih parsial.

## Target build

| Platform | Rust target |
| --- | --- |
| Android ARM64 | `aarch64-linux-android` |
| Android x86-64 | `x86_64-linux-android` |
| Linux musl x86-64 | `x86_64-unknown-linux-musl` |
| Linux musl ARM64 | `aarch64-unknown-linux-musl` |

Android menghasilkan executable ELF native yang terhubung ke Bionic. CI membangun runtime Android berisi `bin/mysh` dan `bin/coreutils` untuk masing-masing ABI. Arsip runtime bukan APK; aplikasi Android harus mengekstraknya ke direktori privat aplikasi dan mengatur `MYSH_PREFIX` ke direktori tersebut.

## Build dan test (host)

Membutuhkan stable Rust:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

Executable host: `target/release/mysh`.

## Penggunaan

```sh
./mysh
./mysh -c 'echo hello'
printf 'pwd\\nexit\\n' | ./mysh
```

Opsi CLI: `--help`, `--version`, dan `--platform`. Opsi `--platform` mencetak `{hostname}-{c-library}-{arch}`.

## Dokumentasi

- [Arsitektur](docs/architecture.md)
- [Referensi perintah dan sintaks](docs/command-reference.md)
- [Roadmap](docs/roadmap.md)
- [Indeks dokumentasi](docs/INDEX.md)
- [Integrasi Wicofe](docs/WICOFE_INTEGRATION.md)
- [Changelog](CHANGELOG.md)

## Lisensi

BSD 3-Clause. Lihat [LICENSE](LICENSE).
