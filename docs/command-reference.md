# Command Reference

This document describes the commands implemented in the current Milestone 1 source tree.

## Built-ins

| Command | Syntax | Description |
| --- | --- | --- |
| `cd` | `cd [directory]` | Change the current working directory. With no argument, uses `HOME`. |
| `pwd` | `pwd` | Print the current working directory. |
| `ls` | `ls [path ...]` | List directory entries or print file names. |
| `mkdir` | `mkdir [-p] directory ...` | Create directories. `-p` creates missing parent directories. |
| `rmdir` | `rmdir directory ...` | Remove empty directories. |
| `touch` | `touch file ...` | Create files that do not already exist. |
| `cp` | `cp source destination` | Copy one regular file. |
| `mv` | `mv source destination` | Rename or move one filesystem entry. |
| `rm` | `rm [-r] [-f] path ...` | Remove files; `-r` enables recursive directory removal and `-f` ignores missing paths. |
| `cat` | `cat [file ...]` | Write file contents to standard output; reads standard input when no file is supplied. |
| `echo` | `echo [-n] [text ...]` | Write arguments separated by spaces. `-n` suppresses the trailing newline. |
| `clear` | `clear` | Clear a compatible terminal using ANSI escape sequences. |
| `history` | `history` | Display commands entered during the current session. |
| `help` | `help` | Display built-in command help. |
| `exit` | `exit [status]` | Exit the shell with the optional numeric status. |

## External Commands

Commands not recognized as built-ins are launched as external executables. Mysh searches the directories in `PATH` when the command name does not contain a path separator.

External command availability depends on the Android environment, executable permissions, ABI compatibility, dynamic linker, and application sandbox.

## Parsing Rules

- Unquoted whitespace separates arguments.
- Single quotes preserve all characters until the next single quote.
- Double quotes group characters into one argument.
- A backslash escapes the next character outside single quotes.
- An unquoted `#` at the beginning of a token starts a comment.
- Empty quoted strings produce empty arguments.

## Limitations

Milestone 1 does not implement pipelines, redirections, variable expansion, command substitution, wildcard expansion, scripting control structures, or persistent history.
