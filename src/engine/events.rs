//! Event loop processor for the new engine.
//!
//! Behavior per event kind:
//! - Text → StreamPreview with throttled live edits
//! - ToolUse/Thinking → freeze and detach preview
//! - PermissionRequest → show buttons, BLOCK waiting for user decision
//! - Result → finalize preview
//! - Error → discard preview

#![allow(dead_code)] // EventLoopResult.output_tokens reserved for future telemetry

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::agent::PermissionResponder;
use crate::config::DisplayConfig;
use crate::core::event::AgentEvent;
use crate::core::platform::{Button, PlatformCapabilities, PreviewHandle, ReplyCtx};
use crate::core::streaming::StreamPreview;
use crate::engine::PermissionDecision;

/// Idle timeout for detecting stalled agent sessions (5 minutes).
const EVENT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Outcome of processing the agent event stream.
pub struct EventLoopResult {
    /// The complete response text produced by the agent.
    pub final_text: String,
    /// Agent session ID returned in the Result event (if any).
    pub session_id: Option<String>,
    /// Token usage from the Result event.
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Process agent events, driving the streaming preview and handling
/// tool-use notifications, permission requests, and the final result.
///
/// `perm_rx` receives permission decisions from the message handler when
/// the user responds to a permission prompt (Allow/Deny/Allow All).
/// `agent_stdin` is used to write control_response back to the agent.
/// `approve_all` tracks whether the user has chosen "Allow All" for this session.
#[allow(clippy::too_many_arguments)] // event loop needs many context refs; context struct would not reduce complexity
pub async fn process_agent_events(
    platform: &Arc<dyn PlatformCapabilities>,
    ctx: &dyn ReplyCtx,
    rx: &mut mpsc::Receiver<AgentEvent>,
    perm_rx: &mut mpsc::Receiver<PermissionDecision>,
    responder: &Arc<dyn PermissionResponder>,
    approve_all: &mut bool,
    pending_flag: &Arc<AtomicBool>,
    stopped_flag: &Arc<AtomicBool>,
    stop_typing_in: Option<Box<dyn FnOnce() + Send>>,
    display: &DisplayConfig,
    event_broadcast: &Arc<tokio::sync::broadcast::Sender<(String, crate::core::event::AgentEvent)>>,
    session_key_for_broadcast: &str,
) -> Result<EventLoopResult> {
    let mut preview = StreamPreview::new();
    let mut handle: Option<Box<dyn PreviewHandle>> = None;
    // Typing indicator is already started by the caller (process_and_drain)
    // before agent spawn, so the user sees feedback during spawn time.
    let mut stop_typing = stop_typing_in;
    let mut tool_count: usize = 0;

    let mut result_info = EventLoopResult {
        final_text: String::new(),
        session_id: None,
        input_tokens: 0,
        output_tokens: 0,
    };

    loop {
        // Use idle timeout to detect stalled agent sessions
        let event = match tokio::time::timeout(EVENT_IDLE_TIMEOUT, rx.recv()).await {
            Ok(Some(event)) => event,
            Ok(None) => {
                tracing::warn!("agent event channel closed unexpectedly");
                discard_preview(platform, &mut handle).await;
                break;
            }
            Err(_) => {
                tracing::error!("agent session idle timeout ({:?})", EVENT_IDLE_TIMEOUT);
                discard_preview(platform, &mut handle).await;
                let _ = platform
                    .reply(ctx, "💤 等太久了，Agent 没响应")
                    .await;
                break;
            }
        };

        // Broadcast event for gateway forwarding
        match event_broadcast.send((session_key_for_broadcast.to_string(), event.clone())) {
            Ok(n) => {
                tracing::info!(receivers = n, "event broadcast sent");
            }
            Err(_) => {
                tracing::warn!("event broadcast: no receivers");
            }
        }

        // Check stop signal (set by /stop command)
        if stopped_flag.load(Ordering::Acquire) {
            tracing::info!("event loop stopped by /stop command");
            discard_preview(platform, &mut handle).await;
            break;
        }

        match event {
            // ----- Streamed text -----
            AgentEvent::Text { content } => {
                if content.is_empty() {
                    continue;
                }

                if let Some(stop_fn) = stop_typing.take() {
                    stop_fn();
                }

                let should_update = preview.append_text(&content);

                if should_update {
                    let display = preview.display_text();

                    match &handle {
                        None => {
                            if let Some(updater) = platform.as_message_updater() {
                                match updater.send_preview(ctx, &display).await {
                                    Ok(h) => {
                                        handle = Some(h);
                                        preview.mark_sent();
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "send_preview failed");
                                    }
                                }
                            }
                        }
                        Some(h) => {
                            if let Some(updater) = platform.as_message_updater() {
                                if let Err(e) =
                                    updater.update_preview(h.as_ref(), &display).await
                                {
                                    tracing::warn!(error = %e, "update_preview failed");
                                }
                                preview.mark_sent();
                            }
                        }
                    }
                }
            }

            // ----- Thinking -----
            AgentEvent::Thinking { content } => {
                if display.thinking_messages && !content.is_empty() {
                    freeze_and_detach_preview(platform, &mut preview, &mut handle).await;
                    let max = display.thinking_max_len;
                    let truncated: String = if content.chars().count() > max {
                        format!("{}...", content.chars().take(max).collect::<String>())
                    } else {
                        content
                    };
                    let _ = platform.reply(ctx, &format!("🧠 {}", truncated)).await;
                }
            }

            // ----- Tool use notification -----
            AgentEvent::ToolUse { tool, input, .. } => {
                tool_count += 1;
                if display.tool_messages {
                    freeze_and_detach_preview(platform, &mut preview, &mut handle).await;
                    let max = display.tool_max_len;
                    let formatted_input = match tool.as_str() {
                        "Bash" => {
                            let cmd = if input.chars().count() > max {
                                format!("{}...", input.chars().take(max).collect::<String>())
                            } else {
                                input.clone()
                            };
                            format!("```bash\n{}\n```", cmd)
                        }
                        _ => {
                            if input.chars().count() > max {
                                format!("`{}...`", input.chars().take(max).collect::<String>())
                            } else {
                                format!("`{}`", input)
                            }
                        }
                    };
                    let text = format!("⚡ {} › {}", tool, formatted_input);
                    let _ = platform.reply(ctx, &text).await;
                }
            }

            // ----- Tool result -----
            AgentEvent::ToolResult { output, is_error, .. } => {
                if display.tool_messages && !output.is_empty() {
                    let icon = if is_error { "💥" } else { "✓" };
                    let max = display.tool_max_len;
                    let truncated: String = if output.chars().count() > max {
                        format!("{}...", output.chars().take(max).collect::<String>())
                    } else {
                        output
                    };
                    let _ = platform.reply(ctx, &format!("{} {}", icon, truncated)).await;
                }
            }

            // ----- Permission request (blocks until user decides) -----
            AgentEvent::PermissionRequest {
                request_id,
                tool,
                input,
                options,
            } => {
                // Check approve_all flag first
                if *approve_all {
                    tracing::debug!(request_id = %request_id, tool = %tool, "auto-approving (approve_all)");
                    let _ = responder.respond(&request_id, true).await;
                    let text = format!("⚡ {} › auto", tool);
                    let _ = platform.reply(ctx, &text).await;
                    continue;
                }

                // Freeze and detach preview
                freeze_and_detach_preview(platform, &mut preview, &mut handle).await;

                // Signal that a permission is pending (so handle_pending_permission activates)
                pending_flag.store(true, Ordering::Release);

                // Show permission prompt with buttons
                let input_summary = summarise_json(&input, 200);
                let text = format!(
                    "🔐 需要你的确认\n⚡ 工具: {}\n📋 参数: {}",
                    tool, input_summary
                );

                if let Some(btn_sender) = platform.as_inline_button_sender() {
                    // ACP agents supply their own options — map each to a button.
                    // Claude (options empty) falls back to the default 3-button UI.
                    let buttons = if options.is_empty() {
                        vec![
                            Button {
                                text: "👍 放行".to_string(),
                                callback_data: format!("perm_approve:{}", request_id),
                            },
                            Button {
                                text: "🚫 拦截".to_string(),
                                callback_data: format!("perm_deny:{}", request_id),
                            },
                            Button {
                                text: "⚡ 全部放行".to_string(),
                                callback_data: format!("perm_allow_all:{}", request_id),
                            },
                        ]
                    } else {
                        options
                            .iter()
                            .map(|o| Button {
                                text: o.label.clone(),
                                callback_data: format!("perm_opt:{}:{}", request_id, o.option_id),
                            })
                            .collect()
                    };
                    let _ = btn_sender.send_with_buttons(ctx, &text, &buttons).await;
                } else {
                    let hint = format!(
                        "{}\n\n回复 `allow` 允许 / `deny` 拒绝 / `allow all` 全部允许",
                        text
                    );
                    let _ = platform.reply(ctx, &hint).await;
                }

                // *** BLOCK waiting for user decision ***
                tracing::info!(request_id = %request_id, tool = %tool, "waiting for permission decision");

                let decision = tokio::time::timeout(
                    Duration::from_secs(600), // 10 min timeout for permission
                    perm_rx.recv(),
                )
                .await;

                // Clear pending flag as soon as we get a decision
                pending_flag.store(false, Ordering::Release);

                match decision {
                    Ok(Some(PermissionDecision::Allow)) => {
                        tracing::info!(request_id = %request_id, "permission: allowed");
                        let _ = responder.respond(&request_id, true).await;
                        let _ = platform.reply(ctx, "👍 已放行").await;
                    }
                    Ok(Some(PermissionDecision::Deny)) => {
                        tracing::info!(request_id = %request_id, "permission: denied");
                        let _ = responder.respond(&request_id, false).await;
                        let _ = platform.reply(ctx, "🚫 已拦截").await;
                    }
                    Ok(Some(PermissionDecision::AllowAll)) => {
                        tracing::info!(request_id = %request_id, "permission: allow all for session");
                        *approve_all = true;
                        let _ = responder.respond(&request_id, true).await;
                        let _ = platform.reply(ctx, "👍 已放行（后续自动通过）").await;
                    }
                    Ok(None) => {
                        tracing::warn!("permission channel closed, auto-denying");
                        let _ = responder.respond(&request_id, false).await;
                        break;
                    }
                    Err(_) => {
                        tracing::warn!("permission timeout (10 min), auto-denying");
                        let _ = responder.respond(&request_id, false).await;
                        let _ = platform.reply(ctx, "💤 权限确认超时，已自动拦截").await;
                    }
                }
            }

            // ----- System handshake -----
            AgentEvent::System { session_id, .. } => {
                tracing::debug!(session_id = %session_id, "agent system handshake");
            }

            // ----- Final result -----
            AgentEvent::Result {
                content,
                session_id,
                input_tokens,
                output_tokens,
            } => {
                if input_tokens == 0
                    && output_tokens == 0
                    && content.is_empty()
                    && !preview.was_active()
                {
                    tracing::debug!("skipping empty resume result");
                    continue;
                }

                if let Some(stop_fn) = stop_typing.take() {
                    stop_fn();
                }

                preview.finish();

                let mut final_text = if content.is_empty() && preview.was_active() {
                    preview.final_text().to_owned()
                } else if content.is_empty() {
                    "(no response)".to_string()
                } else {
                    content.clone()
                };

                // Append context indicator as percentage of context window
                if display.context_indicator && input_tokens > 0 && display.context_window > 0 {
                    let pct = (input_tokens * 100 / display.context_window).min(100);
                    final_text.push_str(&format!("\n\n[ctx: ~{}%]", pct));
                }

                if tool_count > 0 {
                    discard_preview(platform, &mut handle).await;
                    let _ = platform.reply(ctx, &final_text).await;
                } else {
                    match &handle {
                        Some(h) => {
                            if let Some(updater) = platform.as_message_updater() {
                                let _ =
                                    updater.update_preview(h.as_ref(), &final_text).await;
                            }
                        }
                        None => {
                            let _ = platform.reply(ctx, &final_text).await;
                        }
                    }
                }

                result_info = EventLoopResult {
                    final_text,
                    session_id: Some(session_id),
                    input_tokens,
                    output_tokens,
                };
                break;
            }

            // ----- Error -----
            AgentEvent::Error { message } => {
                if let Some(stop_fn) = stop_typing.take() {
                    stop_fn();
                }
                discard_preview(platform, &mut handle).await;
                let error_text = format!("💥 {}", message);
                let _ = platform.reply(ctx, &error_text).await;
                result_info.final_text = error_text;
                break;
            }
        }
    }

    if let Some(stop_fn) = stop_typing.take() {
        stop_fn();
    }

    if result_info.final_text.is_empty() && preview.was_active() {
        let text = preview.final_text().to_owned();
        discard_preview(platform, &mut handle).await;
        let _ = platform.reply(ctx, &text).await;
        result_info.final_text = text;
    }

    Ok(result_info)
}

// ---------------------------------------------------------------------------
// Preview helpers
// ---------------------------------------------------------------------------

async fn freeze_and_detach_preview(
    platform: &Arc<dyn PlatformCapabilities>,
    preview: &mut StreamPreview,
    handle: &mut Option<Box<dyn PreviewHandle>>,
) {
    if !preview.was_active() || preview.is_idle() {
        return;
    }
    preview.freeze();
    if let Some(h) = handle.take() {
        if let Some(updater) = platform.as_message_updater() {
            let text = preview.final_text();
            if !text.is_empty() {
                let _ = updater.update_preview(h.as_ref(), text).await;
            }
        }
    }
    preview.reset();
}

async fn discard_preview(
    platform: &Arc<dyn PlatformCapabilities>,
    handle: &mut Option<Box<dyn PreviewHandle>>,
) {
    if let Some(h) = handle.take() {
        if let Some(updater) = platform.as_message_updater() {
            let _ = updater.delete_preview(h.as_ref()).await;
        }
    }
}

fn summarise_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    if s.chars().count() > max_len {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    } else {
        s
    }
}
