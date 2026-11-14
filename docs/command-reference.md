# Command Reference

This document describes the current implementation and distinguishes shell built-ins from external utilities.

## POSIX Shell Built-ins

| Command | Syntax | Current behavior |
| --- | --- | --- |
| \`:\` | \`:\` | No-op special built-in; returns status 0. |
| \`cd\` | \`cd [directory]\` or \`cd -\` | Changes the current directory. With no operand, uses \`HOME\`; \`-\` changes to \`OLDPWD\` and prints the new directory. |
| \`export\` | \`export [name[=value] ...]\` or \`export -p\` | Marks variables for export and optionally assigns values. With no operands or \`-p\`, lists exported variables. |
| \`unset\` | \`unset name ...\` | Removes shell variables and their export attributes. |
| \`pwd\` | \`pwd [-L|-P]\` | Prints the current working directory. The options are accepted; logical-versus-physical path semantics remain incomplete. |
| \`true\` | \`true\` | Returns status 0. |
| \`false\` | \`false\` | Returns status 1. |
| \`break\` | \`break [n]\` | Exits the current loop or the requested number of enclosing loops. |
| \`continue\` | \`continue [n]\` | Starts the next iteration of the current loop or the requested enclosing loop. |
| \`exit\` | \`exit [status]\` | Exits Mysh with the requested numeric status. |

The built-in set is incomplete. Additional POSIX special and regular built-ins remain to be implemented. The current built-ins are not evidence of full POSIX conformance.

## External Utilities

Utilities are not implemented as Mysh built-ins. The intended external provider is uutils Coreutils 0.12.0, pinned to commit \`dc1efd89948a9ca4c78c3a4b9a6ac891019a8c69\` and built with the Android \`feat_os_unix_android\` feature set.

Examples include \`ls\`, \`cat\`, \`cp\`, \`mv\`, \`rm\`, \`mkdir\`, \`rmdir\`, \`touch\`, \`echo\`, \`printf\`, \`sort\`, \`uniq\`, and \`wc\`. Utilities absent from the selected uutils feature set require a separate external-utility decision; they must not be added as shell built-ins merely because uutils does not provide them.

Mysh resolves an external command in this order:

1. If the command name contains \`/\`, treat it as an explicit path.
2. Otherwise search executable files in \`PATH\`, from left to right.
3. If no standalone executable is found and the name belongs to the pinned uutils Android utility set, search for \`coreutils\` in \`PATH\` and invoke it as \`coreutils utility arguments...\`.
4. If neither route succeeds, report command-not-found with status 127. A found but non-executable command returns status 126.

The runtime package must place the uutils \`coreutils\` multicall binary in a directory included in Mysh \`PATH\`. CI builds and publishes the binary as an ABI-specific artifact; the Android runtime packaging step is not yet complete. Commands containing \`/\` are never redirected through the multicall fallback.

External command availability also depends on ABI compatibility, dynamic linker availability, and Android application sandbox restrictions. Child-process exit codes are returned unchanged when the process exits normally. On Unix-like systems, a process terminated by signal N is represented as status 128 + N.

## Parsing Rules

- Unquoted whitespace separates arguments.
- Single quotes preserve all characters until the next single quote and suppress variable expansion.
- Double quotes group characters into one argument; basic variable expansion remains enabled inside them.
- Adjacent quoted and unquoted fragments are combined into one word.
- A backslash escapes the next character outside single quotes. Escaped dollar signs are not expanded.
- A backslash-newline pair is treated as a line continuation.
- An unquoted \`#\` at the beginning of a token starts a comment.
- Empty quoted strings produce empty arguments.

These rules are partial and do not yet implement the complete POSIX token-recognition and expansion model.

## Command lists, pipelines, and redirections

The command-list parser supports sequential commands separated by \`;\`, plus left-associative conditional lists using \`&&\` and \`||\`. The implemented \`if\` evaluator accepts \`if\`, \`elif\`, and \`else\` clauses, including nested and multiline input. \`for\`, \`while\`, and \`until\` compound commands are partially implemented. Reserved words are recognized only at command boundaries; quoted and escaped keyword-looking words remain ordinary words.

The current implementation supports sequential, buffered pipelines. Pipeline output is collected between stages rather than streamed through concurrently connected operating-system pipes. External utilities such as \`cat\` receive pipeline input through standard input.

Basic redirection operators \`<\`, \`>\`, \`>>\`, and \`2>\` are recognized. Redirection behavior inside multi-stage pipelines is not yet complete. POSIX here-documents are not implemented.

## Variables and environment

The shell tracks shell variables separately from exported variables. \`NAME=value\` creates or updates a persistent shell variable without exporting it. \`NAME=value command\` applies a temporary assignment to that command invocation. Assignments preceding implemented POSIX special built-ins persist in the current shell environment. External commands receive only exported variables.

When \`MYSH_PREFIX\` is set and \`$MYSH_PREFIX/bin\` exists, Mysh prepends that directory to \`PATH\` at startup. Pipeline stages currently use isolated snapshots, and pipelines remain sequential and buffered.

Environment-variable references such as \`$HOME\` and \`\${HOME}\` and the last-exit-status parameter \`$?\` / \`\${?}\` are currently substituted in unquoted and double-quoted fragments. Single-quoted and backslash-escaped text is preserved literally. Expansion remains preliminary: positional parameters, parameter operators, field splitting, pathname expansion, command substitution, and arithmetic expansion are not implemented.

## Limitations

Mysh is not yet POSIX-conformant. Remaining work includes complete token recognition and grammar, POSIX expansion order, positional parameters, functions, \`case\`, here-documents, concurrent pipelines, complete redirection semantics, the remaining POSIX built-ins, traps, signal semantics, job control, and interactive-shell requirements. POSIX.1-2024 conformance must be established by dedicated tests, not inferred from the presence of individual commands.

## Command Ownership and Migration Status

- Mysh owns the POSIX shell language engine and commands that POSIX requires as shell built-ins.
- uutils Coreutils owns external utilities that are included in the pinned Android feature set.
- Duplicate filesystem utility implementations have been removed from the Mysh built-in module. Commands such as \`ls\`, \`mkdir\`, \`rmdir\`, \`touch\`, \`cp\`, \`mv\`, \`rm\`, \`cat\`, and \`echo\` now go through external command resolution.
- The multicall fallback is covered by a shell test using a controlled \`coreutils\` executable. Full integration still requires successful uutils builds for all Android ABIs and runtime packaging into the Mysh prefix.
- Utilities absent from uutils are separate external utilities, not shell built-ins.
- Bash-only commands, syntax, and expansions are outside the supported language and command contract.
