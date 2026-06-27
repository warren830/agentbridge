# Code Structure — agentbridge

单 cargo crate(bin `main.rs` + lib `lib.rs`)。44 个 `.rs` 文件,按职责分层。

## Module Organization

| 路径 | 职责 |
|------|------|
| `main.rs` | clap CLI 入口;子命令 Run/Init/Doctor/Daemon/Relay/Sync/Gateway;每项目起一个 Engine |
| `lib.rs` | 测试用公开面(config/core/dedup/gateway/speech 子集) |
| `daemon.rs` | systemd user-service 安装/启停/日志 |
| `relay.rs` | Unix socket bot-to-bot 中继(RelayServer/RelayEnvelope) |
| `sync.rs` | rsync/SSH 同步 Claude 会话文件 |
| `dedup.rs` | 时间窗口 message-id 去重(DedupTracker) |
| `speech.rs` | STT 语音转文字(SpeechConfig/transcribe) |
| `webhook.rs` | axum HTTP 接收端(默认 9111) |
| `cron.rs` | 定时提示注入(CronScheduler/CronReplyCtx) |
| `lock.rs` | 单实例文件锁 |
| `ratelimit.rs` / `outgoing_ratelimit.rs` | 入站(每用户)/ 出站(每频道)限流 |
| **`core/`** | 平台中立契约(无 agent/platform 依赖) |
| `core/platform.rs` | capability traits(Platform/ReplyCtx/MessageUpdater/ImageSender/FileSender/InlineButtonSender/TypingIndicator) |
| `core/event.rs` | `AgentEvent` 枚举 + `PermissionOption` |
| `core/message.rs` | IncomingMessage/ImageAttachment/FileAttachment |
| `core/session.rs` | Session(AtomicBool busy)/SessionGuard/QueuedMessage/InteractiveState/SessionManager |
| `core/streaming.rs` | StreamPreview 状态机 |
| **`agent/`** | backend(经 AgentSession trait) |
| `agent/mod.rs` | AgentSession/PermissionResponder traits;ClaudeSession + 工厂;stream-json 解析 |
| `agent/registry.rs` | `start_session_for_entry`:运行时 backend 分派 + tmux session 名派生 |
| `agent/tmux/{mod,session}.rs` | TmuxSession:send-keys/capture-pane 屏幕抓取(**本 intent 改造目标**) |
| `agent/acp/{mod,session,protocol,transport,mapping}.rs` | ACP backend(JSON-RPC over stdio) |
| `config/mod.rs` | YAML 配置 + 校验(AppConfig/ProjectConfig/AgentEntry) |
| **`engine/`** | 路由核心 |
| `engine/mod.rs` | Engine + handle_message 12 步管线 + 平台工厂 match |
| `engine/commands.rs` | 内置斜杠命令 dispatch |
| `engine/events.rs` | `process_agent_events`:AgentEvent → capability 调用 |
| `engine/skills.rs` | SkillRegistry:扫 .claude/skills/**/SKILL.md |
| **`gateway/`** | web dashboard fan-out(独立 binary 模式) |
| `gateway/{server,client,protocol,db}.rs` | axum WS+REST / 反向 WS 客户端 / wire 类型 / rusqlite 历史 |
| **`platforms/`** | 适配器(引擎不点名;运行时分派) |
| `platforms/discord/{mod,types}.rs` | Discord Gateway WS + REST + 按钮/线程 + HTTP-CONNECT 代理隧道 |
| `platforms/telegram/{mod,types}.rs` | Telegram 长轮询 + 内联键盘 + 媒体下载 |

## Code Patterns

- 所有 async trait 用 `async-trait`。
- 测试一律 inline `#[cfg(test)] mod tests`(无顶层 `tests/`)。
- 嵌入资源:`scripts/render_term.py` 经 `include_str!` 进 tmux 截图渲染(本 intent 将移除)。
- 配置向后兼容:旧单数 `agent:` vs 新 `agents:`,`resolved_agents()` 归一。
