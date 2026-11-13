//! Mysh executable entry point.
use mysh::Shell;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

fn print_usage() {
    println!("Mysh {}\nUsage: mysh [--version] [--help] [-c command]\n\nWithout options, Mysh reads commands from standard input.\n", env!("CARGO_PKG_VERSION"));
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.as_slice() == ["--version"] {
        println!("Mysh {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if arguments.as_slice() == ["--help"] {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let mut shell = Shell::new();
    if arguments.first().is_some_and(|value| value == "-c") {
        if arguments.len() != 2 {
            eprintln!("mysh: -c requires exactly one command string");
            return ExitCode::from(2);
        }
        let mut stdout = io::stdout().lock();
        let mut stderr = io::stderr().lock();
        shell.execute_line(&arguments[1], &mut stdout, &mut stderr);
        return ExitCode::from(shell.last_status().clamp(0, 255) as u8);
    }
    if !arguments.is_empty() {
        eprintln!("mysh: unknown option or argument: {}", arguments[0]);
        print_usage();
        return ExitCode::from(2);
    }

    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    match shell.run(&mut reader, &mut stdout, interactive) {
        Ok(status) => ExitCode::from(status.clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("mysh: input/output error: {error}");
            ExitCode::FAILURE
        }
    }
}
