//! Incremental shell-input classification and context-sensitive reserved-word helpers.

use crate::lexer::{self, Word};

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
    index == 0
        || tokens[index - 1].is_unquoted_literal(";")
        || tokens[index - 1].is_unquoted_literal("&&")
        || tokens[index - 1].is_unquoted_literal("||")
        || tokens[index - 1].is_unquoted_literal("|")
}

/// Classify buffered input without treating every lexer error as continuation.
///
/// Unterminated quotes and a trailing escape are recoverable at a physical
/// line boundary. Other lexical or structural errors are reported immediately.
pub fn classify_input(input: &str) -> InputState {
    let tokens = match lexer::lex(input) {
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

    let mut depth = 0usize;
    for index in 0..tokens.len() {
        if is_reserved_word(&tokens, index, "if") {
            depth += 1;
        } else if is_reserved_word(&tokens, index, "fi") {
            if depth == 0 {
                return InputState::Invalid;
            }
            depth -= 1;
        }
    }

    if depth > 0 {
        InputState::Incomplete
    } else {
        InputState::Complete
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
        assert!(!is_reserved_word(&tokens, 8, "fi"));
        assert!(is_reserved_word(&tokens, 10, "fi"));
    }

    #[test]
    fn unterminated_quotes_are_incomplete_input() {
        assert_eq!(classify_input("echo 'unfinished"), InputState::Incomplete);
    }
}
