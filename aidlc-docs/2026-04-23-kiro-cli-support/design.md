# Design: kiro-cli 支持 (通用 ACP 适配器 + 多 agent 并存)

## Requirements

### 目标
让 agentbridge 支持 [kiro-cli](https://github.com/aws/kiro-cli)（AWS 出品的类 Claude Code 终端 AI 助手）作为 agent backend，并顺路为未来的其它 ACP 兼容 agent（cursor-agent、gemini ACP 模式等）打好通用接入基础。

### 功能需求
- **通用 ACP 适配器**：基于 Agent Client Protocol (ACP) / JSON-RPC 2.0，可复用于任何 ACP agent，不只是 kiro-cli。
- **多 agent 并存**：同一 project 的 config 可声明多个 agents，运行时用户可 `/agent <name>` 切换。
- **Agent 隔离 session**：每个 agent 有独立的 session 列表（`/list`、`/switch`、`/new` 只作用于当前 agent）。session 存储层单文件共存，靠 `agent_type` 字段区分。
- **默认 agent 部署时确定**：config 里 `default_agent` 字段选定整个 project 的默认 agent（`init` 向导引导，`doctor` 展示）。
- **向后兼容**：现有单 agent 的 `agent: {}` config + 现有 `sessions.json` 零破坏升级，老 session 自动归入 `agent_type: claude`。
- **权限交互**：ACP `session/request_permission` 的 `options` 数组原样映射到平台按钮（不压成 allow/deny）。
- **Cron 绑 agent**：cron 条目记住创建时的 agent，触发时用那个 agent（不跟随 default 漂移）。

### 非目标
- **不跨 agent 携带上下文**：切 agent 就是换聊天窗口。不 replay 历史给新 agent。
- **不重写现有 Claude 适配器**：`src/agent/mod.rs` 原封不动，只接到新的 agent factory 注册表下。
- **不改 Web dashboard UI**：web 端先按原有单 agent 视图展示，后续再做多 agent UX（此 spec 只保证后端数据模型兼容）。
- **不做 kiro-cli 的 `chat --no-interactive` 模式**：ACP 够用，无需第二条路径。
- **不支持 agent 别名**：用户配什么 `name` 就是什么 `name`，无 alias。
- **不支持 session 跨 agent 克隆/迁移**：即便两个 agent 都用同底层模型，不提供"把 claude session 复制给 kiro"的功能。

**Security baseline**: ENABLED

---

## Architecture

### 改动范围概览

```
src/
  agent/
    mod.rs              # 现有 ClaudeSession/ClaudeAgent — 不动
    acp/                # 新增：通用 ACP 适配器
      mod.rs            #   AcpAgent (factory) + AcpSession
      protocol.rs       #   JSON-RPC 2.0 消息类型 (serde)
      mapping.rs        #   ACP event ↔ AgentEvent 转换
    registry.rs         # 新增：backend name → factory 的注册表
  config/mod.rs         # 修改：AgentsConfig 多 agent，向后兼容 agent:{}
  core/session.rs       # 修改：Session 加 agent_type 字段 + SessionManager 按 agent 过滤
  engine/
    mod.rs              # 修改：EngineInteractiveState 按 (userKey, agent_name) 键控
    commands.rs         # 修改：/agent 命令；/list/new/switch 只作用当前 agent
  cron.rs               # 修改：CronEntry 加 agent_name 字段
  main.rs / init.rs     # 修改：init 向导问 default_agent；doctor 打印默认 agent
```

### 组件单元（按实现顺序）

- **U1 ACP 协议层**（`src/agent/acp/protocol.rs` + `mapping.rs`）
  纯数据类型 + 转换函数。零 I/O、纯单测。
  - JSON-RPC 2.0 wire types (Request/Response/Notification/ErrorObject)
  - ACP 方法常量：`initialize`、`session/new`、`session/load`、`session/prompt`、`session/request_permission`、`session/cancel`、通知类 `session/update`
  - `session/update` payload → `AgentEvent::{TextDelta, ToolUse, Thinking, Result, ...}` 映射
  - 忽略 `_kiro.dev/*` 扩展通知（非必需）

- **U2 ACP session transport**（`src/agent/acp/mod.rs`）
  subprocess + stdin/stdout JSON-RPC 客户端。
  - `AcpAgent`（factory）：spawn `command args...`，绑定 work_dir/env
  - `AcpSession` 实现 `AgentSession` trait（已存在于 `src/agent/mod.rs`）
  - 初始化流程：`initialize` → `session/new` (或 `session/load` if resume) → 等到 ready
  - 发消息：`session/prompt` with content blocks (text + image)
  - 权限请求：收到 `session/request_permission` 就 emit `AgentEvent::PermissionRequest{options}` 并把 options 原样带上，等 engine 回 `respond_permission`
  - Session ID 保存：`session/new` 的响应里取 `sessionId`，存进 `session_id` 字段供 resume 用

- **U3 AgentBackend 注册表**（`src/agent/registry.rs`）
  - `trait AgentBackend { async fn start_session(...) -> Box<dyn AgentSession> }`
  - `ClaudeAgent` 和 `AcpAgent` 都实现它
  - 全局静态 map：`"claude" → ClaudeAgent::new`、`"acp" → AcpAgent::new`
  - `engine` 从 registry 取对应 backend，不直接 new `ClaudeAgent`

- **U4 Config 多 agent**（`src/config/mod.rs`）
  - 新增 `AgentsConfig { agents: Vec<AgentEntry>, default_agent: Option<String> }`
  - `AgentEntry { name: String, backend: String, /* backend 特定字段扁平嵌套 */ }`
  - 向后兼容 deserialize：如果 yaml 里只有 `agent:` 而没 `agents:`，自动转成 `agents: [{name: "claude", backend: "claude", ...}]`, `default_agent: Some("claude")`
  - 校验：`default_agent` 必须存在于 `agents` 名单；重复 `name` 报错

- **U5 Session agent_type 过滤**（`src/core/session.rs`）
  - `Session` 加 `agent_type: String` 字段（JSON 默认 `""`，读取时 `""→"claude"`）
  - `SessionManager` API 加 `agent_name: &str` 参数：
    - `get_or_create_active(user_key, agent_name)` — 只从 `(user, agent)` 维度找 active
    - `list_sessions(user_key, agent_name)` — 只返 `agent_type == agent_name` 的
  - 存储快照形状不变，仍单文件 `sessions.json`

- **U6 Engine 热切换 + busy 拒绝**（`src/engine/mod.rs`）
  - `EngineInteractiveState` 的 key 从 `session_key` 变 `(session_key, agent_name)`
  - 用户当前 agent_name 存在：`ActiveAgent: HashMap<user_key, String>`，默认值从 config `default_agent` 取
  - `/agent <name>` 命令：
    - **Busy 判定**：复用现有 engine busy 概念——当前 state 有未结束的 agent turn（即 state 锁被 event loop 持有 / `has_pending_permission` 为 true / pending_messages 非空）→ 拒绝，回复 "agent busy, run /stop first"
    - **Idle 判定**：agent_session 可能是 None（从未创建）、可能 alive 但 idle（subprocess 存活等待下一轮）、可能 dead。都视为可切换
    - 切换动作：若有存活 subprocess，调 `graceful_shutdown`（close stdin → SIGTERM → SIGKILL，现有函数）；写入 `ActiveAgent[user_key] = new_agent_name`；**创建一个全新 session**（不 resume 新 agent 的历史 session）
    - 回复用户确认："Switched to {name}. New session started."

- **U7 Cron 绑 agent**（`src/cron.rs`）
  - `CronEntry` 加 `agent_name: String`
  - `add_cron` 时取当前用户的 active_agent 写进去
  - 触发时用 entry 里的 `agent_name` 起 session（不是 default_agent）
  - 老 cron 文件没这字段 → 默认 `""`，读取时 fallback 成 `"claude"`

- **U8 Init/Doctor UX**（`src/main.rs` 或 init 模块）
  - `agentbridge init`：在 agent 配置步骤询问 "Which agent? [1. Claude Code, 2. Kiro CLI, 3. Both]"；Both 时再问 "Default agent? [claude/kiro]"
  - `agentbridge doctor`：在现有输出里加一行 `Default agent: <name> (backend: <backend>, command: <cmd>)`；对每个 ACP agent 额外检查其 `command` 是否在 PATH

### 数据流

**新消息走通路径（多 agent）:**
```
IncomingMessage
  ↓
engine: 确定 user_key
  ↓
engine: 查 ActiveAgent[user_key]  → agent_name (默认 default_agent)
  ↓
engine: SessionManager.get_or_create_active(user_key, agent_name) → Session
  ↓
engine: EngineInteractiveState[(user_key, agent_name)]
  ↓  
  ├─ 有活 agent_session → 发给它
  └─ 无 → registry.get(backend).start_session(...)  ← 从 agent.backend 字段路由
```

**`/agent kiro` 切换:**
```
/agent kiro
  ↓
state = EngineInteractiveState[(user_key, current_agent)]
  ↓
state.agent_session.alive()?
  ├─ true  → reply "agent busy, /stop first"; 不切
  └─ false → 
        graceful_shutdown(old_session)
        ActiveAgent[user_key] = "kiro"
        SessionManager.create_new(user_key, "kiro")
        reply "Switched to kiro. New session."
```

### ACP 协议映射（关键细节）

实测 kiro-cli 的 ACP 返回：
```
initialize    → agentCapabilities { loadSession, promptCapabilities.image, mcpCapabilities }
session/new   → sessionId, modes (kiro_default / kiro_planner)
session/prompt → 异步通知流: session/update (chunks)
session/request_permission → 带 options 数组
_kiro.dev/*   → 扩展通知，忽略
```

**ACP → AgentEvent 映射表:**

| ACP 事件 | AgentEvent |
|---|---|
| `session/update` with `content.text` (partial) | `TextDelta { text }` |
| `session/update` with `content.tool_call` | `ToolUse { id, tool, input }` |
| `session/update` with `content.thinking` | `Thinking { text }` |
| `session/prompt` response (stop_reason=end_turn) | `Result { content, input_tokens, output_tokens }` |
| `session/request_permission` | `PermissionRequest { request_id, tool, input, options: Vec<PermOption> }` ← **options 带上** |
| JSON-RPC `error` | `Error { message }` |
| `_kiro.dev/*` | 忽略（debug log） |

**新增：`PermOption { id: String, label: String, kind: "allow_once"|"allow_always"|"reject_once"|"reject_always"|... }`**  
Telegram/Discord inline keyboard 按 options 列表生成按钮（每个 option 一个 callback）。engine 收到 callback 后调 `session/request_permission` 的响应 RPC 带 `optionId`。

### Diagram

```
          ┌─────────── config.yaml ───────────┐
          │ agents: [claude, kiro]            │
          │ default_agent: kiro               │
          └──────────────┬────────────────────┘
                         ▼
                   ┌───────────┐
                   │  engine   │
                   └─────┬─────┘
           ┌─────────────┴─────────────┐
           ▼                           ▼
    ActiveAgent[user]            SessionManager
    = "claude" or "kiro"         sessions.json (单文件, agent_type 区分)
           │                           │
           ▼                           ▼
    registry.get(backend)        Session { id, name, agent_type, agent_session_id }
           │
  ┌────────┴────────┐
  ▼                 ▼
ClaudeAgent      AcpAgent
(--input-format   (kiro-cli acp)
 stream-json)           │
  │                     ▼
  ▼              ┌──────────────┐
claude CLI       │ JSON-RPC /   │
                 │ session/prompt│
                 │ session/update│
                 └──────────────┘
```

---

## NFR Plan

### Medium -- quick scan
- [x] 响应时间：ACP 流式通知，延迟与 Claude stream-json 对等
- [x] 并发用户：每个 (user, agent) 独立 subprocess，无全局锁
- [x] 数据保留：sessions.json 单文件，原子写（AtomicWriteFile 风格）
- [x] 日志：`tracing` + `agent_type` 字段进 structured log
- [x] 缓存：无需

### Security
- **Subprocess args**：ACP `command` 和 `args` 都来自 config YAML（trusted source），但仍用 `exec.LookPath` 校验 + 不走 shell
- **Work dir**：ACP session 的 `cwd` 参数传绝对路径，防止相对路径歧义
- **Permission flow**：ACP 的 permission options 不做本地篡改 — agent 说什么选项就给用户什么选项，防止 "看似批准 allow_once 实际传了 allow_always"
- **Config parse**：向后兼容 deserialize 里不接受同时存在 `agent:` 和 `agents:` 字段的 config（避免歧义），启动报错

---

## Error/Rescue Map

| 什么会失败 | 错误名 | 所属单元 | 系统行为 | 用户感知 |
|-----------|-------|---------|---------|---------|
| config 里 `default_agent` 未在 `agents` 列表 | `ConfigInvalidDefaultAgent` | U4 | 启动前校验失败，退出 | CLI 打印具体原因 |
| config 里 `agents` 有重复 name | `ConfigDuplicateAgentName` | U4 | 启动前校验失败，退出 | CLI 打印具体原因 |
| config 同时有 `agent:` 和 `agents:` | `ConfigAmbiguousAgentShape` | U4 | 启动前校验失败，退出 | CLI 提示用户删除老字段 |
| `/agent foo` 但 foo 不存在 | `UnknownAgent` | U6 | 回复 "unknown agent: foo, try /agent" | 聊天提示 |
| `/agent kiro` 但 agent busy | `AgentBusy` | U6 | 回复 "agent busy, /stop first" | 聊天提示 |
| ACP command 不在 PATH | `AcpCommandNotFound` | U2 | 启动 session 失败，返回 anyhow | 聊天回复 "kiro-cli not installed" |
| ACP subprocess 意外退出 | `AcpSubprocessDied` | U2 | 置 alive=false，emit Error event，清 state | 聊天 "agent crashed, /new to retry" |
| ACP JSON-RPC 解析失败 | `AcpProtocolError` | U2 | debug log 原文，继续（容错非关键通知） | 无感知 |
| `session/prompt` JSON-RPC error response | `AcpPromptError` | U2 | emit `AgentEvent::Error` | 聊天回复错误 message |
| 老 sessions.json 没 `agent_type` 字段 | — | U5 | deserialize 默认 `""`，运行时 `""→"claude"` | 无感知（透明迁移） |
| 老 cron 条目没 `agent_name` 字段 | — | U7 | deserialize 默认 `""`，触发时 `""→"claude"` | 无感知 |
| kiro-cli 未登录（authMethods 非空） | `AcpAuthRequired` | U2 | 提示用户本地跑 `kiro-cli login` | 聊天回复 + doctor 也能报 |

---

## 测试策略

### 单元测试（U1-U8 每个单元至少一组）
- **U1**：ACP 协议 serde 往返 + 关键 mapping cases（permission options、tool call、thinking）
- **U2**：模拟 subprocess（`echo` + 固定 JSON）验证 init/new/prompt 流程（无需真 kiro-cli）
- **U4**：config 解析：新老格式、缺 default、重复 name、双字段冲突
- **U5**：SessionManager 带 agent_name 过滤；老文件读取补 agent_type
- **U7**：Cron entry 反序列化无字段时回落 claude

### 集成测试（至少 1 个，依赖本机有 kiro-cli）
- E2E：启动 AcpSession("kiro-cli", ["acp"])、发 prompt、收 Result、查 session_id 非空、再 session/load 同 ID、关闭
- 标记为 `#[ignore]` 默认跳过（CI 没装 kiro-cli），文档写明本地怎么跑

### 手动验证清单
- [ ] Telegram 里 `/agent kiro` 切换成功 + 消息回复 + session_id 在磁盘落下
- [ ] `/list` 在 claude agent 下只显示 claude 的 session；切到 kiro 只显示 kiro 的
- [ ] kiro 里出权限请求时按钮数量 = options 数量，点 `allow_always` 后下次同 tool 不再问
- [ ] `default_agent: kiro` 的 config，新用户第一条消息走 kiro
- [ ] 老 `agent: {}` config 启动零错误、老 session 可用
- [ ] `agentbridge doctor` 打印默认 agent 行 + kiro-cli PATH 检测

---

## 向后兼容矩阵

| 升级前状态 | 升级后行为 |
|---|---|
| config 只有 `agent: {mode: default, model: X}` | 解析为 `agents: [{name: "claude", backend: "claude", mode: "default", model: X}]`, `default_agent: "claude"`。零改动可跑 |
| `sessions.json` 里 Session 无 `agent_type` | 读取时补 `"claude"`，后续 save 带上该字段 |
| `cron` 里 entry 无 `agent_name` | 读取时补 `"claude"`，触发用 claude |
| 用户从未使用过 `/agent` 命令 | 行为完全不变，始终走 default_agent（claude） |

---

## 开放问题（非 blocker）

- ACP `session/update` 的详细字段有可能随 kiro-cli 版本演进 — 需要在 changelog 里记 kiro-cli 的目标版本（当前实测 `Kiro CLI Agent 2.0.1`）
- 多 agent 共存时的 rate-limit 计数：本 spec 按"所有 agent 共用一个 user 的 sliding window"处理（维持现状）。未来可能需要 per-agent limit，现在不做
- Web dashboard 里的 agent 选择 UI — 留给后续 UX spec（本 spec 只保证后端 API 可以返回 agent_name 字段）
