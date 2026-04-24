# Agentbridge 架构重写设计文档

> 决策日期：2026-04-12
> 状态：设计中
> 目标：模块化重写，消除 unsafe、硬编码，引入 session 锁 + 消息队列 + 流式三阶段

---

## 需求

### 核心架构
1. **能力接口替代 unsafe 指针转换** — 不再 `if name == "telegram"` + unsafe cast
2. **Session 锁 + 消息队列** — 忙时入队（最多 5 条），完成后自动排空
3. **流式三阶段** — 预览启动 → 节流编辑 → 最终消息
4. **ReplyContext 透传** — engine 不拆开平台私有上下文
5. **权限暂停/恢复** — 支持交互式审批（按钮）
6. **插件式平台注册** — factory 模式，不硬编码 match

### 新增功能
7. **多工作区支持** — 每个 channel 可绑定不同 work_dir，Agent 子进程池按工作区隔离
8. **Skill 系统** — 多层级 Skill 扫描（项目级 > 全局级），`BuildSkillInvocationPrompt` 包装后发给 Agent
9. **Session 弹性** — Resume 失败自动降级为新会话，路径标准化防 CWD 不匹配
10. **上下文用量追踪** — 从 result 事件提取 token 统计，追加 `[ctx: XX%]` 到消息末尾
11. **平台命令注册** — Telegram setMyCommands / Discord BulkOverwrite，自动 sanitize 名称
12. **集成测试** — 真实 Agent + Mock Platform 模式，Agent 池复用避免冷启动开销

---

## 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs (CLI)                            │
├─────────────────────────────────────────────────────────────────┤
│                      src/core/mod.rs                             │
│  Platform trait + 能力 traits + Event + Message + Session        │
│  ┌─────────┐ ┌────────────┐ ┌──────────┐ ┌───────────────┐    │
│  │Platform │ │MessageUpd. │ │ImgSender │ │InlineButtons  │    │
│  │(必须)   │ │(可选)      │ │(可选)    │ │(可选)         │    │
│  └─────────┘ └────────────┘ └──────────┘ └───────────────┘    │
├─────────────────────────────────────────────────────────────────┤
│                    src/engine/mod.rs                             │
│  消息路由 · Session 锁 · 消息队列 · 流式预览 · 权限审批         │
│  命令分发 · 别名解析 · 速率限制 · 去重                          │
├──────────────────────┬──────────────────────────────────────────┤
│   src/agent/         │       src/platforms/                      │
│  ┌─────────────────┐ │  ┌──────────┐ ┌──────────┐              │
│  │ AgentSession    │ │  │ Telegram │ │ Discord  │  ...         │
│  │ (stream-json)   │ │  │ impl:    │ │ impl:    │              │
│  │ Events channel  │ │  │ Platform │ │ Platform │              │
│  │ Send/Resume     │ │  │ +MsgUpd  │ │ +MsgUpd  │              │
│  └─────────────────┘ │  │ +ImgSnd  │ │ +ImgSnd  │              │
│                       │  │ +Buttons │ │ +Buttons │              │
│                       │  └──────────┘ └──────────┘              │
├───────────────────────┴──────────────────────────────────────────┤
│  src/infra/: session.rs · ratelimit.rs · dedup.rs · i18n.rs     │
└─────────────────────────────────────────────────────────────────┘
```

---

## 核心接口设计

### Platform（必须实现）

```rust
#[async_trait]
pub trait Platform: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self, handler: MessageHandler) -> Result<()>;
    async fn reply(&self, ctx: &dyn ReplyCtx, content: &str) -> Result<()>;
    async fn send(&self, ctx: &dyn ReplyCtx, content: &str) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
```

### ReplyCtx（不透明 trait object）

```rust
/// 每个平台自己定义 struct 实现此 trait
pub trait ReplyCtx: Send + Sync + std::fmt::Debug {
    fn as_any(&self) -> &dyn Any;
    /// 用于 session key 生成
    fn session_key_hint(&self) -> String;
}
```

### 能力 Traits（可选实现）

```rust
#[async_trait]
pub trait MessageUpdater: Platform {
    async fn send_preview(&self, ctx: &dyn ReplyCtx, text: &str) -> Result<Box<dyn PreviewHandle>>;
    async fn update_preview(&self, handle: &dyn PreviewHandle, text: &str) -> Result<()>;
    async fn delete_preview(&self, handle: &dyn PreviewHandle) -> Result<()>;
}

#[async_trait]
pub trait ImageSender: Platform {
    async fn send_image(&self, ctx: &dyn ReplyCtx, data: &[u8], filename: &str) -> Result<()>;
}

#[async_trait]
pub trait FileSender: Platform {
    async fn send_file(&self, ctx: &dyn ReplyCtx, data: &[u8], filename: &str) -> Result<()>;
}

#[async_trait]
pub trait InlineButtonSender: Platform {
    async fn send_with_buttons(&self, ctx: &dyn ReplyCtx, text: &str, buttons: &[Button]) -> Result<Box<dyn PreviewHandle>>;
    async fn answer_callback(&self, callback_id: &str, text: &str) -> Result<()>;
}

#[async_trait]
pub trait TypingIndicator: Platform {
    async fn start_typing(&self, ctx: &dyn ReplyCtx) -> Result<TypingGuard>;
}

pub trait PreviewHandle: Send + Sync {
    fn as_any(&self) -> &dyn Any;
}
```

### 能力查询（QueryInterface 模式）

```rust
/// 平台实现此 trait 以暴露可选能力
pub trait PlatformCapabilities: Platform {
    fn as_message_updater(&self) -> Option<&dyn MessageUpdater> { None }
    fn as_image_sender(&self) -> Option<&dyn ImageSender> { None }
    fn as_file_sender(&self) -> Option<&dyn FileSender> { None }
    fn as_inline_button_sender(&self) -> Option<&dyn InlineButtonSender> { None }
    fn as_typing_indicator(&self) -> Option<&dyn TypingIndicator> { None }
}
```

Engine 使用方式（零 unsafe，零名称检查）：
```rust
if let Some(updater) = platform.as_message_updater() {
    let handle = updater.send_preview(ctx, "Processing...").await?;
    // ...
}
```

---

## 事件模型

```rust
pub enum AgentEvent {
    /// 系统初始化（包含 session_id, tools 列表等）
    System { session_id: String },
    /// 文本输出（增量）
    Text { content: String },
    /// 思考过程
    Thinking { content: String },
    /// 工具调用
    ToolUse { id: String, tool: String, input: String },
    /// 工具结果
    ToolResult { id: String, output: String, is_error: bool },
    /// 权限请求（需要用户决策）
    PermissionRequest { request_id: String, tool: String, input: serde_json::Value },
    /// 完成（包含最终文本、session_id、token 统计）
    Result { content: String, session_id: String, input_tokens: u32, output_tokens: u32 },
    /// 错误
    Error { message: String },
}
```

---

## Session 管理

### Session 结构

```rust
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub agent_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub work_dir: Option<String>,
    busy: AtomicBool,
}

impl Session {
    /// 非阻塞尝试获取锁。成功返回 SessionGuard，失败返回 None。
    pub fn try_lock(&self) -> Option<SessionGuard> { ... }
}

pub struct SessionGuard<'a> { session: &'a Session }
impl Drop for SessionGuard<'_> {
    fn drop(&mut self) { self.session.busy.store(false, Ordering::Release); }
}
```

### 消息队列

```rust
pub struct InteractiveState {
    pub session: Arc<Session>,
    pub agent_session: AgentSession,
    pub pending_messages: VecDeque<QueuedMessage>,  // max 5
    pub stream_preview: Option<StreamPreview>,
}
```

---

## 流式预览状态机

```
               ┌─────────┐
               │  Idle   │
               └────┬────┘
                    │ 收到第一个 Text event
                    ▼
          ┌─────────────────┐
          │  Preview Active │ ← send_preview()
          └────┬───────┬────┘
               │       │
     Text event│       │ Permission/ToolUse
               ▼       ▼
    ┌─────────────┐  ┌──────────┐
    │  Throttled  │  │  Frozen  │ ← 暂停更新
    │  Update     │  └──────────┘
    └──────┬──────┘
           │ Result event
           ▼
    ┌─────────────┐
    │   Finish    │ ← 最终文本 or delete+resend
    └─────────────┘
```

节流参数：
- `MIN_INTERVAL`: 1500ms
- `MIN_DELTA_CHARS`: 30
- `MAX_PREVIEW_LEN`: 2000

---

## 引擎消息处理流程

```
用户消息到达 Platform
     │
     ▼
 dedup check → 重复？丢弃
     │
     ▼
 access control → 不允许？丢弃
     │
     ▼
 alias resolve → 别名转命令
     │
     ▼
 slash command? → 是？handle_command() 直接回复
     │ 否
     ▼
 session.try_lock()
     ├─ 成功 → process_message()
     │           │
     │           ▼
     │     get_or_create AgentSession
     │           │
     │           ▼
     │     agent_session.send(prompt)
     │           │
     │           ▼
     │     event_loop:
     │       Text → stream_preview.append()
     │       ToolUse → notify user (inline button if available)
     │       PermissionRequest → send buttons, await response
     │       Result → stream_preview.finish(), session.unlock()
     │                  → drain pending_messages
     │
     └─ 失败 → queue_message() (max 5)
                  │
                  └─ 队列满？→ reply "会话繁忙，请稍候"
```

---

## 平台注册机制

```rust
// src/platforms/registry.rs
type PlatformFactory = fn(config: &serde_json::Value) -> Result<Arc<dyn PlatformCapabilities>>;

static REGISTRY: LazyLock<Mutex<HashMap<String, PlatformFactory>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("telegram".into(), telegram::create as PlatformFactory);
    m.insert("discord".into(), discord::create as PlatformFactory);
    Mutex::new(m)
});

pub fn create_platform(name: &str, config: &serde_json::Value) -> Result<Arc<dyn PlatformCapabilities>> {
    let registry = REGISTRY.lock().unwrap();
    let factory = registry.get(name).ok_or_else(|| anyhow!("unknown platform: {name}"))?;
    factory(config)
}
```

注：Rust 没有 Go 的 `init()` 机制，但 `LazyLock` + 显式注册同样达到目的。未来可用 `inventory` crate 做真正的自注册。

---

## 权限审批流程

```
Agent 发出 PermissionRequest(request_id, tool, input)
     │
     ▼
Engine 冻结流式预览 (stream_preview.freeze())
     │
     ▼
Platform 有 InlineButtonSender？
     ├─ 是 → 发送 [允许] [拒绝] [全部允许] 按钮
     │         等待用户回调 (callback channel)
     │
     └─ 否 → 自动批准（或发文字提示）
     │
     ▼
收到用户决策
     │
     ▼
agent_session.respond_permission(request_id, allow/deny)
     │
     ▼
Engine 恢复事件循环
```

---

## 目录结构（重写后）

```
src/
├── main.rs                    # CLI 入口
├── core/
│   ├── mod.rs                 # 公开所有核心类型
│   ├── platform.rs            # Platform + 能力 traits
│   ├── agent.rs               # AgentSession trait + Event enum
│   ├── message.rs             # IncomingMessage, Button, ImageAttachment
│   ├── session.rs             # Session, SessionManager, try_lock, queue
│   └── streaming.rs           # StreamPreview 状态机
├── engine/
│   ├── mod.rs                 # Engine 主体
│   ├── commands.rs            # 斜杠命令处理
│   └── events.rs              # Agent 事件循环
├── agent/
│   └── claude.rs              # Claude Code AgentSession 实现
├── platforms/
│   ├── registry.rs            # 平台工厂注册
│   ├── telegram/
│   │   ├── mod.rs             # Platform + 能力 impl
│   │   └── types.rs           # TelegramReplyCtx, PreviewHandle
│   └── discord/
│       ├── mod.rs             # Platform + 能力 impl
│       └── types.rs           # DiscordReplyCtx, InteractionReplyCtx
├── infra/
│   ├── ratelimit.rs           # 入站 + 出站限流
│   ├── dedup.rs               # 消息去重
│   ├── lock.rs                # 单实例锁
│   ├── i18n.rs                # 国际化
│   └── config.rs              # 配置加载
└── features/
    ├── cron.rs                # 定时任务
    ├── relay.rs               # Bot-to-bot 通信
    ├── sync.rs                # Session 跨机同步
    ├── webhook.rs             # HTTP 端点
    └── speech.rs              # STT/TTS
```

---

## 工作单元

| # | 单元 | 依赖 | 并行 |
|---|------|------|:----:|
| 1 | `src/core/` — 定义所有 traits 和类型 | 无 | — |
| 2 | `src/engine/` — 新引擎（session 锁、流式、命令） | 1 | — |
| 3a | `src/platforms/telegram/` — 重写 | 1 | ✅ |
| 3b | `src/platforms/discord/` — 重写 | 1 | ✅ |
| 4 | `src/agent/claude.rs` — 适配新接口 | 1 | ✅ |
| 5 | 集成 + 删除旧代码 + 测试 | 2,3,4 | — |

3a, 3b, 4 可并行开发。

---

## 决策记录

| 决策 | 选择 | 原因 |
|------|------|------|
| 接口断言方式 | `as_*()` 查询方法 | 类型安全，无 unsafe，Rust 惯用 |
| Session 锁 | `AtomicBool` + try_lock | 非阻塞，轻量 |
| ReplyCtx | `dyn ReplyCtx` trait object | Engine 不需要知道平台细节 |
| 平台注册 | `LazyLock<HashMap>` | 简单可靠，未来可换 `inventory` |
| 重写顺序 | 分模块逐步替换 | 风险可控，每步可测试 |
| 流式节流 | 1.5s / 30 chars / 2000 max | 经验值，兼顾响应性与消息编辑配额 |

---

## 替代方案

| 方案 | 考虑 | 为什么不选 |
|------|------|-----------|
| 用 `enum_dispatch` 替代 trait object | 编译时多态，零开销 | 平台数量少，运行时开销可忽略，trait object 更灵活 |
| 用 `dyn Any` downcast | Go 式接口断言 | 不类型安全，需要知道具体类型名 |
| 一次性重写 | 更快完成 | 风险太大，中间状态无法测试 |
| 保持现有架构只修 bug | 最少工作 | unsafe、硬编码问题无法根治 |

---

## NFR 计划

| 维度 | 方案 |
|------|------|
| **性能** | 异步 tokio，session 锁非阻塞，流式节流减少 API 调用 |
| **可靠性** | 指数退避重连，消息队列防丢失，graceful shutdown |
| **安全** | 零 unsafe，输入验证，token 不打日志 |
| **可观测** | tracing spans per message，event type 分级日志 |

---

## Error/Rescue Map

| 边界 | 错误场景 | 恢复策略 |
|------|----------|----------|
| Discord Gateway | 断开连接 | 指数退避重连 (1s-60s) |
| Telegram Poll | HTTP 错误 | 指数退避重试 |
| Claude 进程 | 崩溃退出 | 回复用户错误，下次自动重建 session |
| Claude 进程 | 无响应（超时） | 120s 超时后 kill，回复用户 |
| 消息发送 | 平台 API 限流 429 | 出站速率限制 + 重试 |
| 权限审批 | 用户 60s 不回复 | 超时自动拒绝，恢复事件循环 |
| 流式预览 | 编辑消息失败 | 降级：删除预览，发新消息 |
| Session 队列 | 队列满（5条） | 回复"繁忙请稍候" |
| Resume 失败 | Agent session 过期或损坏 | 清空 session_id，重试新会话，通知用户 |
| 路径不匹配 | symlink/相对路径导致 CWD 偏差 | canonicalize() 标准化路径 |

---

## 新增功能设计

### 多工作区 (Multi-Workspace)

```
用户在 channel A 发 /workspace init git@github.com:user/repo.git
     │
     ▼
Engine 克隆 repo → 绑定 channel A 到该 work_dir
     │
     ▼
后续消息在 channel A → 自动路由到该 work_dir 的 Agent

存储: ~/.agentbridge/workspace_bindings.json
Key: "project:platform:channelID" → work_dir path
```

每个工作区独立的 Agent 子进程：
```rust
pub struct WorkspacePool {
    /// key: work_dir path → AgentSession
    agents: HashMap<PathBuf, AgentSession>,
    /// 15 分钟无活动自动回收（session ID 保留用于 resume）
    idle_timeout: Duration,
}
```

### Skill 系统

```
扫描顺序（先到先得，同名覆盖）:
1. {work_dir}/.claude/skills/       ← 项目级
2. ~/.claude/skills/                ← 用户全局级

Skill 文件结构:
  skills/
    super-aidlc/
      SKILL.md     ← YAML frontmatter (name, description) + prompt body

执行流程:
  用户发 /super-aidlc "task" → Engine 拦截
    → BuildSkillInvocationPrompt(skill, args)
    → 作为普通 user message 发给 AgentSession
    → Agent 按 skill instructions 执行
```

Engine 命令优先级：
```
内置命令 (/new, /list, /dir...) > 自定义命令 (config.commands) > Skill (/super-aidlc)
```

平台命令注册：
- Telegram: `setMyCommands` API（内置 + skills，最多 100 条）
- Discord: `PUT /applications/{id}/commands`（BulkOverwrite，最多 200 条）
- 名称 sanitize: `review-pr` → `review_pr`（Telegram 只允许 [a-z0-9_]）

### Session 弹性

```rust
/// Resume 失败时的降级策略
async fn get_or_resume_agent_session(
    &self, session: &Session, work_dir: &Path
) -> Result<AgentSession> {
    if let Some(ref agent_sid) = session.agent_session_id {
        match self.agent.start_session(Some(agent_sid)).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                tracing::warn!(error = %e, sid = %agent_sid, "resume failed, starting fresh");
                session.clear_agent_session_id();
                // 通知用户
            }
        }
    }
    // 新建
    self.agent.start_session(None).await
}
```

路径标准化：
```rust
fn canonicalize_work_dir(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
```

### 上下文用量追踪

从 `Result` 事件提取 token 统计：
```rust
AgentEvent::Result { input_tokens, output_tokens, .. } => {
    let total = input_tokens + output_tokens;
    let pct = (total as f64 / context_window as f64 * 100.0) as u32;
    if pct > 50 {
        // 追加到最终消息
        final_text.push_str(&format!("\n\n[ctx: {}%]", pct));
    }
    if pct > 80 {
        tracing::warn!(pct = pct, "context usage high, consider /compact");
    }
}
```

### 集成测试架构

```rust
// tests/integration/mod.rs

/// Mock 平台：记录所有发送的消息
struct MockPlatform {
    sent_messages: Arc<Mutex<Vec<String>>>,
    callback_rx: mpsc::Receiver<IncomingMessage>,
}

impl Platform for MockPlatform { ... }
impl MessageUpdater for MockPlatform { ... }

/// Agent 池复用（避免每个测试冷启动 3-6s）
static AGENT_POOL: LazyLock<Mutex<HashMap<PathBuf, AgentSession>>> = ...;

/// 超时指南
const TIMEOUT_SIMPLE: Duration = Duration::from_secs(30);
const TIMEOUT_TOOL_USE: Duration = Duration::from_secs(60);
const TIMEOUT_SLOW: Duration = Duration::from_secs(90);
```

---

## 更新后的工作单元

| # | 单元 | 依赖 | 并行 | 新增 |
|---|------|------|:----:|:----:|
| 1 | `src/core/` — traits, events, message, session | 无 | — | |
| 2 | `src/engine/` — 路由, session 锁, 流式, 命令, skill dispatch | 1 | — | |
| 3a | `src/platforms/telegram/` — 重写 + 命令注册 | 1 | ✅ | |
| 3b | `src/platforms/discord/` — 重写 + interaction + 命令注册 | 1 | ✅ | |
| 4 | `src/agent/claude.rs` — 适配新接口 + resume 弹性 | 1 | ✅ | |
| 5 | `src/engine/skills.rs` — Skill 扫描 + 执行 | 2 | — | ✅ |
| 6 | `src/engine/workspace.rs` — 多工作区 + Agent 池 | 2,4 | — | ✅ |
| 7 | `tests/integration/` — Mock Platform + Agent 池 | 2,3,4 | — | ✅ |
| 8 | 集成 + 删除旧代码 | 全部 | — | |

3a, 3b, 4 并行。5, 6, 7 在核心完成后并行。
