//! Structured semantic validation of a parsed [`AppConfig`].
//!
//! The rules live here once and serve two callers with different disclosure
//! rules. `config::load` renders the verbose message, which interpolates
//! project and agent names, because it is printed to the operator who owns the
//! file. The offline `config check` command reports only the issue kind plus a
//! location built from indices, so machine-readable output can be piped
//! anywhere without carrying config values.

use super::{default_mode, AppConfig, ProjectConfig};
use serde::{Deserialize, Serialize};

/// Category of a semantic config problem. Each variant maps to one fixed,
/// value-free sentence, which is what makes it safe to publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
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

impl IssueKind {
    /// Description with no interpolated config values.
    pub fn safe_message(self) -> &'static str {
        match self {
            IssueKind::NoProjects => "no projects configured",
            IssueKind::EmptyProjectName => "project name is empty",
            IssueKind::NoPlatforms => "project has no platforms configured",
            IssueKind::AgentAndAgentsConflict => {
                "project sets both the legacy 'agent' field and the 'agents' list"
            }
            IssueKind::EmptyAgentName => "agent name is empty",
            IssueKind::DuplicateAgentName => "duplicate agent name within a project",
            IssueKind::MissingAcpConfig => "agent uses backend 'acp' but has no 'acp' config",
            IssueKind::MissingTmuxConfig => "agent uses backend 'tmux' but has no 'tmux' config",
            IssueKind::UnknownDefaultAgent => "default_agent is not present in the agents list",
        }
    }
}

/// One semantic problem, carrying both disclosure levels.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub kind: IssueKind,
    /// Structural path built from field names and indices only, e.g.
    /// `projects[0].agents[1].acp`. Never contains config values.
    pub location: String,
    detail: String,
}

impl ValidationIssue {
    fn new(kind: IssueKind, location: String, detail: String) -> Self {
        Self {
            kind,
            location,
            detail,
        }
    }

    /// Verbose operator-facing message. May embed config values (project and
    /// agent names), so it must stay out of machine-readable output.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Apply every semantic rule, returning the first problem found.
pub fn validate_config(config: &AppConfig) -> Result<(), ValidationIssue> {
    if config.projects.is_empty() {
        return Err(ValidationIssue::new(
            IssueKind::NoProjects,
            "projects".to_string(),
            "No projects configured. Add at least one [[projects]] entry.".to_string(),
        ));
    }

    for (i, p) in config.projects.iter().enumerate() {
        if p.name.is_empty() {
            return Err(ValidationIssue::new(
                IssueKind::EmptyProjectName,
                format!("projects[{}].name", i),
                "Project name cannot be empty".to_string(),
            ));
        }
        if p.platforms.is_empty() {
            return Err(ValidationIssue::new(
                IssueKind::NoPlatforms,
                format!("projects[{}].platforms", i),
                format!("Project '{}' has no platforms configured", p.name),
            ));
        }
        validate_agents(i, p)?;
    }

    Ok(())
}

fn validate_agents(index: usize, project: &ProjectConfig) -> Result<(), ValidationIssue> {
    let has_old_agent = project.agent.mode != default_mode()
        || project.agent.model.is_some()
        || !project.agent.allowed_tools.is_empty()
        || project.agent.max_turns.is_some();
    let has_new_agents = !project.agents.is_empty();

    if has_old_agent && has_new_agents {
        return Err(ValidationIssue::new(
            IssueKind::AgentAndAgentsConflict,
            format!("projects[{}]", index),
            format!(
                "Project '{}': cannot have both 'agent' and 'agents' fields. \
                 Remove the old 'agent:' field and use 'agents:' instead.",
                project.name
            ),
        ));
    }

    if has_new_agents {
        let mut seen_names = std::collections::HashSet::new();
        for (j, entry) in project.agents.iter().enumerate() {
            if entry.name.is_empty() {
                return Err(ValidationIssue::new(
                    IssueKind::EmptyAgentName,
                    format!("projects[{}].agents[{}].name", index, j),
                    format!("Project '{}': agent name cannot be empty", project.name),
                ));
            }
            if !seen_names.insert(&entry.name) {
                return Err(ValidationIssue::new(
                    IssueKind::DuplicateAgentName,
                    format!("projects[{}].agents[{}].name", index, j),
                    format!(
                        "Project '{}': duplicate agent name '{}'",
                        project.name, entry.name
                    ),
                ));
            }
            if entry.backend == "acp" && entry.acp.is_none() {
                return Err(ValidationIssue::new(
                    IssueKind::MissingAcpConfig,
                    format!("projects[{}].agents[{}].acp", index, j),
                    format!(
                        "Project '{}': agent '{}' has backend 'acp' but no 'acp:' config",
                        project.name, entry.name
                    ),
                ));
            }
            if entry.backend == "tmux" && entry.tmux.is_none() {
                return Err(ValidationIssue::new(
                    IssueKind::MissingTmuxConfig,
                    format!("projects[{}].agents[{}].tmux", index, j),
                    format!(
                        "Project '{}': agent '{}' has backend 'tmux' but no 'tmux:' config",
                        project.name, entry.name
                    ),
                ));
            }
        }

        if let Some(ref default_name) = project.default_agent {
            if !project.agents.iter().any(|a| a.name == *default_name) {
                return Err(ValidationIssue::new(
                    IssueKind::UnknownDefaultAgent,
                    format!("projects[{}].default_agent", index),
                    format!(
                        "Project '{}': default_agent '{}' not found in agents list. Available: {}",
                        project.name,
                        default_name,
                        project
                            .agents
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> AppConfig {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn valid_multi_agent_config_has_no_issue() {
        let config = parse(
            r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
    default_agent: kiro
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn issue_reports_kind_and_index_location() {
        let config = parse(
            r#"
projects:
  - name: first
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
  - name: second
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
        let issue = validate_config(&config).unwrap_err();
        assert_eq!(issue.kind, IssueKind::MissingAcpConfig);
        assert_eq!(issue.location, "projects[1].agents[0].acp");
        // The verbose message keeps names; the safe one must not.
        assert!(issue.detail().contains("kiro"));
        assert!(!issue.kind.safe_message().contains("kiro"));
    }

    #[test]
    fn missing_tmux_config_is_reported() {
        let config = parse(
            r#"
projects:
  - name: test
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
        let issue = validate_config(&config).unwrap_err();
        assert_eq!(issue.kind, IssueKind::MissingTmuxConfig);
        assert_eq!(issue.location, "projects[0].agents[0].tmux");
    }

    #[test]
    fn safe_message_omits_names_the_detail_keeps() {
        let config = parse(
            r#"
projects:
  - name: unmistakable-project
    work_dir: /tmp
    agents:
      - name: unmistakable-agent
        backend: claude
      - name: unmistakable-agent
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "t"
"#,
        );
        let issue = validate_config(&config).unwrap_err();
        assert_eq!(issue.kind, IssueKind::DuplicateAgentName);
        assert!(issue.detail().contains("unmistakable-project"));
        assert!(issue.detail().contains("unmistakable-agent"));
        assert!(!issue.kind.safe_message().contains("unmistakable"));
        assert!(!issue.location.contains("unmistakable"));
    }

    #[test]
    fn every_kind_has_a_nonempty_safe_message() {
        for kind in [
            IssueKind::NoProjects,
            IssueKind::EmptyProjectName,
            IssueKind::NoPlatforms,
            IssueKind::AgentAndAgentsConflict,
            IssueKind::EmptyAgentName,
            IssueKind::DuplicateAgentName,
            IssueKind::MissingAcpConfig,
            IssueKind::MissingTmuxConfig,
            IssueKind::UnknownDefaultAgent,
        ] {
            assert!(!kind.safe_message().is_empty(), "{:?}", kind);
        }
    }
}
