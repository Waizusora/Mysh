//! Tokenize command lines using a small, explicitly scoped shell grammar.
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError(&'static str);

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for LexError {}

/// Split a command line into arguments, honoring quotes, escapes, and comments.
pub fn lex(input: &str) -> Result<Vec<String>, LexError> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;

    for character in input.chars() {
        if escaped {
            token.push(character);
            started = true;
            escaped = false;
            continue;
        }
        match quote {
            Some('\'') => {
                if character == '\'' { quote = None; } else { token.push(character); }
            }
            Some('"') => match character {
                '"' => quote = None,
                '\\' => escaped = true,
                _ => token.push(character),
            },
            None => match character {
                '\'' | '"' => { quote = Some(character); started = true; }
                '\\' => { escaped = true; started = true; }
                '#' if !started => break,
                value if value.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut token));
                        started = false;
                    }
                }
                _ => { token.push(character); started = true; }
            },
        }
    }
    if escaped { return Err(LexError("trailing escape character")); }
    if quote.is_some() { return Err(LexError("unterminated quoted string")); }
    if started { tokens.push(token); }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::lex;

    #[test]
    fn splits_unquoted_arguments() {
        assert_eq!(lex("echo hello world").unwrap(), ["echo", "hello", "world"]);
    }
    #[test]
    fn groups_single_and_double_quoted_arguments() {
        assert_eq!(lex("echo 'one two' \"three four\"").unwrap(), ["echo", "one two", "three four"]);
    }
    #[test]
    fn preserves_empty_quoted_arguments() {
        assert_eq!(lex("echo '' \"\"").unwrap(), ["echo", "", ""]);
    }
    #[test]
    fn supports_backslash_escaping() {
        assert_eq!(lex(r"echo hello\ world").unwrap(), ["echo", "hello world"]);
    }
    #[test]
    fn handles_comments_only_at_token_boundaries() {
        assert_eq!(lex("echo a#b # ignored").unwrap(), ["echo", "a#b"]);
    }
    #[test]
    fn rejects_unterminated_quotes_and_escapes() {
        assert!(lex("echo 'unfinished").is_err());
        assert!(lex("echo unfinished\\").is_err());
    }
}
