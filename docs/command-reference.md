# Command Reference

Dokumen ini membedakan shell built-ins dari utility eksternal dan mencatat perilaku source saat ini. Daftar ini bukan klaim konformansi POSIX.

## Built-ins dan special commands

| Command | Syntax | Perilaku saat ini |
| --- | --- | --- |
| : | : | No-op; status 0. |
| . | . file | Membaca dan menjalankan file shell di shell aktif. Argumen positional khusus untuk sourced file belum didukung lengkap. |
| cd | cd [directory], cd - | Mengubah working directory; tanpa argumen memakai HOME; - memakai OLDPWD dan mencetak direktori baru. |
| export | export [name[=value] ...], export -p | Mengatur export attribute/assignment dan menampilkan daftar exported variables. |
| unset | unset name ... | Menghapus shell variable dan export attribute. |
| pwd | pwd [-L|-P] | Mencetak current directory; perbedaan logical/physical belum lengkap. |
| true, false | tanpa argumen | Menghasilkan status 0 dan 1. |
| eval | eval [argument ...] | Menggabungkan argumen dengan spasi lalu mengeksekusi command text dalam shell aktif. |
| break, continue | [n] | Mengontrol loop aktif; level harus integer positif. |
| return | return [status] | Mengakhiri pemanggilan fungsi aktif; di luar fungsi ditolak. Return dari dot-sourced script belum lengkap. |
| shift | shift [n] | Menggeser positional parameters aktif; default satu. |
| exit | exit [status] | Mengakhiri shell dengan status yang diminta. |

Sebagian built-in ditangani pada dispatcher di shell.rs, bukan seluruhnya di builtins/mod.rs.

## Shell grammar yang tersedia

- Command list: ;, &&, ||.
- Pipeline: | dan pipeline negation dengan !.
- Compound commands: { list; }, ( list ), if/then/elif/else/fi, case/in/esac, for, while, dan until.
- Function definition: name() compound-command.
- Redirection dasar: <, >, >>, 2>.

Contoh fungsi:

    greet() {
        echo "Hello, $1"
    }
    greet world

Contoh control flow:

    for item in one two three; do
        echo "$item"
    done

    if test -n "$HOME"; then
        echo "$HOME"
    else
        echo "HOME is empty"
    fi

Compound grammar belum diterima di semua posisi yang diizinkan POSIX. Case pattern mendukung subset *, ?, dan bracket patterns; quote-sensitive matching dan locale-sensitive behavior belum lengkap.

## Expansion

| Expansion | Status |
| --- | --- |
| Named, positional, dan special parameters | Dukungan awal, termasuk $?, $#, $1..., $@, $*, $$, $-, dan $! bila tersedia |
| Braced parameters | Subset multi-digit, length, default/alternate/assignment/required-value, prefix/suffix pattern removal |
| Command substitution | $(...) dan backquoted form; state terisolasi, trailing newlines dihapus |
| Arithmetic expansion | Integer expression dan subset operator/assignment |
| Tilde expansion | Leading unquoted ~ dari HOME |
| IFS field splitting | Subset POSIX; quoted/literal fragments dipertahankan |
| Pathname expansion | *, ?, bracket ranges/negation, ASCII character classes; unmatched pattern tetap literal |

Expansion order dan interaksi antar-ekspansi belum sepenuhnya sesuai POSIX. Parsing word pada parameter operators belum sepenuhnya quote/whitespace-aware. Locale collation dan beberapa pathname edge cases belum didukung.

## Variables and environment

- Environment awal disalin dari proses induk.
- Shell variables dan exported variables dilacak terpisah.
- NAME=value adalah persistent shell assignment tanpa otomatis export.
- NAME=value command adalah temporary command-prefix assignment.
- External process hanya menerima exported variables.
- Jika MYSH_PREFIX disetel dan $MYSH_PREFIX/bin tersedia, direktori tersebut diprioritaskan di awal PATH.
- Fungsi sementara mengubah positional parameters; caller state dipulihkan setelah fungsi selesai.

## External utilities

Utility seperti ls, cat, cp, mv, rm, mkdir, rmdir, touch, echo, printf, sort, uniq, dan wc bukan built-ins Mysh. Ketersediaannya bergantung pada executable eksternal yang terpasang.

Android runtime CI membangun pinned uutils Coreutils dengan feature subset tertentu. Mysh dapat menjalankan utility melalui executable coreutils multicall ketika utility tersebut termasuk dalam daftar yang dikompilasi. Runtime archive per ABI berisi bin/mysh dan bin/coreutils; aplikasi Android harus mengekstrak arsip ke private files directory dan mengatur MYSH_PREFIX. Utility di luar feature set tidak otomatis tersedia.

Command yang mengandung / diperlakukan sebagai explicit path. Nama command lainnya dicari pada PATH secara berurutan. Command tidak ditemukan menghasilkan status 127; command ditemukan tetapi tidak executable menghasilkan status 126. Exit code normal child dipertahankan; terminasi signal N pada Unix menjadi status 128 + N.

## Lexer notes

- Unquoted whitespace memisahkan words.
- Single quotes mempertahankan isi secara literal.
- Double quotes mengelompokkan teks dan tetap mengizinkan ekspansi yang didukung.
- Adjacent quoted/unquoted fragments menjadi satu word.
- Backslash meng-escape karakter berikutnya di luar single quotes; backslash-newline adalah line continuation.
- Unquoted # pada awal token memulai komentar.
- Empty quoted strings menghasilkan argumen kosong.
- Lexer mengenali lebih banyak operator POSIX daripada yang dapat dieksekusi. Descriptor redirection, here-document operators, dan bentuk yang belum didukung ditolak parser/executor; pengenalan token bukan dukungan fitur.

## Pipeline and redirection limitations

Pipeline saat ini sequential dan buffered, bukan concurrent OS pipes. Input/output antar-stage menggunakan buffer; compound stage belum menerima input dari stage sebelumnya. Background execution dengan & belum didukung dan ditolak executor.

Hanya <, >, >>, dan 2> yang menjadi cakupan redirection dasar. Here-documents, descriptor duplication/closure, >|, <>, dan redirection lanjutan belum didukung.

## Not implemented / incomplete

- POSIX.1-2024 shell conformance secara keseluruhan.
- Background jobs, job control, process groups, traps, dan interactive signal semantics.
- Concurrent pipelines dan complete redirection semantics.
- Here-documents dan descriptor duplication/closure.
- Seluruh POSIX built-ins dan complete special-builtin error behavior.
- Complete parameter/field/pathname expansion semantics, locale-sensitive pattern matching, dan semua compound grammar positions.
- Interactive line editing dan persistent history.
