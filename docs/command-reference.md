# Command Reference

This document describes the commands implemented in the current Mysh source tree.

## Built-ins

| Command | Syntax | Description |
| --- | --- | --- |
| `cd` | `cd [directory]` | Change the current working directory. With no argument, uses `HOME`. |
| `export` | `export [name[=value] ...]` | Set environment values or confirm that existing names are exported. With no arguments, list the current environment. |
| `unset` | `unset name ...` | Remove environment variables from the current shell. |
| `pwd` | `pwd` | Print the current working directory. |
| `ls` | `ls [-a] [path ...]` | List directory entries. Hidden entries are omitted unless `-a` is supplied. |
| `mkdir` | `mkdir [-p] directory ...` | Create directories. `-p` creates missing parent directories. |
| `rmdir` | `rmdir directory ...` | Remove empty directories. |
| `touch` | `touch file ...` | Create files and update access and modification timestamps. |
| `cp` | `cp [-r] source ... destination` | Copy regular files. `-r` or `-R` enables recursive directory copying. Multiple sources require an existing destination directory. |
| `mv` | `mv source ... destination` | Rename or move paths. Multiple sources require an existing destination directory. |
| `rm` | `rm [-r] [-f] [-d] path ...` | Remove files. `-r` or `-R` removes directory trees, `-f` ignores missing paths, and `-d` removes empty directories. Combined short options such as `-rf` are accepted. Removal of the current working directory, its ancestors, and filesystem roots is refused. |
| `cat` | `cat [file ...]` | Write file contents to standard output; reads standard input when no file is supplied. |
| `echo` | `echo [-n] [text ...]` | Write arguments separated by spaces. `-n` suppresses the trailing newline. |
| `clear` | `clear` | Clear a compatible terminal using ANSI escape sequences. |
| `history` | `history` | Display commands entered during the current session. |
| `help` | `help` | Display built-in command help. |
| `exit` | `exit [status]` | Exit the shell with the optional numeric status. |

### Filesystem behavior

- `cp` copies regular files and, with `-r`, recursively copies directory trees.
- Symbolic links and special filesystem entries are rejected by recursive copy. Mysh does not follow them implicitly.
- Recursive copy refuses a destination located inside the source directory.
- Recursive copy rejects symbolic links and special filesystem entries rather than following or reproducing them.
- `rm -r` refuses to remove the current working directory, any of its ancestors, or a filesystem root.
- `mv` uses the operating system rename operation. Cross-filesystem moves are not emulated.
- `rm` does not follow symbolic links when deciding whether an entry is a directory.
- The `--` option terminator is supported by `ls`, `mkdir`, `rmdir`, `touch`, `cp`, `mv`, `rm`, and `cat`; it allows paths beginning with a hyphen to be passed as operands.
- Unsupported leading options are rejected by `mkdir`, `rmdir`, `touch`, and `cat` instead of being interpreted as paths.
- `cat -` reads from standard input.
- Filesystem permissions, Android sandboxing, and operating-system policy remain authoritative.

## External Commands

Commands not recognized as built-ins are launched as external executables. Mysh searches the inherited `PATH` value from left to right. An empty `PATH` component denotes the current directory. A command containing `/` is treated as an explicit relative or absolute path and is not searched through `PATH`. Mysh checks that a PATH candidate is a regular executable file before launching it.

An unresolved command returns status 127. A command found in `PATH` but lacking executable permission returns status 126. External command availability also depends on ABI compatibility, dynamic linker availability, and application sandbox restrictions. For example, `ping` is not a built-in command; it runs only when a compatible executable is available through `PATH`.

Child-process exit codes are returned unchanged when the process exits normally. On Unix-like systems, a process terminated by signal N is represented as status 128 + N.

## Parsing Rules

- Unquoted whitespace separates arguments.
- Single quotes preserve all characters until the next single quote and suppress variable expansion.
- Double quotes group characters into one argument; basic variable expansion remains enabled inside them.
- Adjacent quoted and unquoted fragments are combined into one word.
- Double quotes group characters into one argument.
- A backslash escapes the next character outside single quotes. Escaped dollar signs are not expanded.
- A backslash-newline pair is treated as a line continuation.
- An unquoted `#` at the beginning of a token starts a comment.
- Empty quoted strings produce empty arguments.

## Command lists, pipelines, and redirections

The command-list parser supports sequential commands separated by `;`, plus left-associative conditional lists using `&&` and `||`. `&&` runs its right-hand command only when the preceding command succeeds; `||` runs its right-hand command only when the preceding command fails. A trailing `;` is accepted. Background execution with `&` is not implemented and is reported as a syntax error.

The current implementation supports basic sequential pipelines, for example `echo hello | cat` or `echo hello|cat`. Pipeline output is buffered between stages rather than streamed through concurrently connected operating-system pipes. The `cat` built-in accepts pipeline input and input-redirection data.

Basic redirection operators `<`, `>`, `>>`, and `2>` are recognized both with and without surrounding whitespace. For example, `echo hello>output.txt` and `echo hello > output.txt`. Redirection behavior inside multi-stage pipelines is not yet complete.

The shell tracks shell variables separately from exported variables. `NAME=value` creates or updates a persistent shell variable without exporting it. `NAME=value command` applies a temporary exported assignment to that command invocation. Assignments preceding the currently implemented POSIX special built-ins (`export`, `unset`, and `exit`) persist in the current shell environment. `export NAME=value` sets and exports a variable; `export NAME` marks an existing or unset name for export. `unset NAME` removes both the variable and its export attribute. External commands receive only exported variables. When `MYSH_PREFIX` is set and `$MYSH_PREFIX/bin` exists, Mysh prepends that directory to `PATH` at startup. Pipeline stages use isolated snapshots, but pipelines are still sequential and buffered. Environment-variable references such as `$HOME` and `${HOME}` and the last-exit-status parameter `$?` / `${?}` are substituted in unquoted and double-quoted fragments. Single-quoted and backslash-escaped text is preserved literally. Expansion remains preliminary: other POSIX special and positional parameters, parameter operators, field splitting, pathname expansion, command substitution, and arithmetic expansion are not implemented.

## Limitations

Mysh is not yet POSIX-conformant. Concurrent pipelines, complete POSIX quoting and expansion phases, field splitting, pathname expansion, command substitution, control-flow syntax, job control, and persistent history remain unimplemented.
