# Component Inventory — agentbridge

| 组件 | 模块 | 职责 | 依赖 |
|------|------|------|------|
| Engine | `engine/mod.rs` | 12 步消息管线、会话调度、权限流、平台工厂 match | core/*, agent/*, platforms(经 trait) |
| CommandDispatcher | `engine/commands.rs` | 内置斜杠命令(/new /list /switch /resume /attach /dir /model /mode /sync …) | core/session, agent/registry |
| EventLoop | `engine/events.rs` | `process_agent_events`:AgentEvent → capability 调用 + 广播 | core/streaming, core/platform |
| SkillRegistry | `engine/skills.rs` | 扫描/解析 .claude/skills/**/SKILL.md | — |
| Session / SessionManager | `core/session.rs` | try-lock+队列、JSON 持久化、tmux_session 字段 | — |
| StreamPreview | `core/streaming.rs` | 流式预览节流状态机 | — |
| ClaudeSession / ClaudeAgent | `agent/mod.rs` | 原生 claude stream-json backend | tokio::process |
| AcpSession / AcpAgent | `agent/acp/*` | ACP JSON-RPC over stdio backend | transport, mapping |
| TmuxSession / TmuxAgent | `agent/tmux/*` | tmux 屏幕抓取 backend(**改造目标**) | tokio::process(tmux) |
| AgentRegistry | `agent/registry.rs` | 运行时 backend 分派 + tmux 名派生 | 三个 backend |
| DiscordPlatform | `platforms/discord/*` | Discord Gateway WS + REST + 代理隧道 | tokio-tungstenite, reqwest |
| TelegramPlatform | `platforms/telegram/*` | Telegram 长轮询 + 媒体 | reqwest |
| GatewayServer / Client | `gateway/*` | web dashboard fan-out | axum, rusqlite |
| WebhookReceiver | `webhook.rs` | axum HTTP 接收端(9111)—— hook 接收端参考 | axum |
| CronScheduler | `cron.rs` | 定时提示注入 | cron |
| SpeechTranscriber | `speech.rs` | STT | reqwest |
| RateLimiters | `ratelimit.rs`, `outgoing_ratelimit.rs` | 入站/出站限流 | — |
| DedupTracker | `dedup.rs` | message-id 去重 | — |

## 本 intent 将新增/改动的组件

- **新增**:HookReceiver(本地 HTTP,接 hook payload)、HookEventMapper(payload → AgentEvent)、Session→Channel 绑定登记。
- **改动**:TmuxSession(移除 screenshot 路径 + poll 截图 + render_term.py;输入 send-keys 保留)。
- **可能改动**:StreamPreview(修 CJK 字节切片 panic 风险)、PostToolUse 节流策略。
