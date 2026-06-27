//! Configuration loading and validation.
//!
//! Config lives at ~/.agentbridge/config.yaml by default.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level application config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_language")]
    pub language: String,

    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    #[serde(default)]
    pub projects: Vec<ProjectConfig>,

    /// Webhook server config.
    pub webhook: Option<WebhookConfig>,

    /// Hook receiver server config (Claude Code Stop/PostToolUse hook relay).
    pub hook_receiver: Option<HookReceiverConfig>,

    /// Log config.
    pub log: Option<LogConfig>,
}

/// Logging config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log to file in addition to stderr. Directory for log files.
    pub file_dir: Option<String>,
    /// Log file prefix (default: "agentbridge")
    #[serde(default = "default_log_prefix")]
    pub file_prefix: String,
}

fn default_log_prefix() -> String {
    "agentbridge".to_string()
}

/// Webhook HTTP endpoint config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default = "default_webhook_port")]
    pub port: u16,
    /// Optional secret for authentication.
    pub secret: Option<String>,
}

fn default_webhook_port() -> u16 {
    9111
}

/// Hook receiver HTTP endpoint config. Listens on localhost for Claude Code
/// Stop/PostToolUse hook payloads and relays them into the matching session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookReceiverConfig {
    /// Port the localhost receiver binds to. The installer bakes this same
    /// value into the hook command, so the two must agree (ADR-8).
    #[serde(default = "default_hook_receiver_port")]
    pub port: u16,
}

impl Default for HookReceiverConfig {
    fn default() -> Self {
        Self {
            port: default_hook_receiver_port(),
        }
    }
}

/// Default hook receiver port. Distinct from the webhook port (9111) and the
/// gateway port (9900) so the three localhost servers never collide (ADR-8).
pub fn default_hook_receiver_port() -> u16 {
    9123
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub work_dir: PathBuf,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub agents: Vec<AgentEntry>,

    pub default_agent: Option<String>,

    #[serde(default)]
    pub platforms: Vec<PlatformConfig>,

    /// Comma-separated user IDs or "*" for all
    #[serde(default = "default_allow_all")]
    pub allow_from: String,

    /// Admin user IDs for privileged commands
    pub admin_from: Option<String>,

    #[serde(default)]
    pub rate_limit: RateLimitConfig,

    /// Words that cause messages to be silently dropped.
    #[serde(default)]
    pub banned_words: Vec<String>,

    /// Custom slash commands defined by the user.
    #[serde(default)]
    pub commands: Vec<CommandConfig>,

    /// Auto-compress settings.
    #[serde(default)]
    pub auto_compress: AutoCompressConfig,

    /// Command aliases (e.g., "新建" -> "/new").
    #[serde(default)]
    pub aliases: Vec<AliasConfig>,

    /// Session sync config (rsync Claude Code sessions between machines).
    pub sync: Option<SyncConfig>,

    /// Speech (STT/TTS) config.
    pub speech: Option<crate::speech::SpeechConfig>,

    /// Reset agent session after N minutes of inactivity (0 = disabled).
    #[serde(default)]
    pub reset_on_idle_mins: u32,

    /// Display settings for thinking/tool output.
    #[serde(default)]
    pub display: DisplayConfig,
}

/// Controls what intermediate output (thinking, tool use) is shown to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Show thinking/chain-of-thought messages (default: true).
    #[serde(default = "default_true")]
    pub thinking_messages: bool,
    /// Max chars for thinking display (default: 200).
    #[serde(default = "default_thinking_max")]
    pub thinking_max_len: usize,
    /// Show tool use notifications (default: true).
    #[serde(default = "default_true")]
    pub tool_messages: bool,
    /// Max chars for tool input display (default: 200).
    #[serde(default = "default_tool_max")]
    pub tool_max_len: usize,
    /// Show context indicator [tokens: N] in results (default: true).
    #[serde(default = "default_true")]
    pub context_indicator: bool,
    /// Context window size (tokens) for the `[ctx: ~N%]` indicator.
    /// Default 200k matches Claude Sonnet/Opus. Set lower for models with smaller windows.
    #[serde(default = "default_context_window")]
    pub context_window: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            thinking_messages: true,
            thinking_max_len: 200,
            tool_messages: true,
            tool_max_len: 200,
            context_indicator: true,
            context_window: default_context_window(),
        }
    }
}

fn default_true() -> bool { true }
fn default_thinking_max() -> usize { 200 }
fn default_tool_max() -> usize { 200 }
fn default_context_window() -> u32 { 200_000 }

/// Sync Claude Code sessions between local and remote machine via rsync/SSH.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// SSH remote, e.g. "mac:~/.claude/" or "user@192.168.1.10:~/.claude/"
    pub remote: String,
    /// Local Claude directory (default: ~/.claude/)
    #[serde(default = "default_claude_dir")]
    pub local: String,
    /// Auto-sync: pull before each message, push after each reply.
    #[serde(default)]
    pub auto: bool,
}

fn default_claude_dir() -> String {
    dirs_next().join(".claude/").to_string_lossy().to_string()
}

/// Auto-compress: when agent response token count exceeds threshold,
/// automatically send /compact to the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoCompressConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Token threshold to trigger compression (approximate char count / 4).
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

impl Default for AutoCompressConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_tokens: default_max_tokens(),
        }
    }
}

fn default_max_tokens() -> u32 {
    12000
}

/// A command alias mapping a trigger word to a slash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasConfig {
    /// The trigger word (e.g., "新建", "切换", "列表")
    pub trigger: String,
    /// The command to execute (e.g., "/new", "/switch", "/list")
    pub command: String,
}

/// A user-defined custom slash command with a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum messages per window. 0 = disabled.
    #[serde(default = "default_max_messages")]
    pub max_messages: u32,
    /// Sliding window duration in seconds.
    #[serde(default = "default_window_secs")]
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_messages: default_max_messages(),
            window_secs: default_window_secs(),
        }
    }
}

fn default_max_messages() -> u32 {
    20
}

fn default_window_secs() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_mode")]
    pub mode: String,

    pub model: Option<String>,

    #[serde(default)]
    pub allowed_tools: Vec<String>,

    pub max_turns: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            model: None,
            allowed_tools: vec![],
            max_turns: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-agent configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub name: String,
    #[serde(default = "default_backend_claude")]
    pub backend: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub max_turns: Option<u32>,
    pub acp: Option<AcpConfig>,
    pub tmux: Option<TmuxConfig>,
}

impl AgentEntry {
    pub fn to_agent_config(&self) -> AgentConfig {
        AgentConfig {
            mode: self.mode.clone(),
            model: self.model.clone(),
            allowed_tools: self.allowed_tools.clone(),
            max_turns: self.max_turns,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    pub auth_method: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxConfig {
    /// tmux session name to attach to / create.
    pub session: String,
    /// Automatically create the tmux session and start claude on daemon start.
    #[serde(default = "default_true")]
    pub auto_start: bool,
    /// Automatically restart claude if it exits.
    #[serde(default = "default_true")]
    pub auto_restart: bool,
    /// Hook relay mode: the turn's reply text comes from Claude Code's Stop
    /// hook (relayed via the hook receiver) instead of being scraped from the
    /// pane. In this mode the poll loop stops emitting Text/Result and the
    /// visible heartbeat, keeping only permission detection — the hook's Stop
    /// is the single source of the turn-ending Result.
    #[serde(default)]
    pub hook_relay: bool,
}

fn default_backend_claude() -> String {
    "claude".to_string()
}

impl ProjectConfig {
    pub fn resolved_agents(&self) -> Vec<AgentEntry> {
        if !self.agents.is_empty() {
            return self.agents.clone();
        }
        vec![AgentEntry {
            name: "claude".to_string(),
            backend: "claude".to_string(),
            mode: self.agent.mode.clone(),
            model: self.agent.model.clone(),
            allowed_tools: self.agent.allowed_tools.clone(),
            max_turns: self.agent.max_turns,
            acp: None,
            tmux: None,
        }]
    }

    pub fn default_agent_name(&self) -> String {
        if let Some(ref name) = self.default_agent {
            return name.clone();
        }
        let agents = self.resolved_agents();
        agents.first().map(|a| a.name.clone()).unwrap_or_else(|| "claude".to_string())
    }

    #[allow(dead_code)]
    pub fn find_agent(&self, name: &str) -> Option<AgentEntry> {
        self.resolved_agents().into_iter().find(|a| a.name == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    #[serde(rename = "type")]
    pub platform_type: String,

    #[serde(default)]
    pub options: serde_json::Value,
}

// --- Defaults ---

fn default_language() -> String {
    "en".into()
}

fn default_data_dir() -> PathBuf {
    dirs_next().join(".agentbridge")
}

fn default_allow_all() -> String {
    "*".into()
}

fn default_mode() -> String {
    "default".into()
}

fn dirs_next() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// --- Loading ---

pub fn default_config_path() -> PathBuf {
    dirs_next().join(".agentbridge").join("config.yaml")
}

pub fn load(path: Option<&str>) -> Result<AppConfig> {
    let config_path = path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    if !config_path.exists() {
        anyhow::bail!(
            "Config not found at {}\nRun 'agentbridge init' to create one.",
            config_path.display()
        );
    }

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let config: AppConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    validate(&config)?;
    Ok(config)
}

fn validate(config: &AppConfig) -> Result<()> {
    if config.projects.is_empty() {
        anyhow::bail!("No projects configured. Add at least one [[projects]] entry.");
    }

    for p in &config.projects {
        if p.name.is_empty() {
            anyhow::bail!("Project name cannot be empty");
        }
        if p.platforms.is_empty() {
            anyhow::bail!("Project '{}' has no platforms configured", p.name);
        }
        validate_agents(p)?;
    }

    Ok(())
}

fn validate_agents(project: &ProjectConfig) -> Result<()> {
    let has_old_agent = project.agent.mode != default_mode()
        || project.agent.model.is_some()
        || !project.agent.allowed_tools.is_empty()
        || project.agent.max_turns.is_some();
    let has_new_agents = !project.agents.is_empty();

    if has_old_agent && has_new_agents {
        anyhow::bail!(
            "Project '{}': cannot have both 'agent' and 'agents' fields. \
             Remove the old 'agent:' field and use 'agents:' instead.",
            project.name
        );
    }

    if has_new_agents {
        let mut seen_names = std::collections::HashSet::new();
        for entry in &project.agents {
            if entry.name.is_empty() {
                anyhow::bail!("Project '{}': agent name cannot be empty", project.name);
            }
            if !seen_names.insert(&entry.name) {
                anyhow::bail!(
                    "Project '{}': duplicate agent name '{}'",
                    project.name,
                    entry.name
                );
            }
            if entry.backend == "acp" && entry.acp.is_none() {
                anyhow::bail!(
                    "Project '{}': agent '{}' has backend 'acp' but no 'acp:' config",
                    project.name,
                    entry.name
                );
            }
            if entry.backend == "tmux" && entry.tmux.is_none() {
                anyhow::bail!(
                    "Project '{}': agent '{}' has backend 'tmux' but no 'tmux:' config",
                    project.name,
                    entry.name
                );
            }
        }

        if let Some(ref default_name) = project.default_agent {
            if !project.agents.iter().any(|a| a.name == *default_name) {
                anyhow::bail!(
                    "Project '{}': default_agent '{}' not found in agents list. Available: {}",
                    project.name,
                    default_name,
                    project.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let yaml = r#"
language: en
projects:
  - name: test-project
    work_dir: /tmp/test
    platforms:
      - type: telegram
        options:
          token: "123:ABC"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].name, "test-project");
        // serde default gives "default" for mode
        assert_eq!(config.projects[0].agent.mode, "default");
        assert_eq!(config.projects[0].allow_from, "*");
        assert_eq!(config.language, "en");
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
language: zh
projects:
  - name: my-app
    work_dir: /home/user/app
    agent:
      mode: yolo
      model: claude-sonnet-4-20250514
      max_turns: 10
    platforms:
      - type: telegram
        options:
          token: "123:ABC"
      - type: discord
        options:
          token: "discord-token"
          guild_id: "999"
    allow_from: "111,222"
    rate_limit:
      max_messages: 5
      window_secs: 30
    banned_words: ["secret", "password"]
    commands:
      - name: test
        description: Run tests
        prompt: Run the test suite
    auto_compress:
      enabled: true
      max_tokens: 8000
    aliases:
      - trigger: "新建"
        command: "/new"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let p = &config.projects[0];
        assert_eq!(p.agent.mode, "yolo");
        assert_eq!(p.agent.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(p.platforms.len(), 2);
        assert_eq!(p.allow_from, "111,222");
        assert_eq!(p.rate_limit.max_messages, 5);
        assert_eq!(p.banned_words, vec!["secret", "password"]);
        assert_eq!(p.commands.len(), 1);
        assert!(p.auto_compress.enabled);
        assert_eq!(p.auto_compress.max_tokens, 8000);
        assert_eq!(p.aliases.len(), 1);
        assert_eq!(p.aliases[0].trigger, "新建");
    }

    #[test]
    fn validate_empty_projects_fails() {
        let config = AppConfig {
            language: "en".to_string(),
            data_dir: PathBuf::from("/tmp"),
            projects: vec![],
            webhook: None,
            hook_receiver: None,
            log: None,
        };
        assert!(validate(&config).is_err());
    }

    #[test]
    fn validate_empty_name_fails() {
        let yaml = r#"
projects:
  - name: ""
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "x"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(validate(&config).is_err());
    }

    #[test]
    fn validate_no_platforms_fails() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    platforms: []
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(validate(&config).is_err());
    }

    #[test]
    fn defaults_are_sensible() {
        let yaml = r#"
projects:
  - name: x
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let p = &config.projects[0];
        assert_eq!(p.rate_limit.max_messages, 20);
        assert_eq!(p.rate_limit.window_secs, 60);
        assert!(!p.auto_compress.enabled);
        assert_eq!(p.auto_compress.max_tokens, 12000);
        assert!(p.aliases.is_empty());
        assert!(p.banned_words.is_empty());
    }

    // ---- Multi-agent config tests ----

    #[test]
    fn parse_multi_agent_config() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
        mode: default
        model: claude-sonnet-4-20250514
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
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        validate(&config).unwrap();
        let p = &config.projects[0];
        assert_eq!(p.agents.len(), 2);
        assert_eq!(p.agents[0].name, "claude");
        assert_eq!(p.agents[0].backend, "claude");
        assert_eq!(p.agents[1].name, "kiro");
        assert_eq!(p.agents[1].backend, "acp");
        assert_eq!(p.agents[1].acp.as_ref().unwrap().command, "kiro-cli");
        assert_eq!(p.default_agent, Some("kiro".to_string()));
    }

    #[test]
    fn resolved_agents_from_old_format() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agent:
      mode: yolo
      model: claude-opus-4-20250514
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let p = &config.projects[0];
        let agents = p.resolved_agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "claude");
        assert_eq!(agents[0].backend, "claude");
        assert_eq!(agents[0].mode, "yolo");
        assert_eq!(agents[0].model.as_deref(), Some("claude-opus-4-20250514"));
    }

    #[test]
    fn resolved_agents_from_new_format() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let p = &config.projects[0];
        let agents = p.resolved_agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "kiro");
    }

    #[test]
    fn default_agent_name_explicit() {
        let yaml = r#"
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
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.projects[0].default_agent_name(), "kiro");
    }

    #[test]
    fn default_agent_name_first_in_list() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
      - name: claude
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.projects[0].default_agent_name(), "kiro");
    }

    #[test]
    fn default_agent_name_old_format_is_claude() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.projects[0].default_agent_name(), "claude");
    }

    #[test]
    fn find_agent_by_name() {
        let yaml = r#"
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
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let p = &config.projects[0];
        assert!(p.find_agent("claude").is_some());
        assert!(p.find_agent("kiro").is_some());
        assert!(p.find_agent("nonexistent").is_none());
    }

    #[test]
    fn validate_duplicate_agent_name_fails() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
      - name: claude
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&config).unwrap_err();
        assert!(err.to_string().contains("duplicate agent name"));
    }

    #[test]
    fn validate_invalid_default_agent_fails() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: claude
        backend: claude
    default_agent: nonexistent
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&config).unwrap_err();
        assert!(err.to_string().contains("not found in agents list"));
    }

    #[test]
    fn validate_acp_without_config_fails() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agents:
      - name: kiro
        backend: acp
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&config).unwrap_err();
        assert!(err.to_string().contains("no 'acp:' config"));
    }

    #[test]
    fn validate_both_agent_and_agents_fails() {
        let yaml = r#"
projects:
  - name: test
    work_dir: /tmp
    agent:
      mode: yolo
    agents:
      - name: claude
        backend: claude
    platforms:
      - type: telegram
        options:
          token: "t"
"#;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&config).unwrap_err();
        assert!(err.to_string().contains("cannot have both"));
    }

    #[test]
    fn agent_entry_to_agent_config() {
        let entry = AgentEntry {
            name: "test".into(),
            backend: "claude".into(),
            mode: "yolo".into(),
            model: Some("model-x".into()),
            allowed_tools: vec!["tool1".into()],
            max_turns: Some(30),
            acp: None,
            tmux: None,
        };
        let config = entry.to_agent_config();
        assert_eq!(config.mode, "yolo");
        assert_eq!(config.model.as_deref(), Some("model-x"));
        assert_eq!(config.allowed_tools, vec!["tool1"]);
        assert_eq!(config.max_turns, Some(30));
    }
}
