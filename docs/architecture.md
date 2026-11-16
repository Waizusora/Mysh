# Architecture

## Overview

Mysh adalah shell native mandiri yang ditulis terutama menggunakan Rust standard library. Lexer, parser, ekspansi, command dispatch, dan interpretasi shell dilakukan oleh Mysh sendiri; shell lain tidak digunakan sebagai interpreter tersembunyi.

Target bahasa adalah POSIX Shell Command Language. Implementasi saat ini masih parsial dan belum POSIX-conformant. Bash, Zsh, dan ekstensi shell lain bukan target kompatibilitas.

## Runtime flow

1. main.rs memproses --help, --version, --platform, dan -c; tanpa opsi, input dibaca dari stdin. Mode interaktif ditentukan dari terminal stdin.
2. Shell mengelola input, status terakhir, variabel, export attributes, fungsi, positional parameters, dan kontrol loop.
3. Input diklasifikasikan sebagai lengkap, belum lengkap, atau tidak valid untuk mendukung kelanjutan compound command.
4. Lexer menghasilkan shell words dengan fragmen berurutan dan metadata single-quoted, double-quoted, unquoted, atau escaped.
5. Parser membentuk command-list, pipeline, dan typed compound-command nodes.
6. Expansion menangani parameter, command substitution, arithmetic, tilde, field splitting, dan pathname expansion sesuai subset yang tersedia.
7. Built-in dan fungsi dijalankan oleh shell; perintah eksternal dicari melalui PATH dan dijalankan sebagai child process menggunakan Rust process API.
8. Status hasil eksekusi disimpan untuk command berikutnya dan parameter $?.

## Source modules

| Path | Responsibility |
| --- | --- |
| src/main.rs | CLI entry point dan pemilihan mode input |
| src/lib.rs | Deklarasi modul dan public Shell API |
| src/lexer.rs | Tokenisasi kata, operator, quote dan escape metadata |
| src/parser.rs | Input classification, command-list, pipeline, dan compound-command AST |
| src/shell.rs | Input loop, expansion, variables, functions, built-in dispatch, pipeline dan process execution |
| src/builtins/mod.rs | Implementasi built-in yang dieksekusi dalam proses shell |
| src/command.rs | Resolusi executable dan shell script melalui explicit path atau PATH |
| src/platform.rs | Hostname, libc family, dan runtime identifier |

## Shell state and command execution

- Shell variables dan exported variables disimpan terpisah. Assignment-only memperbarui shell state; command-prefix assignment bersifat sementara untuk invocation tersebut.
- External child menerima environment yang diekspor.
- MYSH_PREFIX/bin ditambahkan ke awal PATH bila MYSH_PREFIX disetel dan direktori tersebut tersedia.
- Built-in yang mengubah state, seperti cd, export, dan unset, dijalankan di proses Mysh.
- Fungsi dijalankan di shell aktif dengan positional parameters sementara; nilai pemanggil dipulihkan setelah fungsi selesai.
- Subshell menggunakan salinan shell state dan memulihkan working directory parent setelah eksekusi.
- Command substitution berjalan dalam salinan state terisolasi dan menghapus trailing newline dari hasilnya.
- Executable dicari sesuai urutan PATH; path yang mengandung slash diperlakukan sebagai path eksplisit. Status command-not-found adalah 127, sedangkan executable yang ditemukan tetapi tidak dapat dieksekusi menghasilkan 126.
- Exit code child dipertahankan. Pada Unix, terminasi oleh signal N direpresentasikan sebagai 128 + N.

## Implemented language subset

Parser dan executor saat ini memiliki dukungan awal untuk:

- Command list ;, &&, dan ||, serta negasi pipeline dengan !.
- Pipeline sequential dan buffered, termasuk simple command, brace-group, dan subshell stages.
- Compound commands if/elif/else, case, for, while, until, brace groups, dan subshells.
- Function definition dengan bentuk name() compound-command.
- Basic input/output/append/stderr redirection: <, >, >>, 2>.
- Parameter expansion termasuk positional/special parameters, default/alternate/assignment/required-value forms, dan prefix/suffix pattern removal.
- $(...) dan backquoted command substitution.
- Integer arithmetic expansion dengan subset operator dan assignment.
- Leading tilde expansion, IFS field splitting, dan pathname expansion.

Rincian perilaku yang didukung serta keterbatasan setiap sintaks dicatat dalam Command Reference. Lexer dapat mengenali operator POSIX yang belum didukung parser/executor; lexical recognition bukan bukti dukungan eksekusi.

## Known execution limitations

- Pipeline menggunakan byte buffers antar-stage secara berurutan; belum memakai concurrent connected OS pipes. Compound stages belum menerima input dari stage sebelumnya.
- Background execution dan job control belum diimplementasikan. Pemisah & dipertahankan parser tetapi executor menolaknya.
- Here-documents, descriptor duplication/closure, >|, <>, dan bentuk redirection lanjutan belum didukung.
- Grammar compound command belum menerima seluruh posisi yang diizinkan POSIX.
- Parameter-operator word parsing, seluruh interaksi expansion-order, quote-sensitive case patterns, locale collation, dan beberapa edge case pathname belum lengkap.
- Built-in POSIX, trap/signal semantics, interactive line editing, persistent history, dan job control belum lengkap.

## Portability

Implementasi mengutamakan Rust std; FFI digunakan secara terbatas ketika API platform diperlukan. Contohnya, Android dapat menolak pembacaan hostname dari procfs sehingga platform.rs menyediakan fallback gethostname() melalui libc.

| Platform | Rust target | C library |
| --- | --- | --- |
| Android ARM64 | aarch64-linux-android | Bionic |
| Android x86-64 | x86_64-linux-android | Bionic |
| Linux x86-64 | x86_64-unknown-linux-musl | musl |
| Linux ARM64 | aarch64-unknown-linux-musl | musl |

Android menghasilkan executable ELF standalone, bukan shared library JNI atau APK. Runtime archive Android berisi bin/mysh dan bin/coreutils; aplikasi pemakai bertanggung jawab mengekstrak arsip ke direktori privat dan mengatur MYSH_PREFIX.

## External utilities

Mysh mengimplementasikan shell language dan shell built-ins. Utility command adalah executable eksternal. Android CI membangun pinned uutils Coreutils dengan subset fitur yang dipilih; utility yang tidak disertakan tidak otomatis tersedia. Mysh tidak membungkus utility command sebagai built-in duplikat.

## Wicofe

Wicofe adalah target integrasi yang direncanakan, belum diimplementasikan atau divalidasi. Port membutuhkan ABI, process model, filesystem, terminal/PTY, toolchain, dan Rust userspace runtime yang sesuai. Detail kontrak lintas-repository berada di WICOFE_INTEGRATION.md.
