//! Interactive input loop and command dispatcher.
use crate::builtins::{self, BuiltinOutcome};
use crate::lexer;
use std::io::{self, BufRead, Write};
use std::process::Command;

#[derive(Default)]
pub struct Shell {
    history: Vec<String>,
    last_status: i32,
}

impl Shell {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn last_status(&self) -> i32 {
        self.last_status
    }

    /// Execute one command line. Returns false when an exit command is accepted.
    pub fn execute_line(
        &mut self,
        line: &str,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> bool {
        let tokens = match lexer::lex(line) {
            Ok(tokens) => tokens,
            Err(error) => {
                let _ = writeln!(stderr, "mysh: syntax error: {error}");
                self.last_status = 2;
                return true;
            }
        };
        if tokens.is_empty() {
            return true;
        }
        self.history.push(line.to_owned());

        if let Some(outcome) =
            builtins::run(&tokens[0], &tokens[1..], &self.history, stdout, stderr)
        {
            match outcome {
                BuiltinOutcome::Continue(status) => {
                    self.last_status = status;
                    true
                }
                BuiltinOutcome::Exit(status) => {
                    self.last_status = status;
                    false
                }
            }
        } else {
            self.execute_external(&tokens, stderr);
            true
        }
    }

    /// Read and execute commands until end-of-input or an exit command.
    pub fn run<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
        interactive: bool,
    ) -> io::Result<i32> {
        let mut line = String::new();
        loop {
            if interactive {
                let current = std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "?".to_owned());
                write!(writer, "mysh:{current}$ ")?;
                writer.flush()?;
            }
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            while line.ends_with('\n') || line.ends_with('\r') {
                line.pop();
            }
            let mut stderr = io::stderr().lock();
            if !self.execute_line(&line, writer, &mut stderr) {
                break;
            }
        }
        Ok(self.last_status)
    }

    fn execute_external(&mut self, tokens: &[String], stderr: &mut dyn Write) {
        self.last_status = match Command::new(&tokens[0]).args(&tokens[1..]).status() {
            Ok(status) => external_status_code(status),
            Err(error) => {
                let _ = writeln!(stderr, "mysh: {}: {error}", tokens[0]);
                if error.kind() == io::ErrorKind::NotFound {
                    127
                } else {
                    126
                }
            }
        };
    }
}

fn external_status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map_or(128, |signal| 128 + signal)
    }
    #[cfg(not(unix))]
    {
        128
    }
}

/// Convert a child-process status into the conventional shell exit-code range.

#[cfg(test)]
mod tests {
    use super::{external_status_code, Shell};

    #[test]
    fn executes_builtin_commands() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo hello", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "hello\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn reports_lexer_errors_without_exiting() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo 'unfinished", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8(errors).unwrap().contains("syntax error"));
    }

    #[test]
    fn exit_returns_the_requested_status() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(!shell.execute_line("exit 23", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 23);
    }

    #[cfg(unix)]
    #[test]
    fn maps_signal_termination_to_conventional_shell_status() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(9);
        assert_eq!(external_status_code(status), 137);
    }

}