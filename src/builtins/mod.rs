//! POSIX shell built-ins that must execute in the Mysh process.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

pub enum BuiltinOutcome {
    Continue(i32),
    Exit(i32),
    Break(usize),
    ContinueLoop(usize),
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    name: &str,
    args: &[String],
    _history: &[String],
    environment: &mut BTreeMap<String, String>,
    exported: &mut BTreeSet<String>,
    _input: Option<&[u8]>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Option<BuiltinOutcome> {
    let result = match name {
        ":" | "true" => ok(),
        "false" => BuiltinOutcome::Continue(1),
        "cd" => cd(args, environment, exported, out, err),
        "export" => export(args, environment, exported, out, err),
        "unset" => unset(args, environment, exported, err),
        "pwd" => pwd(args, out, err),
        "exit" => exit_cmd(args, err),
        "break" => loop_control_cmd("break", args, err),
        "continue" => loop_control_cmd("continue", args, err),
        _ => return None,
    };
    Some(result)
}

fn ok() -> BuiltinOutcome {
    BuiltinOutcome::Continue(0)
}

fn fail(message: impl std::fmt::Display, err: &mut dyn Write) -> BuiltinOutcome {
    let _ = writeln!(err, "mysh: {message}");
    BuiltinOutcome::Continue(1)
}

fn usage(message: &str, err: &mut dyn Write) -> BuiltinOutcome {
    let _ = writeln!(err, "mysh: {message}");
    BuiltinOutcome::Continue(2)
}

fn cd(
    args: &[String],
    environment: &mut BTreeMap<String, String>,
    exported: &mut BTreeSet<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> BuiltinOutcome {
    if args.len() > 1 {
        return usage("cd: expected at most one directory", err);
    }

    let print_new_directory = args.first().is_some_and(|value| value == "-");
    let target = if print_new_directory {
        match environment.get("OLDPWD") {
            Some(path) if !path.is_empty() => path.clone(),
            _ => return fail("cd: OLDPWD is not set", err),
        }
    } else if let Some(path) = args.first() {
        path.clone()
    } else {
        match environment.get("HOME") {
            Some(path) if !path.is_empty() => path.clone(),
            _ => return fail("cd: HOME is not set", err),
        }
    };

    let old_directory = match std::env::current_dir() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => return fail(format!("cd: {error}"), err),
    };
    if let Err(error) = std::env::set_current_dir(&target) {
        return fail(format!("cd: {target}: {error}"), err);
    }
    let new_directory = match std::env::current_dir() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => return fail(format!("cd: {error}"), err),
    };

    environment.insert("OLDPWD".to_owned(), old_directory);
    environment.insert("PWD".to_owned(), new_directory.clone());
    exported.insert("OLDPWD".to_owned());
    exported.insert("PWD".to_owned());

    if print_new_directory && writeln!(out, "{new_directory}").is_err() {
        return BuiltinOutcome::Continue(1);
    }
    ok()
}

fn export(
    args: &[String],
    environment: &mut BTreeMap<String, String>,
    exported: &mut BTreeSet<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> BuiltinOutcome {
    let names = if args.first().is_some_and(|value| value == "-p") {
        if args.len() > 1 {
            return usage("export: unexpected operand after -p", err);
        }
        true
    } else {
        args.is_empty()
    };

    if names {
        for name in exported.iter() {
            let value = environment.get(name).map(String::as_str).unwrap_or("");
            if writeln!(out, "export {name}={value:?}").is_err() {
                return BuiltinOutcome::Continue(1);
            }
        }
        return ok();
    }

    let mut status = 0;
    for argument in args {
        let (name, value) = argument.split_once('=').unwrap_or((argument.as_str(), ""));
        if !valid_name(name) {
            status = 1;
            let _ = writeln!(err, "mysh: export: invalid variable name: {name}");
            continue;
        }
        if argument.contains('=') {
            environment.insert(name.to_owned(), value.to_owned());
        }
        exported.insert(name.to_owned());
    }
    BuiltinOutcome::Continue(status)
}

fn unset(
    args: &[String],
    environment: &mut BTreeMap<String, String>,
    exported: &mut BTreeSet<String>,
    err: &mut dyn Write,
) -> BuiltinOutcome {
    if args.is_empty() {
        return usage("unset: usage: unset name ...", err);
    }

    let mut status = 0;
    for name in args {
        if !valid_name(name) {
            status = 1;
            let _ = writeln!(err, "mysh: unset: invalid variable name: {name}");
        } else {
            environment.remove(name);
            exported.remove(name);
        }
    }
    BuiltinOutcome::Continue(status)
}

fn pwd(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    if args.iter().any(|value| !matches!(value.as_str(), "-L" | "-P")) {
        return usage("pwd: supported options are -L and -P", err);
    }

    match std::env::current_dir() {
        Ok(path) => {
            if let Err(error) = writeln!(out, "{}", path.display()) {
                fail(error, err)
            } else {
                ok()
            }
        }
        Err(error) => fail(format!("pwd: {error}"), err),
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn loop_control_cmd(name: &str, args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() > 1 {
        return usage(&format!("{name}: expected at most one positive loop level"), err);
    }
    let level = match args.first() {
        None => 1,
        Some(value) => match value.parse::<usize>() {
            Ok(level) if level > 0 => level,
            _ => return usage(&format!("{name}: level must be a positive integer"), err),
        },
    };
    if name == "break" {
        BuiltinOutcome::Break(level)
    } else {
        BuiltinOutcome::ContinueLoop(level)
    }
}

fn exit_cmd(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() > 1 {
        return usage("exit: expected at most one status", err);
    }
    let status = match args.first() {
        Some(value) => match value.parse::<i32>() {
            Ok(value) => value.clamp(0, 255),
            Err(_) => return usage("exit: status must be an integer", err),
        },
        None => 0,
    };
    BuiltinOutcome::Exit(status)
}
