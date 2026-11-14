//! Incremental shell-input classification and context-sensitive reserved-word helpers.

use crate::lexer::{self, Word};

/// Operator connecting adjacent command-list entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOperator { Sequence, And, Or }

/// A command list split into pipelines and their preceding control operators.
pub type CommandList = Vec<(Vec<Word>, Option<ListOperator>)>;

/// Split tokens into a command list without performing expansion or execution.
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
            commands.push((std::mem::take(&mut current), next_condition.take()));
            next_condition = Some(operator);
        } else { current.push(token); }
    }
    if !current.is_empty() { commands.push((current, next_condition)); }
    else if matches!(next_condition, Some(ListOperator::And | ListOperator::Or)) {
        return Err("conditional operator requires a command after it");
    }
    if commands.is_empty() { return Err("command list is empty"); }
    Ok(commands)
}

/// Split one pipeline into its ordered stages.
pub fn split_pipeline(tokens: Vec<Word>) -> Result<Vec<Vec<Word>>, &'static str> {
    let mut stages = vec![Vec::new()];
    for token in tokens {
        if token.is_unquoted_literal("|") {
            if stages.last().is_some_and(Vec::is_empty) { return Err("empty pipeline stage"); }
            stages.push(Vec::new());
        } else { stages.last_mut().expect("pipeline has a stage").push(token); }
    }
    if stages.last().is_some_and(Vec::is_empty) { return Err("pipeline cannot end with |"); }
    Ok(stages)
}

#[cfg(test)]
mod command_structure_tests {
    use super::{split_command_list, split_pipeline, ListOperator};
    use crate::lexer;
    #[test]
    fn command_lists_preserve_sequence_and_short_circuit_operators() {
        let parsed = split_command_list(lexer::lex("echo one; echo two && echo three || echo four").unwrap()).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].1, None);
        assert_eq!(parsed[1].1, Some(ListOperator::Sequence));
        assert_eq!(parsed[2].1, Some(ListOperator::And));
        assert_eq!(parsed[3].1, Some(ListOperator::Or));
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
        assert_eq!(stages.len(), 3);
        assert_eq!(stages[0].len(), 2);
        assert_eq!(stages[1].len(), 1);
        assert_eq!(stages[2].len(), 1);
    }
    #[test]
    fn pipeline_split_rejects_empty_stages() {
        for input in ["| cat", "echo |", "echo || cat"] {
            assert!(split_pipeline(lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }
    #[test]
    fn quoted_pipe_remains_a_word() {
        let stages = split_pipeline(lexer::lex("echo '|'").unwrap()).unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].len(), 2);
    }
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
    ["if", "then", "elif", "else", "do"]
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
    enum IfPhase {
        AwaitThen,
        InThen,
        InElse,
    }

    let mut stack = Vec::new();
    for index in 0..tokens.len() {
        if is_reserved_word(&tokens, index, "if") {
            stack.push(IfPhase::AwaitThen);
        } else if is_reserved_word(&tokens, index, "then") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != IfPhase::AwaitThen {
                return InputState::Invalid;
            }
            *phase = IfPhase::InThen;
        } else if is_reserved_word(&tokens, index, "elif") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != IfPhase::InThen {
                return InputState::Invalid;
            }
            *phase = IfPhase::AwaitThen;
        } else if is_reserved_word(&tokens, index, "else") {
            let Some(phase) = stack.last_mut() else {
                return InputState::Invalid;
            };
            if *phase != IfPhase::InThen {
                return InputState::Invalid;
            }
            *phase = IfPhase::InElse;
        } else if is_reserved_word(&tokens, index, "fi") {
            let Some(phase) = stack.pop() else {
                return InputState::Invalid;
            };
            if phase == IfPhase::AwaitThen {
                return InputState::Invalid;
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
}
