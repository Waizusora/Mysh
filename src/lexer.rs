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
}

/// A shell word made from one or more quoted or unquoted fragments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Word {
    parts: Vec<WordPart>,
    quoted: bool,
}

impl Word {
    fn push(&mut self, text: char, expandable: bool) {
        if let Some(part) = self.parts.last_mut() {
            if part.expandable == expandable {
                part.text.push(text);
                return;
            }
        }
        self.parts.push(WordPart {
            text: text.to_string(),
            expandable,
        });
    }

    fn mark_quoted(&mut self) {
        self.quoted = true;
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
                    word.push(character, false);
                }
            }
            Some('"') => match character {
                '"' => quote = None,
                '\\' => match characters.peek().copied() {
                    Some('$' | '`' | '"' | '\\') => {
                        word.push(characters.next().expect("peeked character"), false);
                    }
                    Some('\n') => {
                        characters.next();
                    }
                    _ => word.push('\\', true),
                },
                _ => word.push(character, true),
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
                        word.push(escaped, false);
                        started = true;
                    }
                    None => return Err(LexError("trailing escape character")),
                },
                '#' if !started => break,
                value if value.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                _ => {
                    word.push(character, true);
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
    }
}
