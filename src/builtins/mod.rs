//! Built-in commands execute in the Mysh process.
use std::fs::{self, File, FileTimes, OpenOptions};
use std::time::SystemTime;
use std::io::{self, Write};
use std::path::Path;

pub enum BuiltinOutcome { Continue(i32), Exit(i32) }

pub fn run(name: &str, args: &[String], history: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Option<BuiltinOutcome> {
    let result = match name {
        "cd" => cd(args, err),
        "pwd" => pwd(out, err),
        "ls" => ls(args, out, err),
        "mkdir" => mkdir(args, err),
        "rmdir" => rmdir(args, err),
        "touch" => touch(args, err),
        "cp" => cp(args, err),
        "mv" => mv(args, err),
        "rm" => rm(args, err),
        "cat" => cat(args, out, err),
        "echo" => echo(args, out),
        "clear" => clear(out),
        "history" => history_cmd(history, out),
        "help" => help(out),
        "exit" => exit_cmd(args, err),
        _ => return None,
    };
    Some(result)
}

fn ok() -> BuiltinOutcome { BuiltinOutcome::Continue(0) }
fn fail(message: impl std::fmt::Display, err: &mut dyn Write) -> BuiltinOutcome {
    let _ = writeln!(err, "mysh: {message}");
    BuiltinOutcome::Continue(1)
}
fn usage(message: &str, err: &mut dyn Write) -> BuiltinOutcome {
    let _ = writeln!(err, "mysh: {message}");
    BuiltinOutcome::Continue(2)
}

fn cd(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() > 1 { return usage("cd: expected at most one directory", err); }
    let Some(path) = args.first().cloned().or_else(|| std::env::var("HOME").ok()) else {
        return fail("cd: HOME is not set", err);
    };
    if let Err(error) = std::env::set_current_dir(&path) {
        return fail(format!("cd: {path}: {error}"), err);
    }
    if let Ok(path) = std::env::current_dir() { std::env::set_var("PWD", path); }
    ok()
}

fn pwd(out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    match std::env::current_dir() {
        Ok(path) => if let Err(error) = writeln!(out, "{}", path.display()) { fail(error, err) } else { ok() },
        Err(error) => fail(format!("pwd: {error}"), err),
    }
}

fn ls(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    let paths: Vec<&str> = if args.is_empty() { vec!["."] } else { args.iter().map(String::as_str).collect() };
    let mut status = 0;
    for (index, value) in paths.iter().enumerate() {
        let path = Path::new(value);
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => {
                if paths.len() > 1 {
                    if index > 0 { let _ = writeln!(out); }
                    let _ = writeln!(out, "{}:", path.display());
                }
                match fs::read_dir(path) {
                    Ok(entries) => {
                        let mut names = Vec::new();
                        for entry in entries {
                            match entry {
                                Ok(entry) => names.push(entry.file_name()),
                                Err(error) => { status = 1; let _ = writeln!(err, "mysh: ls: {error}"); }
                            }
                        }
                        names.sort();
                        for name in names {
                            if let Err(error) = writeln!(out, "{}", name.to_string_lossy()) { return fail(error, err); }
                        }
                    }
                    Err(error) => { status = 1; let _ = writeln!(err, "mysh: ls: {}: {error}", path.display()); }
                }
            }
            Ok(_) => if let Err(error) = writeln!(out, "{}", path.display()) { return fail(error, err); },
            Err(error) => { status = 1; let _ = writeln!(err, "mysh: ls: {}: {error}", path.display()); }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn mkdir(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut parents = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" { options = false; }
        else if options && value == "-p" { parents = true; }
        else { paths.push(value); }
    }
    if paths.is_empty() { return usage("mkdir: usage: mkdir [-p] directory ...", err); }
    let mut status = 0;
    for path in paths {
        let result = if parents { fs::create_dir_all(path) } else { fs::create_dir(path) };
        if let Err(error) = result { status = 1; let _ = writeln!(err, "mysh: mkdir: {path}: {error}"); }
    }
    BuiltinOutcome::Continue(status)
}

fn rmdir(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.is_empty() { return usage("rmdir: usage: rmdir directory ...", err); }
    let mut status = 0;
    for path in args {
        if let Err(error) = fs::remove_dir(path) { status = 1; let _ = writeln!(err, "mysh: rmdir: {path}: {error}"); }
    }
    BuiltinOutcome::Continue(status)
}

fn touch(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.is_empty() { return usage("touch: usage: touch file ...", err); }
    let mut status = 0;
    for path in args {
        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|file| {
                let now = SystemTime::now();
                file.set_times(FileTimes::new().set_accessed(now).set_modified(now))
            });
        if let Err(error) = result {
            status = 1;
            let _ = writeln!(err, "mysh: touch: {path}: {error}");
        }
    }
    BuiltinOutcome::Continue(status)
}

fn cp(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() != 2 { return usage("cp: usage: cp source destination", err); }
    if let Err(error) = fs::copy(&args[0], &args[1]) { return fail(format!("cp: {}: {error}", args[0]), err); }
    ok()
}

fn mv(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() != 2 { return usage("mv: usage: mv source destination", err); }
    if let Err(error) = fs::rename(&args[0], &args[1]) { return fail(format!("mv: {}: {error}", args[0]), err); }
    ok()
}

fn rm(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut recursive = false;
    let mut force = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" { options = false; }
        else if options && (value == "-r" || value == "-R") { recursive = true; }
        else if options && value == "-f" { force = true; }
        else if options && value.starts_with('-') { return usage("rm: supported options are -r, -R, -f, and --", err); }
        else { paths.push(value); }
    }
    if paths.is_empty() { return usage("rm: usage: rm [-r] [-f] path ...", err); }
    let mut status = 0;
    for value in paths {
        let path = Path::new(value);
        let result = if recursive && path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
        if let Err(error) = result {
            if !(force && error.kind() == io::ErrorKind::NotFound) {
                status = 1; let _ = writeln!(err, "mysh: rm: {value}: {error}");
            }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn cat(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    if args.is_empty() {
        let mut input = io::stdin().lock();
        return match io::copy(&mut input, out) { Ok(_) => ok(), Err(error) => fail(format!("cat: {error}"), err) };
    }
    let mut status = 0;
    for path in args {
        match File::open(path) {
            Ok(mut file) => if let Err(error) = io::copy(&mut file, out) {
                status = 1; let _ = writeln!(err, "mysh: cat: {path}: {error}");
            },
            Err(error) => { status = 1; let _ = writeln!(err, "mysh: cat: {path}: {error}"); }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn echo(args: &[String], out: &mut dyn Write) -> BuiltinOutcome {
    let (newline, values) = if args.first().is_some_and(|value| value == "-n") { (false, &args[1..]) } else { (true, args) };
    if write!(out, "{}", values.join(" ")).is_err() { return BuiltinOutcome::Continue(1); }
    if newline && writeln!(out).is_err() { return BuiltinOutcome::Continue(1); }
    ok()
}
fn clear(out: &mut dyn Write) -> BuiltinOutcome {
    if write!(out, "\x1b[2J\x1b[H").is_err() || out.flush().is_err() { BuiltinOutcome::Continue(1) } else { ok() }
}
fn history_cmd(history: &[String], out: &mut dyn Write) -> BuiltinOutcome {
    for (index, command) in history.iter().enumerate() {
        if writeln!(out, "{:>5}  {command}", index + 1).is_err() { return BuiltinOutcome::Continue(1); }
    }
    ok()
}
fn help(out: &mut dyn Write) -> BuiltinOutcome {
    let text = "Mysh built-in commands:\ncd [directory]       Change the current directory\npwd                  Print the current directory\nls [path ...]        List directory entries\nmkdir [-p] path ...  Create directories\nrmdir path ...       Remove empty directories\ntouch file ...       Create files if absent\ncp source dest       Copy one regular file\nmv source dest       Rename or move a path\nrm [-r] [-f] path    Remove files or directories\ncat [file ...]       Write file contents to standard output\necho [-n] [text ...] Write text to standard output\nclear                Clear the terminal display\nhistory              Show this session's command history\nhelp                 Show this help text\nexit [status]        Exit Mysh\n\nExternal executables are resolved through PATH.\n";
    if out.write_all(text.as_bytes()).is_err() { BuiltinOutcome::Continue(1) } else { ok() }
}
fn exit_cmd(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() > 1 { return usage("exit: expected at most one status", err); }
    let status = match args.first() {
        Some(value) => match value.parse::<i32>() {
            Ok(value) => value.clamp(0, 255),
            Err(_) => return usage("exit: status must be an integer", err),
        },
        None => 0,
    };
    BuiltinOutcome::Exit(status)
}
