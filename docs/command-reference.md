# Command Reference

This document describes the commands implemented in the current Mysh source tree.

## Built-ins

| Command | Syntax | Description |
| --- | --- | --- |
| `cd` | `cd [directory]` | Change the current working directory. With no argument, uses `HOME`. |
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

Commands not recognized as built-ins are launched as external executables. Mysh uses the inherited process environment and standard process lookup rules for `PATH`. A command containing a path separator is resolved as the supplied path.

External command availability depends on executable permissions, ABI compatibility, dynamic linker availability, and application sandbox restrictions. For example, `ping` is not a built-in command; it runs only when a compatible executable is available through `PATH`.

Child-process exit codes are returned unchanged when the process exits normally. On Unix-like systems, a process terminated by signal N is represented as status 128 + N.

## Parsing Rules

- Unquoted whitespace separates arguments.
- Single quotes preserve all characters until the next single quote.
- Double quotes group characters into one argument.
- A backslash escapes the next character outside single quotes.
- An unquoted `#` at the beginning of a token starts a comment.
- Empty quoted strings produce empty arguments.

## Limitations

Mysh does not yet implement pipelines, input/output redirections, variable expansion, command substitution, wildcard expansion, scripting control structures, job control, or persistent history.
