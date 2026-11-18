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
fn compound_stage_output_redirections_apply_in_order_and_override_pipeline_output() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-compound-redirect-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let first = directory.join("first.txt");
    let final_output = directory.join("final.txt");
    std::fs::write(&first, b"old-first").unwrap();
    std::fs::write(&final_output, b"old-final").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let command = format!(
        "{{ printf compound-output; }} > '{}' >> '{}'",
        first.display(),
        final_output.display(),
    );
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(std::fs::read(&first).unwrap(), b"");
    assert_eq!(std::fs::read(&final_output).unwrap(), b"old-finalcompound-output");
    assert!(stdout.is_empty());
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    let pipeline_output = directory.join("pipeline.txt");
    let command = format!(
        "{{ printf redirected-stage; }} > '{}' | cat",
        pipeline_output.display(),
    );
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(std::fs::read(&pipeline_output).unwrap(), b"redirected-stage");
    assert!(stdout.is_empty(), "redirected compound output reached downstream");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compound_stage_input_redirection_reaches_nested_external_commands() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-compound-input-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let input = directory.join("input.txt");
    std::fs::write(&input, b"compound-input\n").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let command = format!("{{ cat; }} < '{}'", input.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, b"compound-input\n");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compound_pipeline_stage_inherits_upstream_stdin() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line("printf pipeline-input | { cat; }", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, b"pipeline-input");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn compound_input_redirection_reaches_nested_external_pipelines() {
    let directory = std::env::temp_dir().join(format!("mysh-compound-nested-pipeline-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let input = directory.join("nested-input.txt");
    std::fs::write(&input, b"nested-input\n").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let command = format!("{{ cat | wc -c; }} < '{}'", input.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "13");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compound_stage_input_redirections_open_in_source_order() {
    let directory = std::env::temp_dir().join(format!("mysh-compound-ordered-input-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let missing = directory.join("missing.txt");
    let final_input = directory.join("final.txt");
    std::fs::write(&final_input, b"final-input").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let command = format!("{{ cat; }} < '{}' < '{}'", missing.display(), final_input.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 1);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("missing.txt"));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compound_pipeline_stage_inherits_upstream_stdin() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line("printf pipeline-input | { cat; }", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, b"pipeline-input");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn compound_stage_input_redirections_open_in_source_order() {
    let directory = std::env::temp_dir().join(format!("mysh-compound-ordered-input-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let missing = directory.join("missing.txt");
    let final_input = directory.join("final.txt");
    std::fs::write(&final_input, b"final-input").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let command = format!("{{ cat; }} < '{}' < '{}'", missing.display(), final_input.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 1);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("missing.txt"));

    std::fs::remove_dir_all(directory).unwrap();
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


#[test]
fn mixed_pipeline_streams_large_final_external_stage_output() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(
        "pwd | sh -c 'head -c 2097152 /dev/zero; head -c 2097152 /dev/zero >&2'",
        &mut stdout,
        &mut stderr,
    ));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout.len(), 2 * 1024 * 1024);
    assert!(stdout.iter().all(|byte| *byte == 0));
    assert_eq!(stderr.len(), 2 * 1024 * 1024);
    assert!(stderr.iter().all(|byte| *byte == 0));
}


#[test]
fn compound_pipeline_spools_large_intermediate_output_to_next_stage() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(
        "{ head -c 8388608 /dev/zero; } | wc -c",
        &mut stdout,
        &mut stderr,
    ));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "8388608");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn pipeline_input_redirection_streams_large_file_without_buffering() {
    let directory = std::env::temp_dir().join(format!("mysh-pipeline-large-input-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let input_path = directory.join("large-input.bin");
    let contents = vec![0x5au8; 8 * 1024 * 1024];
    std::fs::write(&input_path, &contents).unwrap();
    let command = format!("pwd | cat < '{}'", input_path.display());
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, contents);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn pipeline_status_uses_last_stage_and_drains_failed_upstream() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(
        "sh -c 'printf upstream; exit 7' | cat",
        &mut stdout,
        &mut stderr,
    ));
    assert_eq!(stdout, b"upstream");
    assert_eq!(shell.last_status(), 0);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    assert!(shell.execute_line("true | false", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 1);

    assert!(shell.execute_line("false | true", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn negated_pipeline_inverts_the_final_stage_status() {
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line("! true | cat", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 1);

    assert!(shell.execute_line("! false | false", &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
}

#[test]
fn external_pipeline_redirections_override_pipe_edges() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-external-pipeline-redirect-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let output_path = directory.join("upstream.txt");
    let input_path = directory.join("replacement.txt");
    std::fs::write(&input_path, b"replacement-input").unwrap();

    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let command = format!("printf upstream > '{}' | cat", output_path.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stdout.is_empty(), "redirected upstream output reached downstream");
    assert_eq!(std::fs::read(&output_path).unwrap(), b"upstream");

    let command = format!("printf ignored | cat < '{}'", input_path.display());
    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert_eq!(stdout, b"replacement-input");
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn external_pipeline_applies_repeated_redirections_in_source_order() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-ordered-redirect-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let first_output = directory.join("first-output.txt");
    let final_output = directory.join("final-output.txt");
    let first_error = directory.join("first-error.txt");
    let final_error = directory.join("final-error.txt");

    let command = format!(
        "sh -c 'printf stdout; printf stderr >&2' > '{}' > '{}' 2> '{}' 2> '{}' | cat",
        first_output.display(),
        final_output.display(),
        first_error.display(),
        final_error.display(),
    );
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
    assert_eq!(std::fs::read(&first_output).unwrap(), b"");
    assert_eq!(std::fs::read(&final_output).unwrap(), b"stdout");
    assert_eq!(std::fs::read(&first_error).unwrap(), b"");
    assert_eq!(std::fs::read(&final_error).unwrap(), b"stderr");

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn mixed_builtin_pipeline_applies_repeated_output_redirections_in_order() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-mixed-ordered-output-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let first_output = directory.join("first.txt");
    let final_output = directory.join("final.txt");
    std::fs::write(&first_output, b"existing-first").unwrap();
    std::fs::write(&final_output, b"existing-final").unwrap();

    let command = format!(
        "pwd > '{}' >> '{}' | cat",
        first_output.display(),
        final_output.display(),
    );
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(std::fs::read(&first_output).unwrap(), b"");
    assert_eq!(
        std::fs::read_to_string(&final_output).unwrap(),
        format!("existing-final{}\n", std::env::current_dir().unwrap().display()),
    );

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn mixed_builtin_pipeline_opens_every_repeated_input_redirection() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mysh-mixed-ordered-input-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let missing_input = directory.join("missing.txt");
    let final_input = directory.join("final.txt");
    std::fs::write(&final_input, b"final-input").unwrap();

    let command = format!(
        "pwd | cat < '{}' < '{}'",
        missing_input.display(),
        final_input.display(),
    );
    let mut shell = Shell::new();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(shell.execute_line(&command, &mut stdout, &mut stderr));
    assert_eq!(shell.last_status(), 1);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("missing.txt"));

    std::fs::remove_dir_all(directory).unwrap();
}
