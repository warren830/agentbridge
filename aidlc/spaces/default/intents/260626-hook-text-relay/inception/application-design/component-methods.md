# Component Methods — Hook-Based Real-Time Text Relay

> 关键方法签名(Rust,接地于真实类型)。非最终代码,是设计契约。
> **修订 3(最终)**:取消 C-7 独立消费者(ADR-7 退役)。channel 由 `start_session_for_entry` 创建,C-4 持 `event_tx` clone;hook 事件喂既有 channel,原样 `process_agent_events` 消费;工具进度由 events.rs 内 per-turn `tool_progress_inplace` bool 控制。下方 C-7 代码块以注记标为已取消;C-4 签名按 work_dir→event_tx 更新。

## C-2 Hook 接收端

```rust
// src/hook_receiver.rs
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    pub hook_event_name: Option<String>,     // "Stop" | "PostToolUse" | ...
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    // Stop:
    pub last_assistant_message: Option<String>,
    // PostToolUse:
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_response: Option<serde_json::Value>,
    pub duration_ms: Option<u64>,
}

struct HookReceiverState {
    registry: Arc<HookRouteRegistry>,   // C-4
}

pub async fn start(port: u16, registry: Arc<HookRouteRegistry>) -> anyhow::Result<()>;
//   绑定 127.0.0.1:port,route POST /hook-event,tokio::spawn 后台。

async fn handle_hook_event(
    State(state): State<Arc<HookReceiverState>>,
    Json(p): Json<HookPayload>,
) -> StatusCode;
//   1. let Some(tx) = state.registry.resolve(p.cwd.as_deref()) else { warn+return 200 };  // 门控(cwd 前缀匹配)
//   2. let Some(ev) = map_hook(&p) else { return 200 };  // 空轮/不接的事件 → None
//   3. let _ = tx.send(ev).await;  // 投入该会话 hook channel;C-7 消费者消费;send 失败(消费者已退)静默
//   4. StatusCode::OK   // 永远 200,hook 脚本不重试
```

## C-3 映射

```rust
fn map_hook(p: &HookPayload) -> Option<AgentEvent> {
    match p.hook_event_name.as_deref() {
        Some("Stop") => Some(AgentEvent::Result {
            content: p.last_assistant_message.clone().unwrap_or_default(),
            session_id: p.session_id.clone().unwrap_or_default(),
            input_tokens: 0, output_tokens: 0,
        }),
        Some("PostToolUse") => Some(AgentEvent::ToolUse {
            id: /* uuid or duration-based */,
            tool: p.tool_name.clone().unwrap_or_default(),
            input: summarize_tool(p),   // 短摘要,非全 input
        }),
        _ => None,   // 不接的事件类型
    }
    // last_assistant_message 为空 → 返回 None(空轮不发)
}
```

## C-4 路由注册表(`src/hook_route.rs`)

```rust
pub struct HookRouteRegistry { /* Mutex<HashMap<normalized_work_dir, mpsc::Sender<AgentEvent>>> */ }

impl HookRouteRegistry {
    pub fn bind(&self, work_dir: &str, event_tx: mpsc::Sender<AgentEvent>);  // start_session_for_entry 传入 session 的 event_tx clone
    pub fn unbind(&self, work_dir: &str);   // cleanup_agent_session 时
    pub fn resolve(&self, cwd: Option<&str>) -> Option<mpsc::Sender<AgentEvent>>;
    //   cwd 规范化后前缀匹配 work_dir(ADR-6 m-2);命中 clone 返回。
}
```

> **修订 3**:`event_tx` 是 `start_session_for_entry` 创建的 session event channel 的 sender clone(channel rx 由 TmuxSession 持有、被 process_agent_events 消费)。不另起 channel、不另起消费者。

## C-7 常驻消费者 task —— 已取消(修订 3)

修订 2 的独立消费者 + turn_done Notify 被 B-4/M-5 证伪(turn 驱动器不可建、Notify 与 /btw 冲突)。**取代方案**:hook 事件喂既有 channel,原样 `process_agent_events` 消费。下面是修订 3 的两处真实接线:

```rust
// 1) start_session_for_entry(registry.rs, tmux 分支)—— "binder" 真实落点
let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(128);   // 同今:poll 任务用 event_tx
hook_route.bind(&normalized_work_dir, event_tx.clone());        // 修订3 新增:C-4 持 clone
// TmuxSession 持 event_rx(同今);cleanup_agent_session 时 hook_route.unbind(&work_dir)

// 2) events.rs:process_agent_events 加 per-turn bool 参(C-5 / ADR-4 修订3)
//    process_and_drain 已知 backend → 传 tool_progress_inplace = (backend == tmux+hook)
//    ToolUse 分支:
//      if tool_progress_inplace { /* send_preview / update_preview 累积,char-safe */ }
//      else { /* 既有 freeze_and_detach_preview + reply,原样零改 */ }
//    Result 分支:
//      if tool_progress_inplace { finalize progress preview, 再 reply(content) }
//      else { /* 既有 tool_count>0 → discard_preview + reply,原样零改 */ }
```

> hook 的 `Result`(Stop)进同一条 `event_rx`,被正在跑的 `process_agent_events` recv 到、像旧 poll settle Result 一样结束该轮 —— `process_and_drain`/`drain_pending_messages` 零改(B-4.1 解)。capability 调用签名以 `core/platform.rs` 真实定义为准。

## C-6 安装器

```rust
// agentbridge hook-install
pub fn install_global_hooks() -> anyhow::Result<()>;
//   读 ~/.claude/settings.json(无则建)→ serde_json 解析 →
//   合并 hooks.Stop / hooks.PostToolUse 指向 C-1 脚本(去重幂等)→ 原子写回。
//   保留既有 hooks/其他配置(FR-7.2 merge-not-overwrite)。
```
