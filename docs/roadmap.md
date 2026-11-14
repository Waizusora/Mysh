# Mysh Development Roadmap

## Goal

Mysh aims to become a mature, independently implemented POSIX shell, prioritizing Android's Bionic runtime and Linux systems using musl libc. The normative shell-language target is the Shell Command Language in POSIX.1-2024.

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

**Status: Complete for the Phase 2 grammar/parser milestone.** Command-list parsing consumes complete compound-command spans before splitting list separators. The parser builds pipeline stages for brace groups, subshells, `if`, `case`, `for`, `while`, `until`, and POSIX-style function definitions, including nested compound pipelines. Pipeline negation (`!`) is represented in the AST and its status inversion is applied after execution. Asynchronous-list separators (`&`) are retained as AST metadata; execution is explicitly rejected until Phase 6 job control exists. Brace tokens are recognized as reserved words only in command position, and case pattern alternation/optional parentheses remain inside their case stage. Incremental input classification covers incomplete quotes, brace groups, subshells, case arms, and compound commands, with malformed reserved-word sequences and unmatched closing delimiters rejected. The Phase 2 grammar regression map below records positive and negative parser coverage. Redirection forms unsupported by the current executor are rejected explicitly; their execution semantics remain in Phase 4. This phase completion is not a claim of overall POSIX conformance.

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

Current grammar regression map:
| Grammar area | Representative automated coverage |
| --- | --- |
| Command lists (`;`, `&&`, `||`, `&`) | `command_lists_preserve_sequence_and_short_circuit_operators`, `command_lists_reject_missing_operands_and_retain_background_markers`, `compound_lists_retain_asynchronous_terminators` |
| Pipelines (`|`, leading `!`) | `pipeline_split_preserves_stage_order`, `pipeline_negation_is_reserved_only_at_the_pipeline_start`, `all_compound_command_families_can_be_pipeline_stages` |
| Compound commands in pipelines | `compound_command_is_retained_on_the_right_side_of_a_pipeline`, `nested_compound_lists_preserve_compound_pipeline_stages`, plus shell integration tests for `if`, `case`, `for`, `while`, `until`, brace groups, and subshells |
| `case` pattern grammar | `case_parser_builds_pattern_arms_and_nested_bodies`, `case_parser_accepts_optional_parentheses_before_patterns`, `case_pattern_alternation_and_keyword_subjects_stay_inside_the_case_stage` |
| Reserved-word context and quoting | `only_command_position_words_are_reserved`, `keyword_arguments_do_not_create_following_command_positions`, `brace_arguments_do_not_create_reserved_word_context`, `quoted_and_escaped_keywords_are_not_reserved_words` |
| Incremental input and malformed syntax | `open_if_requires_more_input_but_unmatched_fi_is_invalid`, `malformed_if_keyword_order_is_invalid_without_waiting_for_fi`, `unmatched_esac_is_invalid_input`, `unmatched_command_position_brace_is_rejected` |

The parser retains asynchronous-list metadata, but background execution is intentionally deferred to Phase 6. Redirection forms unsupported by the current executor are rejected explicitly rather than accepted as command arguments.

## Phase 3 — POSIX expansion engine

**Status: Core expansion milestone implemented; full POSIX expansion coverage remains in progress.** Parameter expansion covers named, positional (including braced multi-digit references), special parameters, `${#parameter}`, nested brace matching, default/alternate/assignment/required-value operators, and shortest/longest prefix/suffix pattern removal. Both `$(...)` and legacy backquoted command substitution execute in an isolated shell state; trailing newline bytes are removed and nested substitutions have regression coverage. Arithmetic expansion supports integer literals, variables, parentheses, unary operators, comma sequencing, common arithmetic/bitwise/comparison/logical operators, assignment expressions (`=`, `+=`, `-=`, `*=`, `/=`, `%=`, bitwise and shift assignments), right-associative chained assignments, and prefix/postfix increment/decrement on simple and parenthesized variable lvalues. Arithmetic effects are applied against the mutable expansion environment in source order, including nested parameter-word expansions; skipped logical and conditional branches suppress side effects. Leading unquoted `~` and `~user` expand from `HOME` and the matching `/etc/passwd` home-directory entry, respectively. IFS field splitting preserves quoted/literal characters and positional boundaries, distinguishes IFS whitespace from non-whitespace delimiters, and covers empty/unset IFS plus adjacent delimiters. Pathname expansion supports `*`, `?`, ranges, negation, literal leading `]`, and POSIX character classes evaluated through the active C library locale; pathname results and single-character ranges use locale collation where the platform C library provides it. Bracket expressions also recognize collating elements (`[.collating-symbol.]` form) and equivalence classes (`[=equivalence-class=]` form), comparing candidate elements with the active locale collation function and allowing multi-character collating elements. Unmatched patterns remain literal and leading-dot names are excluded unless explicitly matched. Per-character split/glob eligibility survives mixed quoted and unquoted fragments. Nested command substitutions in parameter-operator words are evaluated only when that word is selected; coverage includes default/alternate words, pattern-removal operands, and command substitutions nested in arithmetic expansion. Remaining work is broader locale-matrix validation and the full interaction semantics of nested expansion in required-parameter diagnostics. Logical `&&`/`||` and conditional `?:` parse skipped operands/branches without evaluating them, including regression cases that suppress division-by-zero in unselected expressions. `$!` remains dependent on background-job support in Phase 6. This milestone does not claim complete POSIX conformance.**

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

Current expansion regression map:
| Expansion area | Representative automated coverage |
| --- | --- |
| Parameter references and operators | `nested_parameter_expansions_and_parameter_lengths_are_supported`, `special_parameter_expansion_includes_shell_pid_and_option_flags`, `parameter_expansion_supports_default_and_alternate_operators`, `assignment_parameter_expansion_persists_unset_and_null_values`, `required_parameter_expansion_reports_unset_and_null_values`, `parameter_expansion_removes_shortest_and_longest_patterns` |
| Positional parameters and IFS | `positional_star_and_at_expand_using_ifs_separator`, `quoted_star_joins_positional_parameters_into_one_field`, `quoted_at_with_no_positional_parameters_expands_to_no_fields`, `ifs_field_splitting_tracks_expansion_origin_and_empty_fields`, `quoted_positional_parameter_expansion_keeps_field_boundaries` |
| Command substitution | `command_substitution_expands_fields_and_removes_trailing_newlines`, `command_substitution_uses_an_isolated_shell_environment`, `backquoted_command_substitution_executes`, `nested_command_substitution_is_evaluated_recursively`, `nested_command_substitution_runs_only_in_selected_parameter_words` |
| Arithmetic and tilde | `arithmetic_expansion_obeys_precedence_and_parentheses`, `arithmetic_prefix_and_postfix_increment_decrement_return_correct_values`, `arithmetic_increment_and_decrement_accept_parenthesized_lvalues_and_complex_operands`, `nested_parameter_and_arithmetic_expansions_preserve_left_to_right_effects`, `arithmetic_side_effects_inside_selected_parameter_words_are_ordered`, `arithmetic_assignment_expansion_updates_shell_variables`, `tilde_expansion_is_unquoted_and_only_at_word_start`, `passwd_home_lookup_matches_exact_user_field` |
| Pathname expansion | `pathname_patterns_use_locale_character_classes_and_collation_order`, `pathname_patterns_support_collating_and_equivalence_elements`, `pathname_patterns_support_posix_ascii_character_classes`, `pathname_expansion_combines_quoted_literals_with_unquoted_patterns`, `pathname_expansion_tracks_quoted_and_unquoted_variable_results`, `pathname_expansion_matches_wildcards_and_keeps_unmatched_patterns` |
| Lexer substitution boundaries | `command_and_arithmetic_substitutions_remain_single_words`, `backquoted_command_substitution_remains_a_single_word` |

The regression map documents the implemented core. It does not replace POSIX conformance testing for expansion interactions, locale behavior, or the remaining arithmetic and parameter-word grammar.

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
- **Android / Bionic:** prioritize ARM64 (`aarch64-linux-android`) and x86-64 (`x86_64-linux-android`) builds and runtime validation against Android's Bionic libc.
- **Linux / musl:** prioritize portable-core and runtime validation for musl-based Linux targets. CI now cross-builds Mysh and MYPK for `x86_64-unknown-linux-musl`; runtime packaging and device-level validation remain separate release tasks.
- **MYPK:** continue as a separate package manager and do not couple its repository or package semantics to the shell parser.

## Current implementation boundary

The roadmap is not a statement that phases 1 through 7 are complete. The current implementation has a native execution foundation, initial compound-command support including brace-group/subshell pipeline stages and basic `case` execution, basic expansion, buffered sequential pipelines, limited redirection, and a subset of built-ins. The lexer recognizes a broader set of POSIX operators, while the parser explicitly rejects unsupported descriptor-redirection forms. The README, architecture document, and command reference remain the source of detail about the current code. Update those documents whenever implementation changes materially alter this boundary.
