//! Incremental shell-input classification and context-sensitive reserved-word helpers.

use crate::lexer::{self, Word};

/// Operator connecting adjacent command-list entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOperator { Sequence, And, Or }

/// A pipeline is an ordered sequence of command stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    pub stages: Vec<Vec<Word>>,
}

/// One pipeline and the operator that controls whether it follows the previous entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandListEntry {
    pub pipeline: Pipeline,
    pub connector: Option<ListOperator>,
}

/// A parsed shell command list.
pub type CommandList = Vec<CommandListEntry>;

/// Split tokens into a command list and parse each pipeline structurally.
pub fn split_command_list(tokens: Vec<Word>) -> Result<CommandList, &'static str> {
    let mut commands = Vec::new();
    let mut current = Vec::new();
    let mut next_condition = None;
    for token in tokens {
        let operator = if token.is_unquoted_literal(";") { Some(ListOperator::Sequence) }
        else if token.is_unquoted_literal("&&") { Some(ListOperator::And) }
        else if token.is_unquoted_literal("||") { Some(ListOperator::Or) }
        else if token.is_unquoted_literal("&") { return Err("background execution with '&' is not supported"); }
        else { None };
        if let Some(operator) = operator {
            if current.is_empty() { return Err("command-list operator requires a command before it"); }
            commands.push(CommandListEntry {
                pipeline: split_pipeline(std::mem::take(&mut current))?,
                connector: next_condition.take(),
            });
            next_condition = Some(operator);
        } else { current.push(token); }
    }
    if !current.is_empty() {
        commands.push(CommandListEntry {
            pipeline: split_pipeline(current)?,
            connector: next_condition,
        });
    } else if matches!(next_condition, Some(ListOperator::And | ListOperator::Or)) {
        return Err("conditional operator requires a command after it");
    }
    if commands.is_empty() { return Err("command list is empty"); }
    Ok(commands)
}

/// Split one pipeline into its ordered stages.
pub fn split_pipeline(tokens: Vec<Word>) -> Result<Pipeline, &'static str> {
    let mut stages = vec![Vec::new()];
    for token in tokens {
        if token.is_unquoted_literal("|") {
            if stages.last().is_some_and(Vec::is_empty) { return Err("empty pipeline stage"); }
            stages.push(Vec::new());
        } else { stages.last_mut().expect("pipeline has a stage").push(token); }
    }
    if stages.last().is_some_and(Vec::is_empty) { return Err("pipeline cannot end with |"); }
    Ok(Pipeline { stages })
}

#[cfg(test)]
mod command_structure_tests {
    use super::{split_command_list, split_pipeline, ListOperator};
    use crate::lexer;
    #[test]
    fn command_lists_preserve_sequence_and_short_circuit_operators() {
        let parsed = split_command_list(lexer::lex("echo one; echo two && echo three || echo four").unwrap()).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].connector, None);
        assert_eq!(parsed[1].connector, Some(ListOperator::Sequence));
        assert_eq!(parsed[2].connector, Some(ListOperator::And));
        assert_eq!(parsed[3].connector, Some(ListOperator::Or));
    }
    #[test]
    fn command_lists_reject_missing_operands_and_background_execution() {
        for input in ["; echo one", "echo one &&", "echo one ||", "echo one;;echo two", "echo one & echo two"] {
            assert!(split_command_list(lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }
    #[test]
    fn pipeline_split_preserves_stage_order() {
        let stages = split_pipeline(lexer::lex("echo one | cat | wc").unwrap()).unwrap();
        assert_eq!(stages.stages.len(), 3);
        assert_eq!(stages.stages[0].len(), 2);
        assert_eq!(stages.stages[1].len(), 1);
        assert_eq!(stages.stages[2].len(), 1);
    }
    #[test]
    fn pipeline_split_rejects_empty_stages() {
        for input in ["| cat", "echo |", "echo | | cat"] {
            assert!(split_pipeline(lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }
    #[test]
    fn quoted_pipe_remains_a_word() {
        let stages = split_pipeline(lexer::lex("echo '|'").unwrap()).unwrap();
        assert_eq!(stages.stages.len(), 1);
        assert_eq!(stages.stages[0].len(), 2);
    }
}



/// Kind of a supported loop compound command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind { While, Until }

/// Parsed loop structure. The executor receives explicit condition and body
/// token sequences instead of rediscovering the outer do/done boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStatement {
    pub kind: LoopKind,
    pub condition: Vec<Word>,
    pub body: Vec<Word>,
}

/// Parse one complete while or until compound command.
pub fn parse_loop_statement(tokens: &[Word]) -> Result<LoopStatement, &'static str> {
    let tokens = if tokens.last().is_some_and(|word| word.is_unquoted_literal(";")) {
        &tokens[..tokens.len() - 1]
    } else { tokens };
    let kind = match tokens.first() {
        Some(word) if word.is_unquoted_literal("while") => LoopKind::While,
        Some(word) if word.is_unquoted_literal("until") => LoopKind::Until,
        _ => return Err("expected while or until"),
    };
    let mut depth = 1usize;
    let mut do_index = None;
    let mut done_index = None;
    for index in 1..tokens.len() {
        if is_reserved_word(tokens, index, "while") || is_reserved_word(tokens, index, "until") {
            depth += 1;
        } else if is_reserved_word(tokens, index, "do") && depth == 1 && do_index.is_none() {
            do_index = Some(index);
        } else if is_reserved_word(tokens, index, "done") {
            depth = depth.saturating_sub(1);
            if depth == 0 { done_index = Some(index); break; }
        }
    }
    let (Some(body_start), Some(body_end)) = (do_index, done_index) else {
        return Err("while or until requires do and done");
    };
    if body_end + 1 != tokens.len() || body_start <= 1 {
        return Err("malformed while or until compound command");
    }
    // The semicolon immediately before "do" and "done" is a list
    // separator, not part of either compound-command payload.
    let condition_end = if tokens[body_start - 1].is_unquoted_literal(";") {
        body_start - 1
    } else {
        body_start
    };
    let body_end = if body_end > body_start + 1
        && tokens[body_end - 1].is_unquoted_literal(";")
    {
        body_end - 1
    } else {
        body_end
    };
    if condition_end <= 1 {
        return Err("while or until requires a condition");
    }
    Ok(LoopStatement {
        kind,
        condition: tokens[1..condition_end].to_vec(),
        body: tokens[body_start + 1..body_end].to_vec(),
    })
}

/// One condition/body pair in an if compound command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfClause {
    pub condition: Vec<Word>,
    pub body: Vec<Word>,
}

/// Parsed if/elif/else compound command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStatement {
    pub clauses: Vec<IfClause>,
    pub else_body: Option<Vec<Word>>,
}

/// Parse a complete if compound command while respecting nested if/fi pairs.
pub fn parse_if_statement(tokens: &[Word]) -> Result<IfStatement, &'static str> {
    let tokens = if tokens.last().is_some_and(|word| word.is_unquoted_literal(";")) {
        &tokens[..tokens.len() - 1]
    } else {
        tokens
    };
    if !tokens.first().is_some_and(|word| word.is_unquoted_literal("if")) {
        return Err("expected if");
    }

    let mut nested = 0usize;
    let mut fi_index = None;
    for index in 1..tokens.len() {
        if is_reserved_word(tokens, index, "if") {
            nested += 1;
        } else if is_reserved_word(tokens, index, "fi") {
            if nested == 0 {
                fi_index = Some(index);
                break;
            }
            nested -= 1;
        }
    }
    let Some(fi_index) = fi_index else {
        return Err("if requires a closing fi");
    };
    if fi_index + 1 != tokens.len() {
        return Err("unexpected tokens after fi");
    }

    let mut clauses = Vec::new();
    let mut cursor = 1usize;
    let mut else_body = None;
    loop {
        let mut depth = 0usize;
        let mut then_index = None;
        for index in cursor..fi_index {
            if is_reserved_word(tokens, index, "if") {
                depth += 1;
            } else if is_reserved_word(tokens, index, "fi") {
                depth = depth.saturating_sub(1);
            } else if depth == 0 && is_reserved_word(tokens, index, "then") {
                then_index = Some(index);
                break;
            }
        }
        let Some(then_index) = then_index else {
            return Err("if clause requires then");
        };
        let mut condition_end = then_index;
        if condition_end > cursor && tokens[condition_end - 1].is_unquoted_literal(";") {
            condition_end -= 1;
        }
        if condition_end == cursor {
            return Err("if condition cannot be empty");
        }

        let body_start = then_index + 1;
        depth = 0;
        let mut delimiter = fi_index;
        for index in body_start..fi_index {
            if is_reserved_word(tokens, index, "if") {
                depth += 1;
            } else if is_reserved_word(tokens, index, "fi") {
                depth = depth.saturating_sub(1);
            } else if depth == 0
                && (is_reserved_word(tokens, index, "elif") || is_reserved_word(tokens, index, "else"))
            {
                delimiter = index;
                break;
            }
        }
        let mut body = tokens[body_start..delimiter].to_vec();
        if body.last().is_some_and(|word| word.is_unquoted_literal(";")) {
            body.pop();
        }
        if body.is_empty() {
            return Err("if branch cannot be empty");
        }
        clauses.push(IfClause {
            condition: tokens[cursor..condition_end].to_vec(),
            body,
        });

        if delimiter == fi_index {
            break;
        }
        if is_reserved_word(tokens, delimiter, "elif") {
            cursor = delimiter + 1;
            if cursor >= fi_index {
                return Err("elif clause cannot be empty");
            }
        } else {
            let start = delimiter + 1;
            if start >= fi_index {
                return Err("else branch cannot be empty");
            }
            depth = 0;
            for index in start..fi_index {
                if is_reserved_word(tokens, index, "if") {
                    depth += 1;
                } else if is_reserved_word(tokens, index, "fi") {
                    depth = depth.saturating_sub(1);
                } else if depth == 0
                    && (is_reserved_word(tokens, index, "elif") || is_reserved_word(tokens, index, "else"))
                {
                    return Err("elif or else cannot follow else");
                }
            }
            let mut body = tokens[start..fi_index].to_vec();
            if body.last().is_some_and(|word| word.is_unquoted_literal(";")) {
                body.pop();
            }
            if body.is_empty() {
                return Err("else branch cannot be empty");
            }
            else_body = Some(body);
            break;
        }
    }

    if clauses.is_empty() {
        return Err("incomplete if statement");
    }
    Ok(IfStatement { clauses, else_body })
}

/// State of a physical input buffer before command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputState {
    /// The buffer contains a syntactically closed input unit.
    Complete,
    /// More input can still complete an open quote or compound command.
    Incomplete,
    /// The buffer already contains an unmatched closing compound keyword.
    Invalid,
}

/// Return true when a token is a reserved word in command position.
///
/// A word used as an ordinary argument is never treated as a reserved word.
/// Quoted and escaped words are excluded by the lexer metadata.
pub fn is_reserved_word(tokens: &[Word], index: usize, keyword: &str) -> bool {
    let Some(token) = tokens.get(index) else {
        return false;
    };
    if !token.is_unquoted_literal(keyword) {
        return false;
    }
    if index == 0 {
        return true;
    }

    let previous = &tokens[index - 1];
    if previous.is_unquoted_literal(";")
        || previous.is_unquoted_literal("&&")
        || previous.is_unquoted_literal("||")
        || previous.is_unquoted_literal("|")
    {
        return true;
    }

    // Compound-list introducers establish a command position only when the
    // introducer itself was recognized as a reserved word in its own context.
    ["if", "then", "elif", "else", "do", "while", "until"]
        .iter()
        .any(|introducer| {
            previous.is_unquoted_literal(introducer)
                && is_reserved_word(tokens, index - 1, introducer)
        })
}

/// Classify buffered input without treating every lexer error as continuation.
///
/// Unterminated quotes and a trailing escape are recoverable at a physical
/// line boundary. Other lexical or structural errors are reported immediately.
fn delimit_physical_newlines(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars().peekable();
    let mut quote = None;
    let mut word_started = false;
    let mut comment = false;

    while let Some(character) = characters.next() {
        if comment {
            if character == '\n' {
                comment = false;
                output.push(';');
                word_started = false;
            }
            continue;
        }

        match quote {
            Some('\'') => {
                output.push(character);
                if character == '\'' {
                    quote = None;
                }
            }
            Some('"') => match character {
                '"' => {
                    output.push(character);
                    quote = None;
                }
                '\\' => {
                    if let Some(next) = characters.next() {
                        if next != '\n' {
                            output.push('\\');
                            output.push(next);
                        }
                    } else {
                        output.push('\\');
                    }
                }
                _ => output.push(character),
            }
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    word_started = true;
                    output.push(character);
                }
                '\\' => {
                    if let Some(next) = characters.next() {
                        if next != '\n' {
                            output.push('\\');
                            output.push(next);
                            word_started = true;
                        }
                    } else {
                        output.push('\\');
                    }
                }
                '#' if !word_started => {
                    comment = true;
                }
                '\n' => {
                    output.push(';');
                    word_started = false;
                }
                value if value.is_whitespace() => {
                    output.push(value);
                    word_started = false;
                }
                '|' | '<' | '>' | ';' | '&' => {
                    output.push(character);
                    word_started = false;
                }
                _ => {
                    output.push(character);
                    word_started = true;
                }
            },
            _ => unreachable!("only single and double quote states are used"),
        }
    }
    output
}

/// Classify buffered input without treating every lexer error as continuation.
///
/// Physical newlines outside quotes are command separators. Unterminated quotes
/// and a trailing escape are recoverable at a physical line boundary. Other
/// lexical or structural errors are reported immediately.
pub fn classify_input(input: &str) -> InputState {
    let normalized = delimit_physical_newlines(input);
    let tokens = match lexer::lex(&normalized) {
        Ok(tokens) => tokens,
        Err(error) => {
            let message = error.to_string();
            return if message == "unterminated quoted string"
                || message == "trailing escape character"
            {
                InputState::Incomplete
            } else {
                InputState::Invalid
            };
        }
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CompoundPhase {
        IfAwaitThen,
        IfInThen,
        IfInElse,
        LoopAwaitDo,
        LoopInBody,
    }

    let mut stack = Vec::new();
    for index in 0..tokens.len() {
        if is_reserved_word(&tokens, index, "if") {
            stack.push(CompoundPhase::IfAwaitThen);
        } else if is_reserved_word(&tokens, index, "while")
            || is_reserved_word(&tokens, index, "until")
        {
            stack.push(CompoundPhase::LoopAwaitDo);
        } else if is_reserved_word(&tokens, index, "then") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != CompoundPhase::IfAwaitThen {
                return InputState::Invalid;
            }
            *phase = CompoundPhase::IfInThen;
        } else if is_reserved_word(&tokens, index, "elif") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != CompoundPhase::IfInThen {
                return InputState::Invalid;
            }
            *phase = CompoundPhase::IfAwaitThen;
        } else if is_reserved_word(&tokens, index, "else") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != CompoundPhase::IfInThen {
                return InputState::Invalid;
            }
            *phase = CompoundPhase::IfInElse;
        } else if is_reserved_word(&tokens, index, "do") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != CompoundPhase::LoopAwaitDo {
                return InputState::Invalid;
            }
            *phase = CompoundPhase::LoopInBody;
        } else if is_reserved_word(&tokens, index, "done") {
            match stack.pop() {
                Some(CompoundPhase::LoopInBody) => {}
                _ => return InputState::Invalid,
            }
        } else if is_reserved_word(&tokens, index, "fi") {
            match stack.pop() {
                Some(CompoundPhase::IfInThen | CompoundPhase::IfInElse) => {}
                _ => return InputState::Invalid,
            }
        }
    }

    if stack.is_empty() {
        InputState::Complete
    } else {
        InputState::Incomplete
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_input, is_reserved_word, InputState};
    use crate::lexer;


    #[test]
    fn loop_parser_builds_typed_while_and_until_nodes() {
        use super::{parse_loop_statement, LoopKind};
        let tokens = lexer::lex("while true; do echo body; done").unwrap();
        let parsed = parse_loop_statement(&tokens).unwrap();
        assert_eq!(parsed.kind, LoopKind::While);
        assert_eq!(parsed.condition.len(), 1);
        assert_eq!(parsed.body.len(), 2);
        let tokens = lexer::lex("until false; do echo body; done").unwrap();
        assert_eq!(parse_loop_statement(&tokens).unwrap().kind, LoopKind::Until);
    }

    #[test]
    fn loop_parser_preserves_nested_loop_as_body_tokens() {
        use super::parse_loop_statement;
        let tokens = lexer::lex("while true; do until false; do echo nested; done; echo outer; done").unwrap();
        let parsed = parse_loop_statement(&tokens).unwrap();
        assert!(parsed.body.iter().any(|word| word.is_unquoted_literal("until")));
        assert!(parsed.body.iter().any(|word| word.is_unquoted_literal("done")));
    }

    #[test]
    fn loop_parser_rejects_incomplete_and_trailing_structure() {
        use super::parse_loop_statement;
        for input in ["while true; done", "while true; do echo body", "while true; do echo body; done; echo extra"] {
            assert!(parse_loop_statement(&lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }

    #[test]
    fn ordinary_arguments_do_not_change_compound_depth() {
        assert_eq!(classify_input("echo if\n"), InputState::Complete);
        assert_eq!(classify_input("echo fi\n"), InputState::Complete);
        assert_eq!(classify_input("echo then\n"), InputState::Complete);
    }

    #[test]
    fn quoted_and_escaped_keywords_are_not_reserved_words() {
        for input in ["echo 'if'\n", "echo \"fi\"\n", r"echo \if"] {
            assert_eq!(classify_input(input), InputState::Complete);
        }
    }

    #[test]
    fn open_if_requires_more_input_but_unmatched_fi_is_invalid() {
        assert_eq!(classify_input("if true; then\n"), InputState::Incomplete);
        assert_eq!(classify_input("fi\n"), InputState::Invalid);
        assert_eq!(classify_input("if true; then echo ok; fi\n"), InputState::Complete);
    }

    #[test]
    fn only_command_position_words_are_reserved() {
        let tokens = lexer::lex("echo if; if true; then echo fi; fi").unwrap();
        assert!(!is_reserved_word(&tokens, 1, "if"));
        assert!(is_reserved_word(&tokens, 3, "if"));
        let after_else = lexer::lex("else if true; then echo ok; fi").unwrap();
        assert!(is_reserved_word(&after_else, 1, "if"));
        assert!(!is_reserved_word(&tokens, 8, "fi"));
        assert!(is_reserved_word(&tokens, 10, "fi"));
    }

    #[test]
    fn keyword_arguments_do_not_create_following_command_positions() {
        for input in ["echo if if", "echo then if", "echo else if", "echo do if"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(!is_reserved_word(&tokens, 2, "if"), "{input}");
        }
        let nested = lexer::lex("if if true; then echo ok; fi; then echo done; fi").unwrap();
        assert!(is_reserved_word(&nested, 1, "if"));
    }

    #[test]
    fn malformed_if_keyword_order_is_invalid_without_waiting_for_fi() {
        assert_eq!(classify_input("if true; else\n"), InputState::Invalid);
        assert_eq!(classify_input("if true; then; elif false; else\n"), InputState::Invalid);
        assert_eq!(classify_input("then\n"), InputState::Invalid);
        assert_eq!(classify_input("else\n"), InputState::Invalid);
    }

    #[test]
    fn quoted_newlines_do_not_split_words() {
        assert_eq!(
            classify_input("if true; then echo \"fi\nnot-a-keyword\"; fi\n"),
            InputState::Complete,
        );
    }

    #[test]
    fn unterminated_quotes_are_incomplete_input() {
        assert_eq!(classify_input("echo 'unfinished"), InputState::Incomplete);
    }

    #[test]
    fn while_and_until_compounds_are_classified_incrementally() {
        assert_eq!(classify_input("while true; do echo ok; done\n"), InputState::Complete);
        assert_eq!(classify_input("until false; do echo ok; done\n"), InputState::Complete);
        assert_eq!(classify_input("while true; do\n"), InputState::Incomplete);
        assert_eq!(classify_input("while true;\n"), InputState::Incomplete);
        assert_eq!(classify_input("done\n"), InputState::Invalid);
        assert_eq!(classify_input("while true; done\n"), InputState::Invalid);
    }

    #[test]
    fn nested_loop_and_if_compounds_keep_their_own_closers() {
        assert_eq!(
            classify_input("while true; do if true; then until false; do echo x; done; fi; done\n"),
            InputState::Complete,
        );
        assert_eq!(
            classify_input("while true; do while false; do echo x; done\n"),
            InputState::Incomplete,
        );
    }

    #[test]
    fn quoted_loop_keywords_are_not_compound_delimiters() {
        for input in ["echo 'while'\n", "echo \"done\"\n", r"echo \\until"] {
            assert_eq!(classify_input(input), InputState::Complete);
        }
    }

    #[test]
    fn if_parser_builds_typed_if_elif_and_else_clauses() {
        use super::parse_if_statement;
        let tokens = lexer::lex("if true; then echo one; elif false; then echo two; else echo three; fi").unwrap();
        let parsed = parse_if_statement(&tokens).unwrap();
        assert_eq!(parsed.clauses.len(), 2);
        assert_eq!(parsed.clauses[0].condition[0].literal_value(), "true");
        assert_eq!(parsed.clauses[1].condition[0].literal_value(), "false");
        assert!(parsed.else_body.is_some());
    }

    #[test]
    fn if_parser_preserves_nested_if_boundaries() {
        use super::parse_if_statement;
        let tokens = lexer::lex("if true; then if false; then echo inner; else echo nested; fi; echo outer; fi").unwrap();
        let parsed = parse_if_statement(&tokens).unwrap();
        assert_eq!(parsed.clauses.len(), 1);
        assert!(parsed.clauses[0].body.iter().any(|word| word.is_unquoted_literal("fi")));
        assert!(parsed.else_body.is_none());
    }

    #[test]
    fn if_parser_rejects_malformed_and_trailing_clauses() {
        use super::parse_if_statement;
        for input in [
            "if true; echo missing_then; fi",
            "if; then echo missing_condition; fi",
            "if true; then; fi",
            "if true; then echo one; else; fi",
            "if true; then echo one; else echo two; elif false; then echo three; fi",
            "if true; then echo one; fi; echo trailing",
        ] {
            assert!(parse_if_statement(&lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }

}
