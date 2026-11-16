use mysh::Shell;

#[test]
fn multi_stage_pipeline_drains_each_stderr_stream_concurrently() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let keep_running = shell.execute_line(
        "sh -c 'head -c 131072 /dev/zero; head -c 2097152 /dev/zero >&2' | sh -c 'cat; head -c 2097152 /dev/zero >&2' | cat",
        &mut stdout,
        &mut stderr,
    );

    assert!(keep_running);
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout.len(), 131_072);
    assert!(stdout.iter().all(|byte| *byte == 0));
    assert_eq!(stderr.len(), 4 * 1024 * 1024);
    assert!(stderr.iter().all(|byte| *byte == 0));
}


#[test]
fn builtin_pipeline_stage_preserves_output_and_status() {
    let mut shell = Shell::new();
    let expected = format!("{}\n", std::env::current_dir().unwrap().display());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line("pwd | cat", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, expected.as_bytes());
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn compound_pipeline_stage_runs_in_isolated_shell_state() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(
        "PIPELINE_PARENT=parent; { PIPELINE_PARENT=child; printf 'compound:%s\\n' \"$PIPELINE_PARENT\"; } | cat; printf 'parent:%s\\n' \"$PIPELINE_PARENT\"",
        &mut stdout,
        &mut stderr,
    ));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "compound:child\nparent:parent\n",
    );
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}
