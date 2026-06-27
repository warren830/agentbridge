# Units of Work — Hook-Based Real-Time Text Relay

> 基于 revision-3 应用设计(`decisions.md` ADR-1/3/4/5/6/8、`components.md` C-1..C-6)。每个 unit 独立可实现+可测。

## U-1 Hook payload 类型 + 映射(纯函数)

- **内容**:`src/hook_receiver.rs` 的 `HookPayload`(serde,全 Option)+ `map_hook(&HookPayload) -> Option<AgentEvent>`(C-3)。Stop→Result、PostToolUse→ToolUse、空 `last_assistant_message`→None、不接的事件→None。
- **边界**:纯函数,无 I/O。
- **测试**:单测覆盖每个映射分支 + 空轮 None + 畸形(缺字段)不 panic。
- **依赖**:`core/event.rs`(既有 AgentEvent)。**无其他依赖,可最先建。**
- **对应**:FR-2.2、FR-4、FR-5、ADR-3。

## U-2 路由注册表(`src/hook_route.rs`)

- **内容**:`HookRouteRegistry`(`Arc<Mutex<HashMap<normalized_work_dir, mpsc::Sender<AgentEvent>>>>`)+ `bind`/`unbind`/`resolve`(cwd 规范化前缀匹配,ADR-6 m-2)。
- **边界**:独立模块,无引擎依赖。
- **测试**:单测 bind→resolve 命中、前缀匹配(子目录)、未命中 None、unbind 后 None。
- **依赖**:无(可与 U-1 并行)。
- **对应**:FR-3、ADR-1、ADR-6。

## U-3 Hook 接收端(`src/hook_receiver.rs` axum server)

- **内容**:localhost axum server,`POST /hook-event`,`handle_hook_event`:resolve→map_hook→`event_tx.send`→200。永远 200、防御性。镜像 `webhook.rs`。端口契约 ADR-8。
- **边界**:依赖 U-1(映射)、U-2(路由)。
- **测试**:集成测(起 server,POST 各类 payload,验证 send 到 mock channel / 未命中丢弃 / 畸形 200+warn)。
- **依赖**:U-1、U-2。
- **对应**:FR-2、ADR-2、ADR-8。

## U-4 引擎集成:channel 所有权重构 + 绑定 + 接收端启动

> **修订(arch-review R1 B-1)**:channel **不是**在 `start_session_for_entry` 创建的 —— 真实创建点在 `TmuxAgent::start_session()`(`session.rs:211`),`event_tx` move 进 poll task,`TmuxSession` 只持 `events_rx`(`session.rs:28,234`)。所以"binder 落点"需要一次真实的 channel 所有权重构,触碰 `session.rs`。
- **内容(子交付,风险可见)**:
  - **U-4a channel 所有权重构(session.rs,核心风险)**:让 `TmuxAgent::start_session()` 额外把一个 `event_tx.clone()` 暴露出来(改 `start_session` 返回 `(TmuxSession, mpsc::Sender<AgentEvent>)`,或在内部接受由上层传入的 channel)。保证该 clone 与 poll task 持有的 `event_tx` 同源、与 `TmuxSession` 的 `events_rx` 是同一 channel。**这是 B-1 指出的真实重构,U-4 的主要风险点。**
  - **U-4b 绑定**:`start_session_for_entry`(registry.rs tmux 分支)拿到 U-4a 暴露的 `event_tx.clone()`,`hook_route.bind(normalized_work_dir, clone)`(binder 落点)。
  - **U-4c 注销**:`cleanup_agent_session`(`mod.rs:1657`)调 `unbind`。
  - **U-4d 接收端启动**:进程启动时起 U-3 接收端(像 webhook,main/engine 挂载)。
  - **U-4e 配置**:`hook_receiver.port`(config/mod.rs),**默认 9123**(ADR-8,区隔 webhook 的 9111);此字面量是 U-3/U-7/U-8 共享契约(m-5)。
- **边界**:`src/agent/tmux/session.rs`(U-4a,channel 重构)、`registry.rs`(U-4b)、`engine/mod.rs`(U-4c/d)、`config/mod.rs`(U-4e)。**不碰** `process_and_drain`/`drain_pending_messages` 的 race 逻辑。
- **与 U-5 共享文件(m-3)**:U-4(U-4a)与 U-5 都改 `session.rs`;DAG 边 U-4→U-5 已串行化,U-5 在 U-4a 重构后的 channel/poll 签名上构建。
- **测试**:U-4a 暴露的 clone 与 poll task sender 同源(send 到 clone 能被 TmuxSession 的 events_rx 收到);bind 在 session 创建时、unbind 在 cleanup;resolve 拿到正被 process_agent_events drain 的 channel sender。
- **依赖**:U-2、U-3。
- **对应**:FR-3、ADR-1(binder 落点 + channel 上移真实形态)、ADR-8。

## U-5 ADR-5:tmux 输出模式 + poll 输出门控 + idle 心跳保活

- **内容**:`TmuxConfig` 加输出模式(screenshot|hook);`poll_loop` 在 hook 模式关闭 Text/Result/Image 三个**输出型**发射点,保留 permission 检测。
- **U-5b idle-timeout 静默保活(M-1 + arch-review R2 B-2,关键)**:`process_agent_events` 的 `rx.recv()` 包在 `EVENT_IDLE_TIMEOUT=300s`(`events.rs:27,78`),旧靠 poll 心跳 `Thinking`(`session.rs:418`)重置。
  - **不能复用 `AgentEvent::Thinking` 作保活(B-2 修正)**:`Thinking` 在 `process_agent_events`(`events.rs:157-168`)被渲染成可见 `🧠 Working…`(gated on `display.thinking_messages`,**默认 true**,`config/mod.rs:139`),且触发 `freeze_and_detach_preview` —— 会每 ~12s 刷屏 + 打碎 U-9 的就地进度 preview(违 FR-5.2)。**复用 Thinking = 用新 bug 换旧 bug。**
  - **正确机制(MVP 取此)**:hook 模式的 poll loop 心跳改发一个**真正静默的保活信号** —— 选项 A:在 `process_agent_events` 加一个"仅重置 idle 计时、不 reply、不 freeze preview"的分支(可用现有 `System` 事件,或加一个轻量 `AgentEvent::KeepAlive`,或让心跳走一条不经 freeze 的 timer-reset 路径);选项 B:对 tmux+hook backend 调高/取消 `EVENT_IDLE_TIMEOUT`。**MVP 取选项 A 的"timer-reset-only 分支"**(最小且不碰其他 backend)。具体形态 functional-design 定,但**硬约束:保活绝不能产生用户可见输出、绝不能 detach/reset preview**。
- **边界**:改 `agent/tmux/session.rs` poll_loop(心跳改静默)+ `engine/events.rs`(加 timer-reset-only 分支)+ config(与 U-4a 共享 session.rs,m-3)。
- **测试**:hook 模式下 poll_loop 不发 Text/Result/Image;**permission 仍检测**(m-4);**长轮(>300s)不误触 idle timeout**(M-1);**hook 模式长轮零 `🧠` 可见消息、preview 不被 detach**(B-2 回归断言)。
- **依赖**:U-4(需 hook 路径可用才切模式)。
- **对应**:FR-6(部分,cutover 期门控)、ADR-5(修正:静默保活)、B-3、M-1、B-2。

## U-6 Stop 文字端到端打通(MVP 完成点)

- **内容**:串起 U-1..U-5 的 Stop 路径:hook 脚本(U-7 的 Stop 部分)→ U-3 → U-4 channel → 既有 process_agent_events 消费 Result → Platform.reply。验证手机收到文字。
- **`/btw` 范围决定(M-2)**:`/btw`(`mod.rs:617`)mid-turn send-keys 会产生自己的 Stop→Result,在单消费者模型里会 `break`(`events.rs:397`)原 turn 早退。**MVP 决定:tmux+hook 模式下 `/btw` 显式划为不支持**(文档化于此;若用户在 hook 会话用 `/btw`,行为未定义/可能提前结束 turn)。完整处理(turn 序列化)留 v2。
- **边界**:集成 unit,主要是端到端验证 + 调通,非新代码。
- **测试**:**live 端到端**(自测纪律):真实 cc + tmux + hook,手机/Discord 收到 Stop 文字。**必含 >300s 长轮**(验 M-1 保活,否则 idle gap 被短轮掩盖)。
- **依赖**:U-1..U-5、U-7(Stop)、U-8。
- **对应**:US-1/2/4/5、FR-4。**= MVP / 批次1 完成。**

## U-7 Hook 脚本(`scripts/agentbridge_hook.py`)

- **内容**:python3 单文件,注册为 Stop+PostToolUse hook,读 stdin POST 到接收端。try/except 全包、出错 exit 0、短超时。
- **边界**:独立脚本,契约=ADR-8 端口 + payload 透传。
- **测试**:喂样例 payload,验证 POST body;杀接收端验证 exit 0 不阻塞。
- **依赖**:U-3 端口契约(ADR-8)。可与 Rust 侧并行写。
- **对应**:FR-1。

## U-8 Hook 安装器(`agentbridge hook-install` 子命令)

- **内容**:合并写 ~/.claude/settings.json 的 hooks 段指向 U-7 脚本 + 实际端口(ADR-8),幂等、保留既有。
- **边界**:CLI 子命令(main.rs)。
- **测试**:单测合并逻辑(空 settings / 已有 hooks / 重复安装幂等)。
- **依赖**:U-7 脚本路径、ADR-8 端口。
- **对应**:FR-7。

## U-9 PostToolUse 进度(events.rs per-turn inplace)— 批次2

- **内容**:`process_agent_events` 加 `tool_progress_inplace` bool 参(从 process_and_drain 按 backend 传);ToolUse/Result 的 inplace 子分支(就地编辑 preview);U-7 脚本启用 PostToolUse 上报。
- **边界**:改 events.rs(加子分支,false 路径零改)+ 3 个签名加参 + U-7。
- **测试**:N=1/5/40 工具轮,新消息条数 ≤ 小常数(FR-5.2 验收);claude/acp(false)行为不变(回归测)。
- **依赖**:U-6(MVP 通)。
- **对应**:FR-5、US-3、NFR-2、ADR-4。**= 批次2。**

## U-10 移除截图路径 — 批次3

- **内容**:删 `render_screenshot`/截图 poll 分支/`render_term.py`/`find_python_with_pil`/`RENDER_SCRIPT`/settle;`AgentEvent::Image` 若仅截图用则删;移除 Pillow 运行时依赖引用。
- **边界**:删代码;保留 send-keys 输入。
- **测试**:删后文字回路仍工作(回归 U-6);`cargo build` 无 python/Pillow 引用。
- **依赖**:U-6 验证通过(cutover 规约,最后做)。
- **对应**:FR-6、US-6。**= 批次3。**

## 单元数与批次

- **批次1(MVP)**:U-1, U-2, U-3, U-4, U-5, U-7(Stop), U-8, U-6(集成验证)
- **批次2**:U-9(+ U-7 PostToolUse 部分)
- **批次3**:U-10

## Architecture Review

> Round 1 of max 2. Reviewer: architecture-reviewer (independent, first look at this decomposition).
> Reviewed against the authoritative **revision-3** design (`decisions.md` ADR-1/2/3/4/5/6/8 +
> the rev-3 narrative that RETIRES ADR-7/C-7/binder/Notify) and `components.md` C-1..C-6.
> Verified U-4/U-5 touch-points against real source: `src/agent/registry.rs`,
> `src/agent/tmux/session.rs`, `src/engine/mod.rs`, `src/engine/events.rs`, `src/config/mod.rs`,
> `src/core/event.rs`.
>
> **Note on the stale review block in `decisions.md`:** the `## Architecture Review` section at the
> end of `decisions.md` is the **R2 NOT-READY review of revision-2** (it critiques ADR-7, C-7, the
> binder, and Notify). Revision-3 explicitly retired all of those. The units correctly track
> revision-3, not that stale review. I judged the units against rev-3.

### Verdict: NOT-READY

The decomposition is well-structured, traceable, and the batch/cutover sequencing is sound. The
output side (U-1/U-2/U-3) is genuinely independently buildable and the MVP critical path is
correctly ordered. **But U-4 — the single load-bearing engine-integration unit — mis-locates the
revision-3 mechanism's primary touch-point.** ADR-1 rev-3's whole premise is "channel ownership 上移
到 `start_session_for_entry`," yet the channel is not created there in the real code; it is created
inside `TmuxAgent::start_session()` (`session.rs:211`) and the `event_tx` is moved into the poll
task while `TmuxSession` keeps only `event_rx` (`session.rs:28, 232-237`). Moving ownership up so the
registry can retain an `event_tx.clone()` is a real refactor of `TmuxAgent`/`TmuxSession` construction
— and U-4's declared 边界 ("触碰 registry.rs / engine mod.rs / config") **omits `src/agent/tmux/session.rs`
entirely.** A developer building U-4 from this document would discover mid-build that the channel is
not where the unit says it is, and that the "binder 落点" requires plumbing a sender clone out of
`TmuxAgent::start_session` (or restructuring who creates the channel). That is exactly the kind of
unstated integration risk a unit boundary is supposed to surface. Fix U-4's scope and one idle-timeout
gap in U-5 and this is READY — the rest is solid.

### Blocking

- **B-1 (BLOCKING) — U-4 omits the real channel-creation site; the rev-3 "channel 上移" is
  understated as a registry-only change.** Verified against `session.rs:211`: `let (event_tx, event_rx)
  = mpsc::channel::<AgentEvent>(128)` is created inside `TmuxAgent::start_session()`, `event_tx` is
  `move`d into the spawned `poll_loop` task (`session.rs:220-230`), and `TmuxSession` is constructed
  holding only `events_rx` (`session.rs:28, 234`). `start_session_for_entry` (registry.rs:124-125)
  merely calls `TmuxAgent::new(...).start_session().await`. So to register an `event_tx.clone()` in
  the C-4 registry "at `start_session_for_entry`," the channel must either be created in
  `start_session_for_entry` and threaded *down into* `TmuxAgent`/the poll task (changing the
  `TmuxAgent::start_session` signature/ownership), or `start_session` must return the extra clone
  alongside the `TmuxSession`. Both touch `src/agent/tmux/session.rs`. U-4's 边界 and the DAG risk note
  both list only "registry.rs / engine mod.rs / config" and assert "只在 start_session_for_entry 加
  channel clone 注册" — which is not implementable as scoped. **Required fix:** add
  `src/agent/tmux/session.rs` to U-4's touch-points and name the channel-ownership refactor explicitly
  (who creates the channel, how the clone reaches the registry, how it stays consistent with the
  `event_tx` the poll task holds). Without this, U-4 is not independently buildable as written, and the
  "no other unit touches session.rs except U-5" assumption in the DAG is false (U-4 and U-5 both edit
  session.rs — see m-3 below for the resulting collision).

### Majors

- **M-1 (MAJOR) — U-5 gates off the heartbeat but does not own the resulting idle-timeout gap; this
  can break long MVP turns.** Verified: the poll-loop heartbeat (`session.rs:418-430`,
  `HEARTBEAT_TICKS=80`) emits `AgentEvent::Thinking`, and `process_agent_events` wraps every
  `rx.recv()` in `tokio::time::timeout(EVENT_IDLE_TIMEOUT, ...)` with `EVENT_IDLE_TIMEOUT = 300s`
  (`events.rs:27, 78`). Today the heartbeat *resets* that 300s timer on a busy session. In hook mode,
  U-5 closes the `Thinking` emission (ADR-5 row 3) AND batch-1 has no PostToolUse, so a Stop-only turn
  that runs longer than 300s with no intermediate event will trip the idle timeout in
  `process_agent_events` (`events.rs:85-91`) — the user gets "💤 等太久了" and the turn loop `break`s
  *before* the hook's real Stop/Result arrives. This is a correctness gap on the MVP happy path for
  long tasks, not just a UX nicety. The design's m-1'/m-4 (in the stale R2 block) flagged the timeout
  constant for the retired ADR-7 turn-await; under rev-3 the same risk lands on the *unchanged*
  `EVENT_IDLE_TIMEOUT` consumed by `process_agent_events`. **No unit currently owns this.** U-5 (or
  U-4) must either: keep a lightweight liveness event flowing in hook mode to reset the timer, or
  raise/justify `EVENT_IDLE_TIMEOUT`, or explicitly scope "turns > 300s with zero hook activity" as a
  known MVP limitation. As written, U-6's "live 端到端" gate would pass on short turns and silently
  hide this on long ones.

- **M-2 (MAJOR) — `/btw` mid-turn injection interaction is unaddressed by any unit.** Verified
  `mod.rs:617-636`: `/btw` send-keys into the locked session mid-turn without driving its own
  consumer. Under rev-3 there is exactly one consumer (`process_agent_events` draining the shared
  channel) and `AgentEvent::Result` unconditionally `break`s that loop (`events.rs:397`). A `/btw`
  reply produces its own Stop hook → `Result` on the same channel → it will be the first Result to
  arrive and will **end the original turn early** (the original prompt's Stop is still pending). This
  is the rev-3 form of the retired-review's M-5, and it is real: the units inherit `/btw` from existing
  behavior but none of U-4/U-5/U-6 states what happens when a hook turn receives an interleaved Stop.
  This should be either scoped out for tmux+hook (documented in U-4) or handled, and U-6's live test
  should include a `/btw`-during-turn case. Not blocking MVP if explicitly scoped out, but it must be a
  named decision, not a silent gap.

### Minors

- **m-1 — U-3's "send 到 mock channel" test boundary is clean, but U-3 cannot fully verify routing
  without U-2's real registry; the DAG already encodes U-3←U-1,U-2 so this is consistent.** Just
  confirm U-3's integration test injects a *real* `HookRouteRegistry` (U-2) with a pre-bound mock
  sender rather than mocking `resolve`, so the cwd-prefix-match (ADR-6 m-2) is exercised end-to-end at
  the U-3 level rather than deferred to U-6.

- **m-2 — U-1/U-2 independent-starter claim is valid.** Verified: `AgentEvent` exists
  (`core/event.rs`) with `Result`/`ToolUse` variants; `map_hook` is a pure function over a serde
  struct (no engine coupling), and `HookRouteRegistry` is `Arc<Mutex<HashMap<..>>>` with no engine
  dependency. Both are genuinely buildable+testable in isolation and can run in parallel. The DAG's
  two no-dependency roots are correct. No change needed — recording the positive verification.

- **m-3 — U-4 and U-5 both edit `src/agent/tmux/session.rs`; the DAG's risk table implies only U-5
  does.** Once B-1 is fixed (U-4 touches session.rs for channel ownership) and U-5 touches session.rs
  for poll-loop gating, both units modify the same file. The DAG ordering (U-4→U-5) still serializes
  them correctly, so this is not a cycle — but delivery-planning/construction should know they share a
  file and U-5 builds on U-4's restructured channel/poll signature. Add a one-line note.

- **m-4 — U-5 ADR-5 coverage is complete (4 emission gates verified) but the table is "4" while ADR-5
  enumerates 5 rows.** U-5 says "关闭 Text/Result/Thinking/Image 四个发射点,保留 permission." ADR-5's
  table has 5 rows (the 5th, PermissionRequest, is *retained*). Verified all 5 sites exist: Text
  (`session.rs:500`), settle Result (`session.rs:509`), heartbeat Thinking (`session.rs:430`), Image
  (`session.rs:473`), PermissionRequest (`session.rs:380`). U-5's "保留 permission 检测" correctly keeps
  the 5th. So the count is fine (4 gated + 1 retained = ADR-5's 5 rows); just make U-5's test assert
  PermissionRequest *still fires* in hook mode, not only that the other 4 are silenced.

- **m-5 — U-4's config field name `hook_receiver.port` is new and consistent with ADR-8, but note the
  default must not collide with `default_webhook_port` (webhook's port, `config/mod.rs:45-52`).**
  ADR-8 picks 9123 vs webhook's 9111 — fine. U-4 should state the default value so U-3/U-7/U-8 share
  the literal (ADR-8's contract). Currently U-4 says "配置 hook_receiver.port" without the default;
  U-8/U-7's contract depends on it. Pin the number in U-4 (or reference ADR-8's 9123) so the three
  port-consuming units don't drift.

### Coverage check (C-1..C-6, rev-3 ADRs)

- **Components:** C-1→U-7 ✅; C-2→U-3 ✅; C-3→U-1 ✅; C-4→U-2(+U-4 integration) ✅; C-5→U-9 ✅;
  C-6→U-8 ✅. C-7 is correctly *absent* (retired in rev-3). No orphan component.
- **ADRs:** ADR-1(channel 上移 + binder 落点 + cleanup unbind)→U-4 — lands, **but understated (B-1)**;
  ADR-2(localhost axum/python3)→U-3/U-7 ✅; ADR-3(Stop→Result, empty→None, token-0)→U-1 ✅ (U-1
  explicitly covers empty→None per M-3); ADR-4(per-turn `tool_progress_inplace` bool, 3 signatures)→U-9
  ✅ (matches `events.rs` ToolUse/Result branches verified at 171/339; false-path zero-regression is
  correctly the test); ADR-5(4 gates + permission retained)→U-5 ✅ (see m-4); ADR-6(cwd-prefix
  resolve)→U-2 ✅; ADR-8(port contract)→U-4/U-7/U-8 ✅ (see m-5). ADR-7 correctly *not* unitized
  (retired). **All rev-3 design elements land in a unit; the only defect is U-4's mis-scoped
  touch-points, not a missing unit.**

### DAG correctness

The MVP critical path `U-1/U-2 → U-3 → U-4 → U-5 → U-6` is sound and acyclic. U-7/U-8 can genuinely run
in parallel with the Rust side once ADR-8's port literal is fixed (m-5) — the only cross-edge is the
port contract, which the DAG handles via "端口契约可先定." U-8←U-7 (script path dependency) is correct.
U-9←U-6 and U-10←U-6 are correct (batch-2/3 build on the proven MVP). No circular dependency. The one
correction: U-4 and U-5 share `session.rs` (m-3), already serialized by the existing edge — no DAG
change needed, just annotation.

### Batch alignment

MVP = U-1..U-8 + U-6 收口, batch2 = U-9, batch3 = U-10 is coherent with the cutover constraint:
screenshot removal (U-10) is correctly last and gated on U-6 verification (US-6 / FR-6.3 no-gap
cutover). The dual-track period (screenshot code present but mode=hook via U-5) is the right mechanism
to avoid a "nothing works" window. Batch boundaries respect the upstream traceability table. Sound.

### Testability / granularity

Test boundaries are real: U-1 pure-fn unit tests, U-2 registry unit tests, U-3 axum integration test,
U-5 poll-loop assertion tests (both modes), U-8 settings-merge unit tests, U-9 N=1/5/40 message-count
bound. U-6's **live end-to-end** as the MVP gate is the right call (matches the self-test-before-asking
discipline) — with the caveat from M-1 that it must include a >300s turn (or the idle-timeout gap stays
hidden) and from M-2 a `/btw`-during-turn case. Granularity is appropriate: units are vertical-ish
slices, none so coarse it hides integration risk except U-4 (which is exactly where B-1 bites — U-4
bundles channel-refactor + bind + unbind + receiver-spawn + config into one unit; consider whether the
channel-ownership refactor warrants being called out as its own sub-deliverable inside U-4 so the risk
is visible to construction).

### What is genuinely sound

The output/transport decomposition (U-1/U-2/U-3) is clean, independently buildable, and grounded in
real types. The rev-3 mechanism choice — feed the existing channel, let unchanged `process_agent_events`
consume it, `Result` breaks the loop (`events.rs:397`) exactly like the old poll settle — is verified
correct and is the right simplification over the retired ADR-7/C-7 model. Coverage and traceability are
complete with no orphans. The single thing standing between this and READY is **U-4**: its declared
scope omits the channel-ownership refactor in `session.rs` that ADR-1 rev-3 actually requires (B-1),
and the heartbeat-gating idle-timeout gap (M-1) plus `/btw` interaction (M-2) need an owner. Pin U-4's
real touch-points, assign the idle-timeout and `/btw` decisions, and this is READY.

---

## Architecture Review — Round 2

> Round 2 of max 2. Reviewer: architecture-reviewer (same independent reviewer, re-checking the R1
> findings against the revised `unit-of-work.md` + `unit-of-work-dependency.md`).
> Re-verified against real source: `src/agent/registry.rs:124-125`, `src/agent/tmux/session.rs:211/227/234/418-431`,
> `src/engine/events.rs:27/78/85-91/157-168/397`, `src/config/mod.rs:117/139`.

### Verdict: NOT-READY

Three of the four R1 findings are genuinely resolved, and the resolutions are honest and code-grounded
(B-1, M-2, and all minors). But the M-1 fix — keeping the heartbeat `Thinking` in hook mode as a
"pure keepalive the display layer may not render" — is **factually wrong against the current code** and
introduces a new, user-visible regression that lands squarely on the MVP happy path. The keepalive is
not silent: with the shipped defaults it spams the user with `🧠 Working…` messages every ~12s and
fragments the streamed reply. This is a NEW blocking issue (B-2) created by the revision, not a leftover
from R1. One targeted fix to U-5b closes it.

### Resolved since R1

- **B-1 — RESOLVED.** U-4a now explicitly names the `session.rs` channel-ownership refactor and lists
  `src/agent/tmux/session.rs` in U-4's 边界 (line 38) and in the dependency risk table (line 42).
  Re-verified the real shape: `registry.rs:124-125` calls `TmuxAgent::new(...).start_session().await`
  with no channel handle; `session.rs:211` creates `(event_tx, event_rx)` internally; `event_tx` is
  `move`d into the `poll_loop` task (`session.rs:220-230`); `TmuxSession` holds only `events_rx`
  (`session.rs:234`). U-4a's "change `start_session` to return `(TmuxSession, mpsc::Sender<AgentEvent>)`
  or accept a caller-supplied channel, same-source as the poll task's sender" is exactly the refactor
  required, and the "same-source as poll task sender / TmuxSession events_rx" test (line 40) pins the
  consistency invariant. Honestly scoped and buildable.

- **M-2 — RESOLVED (acceptable scope-out).** U-6 (line 56) now explicitly documents `/btw` as
  UNSUPPORTED for tmux+hook in MVP, with v2 deferral. Re-verified `mod.rs:617` (`/btw` send-keys into
  the locked session) and `events.rs:397` (`Result` unconditionally `break`s the single consumer). For
  a single-user tool an explicit, documented "behavior undefined / may end the turn early" decision is a
  legitimate resolution — the gap is now a named decision, not a silent trap. Acceptable. (Minor: U-6's
  live-test list should add a `/btw`-during-turn smoke so the documented behavior is at least observed
  once, but this is not blocking.)

- **m-3 — RESOLVED.** U-4/U-5 shared `session.rs` is now noted in both the unit (line 39) and the
  dependency risk table (line 43), with the U-4→U-5 serialization called out.

- **m-4 — RESOLVED.** U-5's test list (line 49) now asserts PermissionRequest still fires in hook mode
  AND heartbeat Thinking still fires. Both align with the retained-vs-gated split.

- **m-5 — RESOLVED.** Port `9123` is pinned in U-4e (line 37) as the shared U-3/U-7/U-8 contract and
  re-stated in the dependency risk table (line 46). Re-verified it does not collide with
  `default_webhook_port` (9111, `config/mod.rs:51`).

### Blocking (NEW — introduced by the M-1 revision)

- **B-2 (BLOCKING) — U-5b's "keep heartbeat `Thinking` as a non-rendered keepalive" is false against
  the current code; as written it produces visible `🧠 Working…` spam and fragments the reply on the
  MVP happy path.** U-5b (line 47) and the dependency note (line 44) claim the kept heartbeat is "纯保活,
  display 层可不渲染给用户" (pure keepalive, display layer may choose not to render). The display layer
  has no such opt-out today. Verified path:
  - `process_agent_events` handles `AgentEvent::Thinking` at `events.rs:157-168`. It is gated only on
    `display.thinking_messages`, which **defaults to `true`** (`config/mod.rs:117,139`).
  - When true, each Thinking event calls `freeze_and_detach_preview(...)` (`events.rs:159`) **and**
    `platform.reply(ctx, "🧠 {status}")` (`events.rs:166`) — a real user-visible message.
  - The `!content.is_empty()` guard (`events.rs:158`) does not save it: the heartbeat status is never
    empty — `session.rs:420-426` falls back to `"Working…"` via `unwrap_or`, so `content` is always a
    non-empty string (e.g. `"Working…"`, `"Billowing… (1m 42s · ↓ 2.9k tokens)"`).
  - Cadence: `HEARTBEAT_TICKS = 80` × 150ms poll = a candidate emit every ~12s, de-duped only when the
    status line text changes (`session.rs:428`) — which it does constantly (the elapsed timer/token
    count are embedded in the footer). So on a long Stop-only turn the user receives a stream of
    `🧠 …` messages roughly every ~12s.

  Two concrete regressions follow, both worse than the idle-timeout it was meant to fix:
  1. **Output spam** on exactly the turns M-1 cares about (>300s tasks) — the user's chat fills with
     `🧠 Working…` lines. This is a UX regression versus today's screenshot mode (where the heartbeat
     also fires, but the design's whole point was to move to clean text relay).
  2. **Preview/U-9 interference.** Every heartbeat calls `freeze_and_detach_preview` (`events.rs:159`),
     which freezes and detaches the live streamed-text preview (`events.rs:432-450`). In hook mode the
     final reply text arrives via the Stop→Result preview/`final_text` path; a heartbeat mid-turn
     detaches any active preview and forces the next text into a fresh message — fragmenting the reply.
     This directly collides with U-9's in-place tool-progress preview (batch-2), whose entire value
     proposition (FR-5.2: new-message count ≤ small constant) is destroyed if a ~12s heartbeat keeps
     detaching the preview underneath it.

  The keepalive requirement (reset the 300s `EVENT_IDLE_TIMEOUT`) is legitimate and U-5b correctly
  identifies it — but "keep the existing heartbeat Thinking unchanged" is the wrong mechanism because
  `Thinking` is a *rendered* event by default. **Required fix (pick one and write it into U-5b as the
  owned change, not a parenthetical):**
  - (a) Introduce a dedicated **non-rendered liveness event** (e.g. a `Keepalive`/`Tick` variant, or a
    silent branch in `process_agent_events` that resets the timer without `reply`) and emit *that* from
    the poll loop in hook mode, instead of `Thinking`. This makes "display layer does not render it"
    actually true and is the only option that is also safe for U-9. **Recommended.**
  - (b) Raise/justify `EVENT_IDLE_TIMEOUT` for the hook backend so no intermediate event is needed at
    all (U-5b's listed alternative ②). Simpler, but a bare timeout bump trades a false-timeout for a
    slow stall-detection — name the new value and the trade-off.
  - U-5b's current alternative ① ("keep heartbeat Thinking") must be **removed or rewritten**, because
    as literally scoped it depends on a display-layer suppression that does not exist. If the team still
    wants ①, it must additionally own a change to `events.rs:157-168` so the hook-mode heartbeat is
    consumed silently — at which point it is really option (a) in disguise.

  Until U-5b names a keepalive mechanism that is genuinely non-rendered (and therefore safe for both the
  MVP reply and U-9's preview), a developer building U-5 from this document will either ship the spam or
  silently re-introduce the idle-timeout break by gating the wrong event. That is an unstated assumption
  at exactly the integration seam M-1 was supposed to close.

### Assessment

- **M-1 status: NOT resolved — superseded by B-2.** The revision correctly *identified* the idle-timeout
  gap and correctly tied it to `EVENT_IDLE_TIMEOUT`/the heartbeat, but the chosen MVP mechanism
  (`keep heartbeat Thinking`) is not implementable as a silent keepalive against the current
  `process_agent_events`. The coherence with ADR-5 is fine in intent (ADR-5's "close heartbeat" should
  be corrected to "replace the rendered heartbeat with a silent liveness signal in hook mode"), but the
  unit must carry the real mechanism, not the aspiration.
- Everything else in the decomposition remains sound, as established in R1: clean output/transport split
  (U-1/U-2/U-3), correct rev-3 mechanism (feed the existing channel; unchanged consumer; `Result` breaks
  at `events.rs:397`), complete C-1..C-6 coverage with no orphans, acyclic DAG, and a coherent
  cutover-last batch plan. B-1, M-2, and all minors are genuinely closed.

### What would make this READY

Rewrite U-5b to own a concrete, non-rendered keepalive (option (a): a silent liveness event/branch that
resets `EVENT_IDLE_TIMEOUT` without calling `platform.reply` or `freeze_and_detach_preview`; or option
(b): a named `EVENT_IDLE_TIMEOUT` bump for the hook backend with the stall trade-off stated). Add to
U-5's tests: hook-mode long turn does NOT emit user-visible `🧠` messages and does NOT detach the
preview, while still resetting the idle timer. With that one change, B-1/M-2/m-3/m-4/m-5 already resolved,
the decomposition is READY. This is the last item.
