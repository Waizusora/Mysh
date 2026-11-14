//! Incremental shell-input classification and context-sensitive reserved-word helpers.

use crate::lexer::{self, Word};

/// Operator connecting adjacent command-list entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOperator { Sequence, And, Or }

/// A pipeline is an ordered sequence of command stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    pub stages: Vec<PipelineStage>,
}

/// A pipeline stage is either a simple command or a supported compound command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStage {
    Simple(Vec<Word>),
    Compound(CompoundCommand),
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
    let mut brace_depth = 0usize;
    let mut subshell_depth = 0usize;
    for token in tokens {
        if token.is_unquoted_literal("{") {
            brace_depth += 1;
        } else if token.is_unquoted_literal("}") {
            if brace_depth == 0 { return Err("unmatched closing brace"); }
            brace_depth -= 1;
        } else if token.is_unquoted_literal("(") {
            subshell_depth += 1;
        } else if token.is_unquoted_literal(")") {
            if subshell_depth == 0 { return Err("unmatched closing parenthesis"); }
            subshell_depth -= 1;
        }
        let at_list_level = brace_depth == 0 && subshell_depth == 0;
        let operator = if at_list_level && token.is_unquoted_literal(";") { Some(ListOperator::Sequence) }
        else if at_list_level && token.is_unquoted_literal("&&") { Some(ListOperator::And) }
        else if at_list_level && token.is_unquoted_literal("||") { Some(ListOperator::Or) }
        else if at_list_level && token.is_unquoted_literal("&") { return Err("background execution with '&' is not supported"); }
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
    if brace_depth != 0 || subshell_depth != 0 {
        return Err("compound command in command list is not closed");
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

/// Return true for POSIX operators that the lexer recognizes but the
/// current execution engine does not implement yet.
fn is_unsupported_operator(token: &Word) -> bool {
    if ["<<", "<<-", "<>", "<&", ">&", ">|"]
        .iter()
        .any(|operator| token.is_unquoted_literal(operator))
    {
        return true;
    }

    let value = token.literal_value();
    if !token.is_unquoted_literal(&value) {
        return false;
    }
    let Some(redirection_start) = value.find(|character: char| character == '<' || character == '>') else {
        return false;
    };
    if !value[..redirection_start].chars().all(|character| character.is_ascii_digit()) {
        return false;
    }

    // The current executor only implements the default input/output
    // descriptors and the explicit stderr form "2>". Reject every other
    // descriptor/operator combination instead of passing it as argv data.
    !matches!(value.as_str(), "<" | ">" | ">>" | "2>")
}

/// Split one pipeline into its ordered stages.
pub fn split_pipeline(tokens: Vec<Word>) -> Result<Pipeline, &'static str> {
    let mut raw_stages: Vec<Vec<Word>> = vec![Vec::new()];
    let mut brace_depth = 0usize;
    let mut subshell_depth = 0usize;

    for token in tokens {
        if is_unsupported_operator(&token) || token.is_unquoted_literal(";;") {
            return Err("recognized shell operator is not implemented yet");
        }
        if token.is_unquoted_literal("{") {
            brace_depth += 1;
        } else if token.is_unquoted_literal("}") {
            if brace_depth == 0 { return Err("unmatched closing brace"); }
            brace_depth -= 1;
        } else if token.is_unquoted_literal("(") {
            subshell_depth += 1;
        } else if token.is_unquoted_literal(")") {
            if subshell_depth == 0 { return Err("unmatched closing parenthesis"); }
            subshell_depth -= 1;
        }

        if token.is_unquoted_literal("|") && brace_depth == 0 && subshell_depth == 0 {
            if raw_stages.last().is_some_and(Vec::is_empty) { return Err("empty pipeline stage"); }
            raw_stages.push(Vec::new());
        } else {
            raw_stages.last_mut().expect("pipeline has a stage").push(token);
        }
    }

    if brace_depth != 0 || subshell_depth != 0 { return Err("compound command in pipeline is not closed"); }
    if raw_stages.last().is_some_and(Vec::is_empty) { return Err("pipeline cannot end with |"); }

    let mut stages = Vec::with_capacity(raw_stages.len());
    for stage in raw_stages {
        let Some(first) = stage.first() else { return Err("empty pipeline stage"); };
        if first.is_unquoted_literal("{") || first.is_unquoted_literal("(") {
            let parsed = parse_compound_list(&stage)?;
            if parsed.len() != 1 || parsed[0].connector.is_some() {
                return Err("pipeline compound stage must contain exactly one compound command");
            }
            match &parsed[0].command {
                CompoundCommand::BraceGroup(_) | CompoundCommand::Subshell(_) => {
                    stages.push(PipelineStage::Compound(parsed[0].command.clone()));
                }
                _ => return Err("unsupported compound command in pipeline"),
            }
        } else if stage.iter().any(|word| word.is_unquoted_literal("{")
            || word.is_unquoted_literal("}")
            || word.is_unquoted_literal("(")
            || word.is_unquoted_literal(")"))
        {
            return Err("compound delimiters must form a complete pipeline stage");
        } else {
            stages.push(PipelineStage::Simple(stage));
        }
    }

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
    fn command_list_split_keeps_separators_inside_nested_groups() {
        let parsed = split_command_list(lexer::lex("{ echo one; echo two; } | cat; echo after").unwrap()).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pipeline.stages.len(), 2);
        assert_eq!(parsed[1].connector, Some(ListOperator::Sequence));
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
        assert!(matches!(&stages.stages[0], super::PipelineStage::Simple(words) if words.len() == 2));
        assert!(matches!(&stages.stages[1], super::PipelineStage::Simple(words) if words.len() == 1));
        assert!(matches!(&stages.stages[2], super::PipelineStage::Simple(words) if words.len() == 1));
    }
    #[test]
    fn pipeline_split_rejects_empty_stages() {
        for input in ["| cat", "echo |", "echo | | cat"] {
            assert!(split_pipeline(lexer::lex(input).unwrap()).is_err(), "{input}");
        }
    }
    #[test]
    fn parser_rejects_lexically_recognized_but_unimplemented_operators() {
        for input in [
            "case value in value) echo matched ;; esac",
            "cat <<EOF",
            "cat <<-EOF",
            "cmd 3>&1",
            "cmd 4<&0",
            "cmd 10>>errors",
            "cmd 1>output",
            "cmd 2>>errors",
            "cmd 2>&1",
        ] {
            let tokens = lexer::lex(input).unwrap();
            assert!(split_pipeline(tokens).is_err(), "{input}");
        }
    }


    #[test]
    fn case_parser_builds_pattern_arms_and_nested_bodies() {
        use super::{parse_compound_list, CompoundCommand};
        let tokens = lexer::lex("case $value in one|two) echo first ;; *) if true; then echo fallback; fi ;; esac").unwrap();
        let parsed = parse_compound_list(&tokens).unwrap();
        let CompoundCommand::Case(statement) = &parsed[0].command else { panic!("expected case node"); };
        assert_eq!(statement.arms.len(), 2);
        assert_eq!(statement.arms[0].patterns.len(), 2);
        assert!(matches!(&statement.arms[1].body[0].command, CompoundCommand::If(_)));
    }

    #[test]
    fn case_parser_rejects_missing_patterns_and_closers() {
        for input in ["case value in ) echo bad ;; esac", "case value in value) echo missing", "case value value) echo bad ;; esac"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(super::parse_compound_list(&tokens).is_err(), "{input}");
        }
    }

    #[test]
    fn compound_group_stages_are_preserved_in_pipeline_ast() {
        use super::{CompoundCommand, PipelineStage};
        let left_group = split_pipeline(lexer::lex("{ echo one; echo two; } | wc -l").unwrap()).unwrap();
        assert!(matches!(&left_group.stages[0], PipelineStage::Compound(CompoundCommand::BraceGroup(_))));
        let left_subshell = split_pipeline(lexer::lex("( echo hello ) | cat").unwrap()).unwrap();
        assert!(matches!(&left_subshell.stages[0], PipelineStage::Compound(CompoundCommand::Subshell(_))));
        let right_group = split_pipeline(lexer::lex("echo before | { cat; }").unwrap()).unwrap();
        assert!(matches!(&right_group.stages[1], PipelineStage::Compound(CompoundCommand::BraceGroup(_))));
    }

    #[test]
    fn quoted_pipe_remains_a_word() {
        let stages = split_pipeline(lexer::lex("echo '|'").unwrap()).unwrap();
        assert_eq!(stages.stages.len(), 1);
        assert_eq!(stages.stages[0].len(), 2);
    }

    #[test]
    fn quoted_redirection_looking_words_remain_arguments() {
        let stages = split_pipeline(lexer::lex("echo '10>>' '3<&'").unwrap()).unwrap();
        assert_eq!(stages.stages[0].len(), 3);
    }
}



/// Kind of a supported loop compound command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind { While, Until }

/// Parsed POSIX-style for command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForStatement { pub name: String, pub words: Vec<Word>, pub body: CompoundList }

/// A recursively parsed command in a compound command list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompoundCommand {
    Simple(Vec<Word>),
    If(IfStatement),
    Loop(LoopStatement),
    For(ForStatement),
    Case(CaseStatement),
    BraceGroup(CompoundList),
    Subshell(CompoundList),
}

/// One pattern arm in a POSIX case command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: CompoundList,
}

/// Parsed POSIX case command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseStatement {
    pub word: Word,
    pub arms: Vec<CaseArm>,
}

/// One command and its connector to the preceding command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundListEntry {
    pub command: CompoundCommand,
    pub connector: Option<ListOperator>,
}

/// A command list that can contain nested compound commands.
pub type CompoundList = Vec<CompoundListEntry>;

/// Parsed loop structure with recursively parsed condition and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStatement {
    pub kind: LoopKind,
    pub condition: CompoundList,
    pub body: CompoundList,
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
        if is_reserved_word(tokens, index, "while") || is_reserved_word(tokens, index, "until") || is_reserved_word(tokens, index, "for") {
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
        condition: parse_compound_list(&tokens[1..condition_end])?,
        body: parse_compound_list(&tokens[body_start + 1..body_end])?,
    })
}


/// Parse a complete for compound command. Positional parameters are not yet
/// modeled, so an omitted explicit word list currently produces zero iterations.
pub fn parse_for_statement(tokens: &[Word]) -> Result<ForStatement, &'static str> {
    let tokens = if tokens.last().is_some_and(|word| word.is_unquoted_literal(";")) { &tokens[..tokens.len()-1] } else { tokens };
    if !tokens.first().is_some_and(|word| word.is_unquoted_literal("for")) { return Err("expected for"); }
    let name = tokens.get(1).ok_or("for requires a variable name")?.literal_value().to_owned();
    let mut chars = name.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("for requires a valid variable name");
    }
    let mut do_index = None;
    for index in 2..tokens.len() {
        if is_reserved_word(tokens, index, "do") { do_index = Some(index); break; }
    }
    let body_start = do_index.ok_or("for requires do and done")?;
    let body_end = tokens.len().checked_sub(1).ok_or("for requires done")?;
    if !tokens[body_end].is_unquoted_literal("done") || body_start + 1 > body_end { return Err("malformed for compound command"); }
    let words = if tokens.get(2).is_some_and(|word| word.is_unquoted_literal("in")) {
        let mut end = body_start;
        if end > 3 && tokens[end-1].is_unquoted_literal(";") { end -= 1; }
        tokens[3..end].to_vec()
    } else {
        if body_start != 2 && !(body_start == 3 && tokens[2].is_unquoted_literal(";")) { return Err("expected in or command separator before do"); }
        Vec::new()
    };
    let mut body_end = body_end;
    if body_end > body_start + 1 && tokens[body_end-1].is_unquoted_literal(";") { body_end -= 1; }
    let body = parse_compound_list(&tokens[body_start+1..body_end])?;
    Ok(ForStatement { name, words, body })
}

/// One condition/body pair in an if compound command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfClause {
    pub condition: CompoundList,
    pub body: CompoundList,
}

/// Parsed if/elif/else compound command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStatement {
    pub clauses: Vec<IfClause>,
    pub else_body: Option<CompoundList>,
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
            condition: parse_compound_list(&tokens[cursor..condition_end])?,
            body: parse_compound_list(&body)?,
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
            else_body = Some(parse_compound_list(&body)?);
            break;
        }
    }

    if clauses.is_empty() {
        return Err("incomplete if statement");
    }
    Ok(IfStatement { clauses, else_body })
}



/// Parse a POSIX case command. Patterns are retained as shell words so the
/// execution phase can apply expansion and pattern matching in the case context.
pub fn parse_case_statement(tokens: &[Word]) -> Result<CaseStatement, &'static str> {
    if !tokens.first().is_some_and(|word| word.is_unquoted_literal("case")) {
        return Err("expected case");
    }
    let word = tokens.get(1).ok_or("case requires a word")?.clone();
    let in_index = (2..tokens.len()).find(|&index| tokens[index].is_unquoted_literal("in"))
        .ok_or("case requires in")?;
    let mut index = in_index + 1;
    let mut arms = Vec::new();
    loop {
        while tokens.get(index).is_some_and(|token| token.is_unquoted_literal(";")) { index += 1; }
        if tokens.get(index).is_some_and(|token| is_reserved_word(tokens, index, "esac")) {
            if index + 1 != tokens.len() { return Err("unexpected tokens after esac"); }
            if arms.is_empty() { return Err("case requires at least one pattern arm"); }
            return Ok(CaseStatement { word, arms });
        }
        if index >= tokens.len() { return Err("case requires a closing esac"); }
        let mut patterns = Vec::new();
        loop {
            let pattern = tokens.get(index).ok_or("case pattern is missing")?;
            if pattern.is_unquoted_literal("|") || pattern.is_unquoted_literal(")") {
                return Err("case pattern is missing");
            }
            patterns.push(pattern.clone());
            index += 1;
            match tokens.get(index) {
                Some(token) if token.is_unquoted_literal("|") => { index += 1; }
                Some(token) if token.is_unquoted_literal(")") => { index += 1; break; }
                _ => return Err("case pattern list requires '|' or ')'"),
            }
        }
        let body_start = index;
        let mut nested_case = 0usize;
        let mut terminator = None;
        while index < tokens.len() {
            if is_reserved_word(tokens, index, "case") { nested_case += 1; }
            else if is_reserved_word(tokens, index, "esac") {
                if nested_case == 0 { terminator = Some((index, false)); break; }
                nested_case -= 1;
            } else if tokens[index].is_unquoted_literal(";;") && nested_case == 0 {
                terminator = Some((index, true)); break;
            }
            index += 1;
        }
        let (body_end, has_separator) = terminator.ok_or("case requires ';;' or esac")?;
        let body = if body_end == body_start { Vec::new() } else { parse_compound_list(&tokens[body_start..body_end])? };
        arms.push(CaseArm { patterns, body });
        index = body_end;
        if has_separator {
            index += 1;
            continue;
        }
        if index + 1 != tokens.len() { return Err("unexpected tokens after esac"); }
        if arms.is_empty() { return Err("case requires at least one pattern arm"); }
        return Ok(CaseStatement { word, arms });
    }
}

/// Find the end of a command segment, ignoring list separators nested in groups.
fn command_segment_end(tokens: &[Word], start: usize) -> usize {
    let mut brace_depth = 0usize;
    let mut subshell_depth = 0usize;
    for index in start..tokens.len() {
        let token = &tokens[index];
        if brace_depth == 0 && subshell_depth == 0
            && (token.is_unquoted_literal(";;") || token.is_unquoted_literal(";")
                || token.is_unquoted_literal("&&")
                || token.is_unquoted_literal("||"))
        {
            return index;
        }
        if token.is_unquoted_literal("{") {
            brace_depth += 1;
        } else if token.is_unquoted_literal("}") {
            brace_depth = brace_depth.saturating_sub(1);
        } else if token.is_unquoted_literal("(") {
            subshell_depth += 1;
        } else if token.is_unquoted_literal(")") {
            subshell_depth = subshell_depth.saturating_sub(1);
        }
    }
    tokens.len()
}

/// Parse a command list into recursive simple/if/loop nodes.
pub fn parse_compound_list(tokens: &[Word]) -> Result<CompoundList, &'static str> {
    if tokens.iter().any(|token| {
        is_unsupported_operator(token) || token.is_unquoted_literal("&")
    }) {
        return Err("recognized shell operator is not implemented yet");
    }
    let mut entries = Vec::new();
    let mut index = 0usize;
    let mut connector = None;

    while index < tokens.len() {
        if tokens[index].is_unquoted_literal(";;") {
            return Err("case terminator is only valid inside a case command");
        }
        if tokens[index].is_unquoted_literal(";") {
            connector = Some(ListOperator::Sequence);
            index += 1;
            continue;
        }
        if tokens[index].is_unquoted_literal("&&") || tokens[index].is_unquoted_literal("||") {
            return Err("conditional operator requires a command before it");
        }

        let start = index;
        let command = if tokens[index].is_unquoted_literal("(") {
            index = compound_command_end(tokens, index)?;
            if tokens.get(index).is_some_and(|word| word.is_unquoted_literal("|")) {
                index = command_segment_end(tokens, start);
                CompoundCommand::Simple(tokens[start..index].to_vec())
            } else {
                let closing = index - 1;
                let body = &tokens[start + 1..closing];
                if body.is_empty() {
                    return Err("subshell requires a non-empty compound list");
                }
                CompoundCommand::Subshell(parse_compound_list(body)?)
            }
        } else if tokens[index].is_unquoted_literal("{") {
            index = compound_command_end(tokens, index)?;
            if tokens.get(index).is_some_and(|word| word.is_unquoted_literal("|")) {
                index = command_segment_end(tokens, start);
                CompoundCommand::Simple(tokens[start..index].to_vec())
            } else {
                let closing = index - 1;
                if closing <= start + 1 || !tokens[closing - 1].is_unquoted_literal(";") {
                    return Err("brace group requires a non-empty list terminated by ';'");
                }
                CompoundCommand::BraceGroup(parse_compound_list(&tokens[start + 1..closing - 1])?)
            }
        } else if is_reserved_word(tokens, index, "if")
            || is_reserved_word(tokens, index, "while")
            || is_reserved_word(tokens, index, "until")
            || is_reserved_word(tokens, index, "for")
            || is_reserved_word(tokens, index, "case")
        {
            index = compound_command_end(tokens, index)?;
            let segment = &tokens[start..index];
            if is_reserved_word(tokens, start, "if") {
                CompoundCommand::If(parse_if_statement(segment)?)
            } else if is_reserved_word(tokens, start, "for") {
                CompoundCommand::For(parse_for_statement(segment)?)
            } else if is_reserved_word(tokens, start, "case") {
                CompoundCommand::Case(parse_case_statement(segment)?)
            } else {
                CompoundCommand::Loop(parse_loop_statement(segment)?)
            }
        } else {
            index = command_segment_end(tokens, start);
            if index == start {
                return Err("expected command");
            }
            CompoundCommand::Simple(tokens[start..index].to_vec())
        };

        entries.push(CompoundListEntry {
            command,
            connector: connector.take(),
        });

        if index < tokens.len() {
            if tokens[index].is_unquoted_literal(";") {
                connector = Some(ListOperator::Sequence);
            } else if tokens[index].is_unquoted_literal("&&") {
                connector = Some(ListOperator::And);
            } else if tokens[index].is_unquoted_literal("||") {
                connector = Some(ListOperator::Or);
            } else {
                return Err("unexpected token in compound command list");
            }
            index += 1;
            if index == tokens.len() && matches!(connector, Some(ListOperator::And | ListOperator::Or)) {
                return Err("conditional operator requires a command after it");
            }
        }
    }

    if entries.is_empty() {
        return Err("compound command list is empty");
    }
    Ok(entries)
}

/// Return the exclusive end of a compound command beginning at `start`.
/// This centralizes nested if/fi and while-or-until/done boundary matching
/// in the parser so execution code does not scan reserved words itself.
pub fn compound_command_end(tokens: &[Word], start: usize) -> Result<usize, &'static str> {
    let Some(first) = tokens.get(start) else {
        return Err("expected compound command");
    };

    if first.is_unquoted_literal("(") {
        let mut depth = 1usize;
        for index in start + 1..tokens.len() {
            if tokens[index].is_unquoted_literal("(") {
                depth += 1;
            } else if tokens[index].is_unquoted_literal(")") {
                depth -= 1;
                if depth == 0 {
                    return Ok(index + 1);
                }
            }
        }
        return Err("subshell requires a closing ')'");
    }

    if first.is_unquoted_literal("{") {
        let mut depth = 1usize;
        for index in start + 1..tokens.len() {
            if tokens[index].is_unquoted_literal("{") {
                depth += 1;
            } else if tokens[index].is_unquoted_literal("}") {
                depth -= 1;
                if depth == 0 {
                    return Ok(index + 1);
                }
            }
        }
        return Err("brace group requires a closing '}'");
    }

    if first.is_unquoted_literal("if") && is_reserved_word(tokens, start, "if") {
        let mut depth = 0usize;
        for index in start + 1..tokens.len() {
            if is_reserved_word(tokens, index, "if") {
                depth += 1;
            } else if is_reserved_word(tokens, index, "fi") {
                if depth == 0 {
                    return Ok(index + 1);
                }
                depth -= 1;
            }
        }
        return Err("if requires a closing fi");
    }

    if first.is_unquoted_literal("for") && is_reserved_word(tokens, start, "for") {
        let mut depth = 1usize;
        for index in start + 1..tokens.len() {
            if is_reserved_word(tokens, index, "for") || is_reserved_word(tokens, index, "while") || is_reserved_word(tokens, index, "until") {
                depth += 1;
            } else if is_reserved_word(tokens, index, "done") {
                depth -= 1;
                if depth == 0 { return Ok(index + 1); }
            }
        }
        return Err("for requires a closing done");
    }

    if first.is_unquoted_literal("case") && is_reserved_word(tokens, start, "case") {
        let mut depth = 0usize;
        for index in start + 1..tokens.len() {
            if is_reserved_word(tokens, index, "case") { depth += 1; }
            else if is_reserved_word(tokens, index, "esac") {
                if depth == 0 { return Ok(index + 1); }
                depth -= 1;
            }
        }
        return Err("case requires a closing esac");
    }

    if (first.is_unquoted_literal("while") && is_reserved_word(tokens, start, "while"))
        || (first.is_unquoted_literal("until") && is_reserved_word(tokens, start, "until"))
    {
        let mut depth = 1usize;
        for index in start + 1..tokens.len() {
            if is_reserved_word(tokens, index, "while") || is_reserved_word(tokens, index, "until") || is_reserved_word(tokens, index, "for") {
                depth += 1;
            } else if is_reserved_word(tokens, index, "done") {
                depth -= 1;
                if depth == 0 {
                    return Ok(index + 1);
                }
            }
        }
        return Err("loop requires a closing done");
    }

    Err("expected if, while, or until compound command")
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
        || previous.is_unquoted_literal(";;")
        || previous.is_unquoted_literal("&&")
        || previous.is_unquoted_literal("||")
        || previous.is_unquoted_literal("|")
        || previous.is_unquoted_literal("{")
        || previous.is_unquoted_literal("(")
    {
        return true;
    }

    // Compound-list introducers establish a command position only when the
    // introducer itself was recognized as a reserved word in its own context.
    ["if", "then", "elif", "else", "do", "while", "until", "for", "case"]
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
        ForAwaitDo,
        ForInBody,
    }

    let mut stack = Vec::new();
    let mut subshell_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut case_pattern = Vec::<bool>::new();
    for index in 0..tokens.len() {
        let closes_case_pattern = tokens[index].is_unquoted_literal(")") && case_pattern.last() == Some(&true);
        if is_reserved_word(&tokens, index, "case") {
            case_pattern.push(false);
        } else if tokens[index].is_unquoted_literal("in") && case_pattern.last() == Some(&false) {
            if let Some(active) = case_pattern.last_mut() { *active = true; }
        } else if tokens[index].is_unquoted_literal(";;") && !case_pattern.is_empty() {
            if let Some(active) = case_pattern.last_mut() { *active = true; }
        } else if tokens[index].is_unquoted_literal(")") && case_pattern.last() == Some(&true) {
            if let Some(active) = case_pattern.last_mut() { *active = false; }
        } else if is_reserved_word(&tokens, index, "esac") {
            case_pattern.pop();
        }
        if tokens[index].is_unquoted_literal("(") {
            subshell_depth += 1;
        } else if tokens[index].is_unquoted_literal(")") && !closes_case_pattern {
            if subshell_depth == 0 {
                return InputState::Invalid;
            }
            subshell_depth -= 1;
        } else if tokens[index].is_unquoted_literal("{") {
            brace_depth += 1;
        } else if tokens[index].is_unquoted_literal("}") {
            if brace_depth == 0 {
                return InputState::Invalid;
            }
            brace_depth -= 1;
        }
        if is_reserved_word(&tokens, index, "if") {
            stack.push(CompoundPhase::IfAwaitThen);
        } else if is_reserved_word(&tokens, index, "for") {
            stack.push(CompoundPhase::ForAwaitDo);
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
            if *phase == CompoundPhase::ForAwaitDo {
                *phase = CompoundPhase::ForInBody;
            } else if *phase == CompoundPhase::LoopAwaitDo {
                *phase = CompoundPhase::LoopInBody;
            } else {
                return InputState::Invalid;
            }
        } else if is_reserved_word(&tokens, index, "done") {
            match stack.pop() {
                Some(CompoundPhase::LoopInBody | CompoundPhase::ForInBody) => {}
                _ => return InputState::Invalid,
            }
        } else if is_reserved_word(&tokens, index, "fi") {
            match stack.pop() {
                Some(CompoundPhase::IfInThen | CompoundPhase::IfInElse) => {}
                _ => return InputState::Invalid,
            }
        }
    }

    if !stack.is_empty() || !case_pattern.is_empty() || subshell_depth > 0 || brace_depth > 0 {
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
    fn loop_parser_builds_typed_while_and_until_nodes() {
        use super::{parse_loop_statement, LoopKind};
        let tokens = lexer::lex("while true; do echo body; done").unwrap();
        let parsed = parse_loop_statement(&tokens).unwrap();
        assert_eq!(parsed.kind, LoopKind::While);
        assert_eq!(parsed.condition.len(), 1);
        assert_eq!(parsed.body.len(), 1);
        assert!(matches!(&parsed.body[0].command, super::CompoundCommand::Simple(_)));
        let tokens = lexer::lex("until false; do echo body; done").unwrap();
        assert_eq!(parse_loop_statement(&tokens).unwrap().kind, LoopKind::Until);
    }

    #[test]
    fn loop_parser_builds_nested_loop_nodes_in_body() {
        use super::{parse_loop_statement, CompoundCommand};
        let tokens = lexer::lex("while true; do until false; do echo nested; done; echo outer; done").unwrap();
        let parsed = parse_loop_statement(&tokens).unwrap();
        assert!(matches!(&parsed.body[0].command, CompoundCommand::Loop(_)));
        assert!(matches!(&parsed.body[1].command, CompoundCommand::Simple(_)));
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
    fn compound_group_openers_establish_reserved_word_command_positions() {
        for input in [
            "{ if true; then echo ok; fi; }",
            "( if true; then echo ok; fi )",
            "{ for item in one; do echo $item; done; }",
        ] {
            let tokens = lexer::lex(input).unwrap();
            let keyword = tokens.iter().position(|word| {
                ["if", "for"].iter().any(|value| word.is_unquoted_literal(value))
            }).unwrap();
            assert!(is_reserved_word(&tokens, keyword, &tokens[keyword].literal_value()), "{input}");
            assert!(super::parse_compound_list(&tokens).is_ok(), "{input}");
        }
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
    fn case_patterns_do_not_conflict_with_subshell_delimiters() {
        assert_eq!(classify_input("case value in value) echo matched ;; esac\n"), InputState::Complete);
        assert_eq!(classify_input("case value in value) echo matched ;;\n"), InputState::Incomplete);
        assert_eq!(classify_input("case value in value) ( echo nested ) ;; esac\n"), InputState::Complete);
    }

    #[test]
    fn unterminated_quotes_are_incomplete_input() {
        assert_eq!(classify_input("echo 'unfinished"), InputState::Incomplete);
    }

    #[test]
    fn brace_groups_are_classified_incrementally() {
        assert_eq!(classify_input("{ echo open;"), InputState::Incomplete);
        assert_eq!(classify_input("{ { echo nested; }"), InputState::Incomplete);
        assert_eq!(classify_input("{ echo complete; }"), InputState::Complete);
        assert_eq!(classify_input("{ { echo nested; }; }"), InputState::Complete);
        assert_eq!(classify_input("}"), InputState::Invalid);
        assert_eq!(classify_input("{ echo open; }}"), InputState::Invalid);
        assert_eq!(classify_input("echo '{' '}'"), InputState::Complete);
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
        let super::CompoundCommand::Simple(first_condition) = &parsed.clauses[0].condition[0].command else { panic!("expected simple condition"); };
        let super::CompoundCommand::Simple(second_condition) = &parsed.clauses[1].condition[0].command else { panic!("expected simple condition"); };
        assert_eq!(first_condition[0].literal_value(), "true");
        assert_eq!(second_condition[0].literal_value(), "false");
        assert!(parsed.else_body.is_some());
    }

    #[test]
    fn if_parser_preserves_nested_if_boundaries() {
        use super::parse_if_statement;
        let tokens = lexer::lex("if true; then if false; then echo inner; else echo nested; fi; echo outer; fi").unwrap();
        let parsed = parse_if_statement(&tokens).unwrap();
        assert_eq!(parsed.clauses.len(), 1);
        assert!(matches!(&parsed.clauses[0].body[0].command, super::CompoundCommand::If(_)));
        assert!(matches!(&parsed.clauses[0].body[1].command, super::CompoundCommand::Simple(_)));
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


#[cfg(test)]
mod compound_boundary_tests {
    use super::compound_command_end;
    use crate::lexer;

    #[test]
    fn compound_boundary_matches_nested_if_and_loop_pairs() {
        let cases = [
            ("if true; then if false; then echo no; fi; echo yes; fi; echo after", "fi", 0),
            ("while true; do until false; do echo nested; done; done; echo after", "done", 0),
        ];
        for (input, closing, _) in cases {
            let tokens = lexer::lex(input).unwrap();
            let end = compound_command_end(&tokens, 0).unwrap();
            assert!(tokens[end - 1].is_unquoted_literal(closing));
            assert!(end < tokens.len());
            assert!(tokens[end].is_unquoted_literal(";"));
            assert_eq!(tokens[end + 1].literal_value(), "echo");
        }
    }

    #[test]
    fn compound_boundary_matches_mixed_for_and_while_nesting() {
        let cases = [
            "for item in a; do while false; do echo inner; done; done; echo after",
            "while false; do for item in a; do echo inner; done; done; echo after",
            "for item in a; do until true; do echo inner; done; done; echo after",
        ];
        for input in cases {
            let tokens = lexer::lex(input).unwrap();
            let start = tokens.iter().position(|word| {
                word.is_unquoted_literal("for") || word.is_unquoted_literal("while")
            }).unwrap();
            let end = compound_command_end(&tokens, start).unwrap();
            assert!(tokens[end - 1].is_unquoted_literal("done"), "{input}");
            assert!(tokens[end].is_unquoted_literal(";"), "{input}");
            assert!(tokens[end + 1].is_unquoted_literal("echo"), "{input}");
        }
    }

    #[test]
    fn compound_boundary_rejects_missing_closers_and_non_compounds() {
        for input in ["if true; then echo yes", "while true; do echo yes", "echo ordinary"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(compound_command_end(&tokens, 0).is_err(), "{input}");
        }
    }

    #[test]
    fn compound_boundary_can_start_after_a_command_separator() {
        let tokens = lexer::lex("echo before; if true; then echo inside; fi; echo after").unwrap();
        let start = tokens.iter().position(|word| word.is_unquoted_literal("if")).unwrap();
        let end = compound_command_end(&tokens, start).unwrap();
        assert!(tokens[end - 1].is_unquoted_literal("fi"));
        assert!(tokens[end].is_unquoted_literal(";"));
    }
}


#[cfg(test)]
mod for_parser_tests {
    use super::{parse_for_statement, parse_compound_list, CompoundCommand, InputState, classify_input};
    use crate::lexer;

    #[test]
    fn parses_for_word_list_and_typed_body() {
        let tokens = lexer::lex("for item in alpha beta; do echo $item; done").unwrap();
        let parsed = parse_for_statement(&tokens).unwrap();
        assert_eq!(parsed.name, "item");
        assert_eq!(parsed.words.len(), 2);
        assert!(matches!(&parsed.body[0].command, CompoundCommand::Simple(_)));
    }

    #[test]
    fn parses_nested_for_commands_recursively() {
        let tokens = lexer::lex("for a in one; do for b in two; do echo $a $b; done; done").unwrap();
        let parsed = parse_for_statement(&tokens).unwrap();
        assert!(matches!(&parsed.body[0].command, CompoundCommand::For(_)));
    }

    #[test]
    fn rejects_invalid_for_headers_and_missing_done() {
        for input in ["for; do echo x; done", "for 2bad in x; do echo x; done", "for x in y; do echo x"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(parse_for_statement(&tokens).is_err(), "{input}");
        }
    }

    #[test]
    fn classifies_open_and_closed_for_compounds() {
        assert_eq!(classify_input("for item in a b; do echo $item"), InputState::Incomplete);
        assert_eq!(classify_input("for item in a b; do echo $item; done"), InputState::Complete);
    }

    #[test]
    fn compound_list_recognizes_for_after_a_separator() {
        let tokens = lexer::lex("echo before; for item in x; do echo $item; done").unwrap();
        let parsed = parse_compound_list(&tokens).unwrap();
        assert!(matches!(&parsed[1].command, CompoundCommand::For(_)));
    }
}

#[cfg(test)]
mod brace_group_tests {
    use super::{parse_compound_list, CompoundCommand, InputState, classify_input};
    use crate::lexer;

    #[test]
    fn parses_nested_brace_groups_as_compound_nodes() {
        let tokens = lexer::lex("{ echo outer; { echo inner; }; }").unwrap();
        let parsed = parse_compound_list(&tokens).unwrap();
        let CompoundCommand::BraceGroup(outer) = &parsed[0].command else { panic!("expected outer brace group"); };
        assert_eq!(outer.len(), 2);
        assert!(matches!(&outer[1].command, CompoundCommand::BraceGroup(_)));
    }

    #[test]
    fn rejects_empty_unterminated_and_unterminated_list_brace_groups() {
        for input in ["{ }", "{ echo missing", "{ echo missing }"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(parse_compound_list(&tokens).is_err(), "{input}");
        }
    }

    #[test]
    fn brace_group_boundary_stops_before_following_command() {
        let tokens = lexer::lex("{ echo inside; }; echo outside").unwrap();
        let end = super::compound_command_end(&tokens, 0).unwrap();
        assert!(tokens[end - 1].is_unquoted_literal("}"));
        assert!(tokens[end].is_unquoted_literal(";"));
    }
}

#[cfg(test)]
mod subshell_parser_tests {
    use super::{classify_input, compound_command_end, parse_compound_list, CompoundCommand, InputState};
    use crate::lexer;

    #[test]
    fn parses_nested_subshells_as_typed_compound_nodes() {
        let tokens = lexer::lex("( echo outer; ( echo inner; ) )").unwrap();
        let parsed = parse_compound_list(&tokens).unwrap();
        let CompoundCommand::Subshell(outer) = &parsed[0].command else { panic!("expected subshell"); };
        assert_eq!(outer.len(), 2);
        assert!(matches!(&outer[1].command, CompoundCommand::Subshell(_)));
    }

    #[test]
    fn subshell_boundary_stops_before_following_command() {
        let tokens = lexer::lex("( echo inside; ); echo outside").unwrap();
        let end = compound_command_end(&tokens, 0).unwrap();
        assert!(tokens[end - 1].is_unquoted_literal(")"));
        assert!(tokens[end].is_unquoted_literal(";"));
    }

    #[test]
    fn rejects_empty_or_unclosed_subshells() {
        for input in ["( )", "( echo missing"] {
            let tokens = lexer::lex(input).unwrap();
            assert!(parse_compound_list(&tokens).is_err(), "{input}");
        }
    }

    #[test]
    fn classifies_open_and_unmatched_subshell_delimiters() {
        assert_eq!(classify_input("( echo inside; )"), InputState::Complete);
        assert_eq!(classify_input("( echo inside"), InputState::Incomplete);
        assert_eq!(classify_input(")"), InputState::Invalid);
    }
}
