//! Interactive input loop, shell parsing, command dispatch, and process execution.
use crate::builtins::{self, BuiltinOutcome};
use crate::command::resolve_command;
use crate::lexer::{self, Word};
use crate::parser::{is_reserved_word, split_command_list, ListOperator, Pipeline};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};
use std::collections::{BTreeMap, BTreeSet};

pub struct Shell {
    history: Vec<String>,
    last_status: i32,
    variables: BTreeMap<String, String>,
    exported: BTreeSet<String>,
    loop_depth: usize,
    loop_control: Option<LoopControl>,
}

impl Default for Shell {
    fn default() -> Self {
        let mut variables: BTreeMap<String, String> = std::env::vars().collect();
        let mut exported: BTreeSet<String> = variables.keys().cloned().collect();
        configure_prefix_path(&mut variables, &mut exported);
        Self { history: Vec::new(), last_status: 0, variables, exported, loop_depth: 0, loop_control: None }
    }
}

impl Shell {
    pub fn new() -> Self { Self::default() }
    pub fn last_status(&self) -> i32 { self.last_status }

    /// Execute one command line. Returns false when an exit command is accepted.
    pub fn execute_line(&mut self, line: &str, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        match crate::parser::classify_input(line) {
            crate::parser::InputState::Complete => {}
            crate::parser::InputState::Incomplete => {
                let _ = writeln!(stderr, "mysh: syntax error: incomplete compound command or quoted input");
                self.last_status = 2;
                return true;
            }
            crate::parser::InputState::Invalid => {
                let _ = writeln!(stderr, "mysh: syntax error: invalid reserved-word sequence");
                self.last_status = 2;
                return true;
            }
        }

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

        let has_compound_command = tokens.iter().enumerate().any(|(index, _)| {
            is_reserved_word(&tokens, index, "if")
                || is_reserved_word(&tokens, index, "while")
                || is_reserved_word(&tokens, index, "until")
        });
        if has_compound_command {
            return self.execute_compound_branch(&tokens, stdout, stderr);
        }

        let commands = match split_command_list(tokens) {
            Ok(commands) => commands,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };

        for entry in commands {
            let should_run = match entry.connector {
                None | Some(ListOperator::Sequence) => true,
                Some(ListOperator::And) => self.last_status == 0,
                Some(ListOperator::Or) => self.last_status != 0,
            };
            if should_run && !self.execute_pipeline(&entry.pipeline, stdout, stderr) {
                return false;
            }
        }
        true
    }

    fn execute_loop_statement(&mut self, statement: &crate::parser::LoopStatement, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        self.loop_depth += 1;
        let mut keep_running = true;
        loop {
            if !self.execute_compound_list(&statement.condition, stdout, stderr) {
                keep_running = false;
                break;
            }
            if let Some(control) = self.loop_control.take() {
                match control {
                    LoopControl::Break(1) => break,
                    LoopControl::Continue(1) => continue,
                    LoopControl::Break(level) => { self.loop_control = Some(LoopControl::Break(level - 1)); break; }
                    LoopControl::Continue(level) => { self.loop_control = Some(LoopControl::Continue(level - 1)); break; }
                }
            }
            let should_enter = match statement.kind {
                crate::parser::LoopKind::While => self.last_status == 0,
                crate::parser::LoopKind::Until => self.last_status != 0,
            };
            if !should_enter { break; }
            if !self.execute_compound_list(&statement.body, stdout, stderr) {
                keep_running = false;
                break;
            }
            if let Some(control) = self.loop_control.take() {
                match control {
                    LoopControl::Break(1) => break,
                    LoopControl::Continue(1) => continue,
                    LoopControl::Break(level) => { self.loop_control = Some(LoopControl::Break(level - 1)); break; }
                    LoopControl::Continue(level) => { self.loop_control = Some(LoopControl::Continue(level - 1)); break; }
                }
            }
        }
        self.loop_depth -= 1;
        keep_running
    }

    fn execute_if_statement(&mut self, statement: &crate::parser::IfStatement, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        for clause in &statement.clauses {
            if !self.execute_compound_list(&clause.condition, stdout, stderr) {
                return false;
            }
            if self.loop_control.is_some() { return true; }
            if self.last_status == 0 {
                return self.execute_compound_list(&clause.body, stdout, stderr);
            }
        }

        if let Some(branch) = &statement.else_body {
            return self.execute_compound_list(branch, stdout, stderr);
        }
        true
    }

    fn execute_compound_branch(&mut self, branch: &[Word], stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let commands = match crate::parser::parse_compound_list(branch) {
            Ok(commands) => commands,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };
        self.execute_compound_list(&commands, stdout, stderr)
    }

    fn execute_compound_list(&mut self, commands: &crate::parser::CompoundList, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        for entry in commands {
            let should_run = match entry.connector {
                None | Some(ListOperator::Sequence) => true,
                Some(ListOperator::And) => self.last_status == 0,
                Some(ListOperator::Or) => self.last_status != 0,
            };
            if !should_run {
                continue;
            }

            let keep_running = match &entry.command {
                crate::parser::CompoundCommand::Simple(words) => {
                    self.execute_word_list(words, stdout, stderr)
                }
                crate::parser::CompoundCommand::If(statement) => {
                    self.execute_if_statement(statement, stdout, stderr)
                }
                crate::parser::CompoundCommand::Loop(statement) => {
                    self.execute_loop_statement(statement, stdout, stderr)
                }
            };
            if !keep_running {
                return false;
            }
            if self.loop_control.is_some() {
                return true;
            }
        }
        true
    }

    fn execute_word_list(&mut self, tokens: &[Word], stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let commands = match split_command_list(tokens.to_vec()) {
            Ok(commands) => commands,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };
        for entry in commands {
            let should_run = match entry.connector {
                None | Some(ListOperator::Sequence) => true,
                Some(ListOperator::And) => self.last_status == 0,
                Some(ListOperator::Or) => self.last_status != 0,
            };
            if should_run && !self.execute_pipeline(&entry.pipeline, stdout, stderr) {
                return false;
            }
            if self.loop_control.is_some() { return true; }
        }
        true
    }

    fn execute_pipeline(&mut self, pipeline: &Pipeline, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let stages = &pipeline.stages;
        if stages.len() == 1 {
            return self.execute_stage(&stages[0], stdout, stderr);
        }

        let mut input: Option<Vec<u8>> = None;
        let mut final_status = 0;
        for (index, stage) in stages.iter().enumerate() {
            let is_last = index + 1 == stages.len();
            let (command, redirections) = match parse_redirections(stage, &self.variables, self.last_status) {
                Ok(value) => value,
                Err(message) => {
                    let _ = writeln!(stderr, "mysh: syntax error: {message}");
                    self.last_status = 2;
                    return true;
                }
            };
            let saved_variables = self.variables.clone();
            let saved_exported = self.exported.clone();
            let saved_loop_control = self.loop_control;
            let saved_directory = std::env::current_dir().ok();
            let mut stage_command = command;
            while stage_command.first().is_some_and(|word| assignment(word).is_some()) {
                let word = stage_command.remove(0);
                let (name, value) = assignment(&word).expect("assignment was checked");
                self.variables.insert(name.to_owned(), value.to_owned());
                self.exported.insert(name.to_owned());
            }
            let mut stage_output = Vec::new();
            let mut stage_error = Vec::new();
            final_status = if stage_command.is_empty() {
                0
            } else {
                match self.run_stage(&stage_command, &redirections, input.as_deref(), &mut stage_output, &mut stage_error) {
                    StageOutcome::Continue(status) | StageOutcome::Exit(status) => status,
                }
            };
            self.variables = saved_variables;
            self.exported = saved_exported;
            self.loop_control = saved_loop_control;
            if let Some(directory) = saved_directory {
                let _ = std::env::set_current_dir(directory);
            }
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
        let (mut command, redirections) = match parse_redirections(tokens, &self.variables, self.last_status) {
            Ok(value) => value,
            Err(message) => {
                let _ = writeln!(stderr, "mysh: syntax error: {message}");
                self.last_status = 2;
                return true;
            }
        };
        let mut assignments = Vec::new();
        while command.first().is_some_and(|word| assignment(word).is_some()) {
            let word = command.remove(0);
            let (name, value) = assignment(&word).expect("assignment was checked");
            assignments.push((name.to_owned(), value.to_owned()));
        }
        if command.is_empty() {
            for (name, value) in assignments { self.variables.insert(name, value); }
            self.last_status = 0;
            return true;
        }
        let persist_assignments = command.first().is_some_and(|name| is_special_builtin(name));
        let saved_variables = self.variables.clone();
        let saved_exported = self.exported.clone();
        for (name, value) in assignments.iter() {
            self.variables.insert(name.clone(), value.clone());
            self.exported.insert(name.clone());
        }
        let mut input = None;
        if let Some(path) = redirections.input.as_deref() {
            let mut contents = Vec::new();
            match File::open(path).and_then(|mut file| file.read_to_end(&mut contents)) {
                Ok(_) => input = Some(contents),
                Err(error) => {
                    let _ = writeln!(stderr, "mysh: {path}: {error}");
                    if !assignments.is_empty() {
                        self.variables = saved_variables;
                        self.exported = saved_exported;
                    }
                    self.last_status = 1;
                    return true;
                }
            }
        }
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let outcome = self.run_stage(&command, &redirections, input.as_deref(), &mut output, &mut errors);
        if !assignments.is_empty() && !persist_assignments {
            self.variables = saved_variables;
            self.exported = saved_exported;
        }
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
        if let Some(outcome) = builtins::run(&command[0], &command[1..], &self.history, &mut self.variables, &mut self.exported, input, out, err) {
            return match outcome {
                BuiltinOutcome::Continue(status) => StageOutcome::Continue(status),
                BuiltinOutcome::Exit(status) => StageOutcome::Exit(status),
                BuiltinOutcome::Break(level) => {
                    if self.loop_depth == 0 {
                        let _ = writeln!(err, "mysh: break: not in a loop");
                        StageOutcome::Continue(1)
                    } else if level > self.loop_depth {
                        let _ = writeln!(err, "mysh: break: level exceeds active loop depth");
                        StageOutcome::Continue(1)
                    } else {
                        self.loop_control = Some(LoopControl::Break(level));
                        StageOutcome::Continue(0)
                    }
                }
                BuiltinOutcome::ContinueLoop(level) => {
                    if self.loop_depth == 0 {
                        let _ = writeln!(err, "mysh: continue: not in a loop");
                        StageOutcome::Continue(1)
                    } else if level > self.loop_depth {
                        let _ = writeln!(err, "mysh: continue: level exceeds active loop depth");
                        StageOutcome::Continue(1)
                    } else {
                        self.loop_control = Some(LoopControl::Continue(level));
                        StageOutcome::Continue(0)
                    }
                }
            };
        }
        let search_path = self.variables.get("PATH").map(std::ffi::OsString::from).unwrap_or_default();
        let executable = match resolve_command(&command[0], &search_path) {
            Ok(path) => path,
            Err(error) => {
                let _ = writeln!(err, "mysh: {}: {error}", command[0]);
                return StageOutcome::Continue(if error.kind() == io::ErrorKind::NotFound {
                    127
                } else {
                    126
                });
            }
        };
        let mut process = Command::new(executable);
        process.args(&command[1..]).env_clear().envs(self.exported.iter().map(|name| (name, self.variables.get(name).map(String::as_str).unwrap_or("")))).stdout(Stdio::piped()).stderr(Stdio::piped());
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
        let mut pending = String::new();
        loop {
            if interactive {
                if pending.is_empty() {
                    let current = std::env::current_dir().map(|path| path.display().to_string()).unwrap_or_else(|_| "?".to_owned());
                    write!(writer, "mysh:{current}$ ")?;
                } else {
                    write!(writer, "> ")?;
                }
                writer.flush()?;
            }
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                if !pending.is_empty() {
                    let mut stderr = io::stderr().lock();
                    let _ = self.execute_line(&pending, writer, &mut stderr);
                }
                break;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if !pending.is_empty() {
                let previous = pending.trim_end();
                if previous.ends_with("then") {
                    pending.push(' ');
                } else {
                    pending.push(';');
                }
            }
            pending.push_str(line);
            pending.push('\n');
            if requires_more_input(&pending) {
                continue;
            }
            let mut stderr = io::stderr().lock();
            let keep_running = self.execute_line(&pending, writer, &mut stderr);
            pending.clear();
            if !keep_running {
                break;
            }
        }
        Ok(self.last_status)
    }
}

fn requires_more_input(input: &str) -> bool {
    matches!(crate::parser::classify_input(input), crate::parser::InputState::Incomplete)
}

#[derive(Default)]
struct Redirections { input: Option<String>, output: Option<String>, error: Option<String>, append: bool }
enum StageOutcome { Continue(i32), Exit(i32) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopControl { Break(usize), Continue(usize) }

fn configure_prefix_path(variables: &mut BTreeMap<String, String>, exported: &mut BTreeSet<String>) {
    let Some(prefix) = variables.get("MYSH_PREFIX") else { return; };
    let bin = std::path::PathBuf::from(prefix).join("bin");
    if !bin.is_dir() { return; }
    let current = variables.get("PATH").cloned().unwrap_or_default();
    let already_present = std::env::split_paths(&current).any(|entry| entry == bin);
    if already_present { return; }
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&current));
    if let Ok(path) = std::env::join_paths(paths) {
        variables.insert("PATH".to_owned(), path.to_string_lossy().into_owned());
        exported.insert("PATH".to_owned());
    }
}

/// POSIX special built-ins execute in the current shell environment.
fn is_special_builtin(name: &str) -> bool {
    matches!(name, "export" | "unset" | "exit")
}

fn assignment(value: &str) -> Option<(&str, &str)> {
    let (name, assigned) = value.split_once('=')?;
    let mut chars = name.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some((name, assigned))
}

fn expand_word(word: &Word, environment: &BTreeMap<String, String>, last_status: i32) -> String {
    let mut expanded = String::new();
    for part in word.parts() {
        if part.expandable {
            expanded.push_str(&expand_variables(&part.text, environment, last_status));
        } else {
            expanded.push_str(&part.text);
        }
    }
    expanded
}

fn parse_redirections(tokens: &[Word], environment: &BTreeMap<String, String>, last_status: i32) -> Result<(Vec<String>, Redirections), &'static str> {
    let mut command = Vec::new();
    let mut redirects = Redirections::default();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let operator = ["<", ">", ">>", "2>"]
            .into_iter()
            .find(|operator| token.is_unquoted_literal(operator));
        if let Some(operator) = operator {
            let target = tokens.get(index + 1).ok_or("redirection requires a path")?;
            let target = expand_word(target, environment, last_status);
            match operator {
                "<" => redirects.input = Some(target),
                ">" => { redirects.output = Some(target); redirects.append = false; }
                ">>" => { redirects.output = Some(target); redirects.append = true; }
                "2>" => redirects.error = Some(target),
                _ => unreachable!(),
            }
            index += 2;
        } else {
            command.push(expand_word(token, environment, last_status));
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
fn expand_variables(value: &str, environment: &BTreeMap<String, String>, last_status: i32) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut result = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            result.push(chars[index]);
            index += 1;
            continue;
        }

        if chars.get(index + 1) == Some(&'?') {
            result.push_str(&last_status.to_string());
            index += 2;
            continue;
        }

        if chars.get(index + 1) == Some(&'{') {
            if let Some(relative_end) = chars[index + 2..].iter().position(|character| *character == '}') {
                let end = index + 2 + relative_end;
                let name: String = chars[index + 2..end].iter().collect();
                if name == "?" {
                    result.push_str(&last_status.to_string());
                } else if valid_parameter_name(&name) {
                    result.push_str(environment.get(&name).map(String::as_str).unwrap_or(""));
                } else {
                    result.extend(chars[index..=end].iter());
                }
                index = end + 1;
                continue;
            }
            result.push_str("${");
            index += 2;
            continue;
        }

        let start = index + 1;
        if chars.get(start).is_some_and(|character| character.is_ascii_alphabetic() || *character == '_') {
            index = start + 1;
            while chars.get(index).is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_') {
                index += 1;
            }
            let name: String = chars[start..index].iter().collect();
            result.push_str(environment.get(&name).map(String::as_str).unwrap_or(""));
        } else {
            result.push('$');
            index += 1;
        }
    }
    result
}

fn valid_parameter_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Convert a child-process status into the conventional shell exit-code range.
fn external_status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() { return code; }
    #[cfg(unix)] { use std::os::unix::process::ExitStatusExt; status.signal().map_or(128, |signal| 128 + signal) }
    #[cfg(not(unix))] { 128 }
}

#[cfg(test)]
mod tests {
    use super::{configure_prefix_path, expand_variables, external_status_code, requires_more_input, Shell};
    use std::collections::{BTreeMap, BTreeSet};

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
        let environment = BTreeMap::from([(String::from("MYSH_TEST_VALUE"), String::from("expanded"))]);
        assert_eq!(expand_variables("$MYSH_TEST_VALUE/${MYSH_TEST_VALUE}", &environment, 0), "expanded/expanded");
        assert_eq!(expand_variables("status=$? braced=${?}", &environment, 37), "status=37 braced=37");
    }

    #[test]
    fn last_exit_status_parameter_expands_in_command_arguments() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("mysh_command_that_does_not_exist_9f31", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 127);
        assert!(shell.execute_line("echo status=$? braced=${?}", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "status=127 braced=127\n");
        assert!(String::from_utf8(errors).unwrap().contains("command not found"));
    }

    #[test]
    fn variable_expansion_respects_single_quotes_and_escapes() {
        let mut shell = Shell::new();
        shell.variables.insert("MYSH_QUOTING_TEST".to_owned(), "expanded".to_owned());
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(r#"echo '$MYSH_QUOTING_TEST' "$MYSH_QUOTING_TEST" \$MYSH_QUOTING_TEST"#, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "$MYSH_QUOTING_TEST expanded $MYSH_QUOTING_TEST\n");
        assert!(errors.is_empty());
        
    }

    #[test]
    fn prefix_bin_is_added_before_existing_path() {
        let root = std::env::temp_dir().join(format!("mysh-prefix-test-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let mut variables = BTreeMap::from([
            ("MYSH_PREFIX".to_owned(), root.to_string_lossy().into_owned()),
            ("PATH".to_owned(), "/system/bin".to_owned()),
        ]);
        let mut exported = BTreeSet::from(["MYSH_PREFIX".to_owned(), "PATH".to_owned()]);
        configure_prefix_path(&mut variables, &mut exported);
        let paths: Vec<_> = std::env::split_paths(variables.get("PATH").unwrap()).collect();
        assert_eq!(paths.first(), Some(&bin));
        assert_eq!(paths.get(1), Some(&std::path::PathBuf::from("/system/bin")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn assignment_only_creates_unexported_shell_variable() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("MYSH_LOCAL_TEST='local value'", &mut output, &mut errors));
        assert!(shell.execute_line("echo \"$MYSH_LOCAL_TEST\"", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "local value\n");
        assert!(!shell.exported.contains("MYSH_LOCAL_TEST"));
        assert!(errors.is_empty());
    }

    #[test]
    fn command_prefix_assignment_is_temporary() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("MYSH_TEMP_TEST=value echo temporary", &mut output, &mut errors));
        assert!(shell.execute_line("echo \"$MYSH_TEMP_TEST\"", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "temporary\n\n");
        assert!(!shell.variables.contains_key("MYSH_TEMP_TEST"));
        assert!(errors.is_empty());
    }

    #[test]
    fn assignments_before_special_builtins_persist() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("MYSH_SPECIAL_ASSIGN_TEST=persist export", &mut output, &mut errors));
        assert!(shell.variables.get("MYSH_SPECIAL_ASSIGN_TEST").is_some_and(|value| value == "persist"));
        assert!(shell.exported.contains("MYSH_SPECIAL_ASSIGN_TEST"));
        assert!(errors.is_empty());
    }

    #[test]
    fn export_updates_shell_expansion_and_unset_removes_variable() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("export MYSH_EXPORT_TEST='hello world'", &mut output, &mut errors));
        assert!(shell.execute_line("echo \"$MYSH_EXPORT_TEST\"", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "hello world\n");
        assert!(shell.execute_line("unset MYSH_EXPORT_TEST", &mut Vec::new(), &mut errors));
        let mut after_unset = Vec::new();
        assert!(shell.execute_line("echo \"$MYSH_EXPORT_TEST\"", &mut after_unset, &mut errors));
        assert_eq!(String::from_utf8(after_unset).unwrap(), "\n");
        assert!(errors.is_empty());
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
    fn command_list_runs_semicolon_separated_commands() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo first; echo second;", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "first\nsecond\n");
        assert!(errors.is_empty());
    }

    #[test]
    fn conditional_command_lists_short_circuit_and_chain_left_to_right() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "mysh_missing_conditional_test && echo skipped || echo fallback",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "fallback\n");
        assert!(String::from_utf8(errors).unwrap().contains("command not found"));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn command_list_rejects_missing_conditional_operands() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo missing &&", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8(errors).unwrap().contains("syntax error"));
    }

    #[test]
    fn if_then_executes_the_success_branch() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("if echo condition; then echo success; fi", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "condition\nsuccess\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn if_else_executes_the_failure_branch() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("if mysh_missing_if_test; then echo wrong; else echo fallback; fi", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "fallback\n");
        assert!(String::from_utf8(errors).unwrap().contains("command not found"));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn if_elif_selects_the_first_successful_condition() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("if mysh_missing_first; then echo first; elif echo second; then echo selected; else echo fallback; fi", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "second\nselected\n");
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn if_supports_a_nested_if_as_a_branch() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("if mysh_missing_outer; then echo wrong; else if echo inner; then echo nested; fi; fi", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "inner\nnested\n");
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn if_branch_supports_commands_before_and_after_nested_if() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "if mysh_missing_outer_mixed; then echo wrong; else echo before; if echo inner; then echo nested; fi; echo after; fi",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "before\ninner\nnested\nafter\n");
        assert!(String::from_utf8(errors).unwrap().contains("command not found"));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn if_branch_honors_conditional_command_lists_around_nested_if() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "if echo condition; then mysh_missing_inner && echo skipped; if echo nested; then echo selected; fi; echo final; fi",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "condition\nnested\nselected\nfinal\n");
        assert!(String::from_utf8(errors).unwrap().contains("command not found"));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn multiline_if_is_buffered_until_its_closing_fi() {
        let input = "if echo condition; then\necho before\nif echo nested; then\necho selected\nfi\necho after\nfi\n";
        let mut reader = std::io::Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let mut shell = Shell::new();
        assert_eq!(shell.run(&mut reader, &mut output, false).unwrap(), 0);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "condition\nbefore\nnested\nselected\nafter\n",
        );
    }

    #[test]
    fn continuation_detection_tracks_compound_if_depth() {
        assert!(requires_more_input("if echo condition; then\necho body\n"));
        assert!(requires_more_input("if echo outer; then\nif echo inner; then\necho body\nfi\n"));
        assert!(!requires_more_input("if echo condition; then\necho body\nfi\n"));
        assert!(!requires_more_input("echo ordinary command\n"));
    }

    #[test]
    fn keyword_looking_arguments_do_not_change_if_structure() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "if echo if; then echo fi; echo then; fi",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "if\nfi\nthen\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn quoted_and_escaped_keyword_looking_arguments_remain_words() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            r#"if echo 'if' \fi; then echo "fi"; fi"#,
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "if fi\nfi\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn unmatched_closing_reserved_words_are_syntax_errors() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("fi", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8(errors).unwrap().contains("syntax error"));
    }

    #[test]
    fn if_rejects_missing_then_or_fi() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("if echo condition; echo body", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8(errors).unwrap().contains("syntax error"));
    }

    #[test]
    fn pipeline_parser_rejects_empty_stages() {
    }

    #[test]
    fn pipeline_prefix_assignments_are_temporary_to_the_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("MYSH_PIPE_ASSIGN_TEST=value echo pipeline | cat", &mut output, &mut errors));
        assert!(shell.execute_line("echo \"$MYSH_PIPE_ASSIGN_TEST\"", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "pipeline\n\n");
        assert!(!shell.variables.contains_key("MYSH_PIPE_ASSIGN_TEST"));
        assert!(errors.is_empty());
    }

    #[test]
    fn pipeline_builtin_variable_changes_do_not_leak_to_parent() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("export MYSH_PIPE_EXPORT_TEST=value | cat", &mut output, &mut errors));
        assert!(shell.execute_line("echo \"$MYSH_PIPE_EXPORT_TEST\"", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "\n");
        assert!(!shell.variables.contains_key("MYSH_PIPE_EXPORT_TEST"));
        assert!(!shell.exported.contains("MYSH_PIPE_EXPORT_TEST"));
        assert!(errors.is_empty());
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
    fn pipeline_operator_does_not_require_surrounding_whitespace() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo compact|cat", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "compact\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn output_redirection_does_not_require_surrounding_whitespace() {
        let path = std::env::temp_dir().join(format!("mysh-redirection-{}", std::process::id()));
        let path_text = path.to_string_lossy();
        let command = format!("echo redirected>{path_text}");
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert!(output.is_empty());
        assert!(errors.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "redirected\n");
        std::fs::remove_file(path).unwrap();
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


    #[test]
    fn break_exits_the_current_loop_and_skips_remaining_body_commands() {
        let marker = std::env::temp_dir().join(format!("mysh-break-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!("while test ! -e '{}'; do touch '{}'; echo before; break; echo skipped; done; echo after", marker.display(), marker.display());
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "before\nafter\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        let _ = std::fs::remove_file(marker);
    }

    #[test]
    fn continue_starts_the_next_iteration_and_skips_remaining_body_commands() {
        let marker = std::env::temp_dir().join(format!("mysh-continue-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!("while test ! -e '{}'; do touch '{}'; continue; echo skipped; done; echo after", marker.display(), marker.display());
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        let _ = std::fs::remove_file(marker);
    }


    #[test]
    fn continue_two_resumes_the_outer_loop_iteration() {
        let outer = std::env::temp_dir().join(format!("mysh-continue-outer-{}", std::process::id()));
        let inner = std::env::temp_dir().join(format!("mysh-continue-inner-{}", std::process::id()));
        let _ = std::fs::remove_file(&outer);
        let _ = std::fs::remove_file(&inner);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!("while test ! -e '{}'; do if test -e '{}'; then touch '{}'; else touch '{}'; continue 2; fi; echo skipped; done; echo after", outer.display(), inner.display(), outer.display(), inner.display());
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        let _ = std::fs::remove_file(outer);
        let _ = std::fs::remove_file(inner);
    }

    #[test]
    fn break_two_exits_two_nested_loops() {
        let outer = std::env::temp_dir().join(format!("mysh-break-outer-{}", std::process::id()));
        let inner = std::env::temp_dir().join(format!("mysh-break-inner-{}", std::process::id()));
        let _ = std::fs::remove_file(&outer);
        let _ = std::fs::remove_file(&inner);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!("while test ! -e '{}'; do touch '{}'; while test ! -e '{}'; do touch '{}'; break 2; echo skipped-inner; done; echo skipped-outer; done; echo after", outer.display(), outer.display(), inner.display(), inner.display());
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        let _ = std::fs::remove_file(outer);
        let _ = std::fs::remove_file(inner);
    }

    #[test]
    fn loop_control_rejects_use_outside_loops_and_excessive_levels() {
        let mut shell = Shell::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("break", &mut Vec::new(), &mut errors));
        assert_eq!(shell.last_status(), 1);
        assert!(shell.execute_line("continue 0", &mut Vec::new(), &mut errors));
        assert_eq!(shell.last_status(), 2);
        let marker = std::env::temp_dir().join(format!("mysh-break-depth-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let command = format!("while test ! -e '{}'; do break 2; touch '{}'; done; true", marker.display(), marker.display());
        assert!(shell.execute_line(&command, &mut Vec::new(), &mut errors));
        assert_eq!(shell.last_status(), 0);
        let _ = std::fs::remove_file(marker);
        let text = String::from_utf8(errors).unwrap();
        assert!(text.contains("not in a loop"));
        assert!(text.contains("positive integer"));
        assert!(text.contains("level exceeds active loop depth"));
    }

    #[test]
    fn while_loop_repeats_until_its_condition_fails() {
        let marker = std::env::temp_dir().join(format!("mysh-while-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!(
            "while test ! -e '{}'; do touch '{}'; echo iterated; done",
            marker.display(),
            marker.display(),
        );
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "iterated\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert!(marker.exists());
        let _ = std::fs::remove_file(marker);
    }

    #[test]
    fn until_loop_repeats_while_its_condition_fails() {
        let marker = std::env::temp_dir().join(format!("mysh-until-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!(
            "until test -e '{}'; do touch '{}'; echo iterated; done",
            marker.display(),
            marker.display(),
        );
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "iterated\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert!(marker.exists());
        let _ = std::fs::remove_file(marker);
    }

    #[test]
    fn nested_until_loop_executes_inside_while_body() {
        let marker = std::env::temp_dir().join(format!("mysh-nested-loop-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!(
            "while test ! -e '{}'; do until test -e '{}'; do touch '{}'; echo nested; done; done",
            marker.display(),
            marker.display(),
            marker.display(),
        );
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "nested\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert!(marker.exists());
        let _ = std::fs::remove_file(marker);
    }

    #[test]
    fn false_while_and_true_until_skip_their_bodies() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("while false; do echo wrong; done", &mut output, &mut errors));
        assert!(shell.execute_line("until true; do echo wrong; done", &mut output, &mut errors));
        assert!(output.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn loop_compound_can_be_sequenced_with_commands_and_nested_if() {
        let marker = std::env::temp_dir().join(format!("mysh-loop-if-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let command = format!(
            "echo before; while test ! -e '{}'; do if test ! -e '{}'; then touch '{}'; echo inside; fi; done; echo after",
            marker.display(),
            marker.display(),
            marker.display(),
        );
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "before\ninside\nafter\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert!(marker.exists());
        let _ = std::fs::remove_file(marker);
    }
}
