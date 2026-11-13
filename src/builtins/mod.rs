//! Built-in commands execute in the Mysh process.
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::SystemTime;

pub enum BuiltinOutcome {
    Continue(i32),
    Exit(i32),
}

pub fn run(
    name: &str,
    args: &[String],
    history: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Option<BuiltinOutcome> {
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

fn cd(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    if args.len() > 1 {
        return usage("cd: expected at most one directory", err);
    }
    let Some(path) = args.first().cloned().or_else(|| std::env::var("HOME").ok()) else {
        return fail("cd: HOME is not set", err);
    };
    if let Err(error) = std::env::set_current_dir(&path) {
        return fail(format!("cd: {path}: {error}"), err);
    }
    if let Ok(path) = std::env::current_dir() {
        std::env::set_var("PWD", path);
    }
    ok()
}

fn pwd(out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
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

fn ls(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    let mut show_hidden = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value == "-a" {
            show_hidden = true;
        } else if options && value.starts_with('-') {
            return usage("ls: supported options are -a and --", err);
        } else {
            paths.push(value.as_str());
        }
    }
    if paths.is_empty() {
        paths.push(".");
    }

    let mut status = 0;
    for (index, value) in paths.iter().enumerate() {
        let path = Path::new(value);
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => {
                if paths.len() > 1 {
                    if index > 0 {
                        let _ = writeln!(out);
                    }
                    let _ = writeln!(out, "{}:", path.display());
                }
                match fs::read_dir(path) {
                    Ok(entries) => {
                        let mut names = Vec::new();
                        for entry in entries {
                            match entry {
                                Ok(entry) => {
                                    let name = entry.file_name();
                                    if show_hidden || !name.to_string_lossy().starts_with('.') {
                                        names.push(name);
                                    }
                                }
                                Err(error) => {
                                    status = 1;
                                    let _ = writeln!(err, "mysh: ls: {error}");
                                }
                            }
                        }
                        names.sort();
                        for name in names {
                            if let Err(error) = writeln!(out, "{}", name.to_string_lossy()) {
                                return fail(error, err);
                            }
                        }
                    }
                    Err(error) => {
                        status = 1;
                        let _ = writeln!(err, "mysh: ls: {}: {error}", path.display());
                    }
                }
            }
            Ok(_) => {
                if let Err(error) = writeln!(out, "{}", path.display()) {
                    return fail(error, err);
                }
            }
            Err(error) => {
                status = 1;
                let _ = writeln!(err, "mysh: ls: {}: {error}", path.display());
            }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn mkdir(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut parents = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value == "-p" {
            parents = true;
        } else if options && value.starts_with('-') {
            return usage("mkdir: supported options are -p and --", err);
        } else {
            paths.push(value);
        }
    }
    if paths.is_empty() {
        return usage("mkdir: usage: mkdir [-p] directory ...", err);
    }
    let mut status = 0;
    for path in paths {
        let result = if parents {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        };
        if let Err(error) = result {
            status = 1;
            let _ = writeln!(err, "mysh: mkdir: {path}: {error}");
        }
    }
    BuiltinOutcome::Continue(status)
}

fn rmdir(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') {
            return usage("rmdir: no options are supported; use -- before a path beginning with '-'", err);
        } else {
            paths.push(value);
        }
    }
    if paths.is_empty() {
        return usage("rmdir: usage: rmdir directory ...", err);
    }
    let mut status = 0;
    for path in paths {
        if let Err(error) = fs::remove_dir(path) {
            status = 1;
            let _ = writeln!(err, "mysh: rmdir: {path}: {error}");
        }
    }
    BuiltinOutcome::Continue(status)
}

fn touch(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') {
            return usage("touch: no options are supported; use -- before a path beginning with '-'", err);
        } else {
            paths.push(value);
        }
    }
    if paths.is_empty() {
        return usage("touch: usage: touch file ...", err);
    }
    let mut status = 0;
    for path in paths {
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
    let mut recursive = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && (value == "-r" || value == "-R") {
            recursive = true;
        } else if options && value.starts_with('-') {
            return usage("cp: supported options are -r, -R, and --", err);
        } else {
            paths.push(value.as_str());
        }
    }
    if paths.len() < 2 {
        return usage("cp: usage: cp [-r] source ... destination", err);
    }

    let destination = Path::new(paths[paths.len() - 1]);
    let sources = &paths[..paths.len() - 1];
    let destination_is_directory = destination.is_dir();
    if sources.len() > 1 && !destination_is_directory {
        return usage(
            "cp: destination must be an existing directory when copying multiple sources",
            err,
        );
    }

    let mut status = 0;
    for source_value in sources {
        let source = Path::new(source_value);
        let target = if destination_is_directory {
            match source.file_name() {
                Some(name) => destination.join(name),
                None => {
                    status = 1;
                    let _ = writeln!(err, "mysh: cp: invalid source path: {source_value}");
                    continue;
                }
            }
        } else {
            destination.to_path_buf()
        };
        if let Err(error) = copy_path(source, &target, recursive) {
            status = 1;
            let _ = writeln!(err, "mysh: cp: {source_value}: {error}");
        }
    }
    BuiltinOutcome::Continue(status)
}

/// Copy a regular file or, when requested, a directory tree.
/// Symbolic links are rejected rather than followed implicitly.
fn copy_path(source: &Path, destination: &Path, recursive: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic-link copying is not supported",
        ));
    }
    if metadata.is_dir() {
        if !recursive {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "omitting directory; use -r to copy directories",
            ));
        }

        let source_absolute = fs::canonicalize(source)?;
        let destination_parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent_absolute = fs::canonicalize(destination_parent)?;
        let destination_name = destination.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid destination directory")
        })?;
        let destination_absolute = parent_absolute.join(destination_name);
        if destination_absolute.starts_with(&source_absolute) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot copy a directory into itself",
            ));
        }

        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &destination.join(entry.file_name()), true)?;
        }
        Ok(())
    } else if metadata.is_file() {
        fs::copy(source, destination).map(|_| ())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported filesystem entry type",
        ))
    }
}

fn mv(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') {
            return usage(
                "mv: no options are currently supported; use -- before a path beginning with '-'",
                err,
            );
        } else {
            paths.push(value.as_str());
        }
    }
    if paths.len() < 2 {
        return usage("mv: usage: mv source ... destination", err);
    }

    let destination = Path::new(paths[paths.len() - 1]);
    let sources = &paths[..paths.len() - 1];
    let destination_is_directory = destination.is_dir();
    if sources.len() > 1 && !destination_is_directory {
        return usage(
            "mv: destination must be an existing directory when moving multiple sources",
            err,
        );
    }

    let mut status = 0;
    for source_value in sources {
        let source = Path::new(source_value);
        let target = if destination_is_directory {
            match source.file_name() {
                Some(name) => destination.join(name),
                None => {
                    status = 1;
                    let _ = writeln!(err, "mysh: mv: invalid source path: {source_value}");
                    continue;
                }
            }
        } else {
            destination.to_path_buf()
        };
        if let Err(error) = fs::rename(source, &target) {
            status = 1;
            let _ = writeln!(err, "mysh: mv: {source_value}: {error}");
        }
    }
    BuiltinOutcome::Continue(status)
}

fn rm(args: &[String], err: &mut dyn Write) -> BuiltinOutcome {
    let mut recursive = false;
    let mut force = false;
    let mut directories = false;
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') && value.len() > 1 {
            for option in value[1..].chars() {
                match option {
                    'r' | 'R' => recursive = true,
                    'f' => force = true,
                    'd' => directories = true,
                    _ => return usage("rm: supported options are -r, -R, -f, -d, and --", err),
                }
            }
        } else {
            paths.push(value.as_str());
        }
    }
    if paths.is_empty() {
        return usage("rm: usage: rm [-r] [-f] [-d] path ...", err);
    }

    let current = std::env::current_dir().ok();
    let mut status = 0;
    for value in paths {
        let path = Path::new(value);
        if value == "." || value == ".." {
            status = 1;
            let _ = writeln!(
                err,
                "mysh: rm: refusing to remove the current or parent directory: {value}"
            );
            continue;
        }
        if let (Some(current), Ok(target)) = (current.as_ref(), fs::canonicalize(path)) {
            if current.starts_with(&target) || target.parent().is_none() {
                status = 1;
                let _ = writeln!(err, "mysh: rm: refusing to remove the current directory or filesystem root: {value}");
                continue;
            }
        }

        let result = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() && recursive => fs::remove_dir_all(path),
            Ok(metadata) if metadata.is_dir() && directories => fs::remove_dir(path),
            Ok(metadata) if metadata.is_dir() => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "is a directory; use -r to remove directories",
            )),
            Ok(_) => fs::remove_file(path),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if !(force && error.kind() == io::ErrorKind::NotFound) {
                status = 1;
                let _ = writeln!(err, "mysh: rm: {value}: {error}");
            }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn cat(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> BuiltinOutcome {
    let mut options = true;
    let mut paths = Vec::new();
    for value in args {
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') && value != "-" {
            return usage("cat: no options are supported; use -- before a path beginning with '-'", err);
        } else {
            paths.push(value);
        }
    }
    if paths.is_empty() || paths == ["-"] {
        let mut input = io::stdin().lock();
        return match io::copy(&mut input, out) {
            Ok(_) => ok(),
            Err(error) => fail(format!("cat: {error}"), err),
        };
    }
    let mut status = 0;
    for path in paths {
        match File::open(path) {
            Ok(mut file) => {
                if let Err(error) = io::copy(&mut file, out) {
                    status = 1;
                    let _ = writeln!(err, "mysh: cat: {path}: {error}");
                }
            }
            Err(error) => {
                status = 1;
                let _ = writeln!(err, "mysh: cat: {path}: {error}");
            }
        }
    }
    BuiltinOutcome::Continue(status)
}

fn echo(args: &[String], out: &mut dyn Write) -> BuiltinOutcome {
    let (newline, values) = if args.first().is_some_and(|value| value == "-n") {
        (false, &args[1..])
    } else {
        (true, args)
    };
    if write!(out, "{}", values.join(" ")).is_err() {
        return BuiltinOutcome::Continue(1);
    }
    if newline && writeln!(out).is_err() {
        return BuiltinOutcome::Continue(1);
    }
    ok()
}
fn clear(out: &mut dyn Write) -> BuiltinOutcome {
    if write!(out, "\x1b[2J\x1b[H").is_err() || out.flush().is_err() {
        BuiltinOutcome::Continue(1)
    } else {
        ok()
    }
}
fn history_cmd(history: &[String], out: &mut dyn Write) -> BuiltinOutcome {
    for (index, command) in history.iter().enumerate() {
        if writeln!(out, "{:>5}  {command}", index + 1).is_err() {
            return BuiltinOutcome::Continue(1);
        }
    }
    ok()
}
fn help(out: &mut dyn Write) -> BuiltinOutcome {
    let text = "Mysh built-in commands:\ncd [directory]       Change the current directory\npwd                  Print the current directory\nls [-a] [path ...]   List directory entries\nmkdir [-p] path ...  Create directories\nrmdir path ...       Remove empty directories\ntouch file ...       Create files if absent\ncp [-r] src ... dest Copy files or directory trees\nmv src ... dest      Rename or move paths\nrm [-rf] [-d] path   Remove files or directories\ncat [file ...]       Write file contents to standard output\necho [-n] [text ...] Write text to standard output\nclear                Clear the terminal display\nhistory              Show this session's command history\nhelp                 Show this help text\nexit [status]        Exit Mysh\n\nExternal executables are resolved through PATH.\n";
    if out.write_all(text.as_bytes()).is_err() {
        BuiltinOutcome::Continue(1)
    } else {
        ok()
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
