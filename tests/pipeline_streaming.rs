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


#[test]
fn builtin_pipeline_output_redirection_disconnects_downstream_input() {
    let directory = std::env::temp_dir().join(format!("mysh-pipeline-redirect-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let output_path = directory.join("builtin-output.txt");
    let command = format!("pwd > '{}' | cat", output_path.display());
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stdout.is_empty(), "downstream stage received redirected output");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    let redirected = std::fs::read_to_string(&output_path).unwrap();
    assert_eq!(redirected, format!("{}\n", std::env::current_dir().unwrap().display()));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builtin_pipeline_input_redirection_replaces_upstream_data() {
    let directory = std::env::temp_dir().join(format!("mysh-pipeline-input-redirect-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let input_path = directory.join("builtin-input.txt");
    std::fs::write(&input_path, "redirected-input\\n").unwrap();
    let command = format!("pwd | cat < '{}'", input_path.display());
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, b"redirected-input\\n");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}
