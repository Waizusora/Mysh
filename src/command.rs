//! External command lookup using the shell's PATH value.
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Resolve a command name without invoking another shell.
///
/// Commands containing '/' are treated as explicit paths. Other names are
/// searched in PATH order. Empty PATH components refer to the current directory.
pub fn resolve_command(command: &str, search_path: &OsStr) -> io::Result<PathBuf> {
    if command.contains('/') {
        return Ok(PathBuf::from(command));
    }

    let mut permission_denied = false;
    for directory in std::env::split_paths(search_path) {
        let directory = if directory.as_os_str().is_empty() {
            Path::new(".")
        } else {
            directory.as_path()
        };
        let candidate = directory.join(command);

        match fs::metadata(&candidate) {
            Ok(metadata) if !metadata.is_file() => {}
            Ok(metadata) if is_executable(&metadata) => return Ok(candidate),
            Ok(_) => permission_denied = true,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                permission_denied = true;
            }
            Err(_) => {}
        }
    }

    if permission_denied {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("command found but is not executable: {command}"),
        ))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("command not found: {command}"),
        ))
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::resolve_command;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    fn temporary_directory() -> PathBuf {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mysh-command-resolver-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(unix)]
    fn create_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, b"test executable").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(not(unix))]
    fn create_executable(path: &std::path::Path) {
        fs::write(path, b"test executable").unwrap();
    }

    #[test]
    fn resolves_command_from_path() {
        let directory = temporary_directory();
        let executable = directory.join("mysh-test-command");
        create_executable(&executable);
        let search_path = std::env::join_paths([&directory]).unwrap();

        assert_eq!(
            resolve_command("mysh-test-command", &search_path).unwrap(),
            executable
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn searches_path_entries_in_order() {
        let first = temporary_directory();
        let second = temporary_directory();
        let first_executable = first.join("mysh-test-command");
        let second_executable = second.join("mysh-test-command");
        create_executable(&first_executable);
        create_executable(&second_executable);
        let search_path = std::env::join_paths([&first, &second]).unwrap();

        assert_eq!(
            resolve_command("mysh-test-command", &search_path).unwrap(),
            first_executable
        );
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn preserves_explicit_paths_without_path_search() {
        let explicit = PathBuf::from("./tools/my-command");
        assert_eq!(
            resolve_command("./tools/my-command", OsStr::new("")).unwrap(),
            explicit
        );
    }

    #[test]
    fn reports_missing_commands() {
        let directory = temporary_directory();
        let search_path = std::env::join_paths([&directory]).unwrap();

        let error = resolve_command("mysh-command-that-does-not-exist", &search_path)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reports_non_executable_commands() {
        let directory = temporary_directory();
        let file = directory.join("mysh-non-executable");
        fs::write(&file, b"not executable").unwrap();
        let search_path = std::env::join_paths([&directory]).unwrap();

        let error = resolve_command("mysh-non-executable", &search_path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        fs::remove_dir_all(directory).unwrap();
    }
}
