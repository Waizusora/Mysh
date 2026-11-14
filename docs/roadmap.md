# Mysh Development Roadmap

## Goal

Mysh aims to become a mature, independently implemented POSIX shell for Android, with later Linux and FreeBSD validation. The normative shell-language target is the Shell Command Language in POSIX.1-2024.

Mysh must interpret shell programs itself. Bash, Zsh, BusyBox, and host `/bin/sh` are not runtime interpreters or compatibility targets. Comparison shells may be used only as independent behavioral references during testing.

POSIX shell-language conformance and POSIX utility coverage are separate goals. uutils Coreutils remains the preferred external provider for utilities it implements. Missing utilities must be evaluated as external utilities; they must not be added to the shell merely to fill utility gaps. MYPK remains a separate component.

## Completion policy

A phase is complete only when its required behavior is implemented, documented, and covered by automated tests. A successful compilation alone does not complete a phase. POSIX conformance must not be claimed until the applicable requirements have been systematically verified.

The phase order describes technical dependencies. Work may include narrowly scoped prerequisite fixes from an earlier phase when required by tests or implementation.

## Phase 1 — Native shell foundation

**Status: Foundation established; not a completion claim for POSIX conformance.**

Scope:
- Independent Rust shell executable and command loop.
- Lexer foundation with quote and escape metadata.
- Initial parser and command execution model.
- Shell variables, exported environment, and command lookup.
- Initial built-ins and external executable dispatch.
- Android ARM64 and x86-64 builds.
- uutils Coreutils integration and Android runtime archive layout.

Exit criteria:
- Host tests and Android builds pass for both supported ABIs.
- Runtime package contains the expected executables and can be installed by the Android application into its private files directory.
- Current limitations are accurately documented.
- No unsupported POSIX-conformance claim is made.

## Phase 2 — POSIX grammar and parser

**Status: In progress; brace groups and subshells have recursive AST construction and execution semantics and can be represented as pipeline stages. The parser and executor include initial `case` support, POSIX-style function definitions and invocation, basic function positional parameters, and a function-scoped `return` implementation. Lexical operator coverage has been expanded and unsupported descriptor redirections are rejected explicitly. The grammar and compound-command parser remain partial.**

Scope:
- Implement the complete POSIX shell grammar and reserved-word recognition.
- Complete command lists, pipelines, and all compound-command forms: brace groups, subshells, `if`, `case`, `for`, `while`, and `until`.
- Implement function definitions and invocation.
- Support grammar nesting and valid compound commands in every permitted grammar position, including pipelines.
- Preserve incremental interactive parsing for incomplete quotes, brace groups, subshells, and compound commands while rejecting unmatched closing delimiters and invalid syntax.
- Recognize POSIX compound-command, case terminator, and descriptor-redirection operator tokens without treating quoted lookalikes as operators.
- Reject descriptor-redirection forms that the current execution engine cannot implement, rather than passing them through as command arguments.
- Define syntax-error handling and parser recovery without silently accepting malformed input.

Exit criteria:
- Grammar coverage maps every applicable production to parser tests.
- Positive and negative syntax tests cover nesting, quoting, reserved-word context, and multiline input.
- AST construction preserves the structure required by later execution semantics.

## Phase 3 — POSIX expansion engine

**Status: Not complete; only preliminary parameter substitution exists.**

Scope:
- Implement POSIX parameter expansion, including special and positional parameters and parameter operators.
- Implement command substitution using `$(...)` and the required backquoted form.
- Implement arithmetic expansion.
- Implement field splitting using the applicable IFS rules.
- Implement pathname expansion and its required edge cases.
- Implement quote removal and the specified expansion ordering.
- Preserve quoting and expansion metadata through all phases.

Exit criteria:
- Each expansion class has focused unit and integration tests.
- Tests cover quoted and unquoted contexts, empty values, unset values, IFS behavior, special parameters, and expansion interactions.
- Expansion order and resulting argument vectors match POSIX expectations.

## Phase 4 — POSIX execution semantics

**Status: Partial; buffered pipelines now accept brace-group and subshell stages, but compound-stage input delivery and concurrent pipe execution remain incomplete. Redirection semantics are also limited.**

Scope:
- Implement concurrent pipelines with operating-system pipes rather than sequential buffering.
- Implement POSIX redirections, descriptor duplication and closure, and here-documents.
- Complete subshell environment semantics and compound-command execution semantics; the initial isolated subshell execution path is implemented.
- Implement command-prefix and assignment-only command behavior.
- Implement command search, command classification, and special-builtin precedence according to POSIX.
- Implement command substitution execution environments and exit-status propagation.
- Handle signals, child lifecycle, wait status, and execution errors consistently.

Exit criteria:
- Pipeline stages execute concurrently and correctly propagate input, output, signals, and status.
- Redirection ordering and descriptor behavior pass dedicated tests.
- State changes occur in the correct shell, subshell, function, and pipeline environments.
- Command dispatch follows POSIX precedence and status rules.

## Phase 5 — POSIX shell built-ins

**Status: Incomplete; only a subset is implemented.**

Scope:
- Implement every POSIX special built-in with required semantics.
- Implement every POSIX regular built-in that belongs to the shell.
- Validate option parsing, operands, variable attributes, return statuses, and diagnostics.
- Implement function-local and positional-parameter behavior where required by shell semantics.
- Ensure built-ins behave correctly in functions, pipelines, subshells, and special execution contexts.

Exit criteria:
- The implemented built-in inventory is reconciled against POSIX.1-2024.
- Each built-in has positive, negative, and edge-case tests.
- Special-builtin precedence, assignment persistence, and error consequences are verified.
- Utility commands are not incorrectly classified as shell built-ins.

## Phase 6 — Interactive shell, jobs, and terminal behavior

**Status: Not complete.**

Scope:
- Implement job control, including process groups and foreground/background jobs.
- Implement `jobs`, `fg`, and `bg` semantics with the corresponding shell state.
- Implement terminal ownership and foreground process-group management.
- Implement interactive signal handling and interruption behavior.
- Add persistent command history and reliable line editing.
- Define prompt behavior and interactive-only diagnostics without changing non-interactive script semantics.

Exit criteria:
- Foreground and background jobs can be started, observed, resumed, and waited for correctly.
- Terminal control is stable under interruption, suspension, and child termination.
- Interactive input, history, and prompts work without regressing script execution.
- Android terminal behavior is validated on supported devices and ABI builds.

## Phase 7 — Conformance, security, and release stabilization

**Status: Not complete; this is the final qualification phase.**

Scope:
- Build a traceable POSIX.1-2024 shell requirement and test-coverage matrix.
- Run applicable POSIX test suites and add project regression tests.
- Differential-test behavior against independent shells such as dash, while treating POSIX.1-2024 as the normative authority.
- Fuzz lexer, parser, expansion, and script-input boundaries.
- Stress-test pipelines, descriptors, process lifecycle, and resource exhaustion.
- Audit filesystem access, environment handling, executable lookup, and Android sandbox assumptions.
- Validate runtime packaging, upgrades, and clean installation for ARM64 and x86-64.
- Keep CI reproducible and resolve compiler, Clippy, linker, and workflow warnings.

Exit criteria:
- All declared release requirements have traceable passing tests or documented, explicitly scoped platform limitations.
- No known critical or high-severity correctness/security defects remain open.
- CI passes on supported host and Android targets.
- Release documentation accurately states tested behavior and remaining limitations.
- POSIX conformance is claimed only to the extent supported by the conformance evidence.

## Separate utility and platform work

- **External utilities:** maintain a separately documented inventory of POSIX utilities, uutils-provided commands, and utility gaps. Utility coverage does not determine shell-language conformance.
- **Android:** keep ARM64 (`aarch64-linux-android`) and x86-64 (`x86_64-linux-android`) as the supported Android targets.
- **Linux:** validate the shared implementation after the POSIX shell foundation is stable.
- **FreeBSD:** add a dedicated platform integration after the portable core is stable. OpenBSD and NetBSD are not committed targets.
- **MYPK:** continue as a separate package manager and do not couple its repository or package semantics to the shell parser.

## Current implementation boundary

The roadmap is not a statement that phases 1 through 7 are complete. The current implementation has a native execution foundation, initial compound-command support including brace-group/subshell pipeline stages and basic `case` execution, basic expansion, buffered sequential pipelines, limited redirection, and a subset of built-ins. The lexer recognizes a broader set of POSIX operators, while the parser explicitly rejects unsupported descriptor-redirection forms. The README, architecture document, and command reference remain the source of detail about the current code. Update those documents whenever implementation changes materially alter this boundary.
