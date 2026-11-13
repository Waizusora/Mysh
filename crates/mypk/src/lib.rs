//! Command-line interface foundation for My Package Keeper.

/// A canonical MYPK operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Sync,
    Upgrade,
    Find,
    Install,
    Delete,
    Reinstall,
    List,
    Info,
    Clean,
    Clear,
}

impl Command {
    /// Returns the canonical command name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Upgrade => "upgrade",
            Self::Find => "find",
            Self::Install => "install",
            Self::Delete => "delete",
            Self::Reinstall => "reinstall",
            Self::List => "list",
            Self::Info => "info",
            Self::Clean => "clean",
            Self::Clear => "clear",
        }
    }
}

/// Resolves canonical command names and supported compatibility aliases.
pub fn resolve_command(name: &str) -> Option<Command> {
    match name {
        "sync" | "update" => Some(Command::Sync),
        "upgrade" => Some(Command::Upgrade),
        "find" | "search" => Some(Command::Find),
        "install" => Some(Command::Install),
        "delete" | "remove" => Some(Command::Delete),
        "reinstall" => Some(Command::Reinstall),
        "list" => Some(Command::List),
        "info" => Some(Command::Info),
        "clean" => Some(Command::Clean),
        "clear" => Some(Command::Clear),
        _ => None,
    }
}

/// Runs the MYPK command-line interface and returns a process exit status.
pub fn run(args: &[String]) -> i32 {
    let Some(first) = args.first() else {
        print_help();
        return 0;
    };

    match first.as_str() {
        "--help" | "-h" => {
            print_help();
            0
        }
        "--version" | "-V" => {
            println!("mypk {}", env!("CARGO_PKG_VERSION"));
            0
        }
        _ => match resolve_command(first) {
            Some(command) => {
                eprintln!(
                    "mypk: '{}' is recognized but its package operation is not implemented yet",
                    command.as_str()
                );
                2
            }
            None => {
                eprintln!("mypk: unknown command '{}'", first);
                eprintln!("Try 'mypk --help' for usage.");
                2
            }
        },
    }
}

fn print_help() {
    println!("MYPK - My Package Keeper");
    println!();
    println!("Usage: mypk <command> [arguments]");
    println!();
    println!("Commands:");
    println!("  sync       Synchronize repository indexes (alias: update)");
    println!("  upgrade    Upgrade installed packages");
    println!("  find       Find packages (alias: search)");
    println!("  install    Install packages");
    println!("  delete     Delete packages (alias: remove)");
    println!("  reinstall  Reinstall packages");
    println!("  list       List installed packages");
    println!("  info       Display package information");
    println!("  clean      Remove cached archives and temporary files");
    println!("  clear      Remove unused automatically installed dependencies");
    println!();
    println!("Options:");
    println!("  -h, --help     Display this help");
    println!("  -V, --version  Display version information");
    println!();
    println!("Package operations are under development.");
}

#[cfg(test)]
mod tests {
    use super::{resolve_command, Command};

    #[test]
    fn canonical_commands_are_recognized() {
        let cases = [
            ("sync", Command::Sync),
            ("upgrade", Command::Upgrade),
            ("find", Command::Find),
            ("install", Command::Install),
            ("delete", Command::Delete),
            ("reinstall", Command::Reinstall),
            ("list", Command::List),
            ("info", Command::Info),
            ("clean", Command::Clean),
            ("clear", Command::Clear),
        ];

        for (name, expected) in cases {
            assert_eq!(resolve_command(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn compatibility_aliases_resolve_to_canonical_commands() {
        assert_eq!(resolve_command("update"), Some(Command::Sync));
        assert_eq!(resolve_command("search"), Some(Command::Find));
        assert_eq!(resolve_command("remove"), Some(Command::Delete));
    }

    #[test]
    fn command_names_are_case_sensitive() {
        assert_eq!(resolve_command("SYNC"), None);
        assert_eq!(resolve_command("Install"), None);
    }

    #[test]
    fn unknown_commands_are_rejected() {
        assert_eq!(resolve_command("autoremove"), None);
        assert_eq!(resolve_command("unknown"), None);
    }
}
