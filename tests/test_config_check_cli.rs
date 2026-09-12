//! End-to-end coverage of `agentbridge config check`, run as a real process.
//!
//! The unit tests in `config::check` cover the verdicts; these pin the CLI
//! contract a pipeline actually depends on: exit codes, exactly one JSON object
//! on stdout, `--stdin` accepting piped YAML, and `--stdin` with an explicit
//! `--config` being rejected instead of quietly picking one of them.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_agentbridge");

const SECRET: &str = "s3cr3t-telegram-token-DO-NOT-LEAK";

const VALID_YAML: &str = r#"
language: en
projects:
  - name: piped
    work_dir: /nonexistent/deployment/only
    agents:
      - name: claude
        backend: claude
        mode: yolo
    default_agent: claude
    platforms:
      - type: telegram
        options:
          token: "dummy"
"#;

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Invoke the binary with `stdin_bytes` piped in, in a sandboxed HOME so a
/// default-path lookup can never reach the developer's real config.
fn run_in_home(args: &[&str], stdin_bytes: &[u8], home: &std::path::Path) -> Run {
    let mut child = Command::new(BIN)
        .args(args)
        .env("HOME", home)
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should start");

    {
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        // A usage error exits before reading, so a broken pipe here is expected.
        let _ = stdin.write_all(stdin_bytes);
    }

    let out = child.wait_with_output().expect("process should finish");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn run(args: &[&str], stdin_bytes: &[u8]) -> Run {
    let home = tempfile::tempdir().expect("tempdir");
    run_in_home(args, stdin_bytes, home.path())
}

/// Parse stdout as the single JSON report the `--json` contract promises.
fn sole_json(stdout: &str) -> serde_json::Value {
    let trimmed = stdout.trim_end_matches('\n');
    assert!(
        !trimmed.contains('\n'),
        "expected exactly one JSON object, got: {:?}",
        stdout
    );
    serde_json::from_str(trimmed).unwrap_or_else(|e| panic!("stdout is not JSON ({}): {:?}", e, stdout))
}

#[test]
fn stdin_valid_config_exits_zero_with_one_json_report() {
    let out = run(&["config", "check", "--stdin", "--json"], VALID_YAML.as_bytes());
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    let report = sole_json(&out.stdout);
    assert_eq!(report["schema"], "agentbridge.config-check.v1");
    assert_eq!(report["valid"], true);
    assert_eq!(report["config_path"], "<stdin>");
    assert_eq!(report["projects"], 1);
    assert!(report["error"].is_null());
}

#[test]
fn stdin_semantic_failure_exits_one_with_one_json_report() {
    let out = run(&["config", "check", "--stdin", "--json"], b"projects: []\n");
    assert_eq!(out.code, Some(1), "stderr: {}", out.stderr);
    let report = sole_json(&out.stdout);
    assert_eq!(report["valid"], false);
    assert_eq!(report["config_path"], "<stdin>");
    assert_eq!(report["error"]["kind"], "no_projects");
    assert_eq!(report["error"]["location"], "projects");
}

#[test]
fn stdin_malformed_yaml_exits_one_without_echoing_values() {
    let piped = format!(
        "projects:\n  - name: broken\n    platforms: [\n    token: \"{}\"\n",
        SECRET
    );
    let out = run(&["config", "check", "--stdin", "--json"], piped.as_bytes());
    assert_eq!(out.code, Some(1));
    let report = sole_json(&out.stdout);
    assert_eq!(report["error"]["kind"], "invalid_yaml");
    assert!(!out.stdout.contains(SECRET), "stdout leaked: {}", out.stdout);
    assert!(!out.stderr.contains(SECRET), "stderr leaked: {}", out.stderr);
}

#[test]
fn stdin_empty_input_exits_one() {
    let out = run(&["config", "check", "--stdin", "--json"], b"");
    assert_eq!(out.code, Some(1), "stderr: {}", out.stderr);
    let report = sole_json(&out.stdout);
    assert_eq!(report["valid"], false);
    assert_eq!(report["config_path"], "<stdin>");
}

#[test]
fn stdin_non_utf8_input_exits_one_without_echoing_bytes() {
    let mut bytes = b"token: ".to_vec();
    bytes.extend_from_slice(SECRET.as_bytes());
    bytes.push(0xFF);

    let out = run(&["config", "check", "--stdin", "--json"], &bytes);
    assert_eq!(out.code, Some(1), "stderr: {}", out.stderr);
    let report = sole_json(&out.stdout);
    assert_eq!(report["error"]["kind"], "config_unreadable");
    assert!(report["error"]["location"].is_null());
    assert!(!out.stdout.contains(SECRET), "stdout leaked: {}", out.stdout);
    assert!(!out.stderr.contains(SECRET), "stderr leaked: {}", out.stderr);
}

#[test]
fn stdin_text_output_names_stdin_as_the_input() {
    let out = run(&["config", "check", "--stdin"], VALID_YAML.as_bytes());
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    assert!(out.stdout.contains("config check: ok"), "{}", out.stdout);
    assert!(out.stdout.contains("<stdin>"), "{}", out.stdout);
}

#[test]
fn stdin_with_explicit_config_is_rejected_in_either_order() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), VALID_YAML).expect("write config");
    let path = file.path().to_string_lossy().to_string();

    for args in [
        vec!["--config", path.as_str(), "config", "check", "--stdin"],
        vec!["config", "check", "--stdin", "--config", path.as_str()],
        vec!["config", "check", "--config", path.as_str(), "--stdin"],
        vec!["--config", path.as_str(), "config", "check", "--stdin", "--json"],
    ] {
        let out = run(&args, VALID_YAML.as_bytes());
        assert_ne!(out.code, Some(0), "accepted {:?}", args);
        // A rejected invocation is not a verdict: no report, in either format.
        assert!(
            out.stdout.trim().is_empty(),
            "printed a report for {:?}: {}",
            args,
            out.stdout
        );
        assert!(
            out.stderr.contains("--stdin"),
            "no usage message for {:?}: {}",
            args,
            out.stderr
        );
    }
}

#[test]
fn stdin_ignores_the_default_config_path_and_writes_nothing() {
    // A broken config sits at the default path; the piped one must win, and the
    // check must leave the directory exactly as it found it.
    let home = tempfile::tempdir().expect("tempdir");
    let dir = home.path().join(".agentbridge");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let default_path = dir.join("config.yaml");
    std::fs::write(&default_path, "projects: []\n").expect("write default config");

    let out = run_in_home(
        &["config", "check", "--stdin", "--json"],
        VALID_YAML.as_bytes(),
        home.path(),
    );
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    assert_eq!(sole_json(&out.stdout)["config_path"], "<stdin>");

    assert_eq!(
        std::fs::read_to_string(&default_path).expect("default config still readable"),
        "projects: []\n",
        "the check rewrote the config file"
    );
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(entries.len(), 1, "the check created files: {:?}", entries);
}

#[test]
fn file_input_still_works_and_reports_its_path() {
    // Regression guard: adding --stdin must not disturb the file path.
    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), VALID_YAML).expect("write config");
    let path = file.path().to_string_lossy().to_string();

    let out = run(&["--config", path.as_str(), "config", "check", "--json"], b"");
    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    let report = sole_json(&out.stdout);
    assert_eq!(report["valid"], true);
    assert_eq!(report["config_path"], path);

    let missing = run(
        &["--config", "/nonexistent/agentbridge.yaml", "config", "check", "--json"],
        b"",
    );
    assert_eq!(missing.code, Some(1));
    assert_eq!(sole_json(&missing.stdout)["error"]["kind"], "config_not_found");
}
