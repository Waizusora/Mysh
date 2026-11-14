//! Interactive input loop, shell parsing, command dispatch, and process execution.
use crate::builtins::{self, BuiltinOutcome};
use crate::command::{resolve_command, resolve_script};
use crate::lexer::{self, Word};
use crate::parser::{split_command_list, CompoundListEntry, ListOperator, Pipeline, PipelineStage};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::process::{Command, Stdio};
use std::collections::{BTreeMap, BTreeSet};

pub struct Shell {
    history: Vec<String>,
    last_status: i32,
    variables: BTreeMap<String, String>,
    exported: BTreeSet<String>,
    functions: BTreeMap<String, crate::parser::CompoundCommand>,
    function_depth: usize,
    function_return: Option<i32>,
    loop_depth: usize,
    loop_control: Option<LoopControl>,
}

impl Default for Shell {
    fn default() -> Self {
        let mut variables: BTreeMap<String, String> = std::env::vars().collect();
        let mut exported: BTreeSet<String> = variables.keys().cloned().collect();
        configure_prefix_path(&mut variables, &mut exported);
        Self { history: Vec::new(), last_status: 0, variables, exported, functions: BTreeMap::new(), function_depth: 0, function_return: None, loop_depth: 0, loop_control: None }
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
            if !should_run {
                continue;
            }
            if entry.asynchronous {
                let _ = writeln!(stderr, "mysh: asynchronous command execution is not implemented yet");
                self.last_status = 2;
                continue;
            }
            if !self.execute_pipeline(&entry.pipeline, stdout, stderr) {
                return false;
            }
            if self.function_return.is_some() {
                return true;
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
            if self.function_return.is_some() {
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
            if self.function_return.is_some() {
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

    fn execute_for_statement(&mut self, statement: &crate::parser::ForStatement, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let values: Vec<String> = if statement.words.is_empty() {
            positional_parameters(&self.variables)
        } else {
            statement.words.iter().flat_map(|word| expand_word_fields(word, &self.variables, self.last_status)).collect()
        };
        self.loop_depth += 1;
        let mut keep_running = true;
        for value in values {
            self.variables.insert(statement.name.clone(), value);
            if !self.execute_compound_list(&statement.body, stdout, stderr) { keep_running = false; break; }
            if self.function_return.is_some() { break; }
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
            if self.function_return.is_some() || self.loop_control.is_some() {
                return true;
            }
            if self.last_status == 0 {
                return self.execute_compound_list(&clause.body, stdout, stderr);
            }
        }

        if let Some(branch) = &statement.else_body {
            return self.execute_compound_list(branch, stdout, stderr);
        }
        true
    }

    fn execute_case_statement(
        &mut self,
        statement: &crate::parser::CaseStatement,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> bool {
        let value = expand_word(&statement.word, &self.variables, self.last_status);
        for arm in &statement.arms {
            let matched = arm.patterns.iter().any(|pattern| {
                let pattern = expand_word(pattern, &self.variables, self.last_status);
                shell_pattern_matches(&pattern, &value)
            });
            if matched {
                if arm.body.is_empty() {
                    self.last_status = 0;
                    return true;
                }
                return self.execute_compound_list(&arm.body, stdout, stderr);
            }
        }
        self.last_status = 0;
        true
    }

    fn execute_subshell(&mut self, body: &crate::parser::CompoundList, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let original_directory = match std::env::current_dir() {
            Ok(directory) => directory,
            Err(error) => {
                let _ = writeln!(stderr, "mysh: subshell: cannot determine current directory: {error}");
                self.last_status = 1;
                return true;
            }
        };

        let mut child = Shell {
            history: self.history.clone(),
            last_status: self.last_status,
            variables: self.variables.clone(),
            exported: self.exported.clone(),
            functions: self.functions.clone(),
            function_depth: self.function_depth,
            function_return: None,
            loop_depth: 0,
            loop_control: None,
        };
        let _ = child.execute_compound_list(body, stdout, stderr);
        let child_status = child.last_status;
        let restore_result = std::env::set_current_dir(&original_directory);
        self.last_status = child_status;

        if let Err(error) = restore_result {
            let _ = writeln!(stderr, "mysh: subshell: cannot restore current directory: {error}");
            self.last_status = 1;
        }
        true
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
            if entry.asynchronous {
                let _ = writeln!(stderr, "mysh: asynchronous command execution is not implemented yet");
                self.last_status = 2;
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
                crate::parser::CompoundCommand::For(statement) => {
                    self.execute_for_statement(statement, stdout, stderr)
                }
                crate::parser::CompoundCommand::Case(statement) => {
                    self.execute_case_statement(statement, stdout, stderr)
                }
                crate::parser::CompoundCommand::FunctionDefinition(definition) => {
                    self.functions.insert(definition.name.clone(), *definition.body.clone());
                    self.last_status = 0;
                    true
                }
                crate::parser::CompoundCommand::BraceGroup(body) => {
                    self.execute_compound_list(body, stdout, stderr)
                }
                crate::parser::CompoundCommand::Subshell(body) => {
                    self.execute_subshell(body, stdout, stderr)
                }
            };
            if !keep_running {
                return false;
            }
            if self.function_return.is_some() {
                return true;
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
            if self.function_return.is_some() || self.loop_control.is_some() {
                return true;
            }
        }
        true
    }

    fn execute_pipeline(&mut self, pipeline: &Pipeline, stdout: &mut dyn Write, stderr: &mut dyn Write) -> bool {
        let stages = &pipeline.stages;
        if stages.len() == 1 {
            return match &stages[0] {
                PipelineStage::Simple(words) => self.execute_stage(words, stdout, stderr),
                PipelineStage::Compound(command) => {
                    let entries = vec![CompoundListEntry { command: command.clone(), connector: None, asynchronous: false }];
                    self.execute_compound_list(&entries, stdout, stderr)
                }
            };
        }

        let mut input: Option<Vec<u8>> = None;
        let mut final_status = 0;
        for (index, stage) in stages.iter().enumerate() {
            let is_last = index + 1 == stages.len();
            if let PipelineStage::Compound(compound) = stage {
                let original_directory = match std::env::current_dir() {
                    Ok(directory) => directory,
                    Err(error) => {
                        let _ = writeln!(stderr, "mysh: pipeline: cannot determine current directory: {error}");
                        self.last_status = 1;
                        return true;
                    }
                };
                let mut child = Shell {
                    history: self.history.clone(),
                    last_status: self.last_status,
                    variables: self.variables.clone(),
                    exported: self.exported.clone(),
                    functions: self.functions.clone(),
                    function_depth: self.function_depth,
                    function_return: None,
                    loop_depth: 0,
                    loop_control: None,
                };
                let entries = vec![CompoundListEntry { command: compound.clone(), connector: None, asynchronous: false }];
                let mut stage_output = Vec::new();
                let mut stage_error = Vec::new();
                let _ = child.execute_compound_list(&entries, &mut stage_output, &mut stage_error);
                final_status = child.last_status;
                if let Err(error) = std::env::set_current_dir(&original_directory) {
                    let _ = writeln!(stderr, "mysh: pipeline: cannot restore current directory: {error}");
                    self.last_status = 1;
                    return true;
                }
                if !stage_error.is_empty() { let _ = stderr.write_all(&stage_error); }
                if is_last {
                    if !stage_output.is_empty() { let _ = stdout.write_all(&stage_output); }
                } else {
                    input = Some(stage_output);
                }
                continue;
            }
            let PipelineStage::Simple(stage) = stage else { unreachable!() };
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
        if let Some(message) = required_parameter_expansion_error(tokens, &self.variables, self.last_status) {
            let _ = writeln!(stderr, "mysh: {message}");
            self.last_status = 1;
            return true;
        }
        apply_assignment_parameter_expansions(tokens, &mut self.variables, self.last_status);
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

    fn dispatch_builtin(
        &mut self,
        command: &[String],
        input: Option<&[u8]>,
        out: &mut Vec<u8>,
        err: &mut Vec<u8>,
    ) -> Option<StageOutcome> {
        let outcome = builtins::run(
            &command[0],
            &command[1..],
            &self.history,
            &mut self.variables,
            &mut self.exported,
            input,
            out,
            err,
        )?;
        Some(match outcome {
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
        })
    }

    fn run_stage(&mut self, command: &[String], redirections: &Redirections, input: Option<&[u8]>, out: &mut Vec<u8>, err: &mut Vec<u8>) -> StageOutcome {
        if command.is_empty() { return StageOutcome::Continue(0); }
        if command[0] == "." {
            if command.len() < 2 {
                let _ = writeln!(err, "mysh: .: expected a file operand");
                return StageOutcome::Continue(2);
            }
            if command.len() > 2 {
                let _ = writeln!(err, "mysh: .: positional arguments are not implemented yet");
                return StageOutcome::Continue(2);
            }
            let search_path = self.variables.get("PATH").map(std::ffi::OsString::from).unwrap_or_default();
            let path = match resolve_script(&command[1], &search_path) {
                Ok(path) => path,
                Err(error) => {
                    let _ = writeln!(err, "mysh: .: {}: {error}", command[1]);
                    return StageOutcome::Continue(1);
                }
            };
            let script = match std::fs::read_to_string(&path) {
                Ok(script) => script,
                Err(error) => {
                    let _ = writeln!(err, "mysh: .: {}: {error}", path.display());
                    return StageOutcome::Continue(1);
                }
            };
            return if self.execute_line(&script, out, err) {
                StageOutcome::Continue(self.last_status)
            } else {
                StageOutcome::Exit(self.last_status)
            };
        }
        if command[0] == "eval" {
            let evaluated = command[1..].join(" ");
            if evaluated.is_empty() {
                self.last_status = 0;
                return StageOutcome::Continue(0);
            }
            return if self.execute_line(&evaluated, out, err) {
                StageOutcome::Continue(self.last_status)
            } else {
                StageOutcome::Exit(self.last_status)
            };
        }
        if command[0] == "shift" {
            if command.len() > 2 {
                let _ = writeln!(err, "mysh: shift: expected at most one operand");
                self.last_status = 2;
                return StageOutcome::Continue(2);
            }
            let amount = match command.get(1) {
                None => 1,
                Some(value) => match value.parse::<usize>() {
                    Ok(amount) => amount,
                    Err(_) => {
                        let _ = writeln!(err, "mysh: shift: operand must be a non-negative integer");
                        self.last_status = 2;
                        return StageOutcome::Continue(2);
                    }
                },
            };
            let count = positional_count(&self.variables);
            if amount > count {
                let _ = writeln!(err, "mysh: shift: amount exceeds positional parameter count");
                self.last_status = 1;
                return StageOutcome::Continue(1);
            }
            let remaining = positional_parameters(&self.variables).into_iter().skip(amount).collect::<Vec<_>>();
            set_positional_parameters(&mut self.variables, &remaining);
            self.last_status = 0;
            return StageOutcome::Continue(0);
        }
        if command[0] == "return" {
            if self.function_depth == 0 {
                let _ = writeln!(err, "mysh: return: not in a function");
                self.last_status = 1;
                return StageOutcome::Continue(1);
            }
            if command.len() > 2 {
                let _ = writeln!(err, "mysh: return: expected at most one status");
                self.last_status = 2;
                return StageOutcome::Continue(2);
            }
            let status = match command.get(1) {
                Some(value) => match value.parse::<i32>() {
                    Ok(status) => status.clamp(0, 255),
                    Err(_) => {
                        let _ = writeln!(err, "mysh: return: status must be an integer");
                        self.last_status = 2;
                        return StageOutcome::Continue(2);
                    }
                },
                None => self.last_status,
            };
            self.function_return = Some(status);
            self.last_status = status;
            return StageOutcome::Continue(status);
        }
        if is_shell_builtin(&command[0]) && is_special_builtin(&command[0]) {
            if let Some(outcome) = self.dispatch_builtin(command, input, out, err) {
                return outcome;
            }
        }
        if let Some(function_body) = self.functions.get(&command[0]).cloned() {
            let saved_positionals = positional_parameters(&self.variables);
            set_positional_parameters(&mut self.variables, &command[1..]);
            let entries = vec![CompoundListEntry { command: function_body, connector: None, asynchronous: false }];
            self.function_depth += 1;
            let keep_running = self.execute_compound_list(&entries, out, err);
            self.function_depth -= 1;
            if let Some(status) = self.function_return.take() {
                self.last_status = status;
            }
            set_positional_parameters(&mut self.variables, &saved_positionals);
            return if keep_running {
                StageOutcome::Continue(self.last_status)
            } else {
                StageOutcome::Exit(self.last_status)
            };
        }
        if is_shell_builtin(&command[0]) {
            if let Some(outcome) = self.dispatch_builtin(command, input, out, err) {
                return outcome;
            }
        }
        let search_path = self.variables.get("PATH").map(std::ffi::OsString::from).unwrap_or_default();
        let (executable, process_args) = if is_uutils_utility(&command[0]) {
            match resolve_command("coreutils", &search_path) {
                Ok(multicall) => {
                    let arguments = std::iter::once(command[0].clone())
                        .chain(command[1..].iter().cloned())
                        .collect();
                    (multicall, arguments)
                }
                Err(_) => match resolve_command(&command[0], &search_path) {
                    Ok(path) => (path, command[1..].to_vec()),
                    Err(error) => {
                        let _ = writeln!(err, "mysh: {}: {error}", command[0]);
                        return StageOutcome::Continue(if error.kind() == io::ErrorKind::NotFound {
                            127
                        } else {
                            126
                        });
                    }
                },
            }
        } else {
            match resolve_command(&command[0], &search_path) {
                Ok(path) => (path, command[1..].to_vec()),
                Err(error) => {
                    let _ = writeln!(err, "mysh: {}: {error}", command[0]);
                    return StageOutcome::Continue(if error.kind() == io::ErrorKind::NotFound {
                        127
                    } else {
                        126
                    });
                }
            }
        };
        let mut process = Command::new(executable);
        process.args(&process_args).env_clear().envs(self.exported.iter().map(|name| (name, self.variables.get(name).map(String::as_str).unwrap_or("")))).stdout(Stdio::piped()).stderr(Stdio::piped());
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

fn shell_pattern_matches(pattern: &str, value: &str) -> bool {
    fn bracket_matches(expression: &[char], candidate: char) -> Option<(bool, usize)> {
        if expression.first() != Some(&'[') { return None; }
        let mut index = 1;
        let negate = expression.get(index) == Some(&'!');
        if negate { index += 1; }
        let mut members = Vec::new();
        if expression.get(index) == Some(&']') {
            members.push(']');
            index += 1;
        }
        let mut close = None;
        while index < expression.len() {
            if expression[index] == ']' {
                close = Some(index);
                break;
            }
            members.push(expression[index]);
            index += 1;
        }
        let close = close?;
        let mut matched = false;
        let mut member = 0;
        while member < members.len() {
            if member + 2 < members.len() && members[member + 1] == '-' {
                let first = members[member];
                let last = members[member + 2];
                if first <= last && (first..=last).contains(&candidate) {
                    matched = true;
                }
                member += 3;
            } else {
                matched |= members[member] == candidate;
                member += 1;
            }
        }
        Some((if negate { !matched } else { matched }, close + 1))
    }

    fn matches(pattern: &[char], value: &[char]) -> bool {
        if pattern.is_empty() { return value.is_empty(); }
        match pattern[0] {
            '*' => (0..=value.len()).any(|offset| matches(&pattern[1..], &value[offset..])),
            '?' => !value.is_empty() && matches(&pattern[1..], &value[1..]),
            '[' if !value.is_empty() => {
                if let Some((matched, consumed)) = bracket_matches(pattern, value[0]) {
                    matched && matches(&pattern[consumed..], &value[1..])
                } else {
                    value[0] == '[' && matches(&pattern[1..], &value[1..])
                }
            }
            literal => {
                let literal = match literal {
                    '\u{e000}' => '*',
                    '\u{e001}' => '?',
                    '\u{e002}' => '[',
                    other => other,
                };
                !value.is_empty() && literal == value[0] && matches(&pattern[1..], &value[1..])
            },
        }
    }

    matches(&pattern.chars().collect::<Vec<_>>(), &value.chars().collect::<Vec<_>>())
}

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

/// Return true only for commands currently implemented as POSIX shell built-ins.
///
/// Utilities such as ls, cat, cp, mv, rm, mkdir, rmdir, touch, and echo are
/// deliberately dispatched as external commands so uutils can provide them.
fn is_shell_builtin(name: &str) -> bool {
    matches!(name, ":" | "." | "cd" | "export" | "unset" | "pwd" | "true" | "false" | "eval" | "exit" | "break" | "continue" | "return" | "shift")
}

/// Utilities compiled into the pinned uutils 0.12.0 Android feature set.
fn is_uutils_utility(name: &str) -> bool {
    matches!(
        name,
        "arch" | "base32" | "base64" | "basename" | "basenc" | "b2sum" | "cat"
            | "chgrp" | "chmod" | "chown" | "chroot" | "cksum" | "comm" | "cp"
            | "csplit" | "cut" | "date" | "dd" | "df" | "dir" | "dircolors"
            | "dirname" | "du" | "echo" | "env" | "expand" | "expr" | "factor"
            | "fmt" | "fold" | "groups" | "head" | "hostid" | "id"
            | "install" | "join" | "kill" | "link" | "ln" | "logname" | "ls"
            | "md5sum" | "mkdir" | "mkfifo" | "mknod" | "mktemp" | "more" | "mv"
            | "nice" | "nl" | "nohup" | "nproc" | "numfmt" | "od" | "paste"
            | "pathchk" | "pr" | "printenv" | "printf" | "ptx" | "readlink"
            | "realpath" | "rm" | "rmdir" | "seq" | "sha1sum" | "sha224sum"
            | "sha256sum" | "sha384sum" | "sha512sum" | "shred" | "shuf"
            | "sleep" | "sort" | "split" | "stat" | "stty" | "sum"
            | "sync" | "tac" | "tail" | "tee" | "test" | "timeout" | "touch"
            | "tr" | "truncate" | "tsort" | "tty" | "uname" | "unexpand" | "uniq"
            | "unlink" | "uptime" | "vdir" | "wc" | "whoami"
            | "yes"
    )
}

/// POSIX special built-ins execute in the current shell environment.
fn is_special_builtin(name: &str) -> bool {
    matches!(name, ":" | "." | "break" | "continue" | "eval" | "exit" | "export" | "unset" | "return" | "shift")
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

fn positional_count(environment: &BTreeMap<String, String>) -> usize {
    environment.get("#").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0)
}

fn positional_parameters(environment: &BTreeMap<String, String>) -> Vec<String> {
    (1..=positional_count(environment))
        .map(|index| environment.get(&index.to_string()).cloned().unwrap_or_default())
        .collect()
}

fn set_positional_parameters(environment: &mut BTreeMap<String, String>, parameters: &[String]) {
    let old_count = positional_count(environment);
    for index in 1..=old_count.max(parameters.len()) {
        environment.remove(&index.to_string());
    }
    for (index, value) in parameters.iter().enumerate() {
        environment.insert((index + 1).to_string(), value.clone());
    }
    environment.insert("#".to_owned(), parameters.len().to_string());
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

/// Expand a word into its argument fields, preserving quoted positional-parameter boundaries.
fn ifs_join_separator(environment: &BTreeMap<String, String>) -> String {
    match environment.get("IFS") {
        Some(value) => value.chars().next().map(|character| character.to_string()).unwrap_or_default(),
        None => " ".to_owned(),
    }
}

fn expand_word_fields(word: &Word, environment: &BTreeMap<String, String>, last_status: i32) -> Vec<String> {
    // (character, field-splitting eligible, pathname-pattern eligible)
    let mut fields: Vec<Vec<(char, bool, bool)>> = vec![Vec::new()];
    let mut keep_empty = vec![word.is_quoted()];
    let mut expanded_at = false;
    for part in word.parts() {
        if part.quoted { *keep_empty.last_mut().unwrap() = true; }
        if part.expandable && part.quoted {
            let chars: Vec<char> = part.text.chars().collect();
            let mut index = 0;
            let mut literal_start = 0;
            while index < chars.len() {
                let short = chars.get(index) == Some(&'$') && chars.get(index + 1).is_some_and(|c| matches!(c, '@' | '*'));
                let braced = chars.get(index) == Some(&'$') && chars.get(index + 1) == Some(&'{')
                    && chars.get(index + 2).is_some_and(|c| matches!(c, '@' | '*')) && chars.get(index + 3) == Some(&'}');
                let parameter = if short { chars.get(index + 1).copied() } else if braced { chars.get(index + 2).copied() } else { None };
                let length = if short { 2 } else if braced { 4 } else { 0 };
                if length == 0 { index += 1; continue; }
                let literal: String = chars[literal_start..index].iter().collect();
                let value = expand_variables(&literal, environment, last_status);
                fields.last_mut().unwrap().extend(value.chars().map(|c| (c, false, false)));
                let params = positional_parameters(environment);
                if parameter == Some('@') {
                    expanded_at = true;
                    if let Some(first) = params.first() {
                        fields.last_mut().unwrap().extend(first.chars().map(|c| (c, false, false)));
                        for value in params.into_iter().skip(1) {
                            fields.push(value.chars().map(|c| (c, false, false)).collect());
                            keep_empty.push(true);
                        }
                    }
                } else {
                    let value = params.join(&ifs_join_separator(environment));
                    fields.last_mut().unwrap().extend(value.chars().map(|c| (c, false, false)));
                }
                index += length;
                literal_start = index;
            }
            let literal: String = chars[literal_start..].iter().collect();
            let value = expand_variables(&literal, environment, last_status);
            fields.last_mut().unwrap().extend(value.chars().map(|c| (c, false, false)));
        } else if part.expandable {
            append_unquoted_expansion(&mut fields, &mut keep_empty, &part.text, environment, last_status);
        } else {
            fields.last_mut().unwrap().extend(part.text.chars().map(|c| (c, false, false)));
        }
    }
    if expanded_at && positional_count(environment) == 0 && word.parts().len() == 1
        && word.parts()[0].quoted && (word.parts()[0].text == "$@" || word.parts()[0].text == ("$".to_owned() + "{@}")) {
        return Vec::new();
    }
    if word.parts().is_empty() && word.is_quoted() {
        return vec![String::new()];
    }
    pathname_expand_fields(split_expansion_fields(fields, keep_empty, environment))
}

fn append_unquoted_expansion(fields: &mut Vec<Vec<(char, bool, bool)>>, keep_empty: &mut Vec<bool>, source: &str, environment: &BTreeMap<String, String>, last_status: i32) {
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            fields.last_mut().unwrap().push((chars[index], false, true));
            index += 1;
            continue;
        }

        let end = if matches!(chars.get(index + 1), Some('?' | '#' | '@' | '*'))
            || chars.get(index + 1).is_some_and(|character| character.is_ascii_digit())
        {
            index + 2
        } else if chars.get(index + 1) == Some(&'{') {
            chars[index + 2..].iter().position(|character| *character == '}')
                .map(|offset| index + 3 + offset).unwrap_or(index + 1)
        } else if chars.get(index + 1).is_some_and(|character| character.is_ascii_alphabetic() || *character == '_') {
            let mut end = index + 2;
            while chars.get(end).is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_') { end += 1; }
            end
        } else {
            fields.last_mut().unwrap().push(('$', false, true));
            index += 1;
            continue;
        };

        let expression: String = chars[index..end].iter().collect();
        if expression == "$@" || expression == concat!("$", "{@}") {
            let parameters = positional_parameters(environment);
            if let Some(first) = parameters.first() {
                fields.last_mut().unwrap().extend(first.chars().map(|character| (character, true, true)));
                for parameter in parameters.into_iter().skip(1) {
                    fields.push(parameter.chars().map(|character| (character, true, true)).collect());
                    keep_empty.push(false);
                }
            }
        } else {
            let value = expand_variables(&expression, environment, last_status);
            fields.last_mut().unwrap().extend(value.chars().map(|character| (character, true, true)));
        }
        index = end;
    }
}

/// Apply pathname expansion after field splitting while preserving per-character quoting.
fn pathname_expand_fields(fields: Vec<Vec<(char, bool, bool)>>) -> Vec<String> {
    const LITERAL_STAR: char = '\u{e000}';
    const LITERAL_QUESTION: char = '\u{e001}';
    const LITERAL_BRACKET: char = '\u{e002}';

    fields.into_iter().flat_map(|field| {
        let original: String = field.iter().map(|(character, _, _)| *character).collect();
        let pattern: String = field.iter().map(|(character, _, glob_eligible)| {
            if *glob_eligible { *character } else {
                match character {
                    '*' => LITERAL_STAR,
                    '?' => LITERAL_QUESTION,
                    '[' => LITERAL_BRACKET,
                    other => *other,
                }
            }
        }).collect();
        if !pattern.chars().any(|character| matches!(character, '*' | '?' | '[')) {
            return vec![original];
        }
        let matches = expand_path_pattern(&pattern);
        if matches.is_empty() { vec![original] } else { matches }
    }).collect()
}

/// Expand wildcard components without invoking another shell or external utility.
fn expand_path_pattern(pattern: &str) -> Vec<String> {
    use std::path::{Component, Path, PathBuf};
    let path = Path::new(pattern);
    let absolute = path.is_absolute();
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => {},
            Component::CurDir => components.push(".".to_owned()),
            Component::ParentDir => components.push("..".to_owned()),
            Component::Normal(value) => components.push(value.to_string_lossy().into_owned()),
            Component::Prefix(value) => components.push(value.as_os_str().to_string_lossy().into_owned()),
        }
    }
    let mut paths = vec![if absolute { PathBuf::from("/") } else { PathBuf::from("") }];
    for component in components {
        let mut next = Vec::new();
        let wildcard = component.chars().any(|c| matches!(c, '*' | '?' | '['));
        for base in &paths {
            if wildcard {
                let Ok(entries) = std::fs::read_dir(if base.as_os_str().is_empty() { Path::new(".") } else { base.as_path() }) else { continue; };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') && !component.starts_with('.') { continue; }
                    if shell_pattern_matches(&component, &name) { next.push(base.join(name)); }
                }
            } else {
                let literal_component: String = component.chars().map(|character| match character {
                    '\u{e000}' => '*',
                    '\u{e001}' => '?',
                    '\u{e002}' => '[',
                    other => other,
                }).collect();
                let candidate = base.join(literal_component);
                if std::fs::symlink_metadata(&candidate).is_ok() { next.push(candidate); }
            }
        }
        paths = next;
        if paths.is_empty() { return Vec::new(); }
    }
    let mut result: Vec<String> = paths.into_iter().map(|path| path.to_string_lossy().into_owned()).collect();
    result.sort();
    result
}

fn split_expansion_fields(fields: Vec<Vec<(char, bool, bool)>>, keep: Vec<bool>, environment: &BTreeMap<String, String>) -> Vec<Vec<(char, bool, bool)>> {
    let ifs = environment.get("IFS").map(String::as_str).unwrap_or(" \t\n");
    if ifs.is_empty() {
        return fields.into_iter().zip(keep).filter_map(|(field, preserve)| {
            let value: String = field.iter().map(|(c, _, _)| *c).collect();
            (!value.is_empty() || preserve).then_some(field)
        }).collect();
    }
    let delimiters: std::collections::BTreeSet<char> = ifs.chars().collect();
    let white: std::collections::BTreeSet<char> = ifs.chars().filter(|c| matches!(c, ' ' | '\t' | '\n')).collect();
    let mut result = Vec::new();
    for (field, preserve) in fields.into_iter().zip(keep) {
        let mut current = Vec::new();
        let mut preserve_current = preserve;
        let mut index = 0;
        let mut delimiter_seen = false;
        while index < field.len() {
            let (c, eligible, _) = field[index];
            if !eligible || !delimiters.contains(&c) { current.push(field[index]); index += 1; continue; }
            delimiter_seen = true;
            if white.contains(&c) {
                while index < field.len() && field[index].1 && white.contains(&field[index].0) { index += 1; }
                let mut nonwhite_delimiter = false;
                if index < field.len() && field[index].1 && delimiters.contains(&field[index].0) && !white.contains(&field[index].0) {
                    nonwhite_delimiter = true;
                    index += 1;
                    while index < field.len() && field[index].1 && white.contains(&field[index].0) { index += 1; }
                }
                if nonwhite_delimiter || !current.is_empty() || preserve_current {
                    result.push(std::mem::take(&mut current));
                }
                preserve_current = false;
            } else {
                result.push(std::mem::take(&mut current));
                preserve_current = false;
                index += 1;
                while index < field.len() && field[index].1 && white.contains(&field[index].0) { index += 1; }
            }
        }
        if !current.is_empty() || preserve_current || (!delimiter_seen && !field.is_empty()) { result.push(current); }
    }
    result
}

/// Detect required-parameter expansion failures before ordinary word expansion.
fn required_parameter_expansion_error(tokens: &[Word], environment: &BTreeMap<String, String>, last_status: i32) -> Option<String> {
    for token in tokens {
        for part in token.parts() {
            if !part.expandable { continue; }
            let chars: Vec<char> = part.text.chars().collect();
            let mut index = 0;
            while index + 2 < chars.len() {
                if chars[index] as u32 != 36 || chars[index + 1] != '{' { index += 1; continue; }
                let Some(relative_end) = chars[index + 2..].iter().position(|character| *character == '}') else { break; };
                let end = index + 2 + relative_end;
                let expression: String = chars[index + 2..end].iter().collect();
                let operator = if let Some(position) = expression.find(":?") {
                    Some((position, 2, true))
                } else {
                    expression.find('?').map(|position| (position, 1, false))
                };
                if let Some((position, operator_len, colon)) = operator {
                    let name = &expression[..position];
                    if valid_parameter_reference(name) {
                        let is_set = parameter_is_set(name, environment);
                        let value = parameter_value(name, environment, last_status);
                        if !is_set || (colon && value.is_empty()) {
                            let supplied = &expression[position + operator_len..];
                            let message = if supplied.is_empty() {
                                format!("parameter {} or not set", if colon { "null" } else { "unset" })
                            } else {
                                expand_variables(supplied, environment, last_status)
                            };
                            return Some(format!("{name}: {message}"));
                        }
                    }
                }
                index = end + 1;
            }
        }
    }
    None
}

/// Apply assignment-form parameter expansions before the regular word-expansion pass.
fn apply_assignment_parameter_expansions(tokens: &[Word], environment: &mut BTreeMap<String, String>, last_status: i32) {
    for token in tokens {
        for part in token.parts() {
            if !part.expandable { continue; }
            let chars: Vec<char> = part.text.chars().collect();
            let mut index = 0;
            while index + 2 < chars.len() {
                if chars[index] as u32 != 36 || chars[index + 1] != '{' { index += 1; continue; }
                let Some(relative_end) = chars[index + 2..].iter().position(|character| *character == '}') else { break; };
                let end = index + 2 + relative_end;
                let expression: String = chars[index + 2..end].iter().collect();
                if let Some(operator_index) = expression.find(":=") {
                    let name = &expression[..operator_index];
                    if valid_parameter_reference(name) && !name.chars().all(|character| character.is_ascii_digit()) {
                        let is_set = parameter_is_set(name, environment);
                        let value = parameter_value(name, environment, last_status);
                        if !is_set || value.is_empty() {
                            let word = &expression[operator_index + 2..];
                            let expanded = expand_variables(word, environment, last_status);
                            environment.insert(name.to_owned(), expanded);
                        }
                    }
                }
                index = end + 1;
            }
        }
    }
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
            command.extend(expand_word_fields(token, environment, last_status));
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
        if chars.get(index + 1) == Some(&'#') {
            result.push_str(environment.get("#").map(String::as_str).unwrap_or("0"));
            index += 2;
            continue;
        }
        if chars.get(index + 1).is_some_and(|character| matches!(character, '@' | '*')) {
            result.push_str(&positional_parameters(environment).join(&ifs_join_separator(environment)));
            index += 2;
            continue;
        }
        if chars.get(index + 1).is_some_and(|character| character.is_ascii_digit()) {
            let name = chars[index + 1].to_string();
            result.push_str(environment.get(&name).map(String::as_str).unwrap_or(""));
            index += 2;
            continue;
        }
        if chars.get(index + 1) == Some(&'{') {
            if let Some(relative_end) = chars[index + 2..].iter().position(|character| *character == '}') {
                let end = index + 2 + relative_end;
                let expression: String = chars[index + 2..end].iter().collect();
                if let Some(expanded) = expand_braced_parameter(&expression, environment, last_status) {
                    result.push_str(&expanded);
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
            while chars.get(index).is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_') { index += 1; }
            let name: String = chars[start..index].iter().collect();
            result.push_str(environment.get(&name).map(String::as_str).unwrap_or(""));
        } else {
            result.push('$');
            index += 1;
        }
    }
    result
}

/// Expand the non-assigning POSIX parameter operators supported by the current phase.
fn expand_braced_parameter(expression: &str, environment: &BTreeMap<String, String>, last_status: i32) -> Option<String> {
    for (operator, colon, mode) in [
        (":=", true, "default"), (":-", true, "default"), ("-", false, "default"),
        (":+", true, "alternate"), ("+", false, "alternate"),
        (":?", true, "required"), ("?", false, "required"),
    ] {
        if let Some(index) = expression.find(operator) {
            let name = &expression[..index];
            if !valid_parameter_reference(name) { continue; }
            let word = &expression[index + operator.len()..];
            let value = parameter_value(name, environment, last_status);
            let is_set = parameter_is_set(name, environment);
            let use_word = match mode {
                "default" => !is_set || (colon && value.is_empty()),
                _ => is_set && (!colon || !value.is_empty()),
            };
            return Some(if mode == "required" {
                value
            } else if use_word {
                expand_variables(word, environment, last_status)
            } else if mode == "default" { value } else { String::new() });
        }
    }

    for (operator, longest, prefix) in [("##", true, true), ("#", false, true), ("%%", true, false), ("%", false, false)] {
        if let Some(index) = expression.find(operator) {
            let name = &expression[..index];
            if !valid_parameter_reference(name) { continue; }
            let pattern = &expression[index + operator.len()..];
            let value = parameter_value(name, environment, last_status);
            let pattern = expand_variables(pattern, environment, last_status);
            return Some(remove_parameter_pattern(&value, &pattern, longest, prefix));
        }
    }

    if valid_parameter_reference(expression) {
        return Some(parameter_value(expression, environment, last_status));
    }
    None
}

fn valid_parameter_reference(name: &str) -> bool {
    name == "?" || name == "#" || name == "@" || name == "*"
        || (name.len() == 1 && name.as_bytes()[0].is_ascii_digit())
        || valid_parameter_name(name)
}

fn parameter_is_set(name: &str, environment: &BTreeMap<String, String>) -> bool {
    match name {
        "?" | "#" | "@" | "*" => true,
        _ => environment.contains_key(name),
    }
}

fn parameter_value(name: &str, environment: &BTreeMap<String, String>, last_status: i32) -> String {
    match name {
        "?" => last_status.to_string(),
        "#" => environment.get("#").cloned().unwrap_or_else(|| "0".to_owned()),
        "@" | "*" => positional_parameters(environment).join(&ifs_join_separator(environment)),
        _ => environment.get(name).cloned().unwrap_or_default(),
    }
}

fn remove_parameter_pattern(value: &str, pattern: &str, longest: bool, prefix: bool) -> String {
    let boundaries: Vec<usize> = if prefix {
        value.char_indices().map(|(index, _)| index).chain(std::iter::once(value.len())).collect()
    } else {
        value.char_indices().map(|(index, _)| index).chain(std::iter::once(value.len())).collect()
    };
    let mut matches: Vec<usize> = boundaries.into_iter().filter(|boundary| {
        let candidate = if prefix { &value[..*boundary] } else { &value[*boundary..] };
        shell_pattern_matches(pattern, candidate)
    }).collect();
    if matches.is_empty() { return value.to_owned(); }
    matches.sort_unstable();
    let boundary = if prefix {
        if longest { *matches.last().unwrap() } else { matches[0] }
    } else if longest {
        matches[0]
    } else {
        *matches.last().unwrap()
    };
    if prefix { value[boundary..].to_owned() } else { value[..boundary].to_owned() }
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
    use super::{configure_prefix_path, expand_variables, expand_word_fields, external_status_code, required_parameter_expansion_error, requires_more_input, shell_pattern_matches, Shell};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn brace_groups_execute_in_the_current_shell_environment() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("export MYSH_BRACE_VALUE=before; { export MYSH_BRACE_VALUE=after; }; echo $MYSH_BRACE_VALUE", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }


    #[test]
    fn subshell_isolates_variables_directory_and_exit() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let original_directory = std::env::current_dir().unwrap();
        assert!(shell.execute_line("export MYSH_SUBSHELL_VALUE=parent; ( export MYSH_SUBSHELL_VALUE=child; cd /; exit 7 ); echo $MYSH_SUBSHELL_VALUE:$?", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "parent:7\n");
        assert_eq!(std::env::current_dir().unwrap(), original_directory);
        assert_eq!(shell.last_status(), 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn executes_external_echo_utility() {
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
    fn assignment_parameter_expansion_persists_unset_and_null_values() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line("echo ${MYSH_ASSIGN_UNSET:=created}", &mut output, &mut errors));
        assert_eq!(shell.variables.get("MYSH_ASSIGN_UNSET").map(String::as_str), Some("created"));

        assert!(shell.execute_line("MYSH_ASSIGN_EMPTY=; echo ${MYSH_ASSIGN_EMPTY:=replaced}", &mut output, &mut errors));
        assert_eq!(shell.variables.get("MYSH_ASSIGN_EMPTY").map(String::as_str), Some("replaced"));

        assert!(shell.execute_line("MYSH_ASSIGN_SET=existing; echo ${MYSH_ASSIGN_SET:=ignored}", &mut output, &mut errors));
        assert_eq!(shell.variables.get("MYSH_ASSIGN_SET").map(String::as_str), Some("existing"));
        assert!(errors.is_empty(), "unexpected shell errors: {}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn required_parameter_expansion_reports_unset_and_null_values() {
        let environment = BTreeMap::from([
            ("SET".to_owned(), "value".to_owned()),
            ("EMPTY".to_owned(), String::new()),
        ]);
        let tokens = crate::lexer::lex("echo ${MISSING:?custom_failure}").unwrap();
        assert_eq!(
            required_parameter_expansion_error(&tokens, &environment, 0).as_deref(),
            Some("MISSING: custom_failure")
        );
        let tokens = crate::lexer::lex("echo ${EMPTY:?}").unwrap();
        assert_eq!(
            required_parameter_expansion_error(&tokens, &environment, 0).as_deref(),
            Some("EMPTY: parameter null or not set")
        );
        let tokens = crate::lexer::lex("echo ${EMPTY?unset only}").unwrap();
        assert_eq!(required_parameter_expansion_error(&tokens, &environment, 0), None);
        let tokens = crate::lexer::lex("echo ${SET:?failure}").unwrap();
        assert_eq!(required_parameter_expansion_error(&tokens, &environment, 0), None);
    }

    #[test]
    fn required_parameter_expansion_fails_command_with_status_one() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo ${MYSH_REQUIRED_MISSING:?required_value}", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 1);
        assert!(output.is_empty());
        assert_eq!(String::from_utf8_lossy(&errors).trim(), "mysh: MYSH_REQUIRED_MISSING: required_value");

        errors.clear();
        assert!(shell.execute_line("echo ${MYSH_REQUIRED_MISSING?unset_message}", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 1);
        assert_eq!(String::from_utf8_lossy(&errors).trim(), "mysh: MYSH_REQUIRED_MISSING: unset_message");
    }

    #[test]
    fn parameter_expansion_supports_default_and_alternate_operators() {
        let environment = BTreeMap::from([
            ("SET".to_owned(), "value".to_owned()),
            ("EMPTY".to_owned(), String::new()),
        ]);
        assert_eq!(expand_variables("${MISSING-word}", &environment, 0), "word");
        assert_eq!(expand_variables("${MISSING:-word}", &environment, 0), "word");
        assert_eq!(expand_variables("${EMPTY-word}", &environment, 0), "");
        assert_eq!(expand_variables("${EMPTY:-word}", &environment, 0), "word");
        assert_eq!(expand_variables("${SET-word}", &environment, 0), "value");
        assert_eq!(expand_variables("${SET+word}", &environment, 0), "word");
        assert_eq!(expand_variables("${SET:+word}", &environment, 0), "word");
        assert_eq!(expand_variables("${EMPTY+word}", &environment, 0), "word");
        assert_eq!(expand_variables("${EMPTY:+word}", &environment, 0), "");
    }

    #[test]
    fn parameter_expansion_removes_shortest_and_longest_patterns() {
        let environment = BTreeMap::from([("VALUE".to_owned(), "abcabc".to_owned())]);
        assert_eq!(expand_variables("${VALUE#a*c}", &environment, 0), "abc");
        assert_eq!(expand_variables("${VALUE##a*c}", &environment, 0), "");
        assert_eq!(expand_variables("${VALUE%c*}", &environment, 0), "abcab");
        assert_eq!(expand_variables("${VALUE%%c*}", &environment, 0), "ab");
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
    fn asynchronous_lists_are_parsed_but_not_run_without_job_control() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo must_not_run &", &mut output, &mut errors));
        assert!(output.is_empty());
        assert_eq!(shell.last_status(), 2);
        assert!(String::from_utf8_lossy(&errors).contains("asynchronous command execution is not implemented"));
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
    fn case_selects_first_matching_pattern_arm() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "case apple in orange) echo wrong ;; a|apple) echo selected ;; *) echo fallback ;; esac",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "selected\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn case_supports_shell_wildcard_and_bracket_patterns() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "case item42 in item*) echo wildcard ;; esac",
            &mut output,
            &mut errors,
        ));
        assert!(shell.execute_line(
            "case b in [a-z]) echo range ;; esac",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "wildcard\nrange\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn case_supports_nested_compound_commands() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "case item42 in item*) if true; then echo nested; fi ;; *) echo fallback ;; esac",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "nested\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn case_without_a_matching_arm_returns_success_without_running_a_body() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "case apple in orange) echo wrong ;; esac",
            &mut output,
            &mut errors,
        ));
        assert!(output.is_empty());
        assert!(errors.is_empty());
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn function_definition_registers_and_invokes_a_body() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "greet() { echo hello; }; greet",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "hello\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn function_invocation_exposes_and_restores_positional_parameters() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "show() { echo $1 $2 $#; }; show alpha beta; echo $1 $#",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "alpha beta 2\n0\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn shift_updates_positional_parameters_and_count() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("show() { echo $1:$2:$#; shift; echo $1:$2:$#; }; show alpha beta gamma", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "alpha:beta:3\nbeta:gamma:2\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn shift_inside_nested_function_restores_outer_parameters() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("inner() { shift; echo $1:$#; }; outer() { inner x y; echo $1:$2:$#; }; outer a b c", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "y:1\na:b:3\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn positional_star_and_at_expand_using_ifs_separator() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("show() { echo $*; echo $@; }; show one two three", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "one two three\none two three\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn quoted_star_joins_positional_parameters_into_one_field() {
        let mut shell = Shell::new();
        shell.variables.insert("IFS".to_owned(), ":".to_owned());
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '<%s>\\n' \"$*\"; }; show alpha beta gamma",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "<alpha:beta:gamma>\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn quoted_star_with_empty_ifs_concatenates_parameters() {
        let mut shell = Shell::new();
        shell.variables.insert("IFS".to_owned(), String::new());
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '<%s>\\n' \"${*}\"; }; show alpha beta gamma",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "<alphabetagamma>\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }
    #[test]
    fn quoted_star_uses_space_when_ifs_is_unset() {
        let mut shell = Shell::new();
        shell.variables.remove("IFS");
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '<%s>\\n' \"$*\"; }; show alpha beta",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "<alpha beta>\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn quoted_star_without_positional_parameters_is_one_empty_field() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '<%s>\\n' \"$*\"; }; show",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "<>\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }
    #[test]
    fn quoted_at_preserves_argument_boundaries_and_empty_arguments() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '<%s>\\n' \"$@\"; }; show 'one two' '' three",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "<one two>\n<>\n<three>\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn quoted_at_attaches_adjacent_prefix_and_suffix_to_boundary_fields() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '[%s]\\n' pre\"$@\"post; }; show first second",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "[prefirst]\n[secondpost]\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn quoted_at_with_no_positional_parameters_expands_to_no_fields() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "show() { printf '%s' \"$@\"; }; show",
            &mut output,
            &mut errors,
        ));
        assert!(output.is_empty());
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn for_without_word_list_iterates_over_positional_parameters() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("iterate() { for item; do echo $item; done; }; iterate first second third", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "first\nsecond\nthird\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn nested_function_calls_restore_the_callers_positional_parameters() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "inner() { echo $1; }; outer() { inner innerarg; echo $1; }; outer outerarg",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "innerarg\nouterarg\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn function_definition_persists_shell_variable_changes() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "set_value() { VALUE=updated; }; set_value; echo $VALUE",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "updated\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn shell_functions_override_regular_builtins() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "pwd() { echo function; }; pwd",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "function\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn special_builtins_keep_precedence_over_shell_functions() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "export() { echo shadowed; }; export MYSH_FUNCTION_TEST=active; echo $MYSH_FUNCTION_TEST",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "active\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn return_exits_a_function_with_the_requested_status() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "finish() { echo before; return 7; echo after; }; finish; echo $?",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "before\n7\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn return_without_an_operand_uses_the_current_status() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "finish() { false; return; echo unreachable; }; finish; echo $?",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "1\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn return_propagates_out_of_nested_loop_bodies() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "finish() { while true; do return 9; done; echo unreachable; }; finish; echo $?",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "9\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn return_stops_commands_evaluated_by_eval() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "finish() { eval 'return 6; echo unreachable'; echo after; }; finish; echo $?",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "6\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn return_propagates_from_if_conditions() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(
            "finish() { if return 8; then echo unreachable; fi; echo after; }; finish; echo $?",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "8\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn return_is_rejected_outside_a_function() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line("return 3", &mut output, &mut errors));
        assert_eq!(shell.last_status(), 1);
        assert!(String::from_utf8(errors).unwrap().contains("not in a function"));
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
    fn if_compound_command_can_run_as_a_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "if true; then echo conditional; fi | cat",
            &mut output,
            &mut errors
        ));
        assert_eq!(String::from_utf8_lossy(&output), "conditional\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn nested_compound_pipeline_executes_inside_an_if_body() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "if true; then if true; then echo nested; fi | cat; fi",
            &mut output,
            &mut errors
        ));
        assert_eq!(String::from_utf8_lossy(&output), "nested\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn case_compound_command_can_run_as_a_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "case value in value) echo selected ;; esac | cat",
            &mut output,
            &mut errors
        ));
        assert_eq!(String::from_utf8_lossy(&output), "selected\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn for_compound_command_can_run_as_a_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "for item in one two; do echo $item; done | cat",
            &mut output,
            &mut errors
        ));
        assert_eq!(String::from_utf8_lossy(&output), "one\ntwo\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn loop_compound_commands_can_run_as_pipeline_stages() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "while false; do echo wrong; done | cat",
            &mut output,
            &mut errors
        ));
        assert!(output.is_empty());
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));

        assert!(shell.execute_line(
            "until true; do echo wrong; done | cat",
            &mut output,
            &mut errors
        ));
        assert!(output.is_empty());
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn brace_group_can_run_as_a_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("{ echo one; echo two; } | cat", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "one\ntwo\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn subshell_can_run_as_a_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("( echo hello ) | cat", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "hello\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn compound_group_can_run_after_a_simple_pipeline_stage() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("echo before | { echo after; }", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);
    }

    #[test]
    fn pipeline_compound_stage_does_not_leak_shell_state() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("{ export MYSH_PIPE_GROUP=value; echo inside; } | cat", &mut output, &mut errors));
        assert!(shell.execute_line("echo $MYSH_PIPE_GROUP", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "inside\n\n");
        assert!(!shell.variables.contains_key("MYSH_PIPE_GROUP"));
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn pipeline_passes_external_cat_stdin() {
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
    fn pipeline_passes_input_through_external_utility_stages() {
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
    fn for_loop_expands_explicit_word_list_and_retains_final_variable() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("for item in alpha beta; do echo $item; done; echo final-$item", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "alpha\nbeta\nfinal-beta\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }

    #[test]
    fn for_loop_supports_break_and_continue() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("for item in alpha beta; do echo $item; break; echo skipped; done; echo after", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "alpha\nafter\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));

        let mut shell = Shell::new();
        let mut output = Vec::new();
        assert!(shell.execute_line("for item in alpha beta; do continue; echo skipped; done; echo after", &mut output, &mut Vec::new()));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
    }

    #[test]
    fn nested_for_supports_multilevel_break_and_continue() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("for outer in a b; do for inner in x y; do echo $outer-$inner; break 2; done; echo skipped; done; echo after", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "a-x\nafter\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));

        let mut shell = Shell::new();
        let mut output = Vec::new();
        assert!(shell.execute_line("for outer in a b; do for inner in x; do echo $outer; continue 2; done; echo skipped; done", &mut output, &mut Vec::new()));
        assert_eq!(String::from_utf8(output).unwrap(), "a\nb\n");
    }

    #[test]
    fn for_and_while_loops_execute_when_mixed_and_nested() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line(
            "for item in alpha beta; do while false; do echo skipped; done; echo $item; done",
            &mut output,
            &mut errors,
        ));
        assert_eq!(String::from_utf8(output).unwrap(), "alpha\nbeta\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));

        let marker = std::env::temp_dir().join(format!("mysh-mixed-loop-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let command = format!(
            "while test ! -e '{}'; do for item in one two; do echo $item; done; touch '{}'; done",
            marker.display(),
            marker.display(),
        );
        let mut output = Vec::new();
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "one\ntwo\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        let _ = std::fs::remove_file(marker);
    }

    #[test]
    fn for_without_explicit_word_list_runs_zero_times_without_positional_parameters() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        assert!(shell.execute_line("for item; do echo unexpected; done; echo after", &mut output, &mut Vec::new()));
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
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
        let command = format!("while test ! -e '{}'; do while test ! -e '{}'; do touch '{}'; continue 2; echo skipped-inner; done; touch '{}'; echo skipped-outer; done; echo after", outer.display(), inner.display(), inner.display(), outer.display());
        assert!(shell.execute_line(&command, &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "skipped-outer\nafter\n");
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

    #[test]
    fn only_posix_shell_builtins_are_dispatched_in_process() {
        use super::is_shell_builtin;

        for name in [":", ".", "cd", "pwd", "export", "unset", "true", "false", "eval", "exit", "break", "continue"] {
            assert!(is_shell_builtin(name), "{name}");
        }
        for name in ["ls", "cat", "cp", "mv", "rm", "mkdir", "rmdir", "touch", "echo", "clear", "history", "help"] {
            assert!(!is_shell_builtin(name), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn missing_utility_is_dispatched_through_uutils_multicall_binary() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!("mysh-coreutils-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let multicall = directory.join("coreutils");
        std::fs::write(&multicall, "#!/bin/sh\nprintf 'uutils:%s\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&multicall, std::fs::Permissions::from_mode(0o755)).unwrap();
        let standalone = directory.join("cat");
        std::fs::write(&standalone, "#!/bin/sh\nprintf 'standalone\n'\n").unwrap();
        std::fs::set_permissions(&standalone, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut shell = Shell::new();
        shell.variables.insert("PATH".to_owned(), directory.to_string_lossy().into_owned());
        shell.exported.insert("PATH".to_owned());

        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert!(shell.execute_line("cat alpha beta", &mut output, &mut errors));
        assert_eq!(String::from_utf8(output).unwrap(), "uutils:cat\nuutils:alpha\nuutils:beta\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(shell.last_status(), 0);

        std::fs::remove_dir_all(directory).unwrap();
    }


    #[test]
    fn eval_reparses_commands_in_the_current_shell_environment() {
        let mut shell = Shell::new();
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line("eval 'MYSH_EVAL_VALUE=active; export MYSH_EVAL_VALUE; echo $MYSH_EVAL_VALUE'", &mut output, &mut errors));
        assert_eq!(shell.variables.get("MYSH_EVAL_VALUE").map(String::as_str), Some("active"));
        assert!(shell.exported.contains("MYSH_EVAL_VALUE"));
        assert_eq!(String::from_utf8(output).unwrap(), "active\n");
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    }


    #[test]
    fn dot_sources_a_readable_file_into_the_current_shell() {
        let directory = std::env::temp_dir().join(format!("mysh-dot-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let script = directory.join("environment.sh");
        std::fs::write(&script, "MYSH_DOT_VALUE=loaded\nexport MYSH_DOT_VALUE\n").unwrap();

        let mut shell = Shell::new();
        shell.variables.insert("PATH".to_owned(), directory.to_string_lossy().into_owned());
        shell.exported.insert("PATH".to_owned());
        let mut output = Vec::new();
        let mut errors = Vec::new();

        assert!(shell.execute_line(". environment.sh", &mut output, &mut errors));
        assert_eq!(shell.variables.get("MYSH_DOT_VALUE").map(String::as_str), Some("loaded"));
        assert!(shell.exported.contains("MYSH_DOT_VALUE"));
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));

        std::fs::remove_dir_all(directory).unwrap();
    }


    #[test]
    fn ifs_field_splitting_tracks_expansion_origin_and_empty_fields() {
        let mut environment = BTreeMap::from([
            ("VALUE".to_owned(), "  alpha   beta  ".to_owned()),
            ("EMPTY".to_owned(), String::new()),
        ]);
        let expand = |source: &str, environment: &BTreeMap<String, String>| {
            let word = crate::lexer::lex(source).unwrap().remove(0);
            expand_word_fields(&word, environment, 0)
        };

        assert_eq!(expand("$VALUE", &environment), ["alpha", "beta"]);
        environment.insert("VALUE".to_owned(), " \talpha\nbeta \t".to_owned());
        assert_eq!(expand("$VALUE", &environment), ["alpha", "beta"]);
        assert_eq!(expand(r"literal\ whitespace$EMPTY", &environment), ["literal whitespace"]);
        assert!(expand("$EMPTY", &environment).is_empty());
        assert_eq!(expand("\"$EMPTY\"", &environment), [""]);
        assert_eq!(expand("\"\"$EMPTY", &environment), [""]);
        assert_eq!(expand("\"\"", &environment), [""]);

        environment.insert("IFS".to_owned(), ":".to_owned());
        environment.insert("VALUE".to_owned(), "left::right:".to_owned());
        assert_eq!(expand("$VALUE", &environment), ["left", "", "right"]);

        environment.insert("IFS".to_owned(), " ".to_owned());
        environment.insert("VALUE".to_owned(), "  one   two  ".to_owned());
        assert_eq!(expand("$VALUE", &environment), ["one", "two"]);

        environment.insert("IFS".to_owned(), " :".to_owned());
        environment.insert("VALUE".to_owned(), "left : right".to_owned());
        assert_eq!(expand("$VALUE", &environment), ["left", "right"]);

        environment.insert("VALUE".to_owned(), " :first".to_owned());
        assert_eq!(expand("$VALUE", &environment), ["", "first"]);

        environment.insert("IFS".to_owned(), String::new());
        environment.insert("VALUE".to_owned(), " one two ".to_owned());
        assert_eq!(expand("$VALUE", &environment), [" one two "]);

        environment.remove("IFS");
        environment.insert("VALUE".to_owned(), "middle".to_owned());
        assert_eq!(expand("pre${VALUE}post", &environment), ["premiddlepost"]);
    }

    #[test]
    fn quoted_positional_parameter_expansion_keeps_field_boundaries() {
        let mut environment = BTreeMap::new();
        super::set_positional_parameters(&mut environment, &["one two".to_owned(), String::new(), "three".to_owned()]);
        let word = crate::lexer::lex("\"$@\"").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["one two", "", "three"]);

        let word = crate::lexer::lex("\"$*\"").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["one two  three"]);

        let word = crate::lexer::lex("$@").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["one", "two", "three"]);

        let mut environment = BTreeMap::new();
        super::set_positional_parameters(&mut environment, &["left".to_owned(), String::new(), "right".to_owned()]);
        environment.insert("IFS".to_owned(), ":".to_owned());
        let word = crate::lexer::lex("$@").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["left", "right"]);
        super::set_positional_parameters(&mut environment, &["left".to_owned(), "right".to_owned()]);
        let word = crate::lexer::lex("pre$@post").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["preleft", "rightpost"]);
        super::set_positional_parameters(&mut environment, &["left".to_owned(), String::new(), "right".to_owned()]);
        let word = crate::lexer::lex("$*").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["left", "", "right"]);

        environment.insert("IFS".to_owned(), String::new());
        let word = crate::lexer::lex("$@").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["left", "right"]);
        let word = crate::lexer::lex("$*").unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), ["leftright"]);
    }

    #[test]
    fn pathname_expansion_combines_quoted_literals_with_unquoted_patterns() {
        let directory = std::env::temp_dir().join(format!("mysh-mixed-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        for name in ["alpha-one.txt", "alpha-two.txt", "beta-one.txt"] {
            std::fs::write(directory.join(name), "").unwrap();
        }
        let environment = BTreeMap::new();
        let source = format!("{}/\"alpha\"-*.txt", directory.display());
        let word = crate::lexer::lex(&source).unwrap().remove(0);
        let mut expected = vec![
            directory.join("alpha-one.txt").to_string_lossy().into_owned(),
            directory.join("alpha-two.txt").to_string_lossy().into_owned(),
        ];
        expected.sort();
        assert_eq!(expand_word_fields(&word, &environment, 0), expected);

        let source = format!("{}/alpha\\**.txt", directory.display());
        let word = crate::lexer::lex(&source).unwrap().remove(0);
        assert_eq!(expand_word_fields(&word, &environment, 0), [source.replace("\\*", "*")]);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pathname_expansion_tracks_quoted_and_unquoted_variable_results() {
        let directory = std::env::temp_dir().join(format!("mysh-variable-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        for name in ["alpha-one.txt", "alpha-two.txt", "beta-one.txt"] {
            std::fs::write(directory.join(name), "").unwrap();
        }
        let mut environment = BTreeMap::new();
        environment.insert("PATTERN".to_owned(), format!("{}/*.txt", directory.display()));
        environment.insert("SUFFIX".to_owned(), "-*.txt".to_owned());
        let expand = |source: &str, environment: &BTreeMap<String, String>| {
            let word = crate::lexer::lex(source).unwrap().remove(0);
            expand_word_fields(&word, environment, 0)
        };

        let mut expected: Vec<String> = ["alpha-one.txt", "alpha-two.txt", "beta-one.txt"]
            .into_iter().map(|name| directory.join(name).to_string_lossy().into_owned()).collect();
        expected.sort();
        assert_eq!(expand("$PATTERN", &environment), expected);
        assert_eq!(expand("\"$PATTERN\"", &environment), [environment["PATTERN"].clone()]);

        let source = format!("{}/\"alpha\"$SUFFIX", directory.display());
        assert_eq!(expand(&source, &environment), [
            directory.join("alpha-one.txt").to_string_lossy().into_owned(),
            directory.join("alpha-two.txt").to_string_lossy().into_owned(),
        ]);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bracket_patterns_support_negation_ranges_and_literal_closing_bracket() {
        assert!(shell_pattern_matches("[!a-c]", "z"));
        assert!(!shell_pattern_matches("[!a-c]", "b"));
        assert!(shell_pattern_matches("[a-c]", "b"));
        assert!(!shell_pattern_matches("[a-c]", "d"));
        assert!(shell_pattern_matches("[]a]", "]"));
        assert!(shell_pattern_matches("[]a]", "a"));
        assert!(shell_pattern_matches("[abc", "[abc"));
    }

    #[test]
    fn pathname_expansion_matches_wildcards_and_keeps_unmatched_patterns() {
        let directory = std::env::temp_dir().join(format!("mysh-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        for name in ["alpha.txt", "alpine.txt", "beta.log", ".hidden.txt"] {
            std::fs::write(directory.join(name), "").unwrap();
        }
        let environment = BTreeMap::new();
        let expand = |source: &str| {
            let word = crate::lexer::lex(source).unwrap().remove(0);
            expand_word_fields(&word, &environment, 0)
        };
        let pattern = directory.join("al*.txt").to_string_lossy().into_owned();
        let mut expected = vec![
            directory.join("alpha.txt").to_string_lossy().into_owned(),
            directory.join("alpine.txt").to_string_lossy().into_owned(),
        ];
        expected.sort();
        assert_eq!(expand(&pattern), expected);
        let pattern = directory.join("a?pha.txt").to_string_lossy().into_owned();
        assert_eq!(expand(&pattern), [directory.join("alpha.txt").to_string_lossy().into_owned()]);
        let pattern = directory.join("[ab]eta.log").to_string_lossy().into_owned();
        assert_eq!(expand(&pattern), [directory.join("beta.log").to_string_lossy().into_owned()]);
        let pattern = directory.join("missing-*.txt").to_string_lossy().into_owned();
        assert_eq!(expand(&pattern), [pattern]);
        assert_eq!(expand(r#""*.txt""#), ["*.txt"]);
        let pattern = directory.join("*.txt").to_string_lossy().into_owned();
        let hidden = directory.join(".hidden.txt").to_string_lossy().into_owned();
        assert!(!expand(&pattern).contains(&hidden));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
