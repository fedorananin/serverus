use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

#[test]
fn binary_acts_as_the_configured_editor_when_given_a_cache_path() {
    let directory = tempfile::tempdir().unwrap();
    let cache_file = directory.path().join("edit-success.txt");
    std::fs::write(&cache_file, b"remote success original\n").unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_serverus-e2e-fixtures"))
        .arg(&cache_file)
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(
        std::fs::read(&cache_file).unwrap(),
        b"edited successfully by scenario editor\n"
    );
}

#[test]
fn binary_prints_one_manifest_line_and_waits_for_stdin_eof() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_serverus-e2e-fixtures"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut manifest_line = String::new();

    assert!(stdout.read_line(&mut manifest_line).unwrap() > 0);
    assert!(manifest_line.ends_with('\n'));
    let manifest: serde_json::Value = serde_json::from_str(&manifest_line).unwrap();
    assert_eq!(manifest["ftp"]["host"], "127.0.0.1");
    assert_eq!(manifest["ssh"]["available"], cfg!(unix));
    assert_eq!(
        manifest["editor"]["executable"],
        env!("CARGO_BIN_EXE_serverus-e2e-fixtures")
    );
    assert!(child.try_wait().unwrap().is_none());

    drop(child.stdin.take());
    let mut trailing_stdout = String::new();
    stdout.read_to_string(&mut trailing_stdout).unwrap();
    assert!(trailing_stdout.is_empty());
    assert!(child.wait().unwrap().success());
}
