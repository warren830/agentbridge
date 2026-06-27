//! agentbridge - Bridge Claude Code to chat apps.
//!
//! Usage:
//!   agentbridge              Start the bridge
//!   agentbridge init         Interactive setup wizard
//!   agentbridge doctor       Check configuration health

mod agent;
mod core;
mod config;
mod cron;
mod daemon;
mod dedup;
mod engine;
mod gateway;
mod hook_receiver;
mod hook_route;
mod lock;
mod outgoing_ratelimit;
mod platforms;
mod ratelimit;
mod relay;
mod speech;
mod sync;
mod webhook;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use std::io::{self, Write};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "agentbridge", version, about = "Bridge Claude Code to chat apps")]
struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the bridge (default)
    Run {
        /// Gateway URL to connect to (e.g. ws://gateway:9900)
        #[arg(long)]
        gateway: Option<String>,
        /// Token for gateway authentication
        #[arg(long)]
        gateway_token: Option<String>,
        /// Instance name shown in gateway dashboard
        #[arg(long, default_value = "default")]
        instance_name: String,
    },
    /// Interactive setup wizard
    Init,
    /// Check configuration health
    Doctor,
    /// Install the Claude Code Stop/PostToolUse hook into ~/.claude/settings.json
    HookInstall {
        /// Port the hook receiver listens on (must match config; default 9123)
        #[arg(long)]
        port: Option<u16>,
    },
    /// Manage background service
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Bot-to-bot relay messaging
    Relay {
        #[command(subcommand)]
        action: RelayAction,
    },
    /// Sync Claude Code sessions with remote machine
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Start the web gateway server
    Gateway {
        /// Port to listen on
        #[arg(long, default_value = "9900")]
        port: u16,
        /// API token for frontend authentication
        #[arg(long, env = "AGENTPUSH_API_TOKEN")]
        token: String,
        /// Token required for instance registration
        #[arg(long, env = "AGENTPUSH_GATEWAY_TOKEN")]
        gateway_token: Option<String>,
        /// Directory with static files (Nuxt build output)
        #[arg(long)]
        static_dir: Option<String>,
    },
}

#[derive(Subcommand)]
enum RelayAction {
    /// Send a message to another project's bot
    Send {
        /// Target project name
        #[arg(long)]
        to: String,
        /// Message to send
        message: String,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// Pull sessions from remote machine to local
    Pull,
    /// Push sessions from local to remote machine
    Push,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Install as systemd user service
    Install,
    /// Uninstall the systemd user service
    Uninstall,
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
    /// View daemon logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("agentbridge=info".parse()?))
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Run {
        gateway: None,
        gateway_token: None,
        instance_name: "default".to_string(),
    }) {
        Commands::Run { gateway, gateway_token, instance_name } => {
            run_server(cli.config, gateway, gateway_token, instance_name).await
        }
        Commands::Init => run_init().await,
        Commands::Doctor => run_doctor(cli.config).await,
        Commands::HookInstall { port } => run_hook_install(cli.config, port).await,
        Commands::Daemon { action } => run_daemon(action),
        Commands::Relay { action } => run_relay(action).await,
        Commands::Sync { action } => run_sync(action),
        Commands::Gateway {
            port,
            token,
            gateway_token,
            static_dir,
        } => {
            tracing::info!(port, "starting gateway server");
            gateway::server::start(
                port,
                token.clone(),
                gateway_token.unwrap_or(token),
                static_dir,
            )
            .await
        }
    }
}

async fn run_server(
    config_path: Option<String>,
    gateway_url: Option<String>,
    gateway_token: Option<String>,
    instance_name: String,
) -> anyhow::Result<()> {
    // Single-instance guard
    let _lock = lock::acquire()?;

    let cfg = config::load(config_path.as_deref())?;

    tracing::info!(
        projects = cfg.projects.len(),
        "agentbridge v{} starting",
        env!("CARGO_PKG_VERSION")
    );

    // Start webhook server if configured
    let (webhook_tx, mut _webhook_rx) = tokio::sync::mpsc::channel::<webhook::WebhookEvent>(64);
    if let Some(ref wh_config) = cfg.webhook {
        webhook::start(wh_config, webhook_tx).await?;
    }

    // Start relay server (Unix socket for bot-to-bot messaging)
    let (relay_tx, mut _relay_rx) = tokio::sync::mpsc::channel::<relay::RelayEnvelope>(64);
    let relay_server = relay::RelayServer::new(relay_tx);
    relay_server.start().await?;

    // Shared hook route registry: one localhost hook receiver routes Claude
    // Code Stop/PostToolUse hooks to whichever project engine bound the
    // matching work_dir, so all engines must share the same registry.
    let hook_route = Arc::new(hook_route::HookRouteRegistry::new());

    // Use the NEW engine (architecture rewrite)
    let mut engines = Vec::new();

    for project in &cfg.projects {
        let mut eng = engine::Engine::new(project.clone(), cfg.clone());
        eng.set_hook_route(Arc::clone(&hook_route));
        eng.start().await?;
        tracing::info!(name = %project.name, "project ready");
        engines.push(eng);
    }

    // Start the hook receiver (localhost) so bridged tmux+hook sessions can
    // relay Claude Code's Stop hook back as chat text. Default port 9123.
    let hook_port = cfg
        .hook_receiver
        .as_ref()
        .map(|h| h.port)
        .unwrap_or_else(config::default_hook_receiver_port);
    if let Err(e) = hook_receiver::start(hook_port, Arc::clone(&hook_route)).await {
        tracing::warn!(error = %e, port = hook_port, "hook receiver failed to start");
    }

    tracing::info!("all projects started, waiting for messages...");

    // Resolve display names for any unnamed sessions (e.g. Discord threads).
    for eng in &engines {
        eng.backfill_session_names().await;
    }

    // Connect to gateway if configured
    if let Some(ref gw_url) = gateway_url {
        // If user passes base URL (ws://host:port), append /gateway/ws
        // If they pass the full path, use as-is
        let gw_ws_url = if gw_url.ends_with("/gateway/ws") {
            gw_url.clone()
        } else {
            format!("{}/gateway/ws", gw_url.trim_end_matches('/'))
        };
        let token = gateway_token.clone().unwrap_or_default();
        let instance_id = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // Collect project/session info from SessionManager for registration
        let projects: Vec<gateway::protocol::ProjectInfo> = cfg.projects.iter().map(|p| {
            // Read sessions from disk (same path SessionManager uses)
            let encoded = p.work_dir.to_string_lossy().replace('/', "-");
            let state_path = cfg.data_dir.join("sessions").join(&encoded).join("state.json");
            let sessions = if let Ok(data) = std::fs::read_to_string(&state_path) {
                if let Ok(state) = serde_json::from_str::<serde_json::Value>(&data) {
                    state["sessions"].as_object()
                        .map(|map| {
                            map.values().filter_map(|s| {
                                Some(gateway::protocol::SessionInfo {
                                    session_key: s["key"].as_str()?.to_string(),
                                    session_id: s["id"].as_str()?.to_string(),
                                    name: s["name"].as_str().map(String::from),
                                    agent_session_id: s["agent_session_id"].as_str().map(String::from),
                                    updated_at: s["updated_at"].as_str()
                                        .and_then(|t| t.parse().ok())
                                        .unwrap_or_else(chrono::Utc::now),
                                    is_busy: false,
                                })
                            }).collect()
                        })
                        .unwrap_or_default()
                } else { vec![] }
            } else { vec![] };

            gateway::protocol::ProjectInfo {
                name: p.name.clone(),
                work_dir: p.work_dir.display().to_string(),
                sessions,
            }
        }).collect();

        let (event_tx, mut cmd_rx) = gateway::client::start(
            gw_ws_url,
            token,
            instance_id.clone(),
            instance_name.clone(),
            projects,
        );

        // Get handler and platforms from first engine for message injection
        let gw_handler = engines[0].handler();
        let gw_platforms = engines[0].platforms_map();

        // Handle incoming commands from gateway → inject into engine
        if !gw_platforms.is_empty() {
            let platforms = gw_platforms;
            tokio::spawn(async move {
                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        gateway::protocol::GatewayMessage::SendMessage { session_key, text, from } => {
                            tracing::info!(session_key = %session_key, from = %from, "gateway: injecting message");

                            let parts: Vec<&str> = session_key.splitn(2, ':').collect();
                            let platform_name = parts.first().copied().unwrap_or("web");
                            let channel_id = parts.get(1).copied().unwrap_or("");

                            // Find the correct platform by name
                            let target_platform = match platforms.get(platform_name) {
                                Some(p) => Arc::clone(p),
                                None => {
                                    tracing::warn!(platform = %platform_name, "gateway: platform not found, using first");
                                    match platforms.values().next() {
                                        Some(p) => Arc::clone(p),
                                        None => continue,
                                    }
                                }
                            };

                            let reply_ctx: Box<dyn crate::core::platform::ReplyCtx> = match platform_name {
                                "discord" => Box::new(crate::platforms::discord::types::DiscordReplyCtx {
                                    channel_id: channel_id.to_string(),
                                    message_id: None,
                                    thread_id: Some(channel_id.to_string()),
                                }),
                                "telegram" => {
                                    let chat_id = channel_id.parse::<i64>().unwrap_or(0);
                                    Box::new(crate::platforms::telegram::types::TelegramReplyCtx {
                                        chat_id,
                                        thread_id: None,
                                        message_id: None,
                                    })
                                }
                                _ => Box::new(crate::cron::CronReplyCtx::new(session_key.clone())),
                            };

                            // Send a lightweight notification to the chat platform so
                            // users on Discord/Telegram see that someone on the web
                            // dashboard just said something. The actual agent run
                            // is triggered by the IncomingMessage injection below.
                            if platform_name == "discord" || platform_name == "telegram" {
                                let preview: String = text.chars().take(200).collect();
                                let truncated = if text.chars().count() > 200 { "…" } else { "" };
                                let notice = format!("💬 [web · {}] {}{}", from, preview, truncated);
                                // Build a throwaway ctx so the reply goes to the right
                                // channel/thread without affecting the ctx used for injection.
                                let notice_ctx: Box<dyn crate::core::platform::ReplyCtx> = match platform_name {
                                    "discord" => Box::new(crate::platforms::discord::types::DiscordReplyCtx {
                                        channel_id: channel_id.to_string(),
                                        message_id: None,
                                        thread_id: Some(channel_id.to_string()),
                                    }),
                                    "telegram" => {
                                        let chat_id = channel_id.parse::<i64>().unwrap_or(0);
                                        Box::new(crate::platforms::telegram::types::TelegramReplyCtx {
                                            chat_id,
                                            thread_id: None,
                                            message_id: None,
                                        })
                                    }
                                    _ => unreachable!(),
                                };
                                let platform_for_notice = Arc::clone(&target_platform);
                                tokio::spawn(async move {
                                    if let Err(e) = platform_for_notice.reply(notice_ctx.as_ref(), &notice).await {
                                        tracing::warn!(error = %e, "gateway: failed to send web notice to chat");
                                    }
                                });
                            }

                            // is_group=true + channel_id ensures make_session_key
                            // produces "discord:<channel_id>" matching the original session
                            let msg = crate::core::message::IncomingMessage {
                                id: format!("gw-{}", chrono::Utc::now().timestamp_millis()),
                                from: from.clone(),
                                from_name: Some(from),
                                text,
                                images: vec![],
                                files: vec![],
                                voice: None,
                                is_group: true,
                                channel_id: Some(channel_id.to_string()),
                                channel_name: None,
                                reply_ctx,
                            };

                            gw_handler(target_platform, msg);
                        }
                        gateway::protocol::GatewayMessage::Command { session_key, command, args } => {
                            tracing::info!(session_key = %session_key, command = %command, "gateway: executing command");
                            let text = if let Some(a) = args {
                                format!("/{} {}", command, a)
                            } else {
                                format!("/{}", command)
                            };

                            let parts: Vec<&str> = session_key.splitn(2, ':').collect();
                            let platform_name = parts.first().copied().unwrap_or("web");
                            let channel_id = parts.get(1).copied().unwrap_or("");

                            let target_platform = platforms.get(platform_name)
                                .or_else(|| platforms.values().next())
                                .map(Arc::clone);
                            let Some(target_platform) = target_platform else { continue };

                            let reply_ctx: Box<dyn crate::core::platform::ReplyCtx> = match platform_name {
                                "discord" => Box::new(crate::platforms::discord::types::DiscordReplyCtx {
                                    channel_id: channel_id.to_string(),
                                    message_id: None,
                                    thread_id: Some(channel_id.to_string()),
                                }),
                                "telegram" => {
                                    let chat_id = channel_id.parse::<i64>().unwrap_or(0);
                                    Box::new(crate::platforms::telegram::types::TelegramReplyCtx {
                                        chat_id,
                                        thread_id: None,
                                        message_id: None,
                                    })
                                }
                                _ => Box::new(crate::cron::CronReplyCtx::new(session_key.clone())),
                            };

                            let msg = crate::core::message::IncomingMessage {
                                id: format!("gw-cmd-{}", chrono::Utc::now().timestamp_millis()),
                                from: "web".to_string(),
                                from_name: Some("web".to_string()),
                                text,
                                images: vec![],
                                files: vec![],
                                voice: None,
                                is_group: true,
                                channel_id: Some(channel_id.to_string()),
                                channel_name: None,
                                reply_ctx,
                            };

                            gw_handler(target_platform, msg);
                        }
                        _ => {}
                    }
                }
            });
        }

        // Forward engine events to gateway
        let event_tx_clone = event_tx.clone();
        let instance_id_clone = instance_id.clone();
        let mut event_rx = engines[0].subscribe_events();
        tracing::info!("gateway: event forwarder started, subscribed to engine broadcast");
        tokio::spawn(async move {
            while let Ok((session_key, event)) = event_rx.recv().await {
                tracing::info!(session_key = %session_key, "gateway: forwarding event to gateway client");
                use gateway::protocol::*;

                let payload = match &event {
                    crate::core::event::AgentEvent::Text { content } =>
                        AgentEventPayload::Text { content: content.clone() },
                    crate::core::event::AgentEvent::Thinking { content } =>
                        AgentEventPayload::Thinking { content: content.clone() },
                    crate::core::event::AgentEvent::ToolUse { id, tool, input } =>
                        AgentEventPayload::ToolUse { id: id.clone(), tool: tool.clone(), input: input.clone() },
                    crate::core::event::AgentEvent::ToolResult { id, output, is_error } =>
                        AgentEventPayload::ToolResult { id: id.clone(), output: output.clone(), is_error: *is_error },
                    crate::core::event::AgentEvent::Result { content, input_tokens, output_tokens, .. } =>
                        AgentEventPayload::Result { content: content.clone(), input_tokens: *input_tokens, output_tokens: *output_tokens },
                    crate::core::event::AgentEvent::Error { message } =>
                        AgentEventPayload::Error { message: message.clone() },
                    crate::core::event::AgentEvent::PermissionRequest { request_id, tool, input, .. } =>
                        AgentEventPayload::PermissionRequest { request_id: request_id.clone(), tool: tool.clone(), input: input.to_string() },
                    _ => continue,
                };

                let msg = InstanceMessage::Event {
                    instance_id: instance_id_clone.clone(),
                    event: RelayedEvent {
                        session_key,
                        event: payload,
                    },
                };
                match event_tx_clone.send(msg).await {
                    Ok(_) => tracing::info!("gateway: event sent to client"),
                    Err(e) => tracing::error!("gateway: failed to send event to client: {}", e),
                }
            }
        });

        tracing::info!(gateway = %gw_url, instance = %instance_id, "connected to gateway");
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down...");

    for eng in &engines {
        eng.stop().await?;
    }
    relay_server.cleanup();

    Ok(())
}

fn run_sync(action: SyncAction) -> anyhow::Result<()> {
    let cfg = config::load(None)?;

    // Find first project with sync configured
    let sync_config = cfg
        .projects
        .iter()
        .find_map(|p| p.sync.as_ref())
        .ok_or_else(|| anyhow::anyhow!(
            "No sync configured.\nAdd to config.yaml:\n\nsync:\n  remote: \"your-mac:~/.claude/\""
        ))?;

    let direction = match action {
        SyncAction::Pull => sync::Direction::Pull,
        SyncAction::Push => sync::Direction::Push,
    };

    let result = sync::run_sync(sync_config, direction)?;
    println!("{}", result);

    if !result.success {
        std::process::exit(1);
    }
    Ok(())
}

async fn run_relay(action: RelayAction) -> anyhow::Result<()> {
    match action {
        RelayAction::Send { to, message } => {
            let response = relay::send_relay(&to, &message).await?;
            if response.ok {
                println!("{}", response.reply);
            } else {
                let err = response.error.unwrap_or_else(|| "unknown error".to_string());
                eprintln!("Relay error: {}", err);
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

async fn run_init() -> anyhow::Result<()> {
    println!("agentbridge init - Interactive setup\n");

    // 1. Telegram bot token
    let token = prompt_input("Telegram bot token: ")?;
    if token.is_empty() {
        anyhow::bail!("Bot token is required.");
    }

    // 2. Work directory
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let work_dir = prompt_input(&format!("Work directory [{}]: ", cwd))?;
    let work_dir = if work_dir.is_empty() { cwd } else { work_dir };

    // 3. Mode
    let mode = prompt_input("Mode (default/yolo/plan/auto) [default]: ")?;
    let mode = if mode.is_empty() {
        "default".to_string()
    } else {
        mode
    };

    // 4. Agent selection
    println!("\nWhich agent(s) should this project use?");
    println!("  1. Claude Code only");
    println!("  2. Kiro CLI only");
    println!("  3. Both (choose a default below)");
    let choice = prompt_input("Choice [1]: ")?;
    let choice = if choice.is_empty() { "1".to_string() } else { choice };

    let agents_section = match choice.as_str() {
        "2" => r#"    agents:
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
          display_name: "Kiro"
"#
        .to_string(),
        "3" => {
            println!("\nDefault agent when users send messages?");
            println!("  1. claude");
            println!("  2. kiro");
            let d = prompt_input("Default [1]: ")?;
            let default_name = if d == "2" { "kiro" } else { "claude" };
            format!(
                r#"    agents:
      - name: claude
        backend: claude
        mode: "{mode}"
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
          display_name: "Kiro"
    default_agent: "{default_name}"
"#,
                mode = mode,
                default_name = default_name,
            )
        }
        _ => format!(
            r#"    agent:
      mode: "{mode}"
"#,
        ),
    };

    let config_content = format!(
        r#"language: en
projects:
  - name: my-project
    work_dir: "{work_dir}"
{agents_section}    platforms:
      - type: telegram
        options:
          token: "{token}"
    allow_from: "*"
"#,
    );

    // Write to ~/.agentbridge/config.yaml
    let config_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".agentbridge");
    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.yaml");
    std::fs::write(&config_path, &config_content)?;

    println!("\nConfig written to: {}", config_path.display());
    println!("Run `agentbridge` to start the bridge.");

    Ok(())
}

/// The hook relay script, embedded so `hook-install` is self-contained — it
/// writes this out to a stable path and points Claude Code's settings at it.
const HOOK_SCRIPT: &str = include_str!("../scripts/agentbridge_hook.py");

/// Install (merge) the Claude Code Stop/PostToolUse hook into the user's global
/// `~/.claude/settings.json`.
///
/// Merge-not-overwrite (BR-12): existing settings and other hooks are
/// preserved; re-running is idempotent (the entry is matched by command and not
/// duplicated). The configured port (BR-13) is baked into the hook command.
async fn run_hook_install(config_path: Option<String>, port_override: Option<u16>) -> anyhow::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?;

    // Resolve the port: CLI flag > config hook_receiver.port > default 9123.
    let port = match port_override {
        Some(p) => p,
        None => config::load(config_path.as_deref())
            .ok()
            .and_then(|c| c.hook_receiver.map(|h| h.port))
            .unwrap_or_else(config::default_hook_receiver_port),
    };

    // Write the relay script to a stable path and reference it from settings.
    let ab_dir = home.join(".agentbridge");
    tokio::fs::create_dir_all(&ab_dir).await?;
    let script_path = ab_dir.join("agentbridge_hook.py");
    tokio::fs::write(&script_path, HOOK_SCRIPT).await?;
    set_executable(&script_path).await;

    let command = format!(
        "python3 {} {}",
        script_path.display(),
        port
    );

    // Read existing settings (or start fresh), merge our hook, write back.
    let settings_dir = home.join(".claude");
    tokio::fs::create_dir_all(&settings_dir).await?;
    let settings_path = settings_dir.join("settings.json");

    let mut settings: serde_json::Value = match tokio::fs::read_to_string(&settings_path).await {
        Ok(content) if !content.trim().is_empty() => serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("~/.claude/settings.json is not valid JSON: {}", e))?,
        _ => serde_json::json!({}),
    };

    if !settings.is_object() {
        anyhow::bail!("~/.claude/settings.json must be a JSON object");
    }

    let mut added = false;
    for event in ["Stop", "PostToolUse"] {
        if merge_hook_event(&mut settings, event, &command) {
            added = true;
        }
    }

    // Atomic write: write to a temp file in the same dir, then rename over.
    let serialized = serde_json::to_string_pretty(&settings)?;
    let tmp_path = settings_dir.join("settings.json.agentbridge.tmp");
    tokio::fs::write(&tmp_path, serialized.as_bytes()).await?;
    tokio::fs::rename(&tmp_path, &settings_path).await?;

    if added {
        println!("Installed agentbridge hook (port {}).", port);
    } else {
        println!("agentbridge hook already installed (port {}); no change.", port);
    }
    println!("  script:   {}", script_path.display());
    println!("  settings: {}", settings_path.display());
    Ok(())
}

/// Ensure a hooks event array (e.g. `hooks.Stop`) contains an entry running our
/// command. Returns `true` if an entry was added, `false` if it was already
/// present (idempotency). The entry uses an empty matcher so it fires for all.
fn merge_hook_event(settings: &mut serde_json::Value, event: &str, command: &str) -> bool {
    let Some(root) = settings.as_object_mut() else {
        return false;
    };
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let hooks_obj = match hooks.as_object_mut() {
        Some(o) => o,
        None => {
            // `hooks` exists but isn't an object — leave foreign config alone.
            return false;
        }
    };
    let arr = hooks_obj
        .entry(event.to_string())
        .or_insert_with(|| serde_json::json!([]));
    let Some(arr) = arr.as_array_mut() else {
        return false;
    };

    // Already installed? Match on our command appearing in any nested hook.
    let already = arr.iter().any(|group| {
        group
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|inner| {
                inner.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()) == Some(command)
                })
            })
            .unwrap_or(false)
    });
    if already {
        return false;
    }

    arr.push(serde_json::json!({
        "matcher": "",
        "hooks": [ { "type": "command", "command": command } ]
    }));
    true
}

/// Best-effort chmod +x on the relay script (Unix). On other platforms this is
/// a no-op; python3 is invoked explicitly so the bit is not strictly required.
async fn set_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = tokio::fs::metadata(path).await {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = tokio::fs::set_permissions(path, perms).await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Read a line from stdin with a prompt.
fn prompt_input(prompt: &str) -> anyhow::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

async fn run_doctor(config_path: Option<String>) -> anyhow::Result<()> {
    println!("agentbridge doctor\n");

    let mut all_ok = true;

    // 1. Check config file exists and is valid YAML
    let path = config_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(config::default_config_path);

    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_yaml::from_str::<serde_yaml::Value>(&content) {
                Ok(_) => println!("  Config file ({}) is valid YAML", path.display()),
                Err(e) => {
                    println!("  Config file ({}) has invalid YAML: {}", path.display(), e);
                    all_ok = false;
                }
            },
            Err(e) => {
                println!("  Cannot read config file ({}): {}", path.display(), e);
                all_ok = false;
            }
        }
    } else {
        println!("  Config file not found: {}", path.display());
        all_ok = false;
    }

    // 2. Check claude binary in PATH
    match std::process::Command::new("which").arg("claude").output() {
        Ok(output) if output.status.success() => {
            let claude_path = String::from_utf8_lossy(&output.stdout);
            println!("  claude CLI found: {}", claude_path.trim());
        }
        _ => {
            println!("  claude CLI not found in PATH");
            all_ok = false;
        }
    }

    // 3. Check configured agents (default + ACP command reachability).
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_yaml::from_str::<config::AppConfig>(&content) {
                for project in &cfg.projects {
                    let default_name = project.default_agent_name();
                    let agents = project.resolved_agents();
                    let default = agents.iter().find(|a| a.name == default_name);
                    let summary = match default {
                        Some(a) if a.backend == "acp" => {
                            let cmd = a
                                .acp
                                .as_ref()
                                .map(|c| c.command.as_str())
                                .unwrap_or("?");
                            format!(
                                "Default agent: {} (backend: acp, command: {})",
                                a.name, cmd
                            )
                        }
                        Some(a) => {
                            format!("Default agent: {} (backend: {})", a.name, a.backend)
                        }
                        None => format!("Default agent: {} (MISSING from agents list)", default_name),
                    };
                    println!("  [{}] {}", project.name, summary);
                    if default.is_none() {
                        all_ok = false;
                    }

                    for a in &agents {
                        if a.backend == "acp" {
                            if let Some(ref acp) = a.acp {
                                if !crate::agent::acp::session::command_on_path(&acp.command) {
                                    println!(
                                        "  [{}/{}] ACP command '{}' not in PATH",
                                        project.name, a.name, acp.command
                                    );
                                    all_ok = false;
                                } else {
                                    println!(
                                        "  [{}/{}] ACP command '{}' found",
                                        project.name, a.name, acp.command
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Check platform tokens: format + live API reachability
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_yaml::from_str::<config::AppConfig>(&content) {
                for project in &cfg.projects {
                    for platform in &project.platforms {
                        let label = format!("{}/{}", project.name, platform.platform_type);
                        if platform.platform_type == "telegram" {
                            if let Some(token) = platform.options.get("token").and_then(|v| v.as_str()) {
                                if !is_valid_telegram_token(token) {
                                    println!("  [{}] Telegram token format: invalid (expected number:alphanumeric)", label);
                                    all_ok = false;
                                    continue;
                                }
                                match check_telegram_token(token).await {
                                    Ok(username) => println!("  [{}] Telegram: @{} reachable", label, username),
                                    Err(e) => {
                                        println!("  [{}] Telegram: {}", label, e);
                                        all_ok = false;
                                    }
                                }
                            } else {
                                println!("  [{}] Telegram token: missing", label);
                                all_ok = false;
                            }
                        } else if platform.platform_type == "discord" {
                            if let Some(token) = platform.options.get("token").and_then(|v| v.as_str()) {
                                match check_discord_token(token).await {
                                    Ok(username) => println!("  [{}] Discord: @{} reachable", label, username),
                                    Err(e) => {
                                        println!("  [{}] Discord: {}", label, e);
                                        all_ok = false;
                                    }
                                }
                            } else {
                                println!("  [{}] Discord token: missing", label);
                                all_ok = false;
                            }
                        }
                    }
                }
            }
        }
    }

    println!();
    if all_ok {
        println!("All checks passed.");
    } else {
        println!("Some checks failed. Fix the issues above and try again.");
    }

    Ok(())
}

fn run_daemon(action: DaemonAction) -> anyhow::Result<()> {
    match action {
        DaemonAction::Install => daemon::install_systemd_service(),
        DaemonAction::Uninstall => daemon::uninstall_systemd_service(),
        DaemonAction::Start => daemon::daemon_start(),
        DaemonAction::Stop => daemon::daemon_stop(),
        DaemonAction::Status => daemon::daemon_status(),
        DaemonAction::Logs { follow } => daemon::daemon_logs(follow),
    }
}

/// Validate Telegram bot token format: <number>:<alphanumeric+special>
fn is_valid_telegram_token(token: &str) -> bool {
    let parts: Vec<&str> = token.splitn(2, ':').collect();
    if parts.len() != 2 {
        return false;
    }
    let numeric_part = parts[0];
    let alpha_part = parts[1];
    !numeric_part.is_empty()
        && numeric_part.chars().all(|c| c.is_ascii_digit())
        && !alpha_part.is_empty()
        && alpha_part.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Call Telegram Bot API `getMe` to verify the token is live. Returns the
/// bot's username on success.
async fn check_telegram_token(token: &str) -> anyhow::Result<String> {
    let url = format!("https://api.telegram.org/bot{}/getMe", token);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client.get(&url).send().await
        .map_err(|e| anyhow::anyhow!("network error: {}", e))?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("bad response: {}", e))?;
    if !status.is_success() || body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let desc = body.get("description").and_then(|v| v.as_str()).unwrap_or("unknown error");
        anyhow::bail!("API rejected token ({}): {}", status.as_u16(), desc);
    }
    let username = body.get("result")
        .and_then(|r| r.get("username"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(username)
}

/// Call Discord `users/@me` endpoint to verify the bot token. Returns the
/// bot's username#discriminator on success.
async fn check_discord_token(token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client.get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("network error: {}", e))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("API rejected token (401 Unauthorized)");
    }
    if !status.is_success() {
        anyhow::bail!("API returned HTTP {}", status.as_u16());
    }
    let body: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("bad response: {}", e))?;
    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("unknown");
    Ok(username.to_string())
}

#[cfg(test)]
mod hook_install_tests {
    use super::*;

    fn count_command_entries(settings: &serde_json::Value, event: &str, command: &str) -> usize {
        settings["hooks"][event]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|g| {
                        g.get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|inner| {
                                inner.iter().any(|h| {
                                    h.get("command").and_then(|c| c.as_str()) == Some(command)
                                })
                            })
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn merge_into_empty_settings_adds_entry() {
        let mut settings = serde_json::json!({});
        let cmd = "python3 /home/me/.agentbridge/agentbridge_hook.py 9123";
        assert!(merge_hook_event(&mut settings, "Stop", cmd));
        assert_eq!(count_command_entries(&settings, "Stop", cmd), 1);
    }

    #[test]
    fn merge_is_idempotent() {
        let mut settings = serde_json::json!({});
        let cmd = "python3 /h/agentbridge_hook.py 9123";
        assert!(merge_hook_event(&mut settings, "Stop", cmd));
        // Second call must NOT add a duplicate and must report no change.
        assert!(!merge_hook_event(&mut settings, "Stop", cmd));
        assert_eq!(count_command_entries(&settings, "Stop", cmd), 1);
    }

    #[test]
    fn merge_preserves_existing_hooks_and_settings() {
        // Existing unrelated settings + a foreign Stop hook must be kept.
        let mut settings = serde_json::json!({
            "model": "claude-opus",
            "hooks": {
                "Stop": [
                    { "matcher": "", "hooks": [ { "type": "command", "command": "echo other" } ] }
                ],
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [ { "type": "command", "command": "guard.sh" } ] }
                ]
            }
        });
        let cmd = "python3 /h/agentbridge_hook.py 9123";
        assert!(merge_hook_event(&mut settings, "Stop", cmd));

        // Our entry was added alongside the existing foreign Stop hook.
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "should keep the foreign Stop hook and add ours");
        assert_eq!(count_command_entries(&settings, "Stop", cmd), 1);
        // Unrelated settings untouched.
        assert_eq!(settings["model"], serde_json::json!("claude-opus"));
        assert!(settings["hooks"]["PreToolUse"].is_array());
    }

    #[test]
    fn merge_handles_both_events() {
        let mut settings = serde_json::json!({});
        let cmd = "python3 /h/agentbridge_hook.py 9123";
        assert!(merge_hook_event(&mut settings, "Stop", cmd));
        assert!(merge_hook_event(&mut settings, "PostToolUse", cmd));
        assert_eq!(count_command_entries(&settings, "Stop", cmd), 1);
        assert_eq!(count_command_entries(&settings, "PostToolUse", cmd), 1);
    }

    #[test]
    fn merge_skips_when_hooks_is_not_object() {
        // A malformed `hooks` (array instead of object) must be left alone.
        let mut settings = serde_json::json!({ "hooks": [] });
        let cmd = "python3 /h/agentbridge_hook.py 9123";
        assert!(!merge_hook_event(&mut settings, "Stop", cmd));
        assert!(settings["hooks"].is_array(), "foreign shape preserved");
    }
}
