//! Interactive input loop, shell parsing, command dispatch, and process execution.
use crate::builtins::{self, BuiltinOutcome};
use crate::lexer::{self, Word};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};

#[derive(Default)]
pub struct Shell {
    history: Vec<String>,
    last_status: i32,
}

impl Shell {
    pub fn new() -> Self { Self::default() }
    pub fn last_status(&self) -> i32 { self.last_status }

    /// Execute one command line. Returns false when an exit command is accepted.
    pub fn execute_line(&mut self, line: &str, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let tokens = match lexer::lex(line) {
            Ok(tokens) => tokens,
            Err(error) => {
                let _ = writeln!(stderr, "mysh: syntax error: {error}");
                self.last_status = 2;
                return true;
            }
        };
        if tokens.is_empty() { return true; }
        self.history.push(line.to_owned());

        let stages = match split_pipeline(tokens) {
            Ok(stages) => stages,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };
        if stages.len() == 1 {
            return self.execute_stage(&stages[0], stdout, stderr);
        }

        let mut input: Option<Vec<u8>> = None;
        let mut final_status = 0;
        for (index, stage) in stages.iter().enumerate() {
            let is_last = index + 1 == stages.len();
            let (command, redirections) = match parse_redirections(stage) {
                Ok(value) => value,
                Err(message) => {
                    let _ = writeln!(stderr, "mysh: syntax error: {message}");
                    self.last_status = 2;
                    return true;
                }
            };
            if command.is_empty() {
                let _ = writeln!(stderr, "mysh: syntax error: empty pipeline stage");
                self.last_status = 2;
                return true;
            }
            let mut stage_output = Vec::new();
            let mut stage_error = Vec::new();
            final_status = match self.run_stage(&command, &redirections, input.as_deref(), &mut stage_output, &mut stage_error) {
                StageOutcome::Continue(status) | StageOutcome::Exit(status) => status,
            };
            if !stage_error.is_empty() { let _ = stderr.write_all(&stage_error); }
            if is_last {
                if !stage_output.is_empty() { let _ = stdout.write_all(&stage_output); }
            } else {
                input = Some(stage_output);
            }
        }
        self.last_status = final_status;
        true
    }

    fn execute_stage(&mut self, tokens: &[Word], stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let (command, redirections) = match parse_redirections(tokens) {
            Ok(value) => value,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };
        if command.is_empty() { return true; }
        let mut input = None;
        if let Some(path) = redirections.input.as_deref() {
            let mut contents = Vec::new();
            match File::open(path).and_then(|mut file| file.read_to_end(&mut contents)) {
                Ok(_) => input = Some(contents),
                Err(error) => {
                    let _ = writeln!(stderr, "mysh: {path}: {error}");
                    self.last_status = 1;
                    return true;
                }
            }
        }
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let outcome = self.run_stage(&command, &redirections, input.as_deref(), &mut output, &mut errors);
        if let Some(path) = redirections.output.as_deref() {
            let opened = open_output(path, redirections.append);
            match opened.and_then(|mut file| file.write_all(&output)) {
                Ok(()) => {}
                Err(error) => { let _ = writeln!(stderr, "mysh: {path}: {error}"); self.last_status = 1; return true; }
            }
        } else if !output.is_empty() { let _ = stdout.write_all(&output); }
        if let Some(path) = redirections.error.as_deref() {
            match open_output(path, false).and_then(|mut file| file.write_all(&errors)) {
                Ok(()) => {}
                Err(error) => { let _ = writeln!(stderr, "mysh: {path}: {error}"); self.last_status = 1; return true; }
            }
        } else if !errors.is_empty() { let _ = stderr.write_all(&errors); }
        match outcome {
            StageOutcome::Exit(status) => { self.last_status = status; false }
            StageOutcome::Continue(status) => { self.last_status = status; true }
        }
    }

    fn run_stage(&mut self, command: &[String], redirections: &Redirections, input: Option<&[u8]>, out: &mut Vec<u8>, err: &mut Vec<u8>) -> StageOutcome {
        if command.is_empty() { return StageOutcome::Continue(0); }
        if let Some(outcome) = builtins::run(&command[0], &command[1..], &self.history, input, out, err) {
            return match outcome {
                BuiltinOutcome::Continue(status) => StageOutcome::Continue(status),
                BuiltinOutcome::Exit(status) => StageOutcome::Exit(status),
            };
        }
        let mut process = Command::new(&command[0]);
        process.args(&command[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
        if redirections.input.is_some() || input.is_some() {
            process.stdin(Stdio::piped());
        }
        match process.spawn() {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    if let Some(input) = input {
                        if let Err(error) = stdin.write_all(input) {
                            let _ = writeln!(err, "mysh: {}: stdin: {error}", command[0]);
                        }
                    }
                }
                match child.wait_with_output() {
                    Ok(result) => {
                        out.extend_from_slice(&result.stdout);
                        err.extend_from_slice(&result.stderr);
                        StageOutcome::Continue(external_status_code(result.status))
                    }
                    Err(error) => {
                        let _ = writeln!(err, "mysh: {}: {error}", command[0]);
                        StageOutcome::Continue(126)
                    }
                }
            }
            Err(error) => {
                let _ = writeln!(err, "mysh: {}: {error}", command[0]);
                StageOutcome::Continue(if error.kind() == io::ErrorKind::NotFound { 127 } else { 126 })
            }
        }
    }

    /// Read and execute commands until end-of-input or an exit command.
    pub fn run<R: BufRead, W: Write>(&mut self, reader: &mut R, writer: &mut W, interactive: bool) -> io::Result<i32> {
        let mut line = String::new();
        loop {
            if interactive {
                let current = std::env::current_dir().map(|path| path.display().to_string()).unwrap_or_else(|_| "?".to_owned());
                write!(writer, "mysh:{current}$ ")?;
                writer.flush()?;
            }
            line.clear();
            if reader.read_line(&mut line)? == 0 { break; }
            while line.ends_with('\n') || line.ends_with('\r') { line.pop(); }
            let mut stderr = io::stderr().lock();
            if !self.execute_line(&line, writer, &mut stderr) { break; }
        }
        Ok(self.last_status)
    }
}

#[derive(Default)]
struct Redirections { input: Option<String>, output: Option<String>, error: Option<String>, append: bool }
enum StageOutcome { Continue(i32), Exit(i32) }

fn expand_word(word: &Word) -> String {
    let mut expanded = String::new();
    for part in word.parts() {
        if part.expandable {
            expanded.push_str(&expand_variables(&part.text));
        } else {
            expanded.push_str(&part.text);
        }
    }
    expanded
}

fn split_pipeline(tokens: Vec<Word>) -> Result<Vec<Vec<Word>>, &'static str> {
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

fn parse_redirections(tokens: &[Word]) -> Result<(Vec<String>, Redirections), &'static str> {
    let mut command = Vec::new();
    let mut redirects = Redirections::default();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let operator = [" >", " >>", " <", " 2>"].iter().find(|candidate| {
            token.is_unquoted_literal(candidate.trim())
        });
        if let Some(operator) = operator {
            let target = tokens.get(index + 1).ok_or("redirection requires a path")?;
            let target = expand_word(target);
            match operator.trim() {
                "<" => redirects.input = Some(target),
                ">" => { redirects.output = Some(target); redirects.append = false; }
                ">>" => { redirects.output = Some(target); redirects.append = true; }
                "2>" => redirects.error = Some(target),
                _ => unreachable!(),
            }
            index += 2;
        } else {
            command.push(expand_word(token));
            index += 1;
        }
    }
    Ok((command, redirects))
}

fn open_output(path: &str, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append { options.append(true); } else { options.truncate(true); }
    options.open(path)
}

/// Expand the currently supported simple environment-variable references.
fn expand_variables(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut result = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' { result.push(chars[index]); index += 1; continue; }
        index += 1;
        let braced = chars.get(index) == Some(&'{');
        if braced { index += 1; }
        let start = index;
        if chars.get(index).is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
            index += 1;
            while chars.get(index).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_') { index += 1; }
            let name: String = chars[start..index].iter().collect();
            result.push_str(&std::env::var(&name).unwrap_or_default());
        } else {
            result.push('$');
            continue;
        }
        if braced {
            if chars.get(index) == Some(&'}') { index += 1; } else { result.push_str("{}"); }
        }
    }
    result
}

/// Convert a child-process status into the conventional shell exit-code range.
fn external_status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() { return code; }
    #[cfg(unix)] { use std::os::unix::process::ExitStatusExt; status.signal().map_or(128, |signal| 128 + signal) }
    #[cfg(not(unix))] { 128 }
}

#[cfg(test)]
mod tests {
    use super::{expand_variables, external_status_code, split_pipeline, Shell};
    use crate::lexer;

    #[test]
    fn executes_builtin_commands() {
        let mut shell = Shell::new(); let mut output = Vec::new(); let mut errors = Vec::new();
        assert!(shell.execute_line("echo hello", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "hello\n");
        assert!(errors.is_empty()); assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn reports_lexer_errors_without_exiting() {
        let mut shell = Shell::new(); let mut output = Vec::new(); let mut errors = Vec::new();
        assert!(shell.execute_line("echo 'unfinished", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8(errors).unwrap().contains("syntax error"));
    }

    #[test]
    fn exit_returns_the_requested_status() {
        let mut shell = Shell::new(); let mut output = Vec::new(); let mut errors = Vec::new();
        assert!(!shell.execute_line("exit 23", &mut output, &mut errors)); assert_eq!(shell.last_status(), 23);
    }

    #[test]
    fn variable_expansion_supports_braced_and_unbraced_names() {
        std::env::set_var("MYSH_TEST_VALUE", "expanded");
        assert_eq!(expand_variables("$MYSH_TEST_VALUE/${MYSH_TEST_VALUE}"), "expanded/expanded");
        std::env::remove_var("MYSH_TEST_VALUE");
    }

    #[test]
    fn variable_expansion_respects_single_quotes_and_escapes() {
        std::env::set_var("MYSH_QUOTING_TEST", "expanded");
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(r#"echo '$MYSH_QUOTING_TEST' "$MYSH_QUOTING_TEST" \\$MYSH_QUOTING_TEST"#, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "$MYSH_QUOTING_TEST expanded $MYSH_QUOTING_TEST\n");
        assert!(errors.is_empty());
        std::env::remove_var("MYSH_QUOTING_TEST");
    }

    #[test]
    fn quoted_pipe_is_a_word_not_a_pipeline_operator() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo '|' ", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "|\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn quoted_redirection_operator_is_an_argument() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(r#"echo ">" "#, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), ">\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn pipeline_parser_rejects_empty_stages() {
        assert!(split_pipeline(lexer::lex("| cat").unwrap()).is_err());
        assert!(split_pipeline(lexer::lex("echo |").unwrap()).is_err());
    }

    #[test]
    fn pipeline_passes_builtin_output_to_cat_stdin() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo pipeline | cat", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "pipeline\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn pipeline_passes_input_through_multiple_builtin_stages() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo multi | cat | cat", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "multi\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn maps_signal_termination_to_conventional_shell_status() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(external_status_code(std::process::ExitStatus::from_raw(9)), 137);
    }
}
