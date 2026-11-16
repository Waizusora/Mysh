# Command Reference

This document distinguishes shell built-ins from external utilities and records current source behavior. It does not claim POSIX conformance.

## Built-ins and special commands

| Command | Syntax | Current behavior |
| --- | --- | --- |
| `:` | `:` | No-op; status 0. |
| `.` | `. file` | Reads and executes a shell file in the current shell. Positional arguments specific to sourced files are not fully supported. |
| `cd` | `cd [directory]`, `cd -` | Changes the working directory; without an operand uses HOME; - uses OLDPWD and prints the new directory. |
| `export` | `export [name[=value] ...]`, `export -p` | Sets export attributes/values and lists exported variables. |
| `unset` | `unset name ...` | Removes shell variables and export attributes. |
| `pwd` | `pwd [-L|-P]` | Prints the current directory; logical/physical path distinctions are incomplete. |
| `true`, `false` | no arguments | Return status 0 and 1. |
| `eval` | `eval [argument ...]` | Joins arguments with spaces and executes the resulting command text in the current shell. |
| `break`, `continue` | `[n]` | Controls the active loop; the level must be a positive integer. |
| `return` | `return [status]` | Exits the active function; rejected outside a function. Return from a dot-sourced script is incomplete. |
| `shift` | `shift [n]` | Shifts the active positional parameters; default is one. |
| `exit` | `exit [status]` | Exits the shell with the requested status. |

Some built-ins are dispatched in shell.rs rather than all being implemented in builtins/mod.rs.

## Supported shell grammar

- Command lists: ;, &&, ||.
- Pipelines: `|` and pipeline negation with `!`. All-external simple-command pipelines use connected OS pipes and drain each child stderr concurrently with the final stdout stream. Pipelines containing shell built-ins, functions, compound stages, or unresolved commands use a sequential buffered fallback.
- Compound commands: { list; }, ( list ), if/then/elif/else/fi, case/in/esac, for, while, and until.
- Function definitions: name() compound-command.
- Basic redirection: <, >, >>, 2>.

Function example:

    greet() {
        echo "Hello, $1"
    }
    greet world

Control-flow example:

    for item in one two three; do
        echo "$item"
    done

    if test -n "$HOME"; then
        echo "$HOME"
    else
        echo "HOME is empty"
    fi

Compound grammar is not accepted in every position permitted by POSIX. Case patterns support a subset of *, ?, and bracket patterns; quote-sensitive matching and locale-sensitive behavior remain incomplete.

## Expansion

| Expansion | Status |
| --- | --- |
| Named, positional, and special parameters | Initial support, including $?, $#, $1..., $@, $*, $$, $-, and $! when available |
| Braced parameters | Subset of multi-digit, length, default/alternate/assignment/required-value, and prefix/suffix pattern-removal forms |
| Command substitution | $(...) and backquoted forms; isolated state, trailing newlines removed |
| Arithmetic expansion | Integer expressions and a subset of operators/assignments |
| Tilde expansion | Leading unquoted ~ from HOME |
| IFS field splitting | Partial POSIX behavior; quoted/literal fragments are preserved |
| Pathname expansion | *, ?, bracket ranges/negation, ASCII character classes; unmatched patterns remain literal |

Expansion order and interactions are not fully POSIX-compliant. Parsing of word operands in parameter operators is not fully quote/whitespace-aware. Locale collation and some pathname edge cases are unsupported.

## Variables and environment

- The initial environment is copied from the parent process.
- Shell variables and exported variables are tracked separately.
- NAME=value is a persistent shell assignment and is not automatically exported.
- NAME=value command is a temporary command-prefix assignment.
- External processes receive exported variables only.
- When MYSH_PREFIX is set and $MYSH_PREFIX/bin exists, that directory is prepended to PATH.
- Functions temporarily replace positional parameters; caller state is restored when the function completes.

## External utilities

Utilities such as ls, cat, cp, mv, rm, mkdir, rmdir, touch, echo, printf, sort, uniq, and wc are not Mysh built-ins. Their availability depends on installed external executables.

Android runtime CI builds pinned uutils Coreutils with a selected feature set. Mysh can dispatch supported utilities through the coreutils multicall executable when that utility is included in the build. Each Android runtime archive contains bin/mysh and bin/coreutils; the Android application must extract the archive into its private files directory and set MYSH_PREFIX. Utilities outside the selected feature set are not automatically available.

Commands containing / are treated as explicit paths. Other command names are searched through PATH in order. A missing command returns status 127; a found but non-executable command returns status 126. Normal child exit codes are preserved; signal N termination on Unix maps to status 128 + N.

## Lexer notes

- Unquoted whitespace separates words.
- Single quotes preserve their contents literally.
- Double quotes group text and allow supported expansions.
- Adjacent quoted/unquoted fragments form one word.
- Backslash escapes the following character outside single quotes; backslash-newline is a line continuation.
- An unquoted # at a token boundary starts a comment.
- Empty quoted strings produce empty arguments.
- The lexer recognizes more POSIX operators than the executor supports. Unsupported descriptor redirections, here-document operators, and other forms are rejected; token recognition does not imply feature support.

## Pipeline and redirection limitations

Only pipelines consisting entirely of simple external commands use concurrent OS pipes. In this path, all stages are spawned with connected stdin/stdout and each stderr stream is drained concurrently with the final stdout stream. This avoids blocking when an intermediate stage writes substantial stderr while downstream stages are still consuming data. Pipelines that require in-process shell execution (built-ins, functions, compound commands, or unresolved external commands) use the sequential buffered fallback. Compound stages in that fallback do not yet receive input from the previous stage. Background execution with & is not implemented and is rejected by the executor.

Only <, >, >>, and 2> are included in the basic redirection implementation. Here-documents, descriptor duplication/closure, >|, <>, and advanced redirections are unsupported.

## Not implemented or incomplete

- Overall POSIX.1-2024 shell conformance.
- Background jobs, job control, process groups, traps, and interactive signal semantics.
- Concurrent OS-pipe execution for mixed pipelines involving built-ins/functions/compound stages, and complete redirection semantics.
- Here-documents and descriptor duplication/closure.
- The full set of POSIX built-ins and complete special-builtin error behavior.
- Complete parameter/field/pathname expansion semantics, locale-sensitive pattern matching, and all compound-grammar positions.
- Interactive line editing and persistent history.
