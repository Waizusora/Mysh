//! Tokenize shell words while retaining quote and escape metadata.
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError(&'static str);

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for LexError {}

/// A fragment of a shell word. Expansion is enabled only where POSIX quoting
/// rules permit parameter, command, or arithmetic expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordPart {
    pub text: String,
    pub expandable: bool,
    pub quoted: bool,
}

/// A shell word made from one or more quoted or unquoted fragments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Word {
    parts: Vec<WordPart>,
    quoted: bool,
}

impl Word {
    fn push(&mut self, text: char, expandable: bool, quoted: bool) {
        if let Some(part) = self.parts.last_mut() {
            if part.expandable == expandable && part.quoted == quoted {
                part.text.push(text);
                return;
            }
        }
        self.parts.push(WordPart {
            text: text.to_string(),
            expandable,
            quoted,
        });
    }

    fn mark_quoted(&mut self) {
        self.quoted = true;
    }

    fn operator(value: &str) -> Self {
        let mut word = Self::default();
        for character in value.chars() {
            word.push(character, true, false);
        }
        word
    }

    /// Return true only when the word is exactly an unquoted literal.
    /// Quoted and escaped operator-looking words must remain ordinary words.
    pub fn is_unquoted_literal(&self, value: &str) -> bool {
        !self.quoted
            && self.parts.len() == 1
            && self.parts[0].expandable
            && self.parts[0].text == value
    }

    /// Return the fragments in source order for the expansion phase.
    pub fn is_quoted(&self) -> bool { self.quoted }

    pub fn parts(&self) -> &[WordPart] {
        &self.parts
    }

    /// Return the quote-removed literal value without performing expansion.
    pub fn literal_value(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }
}

/// Split a command line into words while retaining quote and escape metadata.
///
/// This is a lexical foundation, not a complete POSIX shell parser. Operator
/// recognition and grammar parsing are handled by later parser stages.
pub fn lex(input: &str) -> Result<Vec<Word>, LexError> {
    let mut tokens = Vec::new();
    let mut word = Word::default();
    let mut quote = None;
    let mut started = false;
    let mut characters = input.chars().peekable();

    while let Some(character) = characters.next() {
        match quote {
            Some('\'') => {
                if character == '\'' {
                    quote = None;
                } else {
                    word.push(character, false, true);
                }
            }
            Some('"') => match character {
                '"' => quote = None,
                '\\' => match characters.peek().copied() {
                    Some('$' | '`' | '"' | '\\') => {
                        word.push(characters.next().expect("peeked character"), false, true);
                    }
                    Some('\n') => {
                        characters.next();
                    }
                    _ => word.push('\\', true, true),
                },
                _ => word.push(character, true, true),
            },
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    word.mark_quoted();
                    started = true;
                }
                '\\' => match characters.next() {
                    Some('\n') => {}
                    Some(escaped) => {
                        word.push(escaped, false, false);
                        started = true;
                    }
                    None => return Err(LexError("trailing escape character")),
                },
                value if value.is_ascii_digit() && !started => {
                    let mut digits = value.to_string();
                    while characters.peek().is_some_and(|next| next.is_ascii_digit()) {
                        digits.push(characters.next().expect("peeked digit"));
                    }
                    if matches!(characters.peek(), Some('<' | '>')) {
                        let redirection = characters.next().expect("peeked redirection operator");
                        let mut operator = digits;
                        operator.push(redirection);
                        if matches!(
                            (redirection, characters.peek().copied()),
                            ('>', Some('>')) | ('>', Some('|')) | ('>', Some('&'))
                                | ('<', Some('<')) | ('<', Some('>')) | ('<', Some('&'))
                        ) {
                            operator.push(characters.next().expect("peeked redirection suffix"));
                            if operator.ends_with("<<") && characters.peek() == Some(&'-') {
                                operator.push(characters.next().expect("peeked here-document modifier"));
                            }
                        }
                        tokens.push(Word::operator(&operator));
                    } else {
                        for digit in digits.chars() {
                            word.push(digit, true, false);
                        }
                        started = true;
                    }
                }
                '|' | '<' | '>' | ';' | '&' | '(' | ')' => {
                    if started {
                        tokens.push(std::mem::take(&mut word));
                        started = false;
                    }
                    let mut operator = character.to_string();
                    let paired = matches!(
                        (character, characters.peek().copied()),
                        ('>', Some('>')) | ('>', Some('|')) | ('>', Some('&'))
                            | ('<', Some('<')) | ('<', Some('>')) | ('<', Some('&'))
                            | ('|', Some('|')) | ('&', Some('&')) | (';', Some(';'))
                    );
                    if paired {
                        operator.push(characters.next().expect("peeked operator suffix"));
                        if operator == "<<" && characters.peek() == Some(&'-') {
                            operator.push(characters.next().expect("peeked here-document modifier"));
                        }
                    }
                    tokens.push(Word::operator(&operator));
                }
                '#' if !started => break,
                value if value.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                _ => {
                    word.push(character, true, false);
                    started = true;
                }
            },
            _ => unreachable!("only single and double quote states are used"),
        }
    }

    if quote.is_some() {
        return Err(LexError("unterminated quoted string"));
    }
    if started {
        tokens.push(word);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::lex;

    fn values(input: &str) -> Vec<String> {
        lex(input)
            .unwrap()
            .iter()
            .map(|word| word.literal_value())
            .collect()
    }

    #[test]
    fn splits_unquoted_arguments() {
        assert_eq!(values("echo hello world"), ["echo", "hello", "world"]);
    }

    #[test]
    fn groups_single_and_double_quoted_arguments() {
        assert_eq!(
            values("echo 'one two' \"three four\""),
            ["echo", "one two", "three four"]
        );
    }

    #[test]
    fn preserves_empty_quoted_arguments() {
        assert_eq!(values("echo '' \"\""), ["echo", "", ""]);
    }

    #[test]
    fn supports_backslash_escaping() {
        assert_eq!(values(r"echo hello\ world"), ["echo", "hello world"]);
    }

    #[test]
    fn handles_comments_only_at_token_boundaries() {
        assert_eq!(values("echo a#b # ignored"), ["echo", "a#b"]);
    }

    #[test]
    fn rejects_unterminated_quotes_and_escapes() {
        assert!(lex("echo 'unfinished").is_err());
        assert!(lex("echo unfinished\\").is_err());
    }

    #[test]
    fn retains_single_quote_and_double_quote_expansion_metadata() {
        let words = lex("echo '$NAME' \"$NAME\"").unwrap();
        assert!(!words[1].parts()[0].expandable);
        assert!(words[2].parts()[0].expandable);
    }

    #[test]
    fn escaped_dollar_is_not_expandable() {
        let words = lex(r"echo \$NAME").unwrap();
        assert!(!words[1].parts()[0].expandable);
        assert_eq!(words[1].literal_value(), "$NAME");
    }

    #[test]
    fn recognizes_supported_operators_without_whitespace() {
        let words = lex("echo hi|cat >out 2>err").unwrap();
        let actual: Vec<String> = words.iter().map(|word| word.literal_value()).collect();
        assert_eq!(actual, ["echo", "hi", "|", "cat", ">", "out", "2>", "err"]);
        assert!(words[2].is_unquoted_literal("|"));
        assert!(words[4].is_unquoted_literal(">"));
        assert!(words[6].is_unquoted_literal("2>"));
    }

    #[test]
    fn recognizes_command_list_operators() {
        let words = lex("echo first; echo second && echo third || echo fourth").unwrap();
        let actual: Vec<String> = words.iter().map(|word| word.literal_value()).collect();
        assert_eq!(actual, ["echo", "first", ";", "echo", "second", "&&", "echo", "third", "||", "echo", "fourth"]);
        assert!(words[2].is_unquoted_literal(";"));
        assert!(words[5].is_unquoted_literal("&&"));
        assert!(words[8].is_unquoted_literal("||"));
    }

    #[test]
    fn quoted_operator_is_not_an_unquoted_operator() {
        let words = lex("echo '|' | cat").unwrap();
        assert!(!words[1].is_unquoted_literal("|"));
        assert!(words[2].is_unquoted_literal("|"));
    }

    #[test]
    fn adjacent_quoted_and_unquoted_fragments_form_one_word() {
        let words = lex("echo pre\"$NAME\"'post'").unwrap();
        assert_eq!(words[1].literal_value(), "pre$NAMEpost");
        assert_eq!(words[1].parts().len(), 3);
        assert!(words[1].parts()[1].quoted);
    }

    #[test]
    fn recognizes_posix_compound_and_case_operators() {
        let words = lex("( echo one ) ; case x in x) : ;; esac").unwrap();
        let actual: Vec<String> = words.iter().map(|word| word.literal_value()).collect();
        assert_eq!(
            actual,
            ["(", "echo", "one", ")", ";", "case", "x", "in", "x", ")", ":", ";;", "esac"]
        );
        assert!(words[0].is_unquoted_literal("("));
        assert!(words[3].is_unquoted_literal(")"));
        assert!(words[11].is_unquoted_literal(";;"));
    }

    #[test]
    fn recognizes_posix_redirection_operator_tokens() {
        let words = lex("cmd 10>>log 3<&0 4>&1 <<-EOF <>file >|out").unwrap();
        let actual: Vec<String> = words.iter().map(|word| word.literal_value()).collect();
        assert_eq!(actual, ["cmd", "10>>", "log", "3<&", "0", "4>&", "1", "<<-", "EOF", "<>", "file", ">|", "out"]);
    }

    #[test]
    fn redirection_digits_are_only_io_numbers_when_adjacent() {
        let words = lex("echo 2 >out echo2>other").unwrap();
        let actual: Vec<String> = words.iter().map(|word| word.literal_value()).collect();
        assert_eq!(actual, ["echo", "2", ">", "out", "echo2", ">", "other"]);
    }

    #[test]
    fn quoted_parentheses_and_case_terminators_remain_words() {
        let words = lex("echo '(' ';;'").unwrap();
        assert!(!words[1].is_unquoted_literal("("));
        assert!(!words[2].is_unquoted_literal(";;"));
    }

}
