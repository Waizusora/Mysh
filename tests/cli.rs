use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn command_string_mode_runs_a_builtin() {
    let output = Command::new(env!("CARGO_BIN_EXE_mysh")).args(["-c", "echo hello"]).output().expect("Mysh process should start");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n");
}

#[test]
fn command_string_mode_preserves_exit_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_mysh")).args(["-c", "exit 7"]).output().expect("Mysh process should start");
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn standard_input_mode_runs_multiple_commands() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mysh"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().expect("Mysh process should start");
    child.stdin.take().expect("stdin should be piped")
        .write_all(b"echo first\necho second\nexit 0\n").expect("commands should be written");
    let output = child.wait_with_output().expect("Mysh process should finish");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "first\nsecond\n");
}
