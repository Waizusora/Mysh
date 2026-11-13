use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("mysh-test-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).expect("temporary directory should be created");
    path
}

fn run_in(directory: &std::path::Path, command: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mysh"))
        .args(["-c", command])
        .current_dir(directory)
        .output()
        .expect("Mysh process should start")
}

#[test]
fn filesystem_builtins_create_copy_move_and_remove_files() {
    let directory = temporary_directory();

    let output = run_in(&directory, "mkdir -p nested/deep");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = run_in(&directory, "touch nested/deep/source");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::write(directory.join("nested/deep/source"), b"Mysh")
        .expect("source file should be written");

    let output = run_in(&directory, "cat nested/deep/source");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Mysh");

    let output = run_in(&directory, "cp nested/deep/source copied");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(directory.join("copied")).unwrap(), b"Mysh");

    let output = run_in(&directory, "mv copied moved");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("copied").exists());
    assert_eq!(fs::read(directory.join("moved")).unwrap(), b"Mysh");

    let output = run_in(&directory, "rm moved");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("moved").exists());

    let output = run_in(&directory, "rm -r nested");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("nested").exists());

    fs::remove_dir_all(directory).expect("temporary directory should be removed");
}

#[test]
fn rmdir_removes_empty_directories() {
    let directory = temporary_directory();
    fs::create_dir(directory.join("empty")).unwrap();

    let output = run_in(&directory, "rmdir empty");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("empty").exists());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn standard_input_mode_preserves_directory_changes() {
    let directory = temporary_directory();
    fs::create_dir(directory.join("child")).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mysh"))
        .current_dir(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Mysh process should start");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"cd child\npwd\nexit\n")
        .expect("commands should be written");

    let output = child
        .wait_with_output()
        .expect("Mysh process should finish");
    assert!(output.status.success());
    let printed = String::from_utf8(output.stdout).unwrap();
    assert!(printed.trim_end().ends_with("child"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unknown_external_command_returns_not_found_status() {
    let directory = temporary_directory();
    let output = run_in(&directory, "mysh-command-that-does-not-exist");
    assert_eq!(output.status.code(), Some(127));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recursive_copy_preserves_directory_structure() {
    let directory = temporary_directory();
    fs::create_dir_all(directory.join("source/nested")).unwrap();
    fs::write(directory.join("source/root.txt"), b"root").unwrap();
    fs::write(directory.join("source/nested/child.txt"), b"child").unwrap();

    let output = run_in(&directory, "cp -r source copied");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.join("copied/root.txt")).unwrap(),
        b"root"
    );
    assert_eq!(
        fs::read(directory.join("copied/nested/child.txt")).unwrap(),
        b"child"
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn copy_and_move_accept_multiple_sources_for_directory_destinations() {
    let directory = temporary_directory();
    fs::create_dir(directory.join("destination")).unwrap();
    fs::write(directory.join("first"), b"one").unwrap();
    fs::write(directory.join("second"), b"two").unwrap();

    let output = run_in(&directory, "cp first second destination");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(directory.join("destination/first")).unwrap(),
        b"one"
    );
    assert_eq!(
        fs::read(directory.join("destination/second")).unwrap(),
        b"two"
    );

    fs::create_dir(directory.join("moved")).unwrap();
    let output = run_in(&directory, "mv destination/first destination/second moved");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(directory.join("moved/first")).unwrap(), b"one");
    assert_eq!(fs::read(directory.join("moved/second")).unwrap(), b"two");

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn ls_hides_dot_entries_unless_all_option_is_used() {
    let directory = temporary_directory();
    fs::write(directory.join(".hidden"), b"hidden").unwrap();
    fs::write(directory.join("visible"), b"visible").unwrap();

    let output = run_in(&directory, "ls");
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("visible"));
    assert!(!listing.contains(".hidden"));

    let output = run_in(&directory, "ls -a");
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("visible"));
    assert!(listing.contains(".hidden"));

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rm_accepts_combined_options_and_refuses_working_directory_removal() {
    let directory = temporary_directory();
    fs::create_dir_all(directory.join("tree/nested")).unwrap();
    fs::write(directory.join("tree/nested/file"), b"data").unwrap();

    let output = run_in(&directory, "rm -rf tree");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("tree").exists());

    let output = run_in(&directory, "rm -rf .");
    assert_eq!(output.status.code(), Some(1));
    assert!(directory.exists());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn double_dash_allows_paths_that_begin_with_a_hyphen() {
    let directory = temporary_directory();
    fs::write(directory.join("-notes"), b"notes").unwrap();

    let output = run_in(&directory, "rm -- -notes");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("-notes").exists());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recursive_copy_rejects_copying_a_directory_into_itself() {
    let directory = temporary_directory();
    fs::create_dir_all(directory.join("source/nested")).unwrap();
    fs::write(directory.join("source/file"), b"data").unwrap();

    let output = run_in(&directory, "cp -r source source/nested/copy");
    assert_eq!(output.status.code(), Some(1));
    assert!(!directory.join("source/nested/copy").exists());

    fs::remove_dir_all(directory).unwrap();
}
