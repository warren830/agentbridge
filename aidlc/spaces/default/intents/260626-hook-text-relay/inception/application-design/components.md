# Components — Hook-Based Real-Time Text Relay

> 上游:`requirements.md`(FR-1..7)、`stories.md`、codekb `architecture.md`/`component-inventory.md`、`team-practices.md`。
> **修订 3(2026-06-27,architecture-reviewer R2 后)**:取消修订 2 的"常驻消费者 task + 引擎分流 turn"(B-4:turn 驱动器不可建、binder 无生命周期落点)。**最终机制**:channel 所有权上移到 `start_session_for_entry`(registry.rs tmux 分支,即"binder"真实落点);C-4 持一个 `event_tx` clone;hook 事件经此 clone 喂进 session **既有 event channel**,由**原样不变的 `process_agent_events`** 消费;工具进度由 events.rs 内一个 **per-turn backend bool** 控制。详见 `decisions.md` ADR-1/ADR-4(修订3)。**C-7 已取消**;idle-only 推送缩到 v2。

## C-1 Hook 脚本(`scripts/agentbridge_hook.py`)

- **职责**:注册为 Claude Code 的 Stop + PostToolUse hook;从 stdin 读 payload JSON,POST 到 agentbridge 本地接收端。
- **形态**:单文件 python3(系统自带;Pillow 不再需要)。~30 行。无状态。
- **健壮性(FR-1.4)**:整个脚本包在 try/except,任何异常(读 stdin、网络、超时)一律 `sys.exit(0)`;POST 用短超时(如 2s),失败静默放过。**永不阻塞 cc**。
- **传什么**:原样转发 hook payload(已含 `hook_event_name`、`session_id`、`cwd`、`last_assistant_message`(Stop)、`tool_name`/`tool_input`/`tool_response`/`duration_ms`(PostToolUse)),附 agentbridge 接收端 URL(从环境变量或固定端口)。
- **映射**:FR-1。

## C-2 Hook 接收端(`src/hook_receiver.rs`,新模块)

- **职责**:仅监听 localhost 的 axum HTTP server(镜像 `webhook.rs` 结构),`POST /hook-event`,接 C-1 的 payload。端口契约见 ADR-8。
- **反序列化(FR-2.2)**:`#[derive(Deserialize)]` 的 `HookPayload`,所有可选字段 `Option<T>`,`hook_event_name` 驱动分支;serde 失败 → 返回 200(不让 cc 侧脚本重试/报错)+ `tracing::warn`,不 panic。
- **路由(FR-3)**:用 payload 的 `cwd`(规范化前缀匹配,优先)/ 兜底(见 ADR-6 m-2)查 C-4 注册表 → 拿到该会话的 `event_tx` clone(channel 由 `start_session_for_entry` 创建);查不到 → 丢弃 + debug 日志(门控)。
- **映射成事件(FR-4/FR-5)**:见 C-3。把得到的 `AgentEvent` `event_tx.send(...).await` 投入 session **既有 event channel**;由**原样不变的 `process_agent_events`** 消费派发(MVP 场景:手机消息触发的 engine turn 正在 drain 该 channel)—— **无需碰 platform 注册、无平台名分支**(FR-2.3 / NFR-7)。
- **生命周期**:`tokio::spawn` 后台;`async`(NFR-3)。端口契约见 ADR-8。
- **映射**:FR-2。

## C-3 Hook→AgentEvent 映射(`src/hook_receiver.rs` 内的 `fn map_hook(payload) -> Option<AgentEvent>`)

- **Stop** → `AgentEvent::Result { content: last_assistant_message, session_id, input_tokens: 0, output_tokens: 0 }`。
  - 选 `Result` 而非 `Text`:Result 在 `events.rs` 走 finalize-preview 路径(收尾一轮),语义正好是"这一轮答完了"。token 数 hook 不给,填 0(或从 transcript meta 取,见 OQ 决议)。
- **PostToolUse** → `AgentEvent::ToolUse { id, tool: tool_name, input: 摘要 }`,但**经 C-5 进度协调器**(不是直接发 reply)。
- **映射**:FR-4(Stop)、FR-5(PostToolUse)。OQ-1 决议见 `decisions.md` ADR-3。

## C-4 Session→Channel 绑定注册表(`src/hook_route.rs` 新模块 + 引擎集成)

- **职责**:维护 规范化 `work_dir` → 该会话 `event_tx: mpsc::Sender<AgentEvent>` clone 的映射。channel 由 `start_session_for_entry`(registry.rs tmux 分支)创建并持有 rx;C-4 只持 sender clone(非 clone 自 TmuxSession 内部 —— 是构造期同源 sender 的 clone)。
- **binder 落点(B-4.2 解)**:在 `start_session_for_entry` tmux 分支登记;在既有 `cleanup_agent_session`(`mod.rs:1657`)移除。均真实生命周期点,无需新钩子。
- **查找(FR-3.2)**:`cwd` 规范化后**前缀匹配** `work_dir`(容子目录,ADR-6 m-2);命中返回 `event_tx.clone()`。
- **门控(FR-3.3)**:未命中 → None → 接收端丢弃。保证非桥接本地 cc 静默。
- **映射**:FR-3。

## C-5 工具进度(在 `process_agent_events` 内,由 per-turn bool 控制)

- **职责**:把一轮内多个 `ToolUse` 聚合到**同一条 preview 消息**就地编辑。**修订 3**:回到 `events.rs`,由**单个 per-turn `tool_progress_inplace` bool**(backend 决定,tmux+hook=true)控制,非 per-event 判别 —— 避开 M-1(per-event 无判别字段);Result 的就地子分支自管 preview 生命周期,避开 M-2(原 discard 逻辑在 false 路径不变)。claude/acp 传 false → 原样零回归。
- **机制**:`inplace=true` 时,首个 ToolUse → `send_preview`;后续 → `update_preview`(累积);Result → finalize progress preview 再发文字。
- **节流(NFR-2)**:`StreamPreview` MIN_INTERVAL/MIN_DELTA + `outgoing_ratelimit`。消息 ~1 条/轮,与 N 解耦。char-boundary 安全(NFR-5)。
- **穿线**:`process_and_drain`(已知 backend)→ `run_event_loop_and_save` → `process_agent_events`,加一个 bool 参。
- **映射**:FR-5、NFR-2。详见 `decisions.md` ADR-4(修订3)。

> **C-7 常驻消费者 task —— 已取消(修订 3)**。其职责由"hook 事件喂既有 channel + 原样 process_agent_events 消费"替代(ADR-1 修订3)。代价:idle-only 推送(无 engine turn 时)缩到 v2。

## C-6 Hook 安装器(`src/main.rs` 子命令 / 复用 daemon 风格)

- **职责**:把 Stop + PostToolUse hook 合并写入全局 `~/.claude/settings.json`(FR-7.1/7.2),幂等、保留既有配置。
- **形态**:CLI 子命令(如 `agentbridge hook-install`),读现有 settings.json → 合并 hooks 段(不覆盖)→ 写回。参考 remote-claude-control 的 setup 合并语义。
- **映射**:FR-7。

## 不改的部分

- tmux **输入** 路径(`send-keys`)—— 不动(US-4 回归守卫)。
- `Platform`/`ReplyCtx` capability traits、platform 注册 —— 不动。
- `AgentSession` trait —— 不动(hook 是 tmux session 的"出"路径增强,不是新 agent 类型)。
