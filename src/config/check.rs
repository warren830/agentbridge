//! Offline config check behind `agentbridge config check`.
//!
//! The check is deliberately inert: it reads one config (a file, or standard
//! input with `--stdin`), parses it, and applies the same semantic rules as
//! [`super::load`]. It never touches the network, never looks for agent
//! executables, never writes anything, and never starts a service — so it is
//! safe in CI with placeholder platform tokens, absent agent binaries and work
//! dirs that only exist on the deployment host.
//!
//! Every diagnostic is value-free by construction (see [`CheckErrorKind`]):
//! kinds are fixed sentences and locations are field names plus indices, so a
//! report can be logged or shipped to a dashboard without leaking tokens,
//! secrets or paths from inside the config.

use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::validation::{self, IssueKind};
use super::AppConfig;

/// Schema identifier emitted with every report so consumers can pin a shape.
pub const SCHEMA: &str = "agentbridge.config-check.v1";

/// Value of `config_path` when the YAML was piped in instead of read from a
/// file. A fixed sentinel, so a report from a pipeline is still self-describing
/// without inventing a path that does not exist.
pub const STDIN_INPUT: &str = "<stdin>";

/// Machine-readable outcome. Exactly one of these is printed per invocation,
/// for both the valid and the invalid outcome; all fields are always present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    /// Always [`SCHEMA`].
    pub schema: String,
    /// True only when the config parsed and every semantic rule passed.
    pub valid: bool,
    /// Config the check resolved: the path from `--config`, the default path,
    /// or [`STDIN_INPUT`] under `--stdin`. Comes from the invocation, not from
    /// inside the config.
    pub config_path: String,
    /// Number of projects parsed. Zero when the config could not be
    /// read/parsed.
    pub projects: usize,
    /// `null` when valid, otherwise the safe diagnostic.
    pub error: Option<CheckError>,
}

/// Value-free diagnostic for a failed check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckError {
    pub kind: CheckErrorKind,
    /// Where the problem is, never what the value was: `line 7, column 3` for
    /// parse failures, `projects[0].agents[1].acp` for semantic ones, `null`
    /// when the file never got far enough to have a location.
    pub location: Option<String>,
    /// Fixed sentence for `kind`, kept for humans reading raw JSON.
    pub message: String,
}

/// Failure categories. The first three cover reaching and parsing the input;
/// the rest mirror [`IssueKind`] one-for-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckErrorKind {
    ConfigNotFound,
    ConfigUnreadable,
    InvalidYaml,
    NoProjects,
    EmptyProjectName,
    NoPlatforms,
    AgentAndAgentsConflict,
    EmptyAgentName,
    DuplicateAgentName,
    MissingAcpConfig,
    MissingTmuxConfig,
    UnknownDefaultAgent,
}

impl CheckErrorKind {
    /// Fixed, value-free description.
    pub fn message(self) -> &'static str {
        match self {
            CheckErrorKind::ConfigNotFound => "config file not found",
            CheckErrorKind::ConfigUnreadable => "config file could not be read",
            CheckErrorKind::InvalidYaml => "config is not valid YAML or has a field of the wrong type",
            CheckErrorKind::NoProjects => IssueKind::NoProjects.safe_message(),
            CheckErrorKind::EmptyProjectName => IssueKind::EmptyProjectName.safe_message(),
            CheckErrorKind::NoPlatforms => IssueKind::NoPlatforms.safe_message(),
            CheckErrorKind::AgentAndAgentsConflict => {
                IssueKind::AgentAndAgentsConflict.safe_message()
            }
            CheckErrorKind::EmptyAgentName => IssueKind::EmptyAgentName.safe_message(),
            CheckErrorKind::DuplicateAgentName => IssueKind::DuplicateAgentName.safe_message(),
            CheckErrorKind::MissingAcpConfig => IssueKind::MissingAcpConfig.safe_message(),
            CheckErrorKind::MissingTmuxConfig => IssueKind::MissingTmuxConfig.safe_message(),
            CheckErrorKind::UnknownDefaultAgent => IssueKind::UnknownDefaultAgent.safe_message(),
        }
    }
}

impl From<IssueKind> for CheckErrorKind {
    fn from(kind: IssueKind) -> Self {
        match kind {
            IssueKind::NoProjects => CheckErrorKind::NoProjects,
            IssueKind::EmptyProjectName => CheckErrorKind::EmptyProjectName,
            IssueKind::NoPlatforms => CheckErrorKind::NoPlatforms,
            IssueKind::AgentAndAgentsConflict => CheckErrorKind::AgentAndAgentsConflict,
            IssueKind::EmptyAgentName => CheckErrorKind::EmptyAgentName,
            IssueKind::DuplicateAgentName => CheckErrorKind::DuplicateAgentName,
            IssueKind::MissingAcpConfig => CheckErrorKind::MissingAcpConfig,
            IssueKind::MissingTmuxConfig => CheckErrorKind::MissingTmuxConfig,
            IssueKind::UnknownDefaultAgent => CheckErrorKind::UnknownDefaultAgent,
        }
    }
}

impl CheckReport {
    fn valid(input: &str, projects: usize) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            valid: true,
            config_path: input.to_string(),
            projects,
            error: None,
        }
    }

    fn invalid(
        input: &str,
        projects: usize,
        kind: CheckErrorKind,
        location: Option<String>,
    ) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            valid: false,
            config_path: input.to_string(),
            projects,
            error: Some(CheckError {
                kind,
                location,
                message: kind.message().to_string(),
            }),
        }
    }

    /// The single JSON object for `--json`.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Human-readable rendering. Holds to the same disclosure rules as the JSON.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.valid {
            out.push_str("config check: ok\n");
        } else {
            out.push_str("config check: invalid\n");
        }
        out.push_str(&format!("  path:     {}\n", self.config_path));
        out.push_str(&format!("  projects: {}\n", self.projects));
        if let Some(ref err) = self.error {
            let kind = serde_json::to_value(err.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string());
            out.push_str(&format!("  error:    {} ({})\n", err.message, kind));
            if let Some(ref loc) = err.location {
                out.push_str(&format!("  at:       {}\n", loc));
            }
        }
        out
    }
}

/// Validate the config at `path` (or the default path) without side effects.
///
/// Returns a report rather than a `Result`: both outcomes are reportable
/// results of the check, and the caller turns `valid` into the exit code.
pub fn check(path: Option<&str>) -> CheckReport {
    let config_path: PathBuf = path
        .map(PathBuf::from)
        .unwrap_or_else(super::default_config_path);
    let input = config_path.display().to_string();

    // read_to_string covers "missing" and "unreadable" (permissions, a
    // directory in place of the file, non-UTF-8 bytes) in one syscall, so a
    // file that vanishes mid-check cannot be reported as present.
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(e) => {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                CheckErrorKind::ConfigNotFound
            } else {
                CheckErrorKind::ConfigUnreadable
            };
            return CheckReport::invalid(&input, 0, kind, None);
        }
    };

    check_content(&input, &content)
}

/// Validate the config on standard input — the `--stdin` path, for CI and shell
/// pipelines that generate YAML and never want it on disk.
pub fn check_stdin() -> CheckReport {
    check_reader(std::io::stdin().lock())
}

/// Validate config YAML read from `reader`, reported as [`STDIN_INPUT`].
///
/// Same rules and same value-free diagnostics as [`check`]; only where the
/// bytes come from differs. Decoding happens up front so non-UTF-8 input is
/// reported exactly like a non-UTF-8 file — unreadable, with no location — and
/// the undecodable bytes are dropped rather than echoed.
pub fn check_reader<R: Read>(mut reader: R) -> CheckReport {
    let mut bytes = Vec::new();
    if reader.read_to_end(&mut bytes).is_err() {
        return CheckReport::invalid(
            STDIN_INPUT,
            0,
            CheckErrorKind::ConfigUnreadable,
            None,
        );
    }

    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => {
            return CheckReport::invalid(
                STDIN_INPUT,
                0,
                CheckErrorKind::ConfigUnreadable,
                None,
            )
        }
    };

    check_content(STDIN_INPUT, &content)
}

/// Parse and validate already-read YAML. The single place the rules live, so a
/// piped config cannot drift from a file one; `input` is only a report label.
fn check_content(input: &str, content: &str) -> CheckReport {
    let config: AppConfig = match serde_yaml::from_str(content) {
        Ok(config) => config,
        Err(e) => {
            // serde_yaml's message quotes the offending scalar ("invalid type:
            // string \"abc\"") and would leak config values, so only the
            // position survives. Empty input lands here too (EOF while parsing
            // a value), with no position to report.
            let location = e
                .location()
                .map(|l| format!("line {}, column {}", l.line(), l.column()));
            return CheckReport::invalid(input, 0, CheckErrorKind::InvalidYaml, location);
        }
    };

    match validation::validate_config(&config) {
        Ok(()) => CheckReport::valid(input, config.projects.len()),
        Err(issue) => CheckReport::invalid(
            input,
            config.projects.len(),
            issue.kind.into(),
            Some(issue.location),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Write `yaml` into a temp dir and return (dir, path). The dir must stay
    /// alive for the path to remain valid.
    fn write_config(yaml: &str) -> (TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        (dir, path.to_string_lossy().to_string())
    }

    const SECRET: &str = "s3cr3t-telegram-token-DO-NOT-LEAK";

    #[test]
    fn valid_legacy_agent_config_passes() {
        // Dummy token, agent binary that does not exist, work_dir that only
        // exists on the deployment host: none of it may fail the check.
        let (_dir, path) = write_config(&format!(
            r#"
language: en
projects:
  - name: legacy
    work_dir: /nonexistent/deployment/only
    agent:
      mode: yolo
      model: sonnet
    platforms:
      - type: telegram
        options:
          token: "{}"
"#,
            SECRET
        ));
        let report = check(Some(&path));
        assert!(report.valid, "{:?}", report.error);
        assert_eq!(report.projects, 1);
        assert!(report.error.is_none());
        assert_eq!(report.schema, SCHEMA);
        assert_eq!(report.config_path, path);
    }

    #[test]
    fn valid_multi_agent_config_passes() {
        let (_dir, path) = write_config(
            r#"
projects:
  - name: multi
    work_dir: /nonexistent/deployment/only
    agents:
      - name: claude
        backend: claude
        mode: yolo
      - name: kiro
        backend: acp
        acp:
          command: /nonexistent/kiro-cli
          args: ["acp"]
      - name: pane
        backend: tmux
        tmux:
          session: work
    default_agent: kiro
    platforms:
      - type: discord
        options:
          token: "dummy"
  - name: second
    work_dir: /tmp
    platforms:
      - type: feishu
        options:
          app_id: "cli_dummy"
          app_secret: "dummy"
"#,
        );
        let report = check(Some(&path));
        assert!(report.valid, "{:?}", report.error);
        assert_eq!(report.projects, 2);
    }

    #[test]
    fn missing_file_is_reported_without_reading_anything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.yaml").to_string_lossy().to_string();
        let report = check(Some(&path));
        assert!(!report.valid);
        assert_eq!(report.projects, 0);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::ConfigNotFound);
        assert_eq!(err.location, None);
    }

    #[test]
    fn directory_in_place_of_file_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let report = check(Some(&dir.path().to_string_lossy()));
        assert!(!report.valid);
        assert_eq!(
            report.error.unwrap().kind,
            CheckErrorKind::ConfigUnreadable
        );
    }

    #[test]
    fn malformed_yaml_reports_position_only() {
        let (_dir, path) = write_config(&format!(
            r#"
projects:
  - name: broken
    platforms: [
    token: "{}"
"#,
            SECRET
        ));
        let report = check(Some(&path));
        assert!(!report.valid);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::InvalidYaml);
        assert!(err.location.is_some());
        assert!(!err.message.contains(SECRET));
    }

    #[test]
    fn invalid_field_type_does_not_echo_the_value() {
        // serde_yaml would say: invalid type: string "not-a-port"...
        let (_dir, path) = write_config(
            r#"
webhook:
  port: "not-a-port"
projects:
  - name: typed
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let report = check(Some(&path));
        assert!(!report.valid);
        let json = report.to_json().unwrap();
        assert!(json.contains("invalid_yaml"));
        assert!(!json.contains("not-a-port"), "leaked value: {}", json);
    }

    #[test]
    fn empty_projects_is_a_semantic_error() {
        let (_dir, path) = write_config("projects: []\n");
        let report = check(Some(&path));
        assert!(!report.valid);
        assert_eq!(report.projects, 0);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::NoProjects);
        assert_eq!(err.location.as_deref(), Some("projects"));
    }

    #[test]
    fn duplicate_agent_name_is_a_semantic_error() {
        let (_dir, path) = write_config(
            r#"
projects:
  - name: dup
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
      - name: claude
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let report = check(Some(&path));
        assert!(!report.valid);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::DuplicateAgentName);
        assert_eq!(
            err.location.as_deref(),
            Some("projects[0].agents[1].name")
        );
    }

    #[test]
    fn unknown_default_agent_is_a_semantic_error() {
        let (_dir, path) = write_config(
            r#"
projects:
  - name: defaults
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
    default_agent: nope
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let report = check(Some(&path));
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::UnknownDefaultAgent);
        assert_eq!(
            err.location.as_deref(),
            Some("projects[0].default_agent")
        );
    }

    #[test]
    fn missing_acp_and_tmux_sections_are_reported() {
        let (_dir, acp_path) = write_config(
            r#"
projects:
  - name: acp
    work_dir: /tmp
    agents:
      - name: kiro
        backend: acp
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        assert_eq!(
            check(Some(&acp_path)).error.unwrap().kind,
            CheckErrorKind::MissingAcpConfig
        );

        let (_dir2, tmux_path) = write_config(
            r#"
projects:
  - name: tmux
    work_dir: /tmp
    agents:
      - name: pane
        backend: tmux
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        assert_eq!(
            check(Some(&tmux_path)).error.unwrap().kind,
            CheckErrorKind::MissingTmuxConfig
        );
    }

    #[test]
    fn semantic_error_output_excludes_secrets_and_names() {
        let (_dir, path) = write_config(&format!(
            r#"
projects:
  - name: leaky-project
    work_dir: /home/someone/private/repo
    agents:
      - name: leaky-agent
        backend: claude
      - name: leaky-agent
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "{}"
          admin_password: "hunter2"
"#,
            SECRET
        ));
        let report = check(Some(&path));
        let json = report.to_json().unwrap();
        let text = report.to_text();
        for rendering in [&json, &text] {
            for leaked in [SECRET, "hunter2", "leaky-project", "leaky-agent", "private/repo"] {
                assert!(
                    !rendering.contains(leaked),
                    "rendering leaked {:?}: {}",
                    leaked,
                    rendering
                );
            }
        }
    }

    #[test]
    fn valid_report_serializes_all_documented_fields() {
        let (_dir, path) = write_config(
            r#"
projects:
  - name: ok
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let json = check(Some(&path)).to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = parsed.as_object().unwrap();
        assert_eq!(obj.len(), 5, "unexpected report shape: {}", json);
        assert_eq!(obj["schema"], SCHEMA);
        assert_eq!(obj["valid"], true);
        assert_eq!(obj["projects"], 1);
        assert!(obj["error"].is_null());
        assert!(obj["config_path"].is_string());
    }

    #[test]
    fn invalid_report_serializes_all_documented_fields() {
        let (_dir, path) = write_config("projects: []\n");
        let json = check(Some(&path)).to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = parsed.as_object().unwrap();
        assert_eq!(obj.len(), 5, "unexpected report shape: {}", json);
        assert_eq!(obj["valid"], false);
        let err = obj["error"].as_object().unwrap();
        assert_eq!(err.len(), 3, "unexpected error shape: {}", json);
        assert_eq!(err["kind"], "no_projects");
        assert_eq!(err["location"], "projects");
        assert!(err["message"].is_string());
    }

    #[test]
    fn report_round_trips_through_json() {
        let (_dir, path) = write_config("projects: []\n");
        let report = check(Some(&path));
        let back: CheckReport = serde_json::from_str(&report.to_json().unwrap()).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn text_rendering_states_the_verdict() {
        let (_dir, path) = write_config(
            r#"
projects:
  - name: ok
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let text = check(Some(&path)).to_text();
        assert!(text.contains("config check: ok"));
        assert!(text.contains("projects: 1"));

        let (_dir2, bad) = write_config("projects: []\n");
        let text = check(Some(&bad)).to_text();
        assert!(text.contains("config check: invalid"));
        assert!(text.contains("no_projects"));
    }

    #[test]
    fn check_agrees_with_load() {
        // The check must not drift from the loader's verdict.
        let (_dir, good) = write_config(
            r#"
projects:
  - name: ok
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        assert!(check(Some(&good)).valid);
        assert!(super::super::load(Some(&good)).is_ok());

        let (_dir2, bad) = write_config("projects: []\n");
        assert!(!check(Some(&bad)).valid);
        assert!(super::super::load(Some(&bad)).is_err());
    }

    // --- stdin (`--stdin`) ---

    const VALID_YAML: &str = r#"
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

    /// Reader that fails mid-stream, standing in for a broken pipe.
    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed"))
        }
    }

    #[test]
    fn piped_valid_config_passes_and_is_labelled_stdin() {
        let report = check_reader(VALID_YAML.as_bytes());
        assert!(report.valid, "{:?}", report.error);
        assert_eq!(report.projects, 1);
        assert!(report.error.is_none());
        assert_eq!(report.schema, SCHEMA);
        assert_eq!(report.config_path, STDIN_INPUT);
    }

    #[test]
    fn piped_input_never_consults_the_default_path() {
        // No path is resolved at all for stdin: the same bytes must verdict the
        // same on a host with no ~/.agentbridge/config.yaml as on one with a
        // broken config sitting there.
        let report = check_reader("projects: []\n".as_bytes());
        assert_eq!(report.config_path, STDIN_INPUT);
        assert_eq!(report.error.unwrap().kind, CheckErrorKind::NoProjects);
    }

    #[test]
    fn piped_empty_input_fails_without_leaking_or_defaulting() {
        // An empty pipe must never pass and must never silently fall back to
        // the config file on disk. Which failure it is depends on the flavour of
        // emptiness — bare `""` deserializes into an all-defaults config and
        // trips the semantic rule, whitespace-only input trips the parser — so
        // the test pins the properties that matter, not one kind.
        for empty in ["", "   \n\n\t", "---\n", "# only a comment\n"] {
            let report = check_reader(empty.as_bytes());
            assert!(!report.valid, "empty input passed: {:?}", empty);
            assert_eq!(report.projects, 0);
            assert_eq!(report.config_path, STDIN_INPUT);
            let err = report.error.expect("a failed check must carry a diagnostic");
            assert!(
                matches!(
                    err.kind,
                    CheckErrorKind::NoProjects | CheckErrorKind::InvalidYaml
                ),
                "unexpected kind {:?} for {:?}",
                err.kind,
                empty
            );

            // Same verdict as handing the same bytes over as a file.
            let (_dir, path) = write_config(empty);
            assert_eq!(check(Some(&path)).error, Some(err), "input: {:?}", empty);
        }
    }

    #[test]
    fn piped_malformed_yaml_reports_position_only() {
        let piped = format!(
            r#"
projects:
  - name: broken
    platforms: [
    token: "{}"
"#,
            SECRET
        );
        let report = check_reader(piped.as_bytes());
        assert!(!report.valid);
        let json = report.to_json().unwrap();
        assert!(!json.contains(SECRET), "leaked secret: {}", json);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::InvalidYaml);
        assert!(err.location.is_some());
    }

    #[test]
    fn piped_non_utf8_input_is_unreadable_and_echoes_nothing() {
        // A lone 0xFF cannot start a UTF-8 sequence; the surrounding bytes
        // spell out a token that must not reach the report.
        let mut bytes = b"token: ".to_vec();
        bytes.extend_from_slice(SECRET.as_bytes());
        bytes.push(0xFF);

        let report = check_reader(&bytes[..]);
        assert!(!report.valid);
        assert_eq!(report.projects, 0);
        assert_eq!(report.config_path, STDIN_INPUT);
        let json = report.to_json().unwrap();
        assert!(!json.contains(SECRET), "leaked secret: {}", json);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::ConfigUnreadable);
        assert_eq!(err.location, None);
    }

    #[test]
    fn unreadable_pipe_is_reported_as_unreadable() {
        let report = check_reader(FailingReader);
        assert!(!report.valid);
        assert_eq!(report.config_path, STDIN_INPUT);
        let err = report.error.unwrap();
        assert_eq!(err.kind, CheckErrorKind::ConfigUnreadable);
        assert_eq!(err.location, None);
    }

    #[test]
    fn piped_semantic_error_matches_the_file_verdict() {
        // Same bytes through both paths must agree on kind and location, so a
        // pipeline check cannot pass what a file check would reject.
        let yaml = r#"
projects:
  - name: dup
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
      - name: claude
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let (_dir, path) = write_config(yaml);
        let from_file = check(Some(&path));
        let from_stdin = check_reader(yaml.as_bytes());

        assert_eq!(from_stdin.valid, from_file.valid);
        assert_eq!(from_stdin.projects, from_file.projects);
        assert_eq!(from_stdin.error, from_file.error);
        assert_eq!(from_stdin.config_path, STDIN_INPUT);
        assert_eq!(from_file.config_path, path);
    }

    #[test]
    fn piped_and_file_checks_agree_across_every_outcome() {
        for yaml in [
            VALID_YAML,
            "projects: []\n",
            "projects:\n  - name: \"\"\n    platforms:\n      - type: telegram\n",
            "projects:\n  - name: noplat\n    platforms: []\n",
            "webhook:\n  port: \"not-a-port\"\nprojects: []\n",
        ] {
            let (_dir, path) = write_config(yaml);
            let from_file = check(Some(&path));
            let from_stdin = check_reader(yaml.as_bytes());
            assert_eq!(
                from_stdin.valid, from_file.valid,
                "verdict differs for: {}",
                yaml
            );
            assert_eq!(
                from_stdin.error, from_file.error,
                "diagnostic differs for: {}",
                yaml
            );
        }
    }

    #[test]
    fn piped_semantic_error_output_excludes_secrets_and_names() {
        let piped = format!(
            r#"
projects:
  - name: leaky-project
    work_dir: /home/someone/private/repo
    agents:
      - name: leaky-agent
        backend: claude
      - name: leaky-agent
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "{}"
          admin_password: "hunter2"
"#,
            SECRET
        );
        let report = check_reader(piped.as_bytes());
        let json = report.to_json().unwrap();
        let text = report.to_text();
        for rendering in [&json, &text] {
            for leaked in [SECRET, "hunter2", "leaky-project", "leaky-agent", "private/repo"] {
                assert!(
                    !rendering.contains(leaked),
                    "rendering leaked {:?}: {}",
                    leaked,
                    rendering
                );
            }
        }
    }

    #[test]
    fn piped_report_serializes_all_documented_fields() {
        let json = check_reader(VALID_YAML.as_bytes()).to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = parsed.as_object().unwrap();
        assert_eq!(obj.len(), 5, "unexpected report shape: {}", json);
        assert_eq!(obj["schema"], SCHEMA);
        assert_eq!(obj["valid"], true);
        assert_eq!(obj["projects"], 1);
        assert!(obj["error"].is_null());
        assert_eq!(obj["config_path"], STDIN_INPUT);
    }

    #[test]
    fn piped_text_rendering_names_stdin_as_the_input() {
        let text = check_reader(VALID_YAML.as_bytes()).to_text();
        assert!(text.contains("config check: ok"));
        assert!(text.contains(STDIN_INPUT), "{}", text);
    }
}
