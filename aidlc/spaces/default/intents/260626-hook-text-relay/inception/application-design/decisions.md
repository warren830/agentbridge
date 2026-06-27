# Architecture Decisions — Hook-Based Real-Time Text Relay

> **修订 3(2026-06-27)**:architecture-reviewer R2(达 max 2 轮)NOT-READY,新 blocking B-4:修订 2 的"常驻消费者 + ADR-7 引擎按 backend 分流 turn"把缺陷从消费者转移到 turn 驱动器 —— `process_and_drain` 对所有 backend 统一硬接 `take_events`/`process_agent_events`,且无"bind"生命周期钩子;一种读法会在 300s idle 死锁。**修订 3 取消 ADR-7 的引擎分流与独立消费者 task**,改为**hook 事件喂进 session 既有 event channel,由原样不变的 `process_agent_events` 消费**(turn 协调零改动)。B-4/M-4/M-5 由此消解;代价(纯空闲推送)显式缩到 v2。
>
> **修订 2(已被修订 3 取代,保留作记录)**:R1 NOT-READY 后改"每绑定会话常驻消费者 task";R2 发现其 turn 协调(ADR-7)不可建 → 修订 3 取代。

## ADR-1(修订 3)hook 事件喂进 session 既有 event channel,经原样 `process_agent_events` 消费

- **Context**:hook payload 要变成手机消息。修订 2 的独立消费者需引擎按 backend 分流 turn(ADR-7),但 `process_and_drain`(`mod.rs:789-1086`)对所有 backend 统一走 `take_events`→`process_agent_events`→`put_events_back`,且 race-free 解锁逻辑(`drain_pending_messages:1162-1325`)写死在这套契约上;另起 turn 驱动器要重复这套(B-4.1),且"binder"在引擎里无生命周期落点(B-4.2)。
- **Decision**:**不另起 turn 驱动器、不另起消费者 task。** hook 事件流进 session 既有的那条 `events_rx`(`process_agent_events` 正在 drain 的同一条):
  1. **channel 所有权上移到 `start_session_for_entry`(`registry.rs`,tmux 分支)——这就是"binder"的真实落点**(B-4.2 解)。它创建 `(event_tx, event_rx)`,把 `event_tx` 交给 TmuxSession 的 poll 任务(同今),把 `event_rx` 给 TmuxSession 持有(同今),**额外保留一个 `event_tx.clone()` 注册进 C-4 路由表**(keyed by 规范化 work_dir)。
  2. hook 接收端 `resolve(cwd)` → 拿到该 session 的 `event_tx` clone → `send(AgentEvent)` 进同一条 channel。
  3. 当手机消息触发 engine turn 时,`process_agent_events` 正在 drain `event_rx`;hook 的 `Result`(Stop)到达即结束该轮 —— **与旧 poll-loop settle Result 完全相同的消费路径**(B-4.1 解,`process_and_drain` 零改)。
  4. C-4 的 `event_tx` clone 在 **既有的 `cleanup_agent_session`(`mod.rs:1657`)** 移除 —— 真实落点,无需新钩子。
- **Consequences**:
  - ✅ `process_and_drain`/`drain_pending_messages` 的 race-free 解锁逻辑**零改动**(B-4.1 解);
  - ✅ "binder" = `start_session_for_entry` tmux 分支,真实存在(B-4.2 解);
  - ✅ 无独立消费者 → 无 M-4 派发漂移(只有 events.rs 一份派发);
  - ✅ 无 Notify → 无 M-5 的 `/btw`/lost-wakeup 问题(`/btw` 的 Stop 像任何 Result 一样被 process_agent_events 处理);
  - ✅ 只经 capability traits、零平台分支(NFR-7);AgentSession 不特判;
  - ⚠️ **B-1 的 sender-clone 取舍(诚实记录)**:C-4 持 `event_tx` clone → poll 任务退出后 `event_rx.recv()` 不立即返回 `None`,死会话检测从"即时 None"退化为 `EVENT_IDLE_TIMEOUT`(300s,`events.rs:27`)兜底 + `cleanup_agent_session` 移除 clone 后恢复。hook 模式下 turn 完成信号本就是 hook Result(非 poll settle),此退化可接受;
  - ⚠️ **范围缩减(诚实记录,B-2 的 idle 子情形)**:纯空闲会话推送(电脑端直接驱动 cc、无任何 engine turn 在跑)在 MVP **不支持** —— 因 `event_rx` 仅在 engine turn 内被 `process_agent_events` drain。MVP 主场景(手机发消息→turn 在跑→Stop 回来)完全支持。**idle-only 推送列为 v2 增强**(届时再引入常驻消费者,但那时 turn 协调问题已被 v1 验证的事件契约简化)。
- **Alternatives**:① 修订 2 的独立消费者 + ADR-7 分流(否:B-4,turn 驱动器不可建、binder 无家);② clone 注入但 v1 就做 idle 推送(否:需常驻消费者,重蹈 B-4)。

## ADR-7(已取消 / RETIRED)

修订 2 的"独立消费者 + 引擎按 backend 分流 turn + turn_done Notify"已被**修订 3 的 ADR-1 取代**。原因:B-4(turn 驱动器不可建、binder 无生命周期落点、一种读法 idle 死锁)+ M-5(Notify 与 `/btw` 冲突)。修订 3 让 hook 事件走既有 `process_agent_events`,turn 协调零改动,这些问题全部消解。本条保留仅作决策轨迹。

## ADR-2 传输用 localhost HTTP(axum),hook 脚本用 python3 —— 不变

- localhost-only axum 接收端(镜像 `webhook.rs`);python3 脚本 POST。零新 crate 依赖(axum/serde 已在)。仅 localhost 无需鉴权/TLS。
- 见 ADR-8 的端口契约(原 m-3)。

## ADR-3(OQ-1)Stop→`Result`,PostToolUse→`ToolUse` —— 基本不变,补 M-3

- Stop 的 `last_assistant_message` → `AgentEvent::Result`(语义=一轮收尾);PostToolUse → 复用 `ToolUse`(不改枚举)。
- **M-3 补正**:① token 填 0 → `[ctx ~N%]` 指示器(`events.rs:369` gated on `input_tokens>0`)对 hook 轮**不显示** —— 记为能力损失(非"显示 0%");消费者侧不复制该指示器逻辑。② `map_hook` 对空 `last_assistant_message` **必须返回 None**(不发空轮)—— 这是消费者侧的强制规则(不是注释),由单测覆盖。注:消费者是独立 dispatcher,不经 events.rs 的 empty-resume guard(`events.rs:345`),所以空轮判断在 `map_hook` 自己做。

## ADR-4(修订 3)工具进度 = `process_agent_events` 内由**单个 per-turn backend 标志**控制的就地编辑 preview

- **Context**:FR-5.2 要逐工具可见但消息条数与 N 解耦。修订 3 取消了独立消费者,工具进度回到 `process_agent_events`。重新面对:① 如何不影响 claude/acp 的既有 ToolUse 行为(M-1);② events.rs Result 分支 `tool_count>0` 时 `discard_preview`+`reply` 与就地编辑冲突(M-2)。但修订 3 有个修订 2 没有的优势:**turn 知道自己的 backend**(`process_and_drain` 已有 `current_entry.backend`,`mod.rs:739`)。
- **Decision**:给 `process_agent_events` 传**一个 per-turn bool**(`tool_progress_inplace`,由 backend 决定:tmux+hook=true,claude/acp=false),而非 per-event 判别(M-1 的核心难点是 per-event 无判别字段 —— 但 per-turn 标志只需在调用点传一次):
  - `tool_progress_inplace=false`(claude/acp,默认):ToolUse 分支**原样不变**(freeze+reply),Result 分支**原样不变** —— 零回归。
  - `tool_progress_inplace=true`(tmux+hook):ToolUse → 就地编辑同一 progress preview(首个 send_preview,后续 update_preview,累积);Result → finalize progress preview(替代 `discard_preview`+reply 的冲突路径,M-2 在此分支内单独处理)再发文字。
  - 标志穿线:`process_and_drain`(已知 backend)→ `run_event_loop_and_save` → `process_agent_events`,加一个 bool 参数(3 个签名各加一参;签名已 `#[allow(too_many_arguments)]`,加一个 bool 影响可控,远小于修订 2 的独立 turn 驱动器)。
  - 节流复用 `StreamPreview` MIN_INTERVAL/MIN_DELTA + `outgoing_ratelimit`。消息 ~1/轮,与 N 解耦。`render_progress` char-boundary 安全(NFR-5)。
- **Consequences**:✅ M-1 解(per-turn 标志,非 per-event;claude/acp 传 false 走原路,零回归);✅ M-2 解(就地分支内自管 preview 生命周期,不碰原 discard 逻辑;原分支 false 时不变);✅ FR-5.2 不刷屏+逐工具可见;✅ 无 M-4 漂移(只有 events.rs 一份派发);⚠️ events.rs 的 ToolUse/Result 分支各加一个 `if inplace {}` 子分支(复杂度小幅上升,需测两条路径)。
- **Alternatives**:① 修订 2 的独立消费者(否:B-4);② per-event 判别字段(否:要改 AgentEvent 枚举给每个事件加 source,blast radius 大且 M-1 指出无现成字段)。

## ADR-5(重设计)hook 模式下 poll loop 输出全关 —— 枚举每个发射点(B-3)

- **Context**:原 ADR-5 只说"关文字抽取",但 reviewer 指出 poll loop 的 `Result`(`session.rs:507`)、心跳 `Thinking`(`session.rs:418`)、permission 发射**不受 screenshot flag 控制**,只关文字仍会有第二个 `Result` 与 hook 的 `Result` 竞争(B-3)。
- **Decision**:tmux backend 引入显式**输出模式**(`screenshot` | `hook`),hook 模式下**枚举并关闭** poll loop 的**所有输出型发射**:
  | poll loop 发射点 | 源位置 | hook 模式 |
  |---|---|---|
  | 文字块 `Text` | `session.rs:500` (`extract_reply_blocks`) | **关** |
  | 收尾 `Result` | `session.rs:507` (settle) | **关**(hook 的 Stop 是唯一 Result 源) |
  | 心跳 `Thinking` | `session.rs:418` (HEARTBEAT_TICKS) | **关**(改由 ADR-7 的 turn-await 超时 + PostToolUse 活动驱动,见 m-1) |
  | 截图 `Image` | screenshot 分支 | 关(本就只在 screenshot 模式) |
  | `PermissionRequest` | 数字菜单检测 | **保留**(权限菜单仍需检测;非 Result 源,不冲突) |
- **Consequences**:✅ 唯一 Result 源是 hook,无竞争(B-3 解);✅ cutover 期(批次1/2,截图代码未删但模式=hook)输出只走 hook;✅ 保留 permission 检测(否则 tmux 交互式 cc 的权限弹窗没法处理);⚠️ 心跳没了 → idle timeout 缺口,见 m-1(ADR-7 的 turn-await 用专门超时)。
- **Alternatives**:全局删 poll loop 输出(否:批次1/2 截图仍是 fallback,US-6 不留空窗;模式开关才能分批 cutover)。

## ADR-6 门控在 agentbridge 侧(C-4 注册表),hook 脚本无状态 —— 不变,补 m-2

- 门控在 C-4 `resolve` 未命中即丢弃;hook 脚本无状态、永远 POST。
- **m-2 补正(cwd 路由盲区)**:attach 场景下 cc 进程 `cwd` 可能与绑定的 `work_dir` 不一致(用户在子目录起 cc 或 `cd` 过),且 tmux session 的 `session_id()` 返回 None、hook 的 `session_id` 是 cc 的 agent session id(agentbridge 对外部起的 cc 不知道)。**Decision**:C-4 `resolve` 的匹配策略 = ① `cwd` 规范化后**前缀匹配**绑定的 `work_dir`(不只精确相等,容子目录);② 仍无则 `tmux_session` 名匹配(若 hook payload 能带 —— 注:hook 不直接给 tmux session 名,故主路是 cwd 前缀)。绑定时记 `work_dir` 规范化值。盲区(同一 work_dir 下多个 cc)记为已知限制 —— 单用户单会话场景可接受。

## ADR-8(新增)接收端端口契约(m-3)

- **Context**:C-1(脚本)、C-2(接收端)、C-6(安装器)要对齐端口,否则无法独立构建。
- **Decision**:固定默认端口(如 `9123`,与 webhook 的 9111、gateway 端口区隔;可经 config `hook_receiver.port` 覆盖)。C-6 安装器把**实际端口**写进全局 settings.json 的 hook command(`... POST http://127.0.0.1:<port>/hook-event`),所以脚本不需猜端口 —— 端口在安装期固化进 hook 命令行/环境变量。C-2 绑同一端口。
- **Consequences**:✅ C-1/C-2/C-6 有共享契约,可独立构建;✅ 端口可配。⚠️ 端口冲突时安装器应检测并报错(functional-design 落地)。

## 约束符合性核对(修订 3)

| 约束 | 符合 |
|------|------|
| 引擎不分支平台(NFR-7) | ✅ 经既有 process_agent_events → capability traits;hook 只是同一 channel 的另一事件源 |
| 零新依赖 | ✅ axum/serde/tokio 已有 |
| 无 unsafe / anyhow 边界 / tracing | ✅ |
| 不换 session try-lock 模式 | ✅ **`process_and_drain`/`drain_pending_messages` 零改动**(ADR-1 修订3,B-4.1 解) |
| TmuxSession channel | ⚠️ channel 所有权上移到 `start_session_for_entry`(binder 真实落点);C-4 持 event_tx clone → 死会话检测退化为 idle-timeout 兜底(已记,可接受);`cleanup_agent_session` 移除 clone |
| CJK 安全切片 | ✅ render_progress + 文字均 char-boundary 安全(NFR-5) |
| AgentSession 不特判新 agent | ✅ hook 是 tmux session 的第二事件源,非新 agent 类型;无新 trait/分派 |
| 不回归 claude/acp | ✅ per-turn `tool_progress_inplace=false` 时 events.rs 原样(ADR-4 修订3,M-1/M-2 解) |
| turn 协调 | ✅ 零改(hook Result 走既有 process_agent_events,无 Notify/无 backend 分流;M-5 消解) |
| cutover 不留空窗(US-6) | ✅ 输出模式开关分批 cutover(ADR-5) |
| 范围诚实 | ⚠️ idle-only 推送(无 engine turn 时)缩到 v2(ADR-1 修订3,B-2 idle 子情形) |

## Architecture Review

> Round 2 of max 2. Reviewer: architecture-reviewer (independent). Verified against real code:
> `src/agent/tmux/session.rs`, `src/engine/mod.rs`, `src/engine/events.rs`, `src/core/session.rs`,
> `src/core/platform.rs`, `src/core/streaming.rs`, `src/agent/mod.rs`, `src/agent/registry.rs`,
> `src/config/mod.rs`, `src/core/event.rs`.

### Verdict: NOT-READY

The seam redesign (binder-owned channel + always-on C-7 consumer) genuinely dissolves B-1,
B-2, M-1, M-2 — those were the hard R1 blockers and the new model resolves them cleanly and is
grounded in the real types. **However, ADR-7's engine turn-coordination introduces a new blocking
seam problem (B-4) that is not implementable as written against the real `process_and_drain`
pipeline, and the design does not name where the "binder" lives.** One round-1 blocker (B-3) and
two majors are downgraded but one new structural gap remains. A developer cannot build the engine
integration from this document without architectural decisions that are not yet made.

### R1 findings — re-judged

- **B-1 (TmuxSession sender ownership) — RESOLVED.** Verified `session.rs:26-34`: `TmuxSession`
  retains only `events_rx`; the sole `event_tx` is moved into the poll task (`session.rs:211,
  220-230`). The new model never touches this — the binder creates an independent
  `(hook_tx, hook_rx)` (`component-methods.md` C-7, `mpsc::channel(128)`) and registers `hook_tx`
  in C-4. C-2 reaches it via `registry.resolve(cwd)` → `hook_tx.clone()`. The channel is genuinely
  binder-owned and reachable. No clone of TmuxSession's sender. Correct.

- **B-2 (idle-cc / agent-initiated case) — RESOLVED in principle.** The always-on C-7 task
  (lifecycle = binding lifecycle, not turn lifecycle) does solve the "no engine turn in flight"
  case: `hook_rx.recv()` has a permanent consumer independent of `process_agent_events`. This is
  the right shape. Coexistence purity holds *as specified*: C-7 output never takes the session
  lock (ADR-7), and only C-7 reads `hook_rx`, so no double-consume. The claude/acp paths never
  feed `hook_rx`, so no double-reply from those. **Caveat folded into B-4 below:** the design
  asserts the engine's tmux turn "no longer calls `process_agent_events`" — but it never says what
  *does* drive the turn, and the real `process_and_drain` unconditionally calls
  `cs.take_events()` + `process_agent_events` + `put_events_back` (`mod.rs:890, 919-923, 1038`)
  for *every* backend. The double-reply is only avoided if that hard-wired path is actually
  bypassed for tmux+hook, which is exactly the unbuilt seam.

- **B-3 (poll-loop emission gating) — RESOLVED at the design level, but verify the flag plumbing.**
  ADR-5 now enumerates all four output emissions and matches the real source sites:
  Text (`session.rs:500`), settle Result (`session.rs:509`), heartbeat Thinking (`session.rs:430`),
  Image (`session.rs:473`), with PermissionRequest (`session.rs:380`) correctly retained as a
  non-Result source. Gating all four in hook mode does eliminate the duplicate-Result race. This
  is sound. One unstated dependency: `TmuxConfig` (`config/mod.rs`) currently has only a
  `screenshot: bool` — the design's "output mode (`screenshot` | `hook`)" requires a new config
  field threaded into `poll_loop`'s signature (it takes `screenshot: bool` at `session.rs:218,
  252`). That is a mechanical add and the design flags it, so B-3 is resolved, not blocking.

- **M-1 (no source discriminator) — RESOLVED.** Moving progress-preview into C-7 means
  `AgentEvent::ToolUse` from claude/acp never enters the hook consumer (they flow through
  `process_agent_events` unchanged). The source-discrimination problem is dissolved, not patched —
  this is the correct architectural move. Verified `events.rs:171-196` ToolUse branch is untouched.

- **M-2 (Result-discard conflict) — RESOLVED.** C-7 owns its own preview lifecycle
  (`component-methods.md` C-7: `delete_preview` on Result then `reply`), so it never hits the
  `events.rs:374` `tool_count>0 → discard_preview + reply` branch. The conflict is avoided by
  separation. Correct.

- **M-3 (empty / token-0 guard) — RESOLVED.** ADR-3 now mandates `map_hook` returns `None` on empty
  `last_assistant_message` (enforced in C-3, single-test-covered), and C-7's Result arm guards
  `if !content.is_empty()` (`component-methods.md:112`). Since C-7 does not pass through the
  `events.rs:345` empty-resume guard, doing the check in `map_hook` is the right call. The ctx-%
  indicator being absent for hook turns (token=0) is correctly logged as a capability loss, not a
  "0%" bug.

- **m-1 (idle heartbeat gap) — PARTIALLY addressed; see m-1' below.** ADR-7 replaces the poll-loop
  heartbeat with a `turn_done` timeout on the engine's await. This covers the *engine's* stall
  detection. But it silently drops the heartbeat's *second* job. The real heartbeat
  (`session.rs:418-432`) also surfaced live progress to the user during a long Stop-only (batch-1)
  turn. In batch 1 there is no PostToolUse, so a 10-minute turn shows the user nothing until Stop.
  The old `EVENT_IDLE_TIMEOUT` is 300s (`events.rs:27`); the design defers the new timeout constant
  to functional-design without noting it must exceed realistic turn length or it will false-timeout
  long tasks. Downgrade to minor m-1', not blocking, but name it.

- **m-2 (cwd routing) — RESOLVED with a correctly-scoped known limitation.** cwd-prefix-match
  against canonicalized `work_dir` is the right primary strategy (SessionManager already
  canonicalizes work_dir, `session.rs:279-282`). The "multiple cc under one work_dir" blind spot is
  explicitly accepted for single-user/single-session. The tmux-session-name fallback is correctly
  noted as mostly-unavailable (hook payload carries cc's agent session_id, not the tmux name;
  `TmuxSession::session_id()` returns `None`, `session.rs:88-91`). Honest and adequate.

- **m-3 (port contract) — RESOLVED.** ADR-8 fixes a default port, has the installer bake the actual
  port into the hook command line, and gives C-1/C-2/C-6 a shared contract. Coherent.

### New blocking finding introduced by the redesign

- **B-4 (BLOCKING) — ADR-7's engine turn-coordination is underspecified and collides with the real
  per-turn pipeline; the "binder" has no home in the current engine.** Two concrete gaps:

  1. **No backend branch point is identified, and the bypass is non-trivial.** The real turn engine
     is `process_and_drain` (`mod.rs:789-1086`) → `run_event_loop_and_save` → `process_agent_events`.
     This path is *uniform over `Box<dyn AgentSession>`*: it calls `cs.send()`, then
     `cs.take_events()` (`mod.rs:890`), runs `process_agent_events` (`mod.rs:1105`), and
     `put_events_back` (`mod.rs:1038`). The engine never knows the backend inside this function.
     ADR-7 says tmux+hook becomes "try-lock → send-keys → await turn_done → unlock → drain" and
     "no longer calls process_agent_events" — but does not say *where* that fork happens. The
     backend string is reachable (`current_entry = config.find_agent(&current_agent)` at
     `mod.rs:739` exposes `entry.backend`), so a branch is *possible* — but the entire body of
     `process_and_drain` (typing indicator, agent-spawn-on-demand, dead-agent retry at
     `mod.rs:928-1025`, auto-compress, **and the race-free drain-queue unlock at
     `drain_pending_messages:1162-1325`**) is written around the `take_events`/`process_agent_events`
     contract. A tmux+hook turn that instead awaits a `Notify` needs a parallel implementation of
     that whole control-flow (including the load-bearing "unlock while holding the state mutex"
     race guard, `mod.rs:1175-1184`). The design says "functional-design must坐实分流点" but the
     *shape* of the fork is the architecture decision, and it is deferred. As written, a developer
     cannot tell whether tmux+hook reuses `process_and_drain` (then it WILL call
     `process_agent_events` and double-drive the turn against a now-silent event channel — the poll
     loop emits nothing in hook mode, so `process_agent_events` would block on `rx.recv()` until the
     300s idle timeout, **breaking every turn**), or whether a second turn-driver is built (then the
     queue/lock/retry/compress logic must be duplicated or refactored, which is a far larger change
     than "tmux backend output enhancement").

  2. **The "binder" is a new actor with no location in the codebase.** ADR-1/ADR-7 and all five
     docs attribute channel creation, C-4 registration, C-7 spawn, and `turn_done` ownership to "the
     binder (引擎在桥接会话时)". But there is no binder today. Sessions are created lazily inside
     `process_and_drain` via `start_agent_session_for_key` → `start_session_for_entry`
     (`registry.rs`), which returns an opaque `Box<dyn AgentSession>` and stores it in
     `EngineInteractiveState.agent_session` (`mod.rs:55, 861`). There is no "bridge a session" event
     distinct from "first message spawns the agent." `/attach` only writes `tmux_session` into
     SessionManager (`session.rs:533-556`) and then `cleanup_agent_session` (`mod.rs:1657-1659`); it
     does not spawn anything. So "binder spawns C-7 at bind time" must be mapped onto a real
     lifecycle point — most plausibly the `need_new` agent-spawn block in `process_and_drain`
     (`mod.rs:843-871`), which means C-7's lifecycle is actually coupled to agent-session creation,
     not to `/attach`. That contradicts the stated "task lifecycle = binding lifecycle (/attach …
     start)" and reopens B-2's idle case: if C-7 is only spawned on the first message's agent spawn,
     an idle hand-attached cc that the user drives from the laptop *before* ever messaging from the
     phone has no consumer — the very scenario B-2 was meant to fix. The design must pin C-7's spawn
     to a lifecycle hook that actually fires for attach-without-first-message, and that hook does not
     exist in the engine today.

  Net: the output side (C-2/C-3/C-4/C-7/C-5) is implementable and sound. The *input/turn-coordination
  side* (ADR-7) names a model but not a buildable integration, and one reading of it
  (reuse `process_and_drain`) deadlocks every tmux+hook turn on the idle timeout. This is the same
  class of defect as R1's B-2 (a control-flow assumption that the real code does not support),
  relocated from the consumer to the turn-driver. It must be resolved before code-generation.

### Majors

- **M-4 (drift mitigation is asserted, not designed).** ADR-1/ADR-4/C-7 repeatedly promise a "shared
  helper" so C-7's AgentEvent→capability dispatch does not drift from `events.rs`. But the two call
  sites are not actually congruent: `events.rs` ToolUse does `reply()` per tool
  (`events.rs:194`), while C-7 does `send_preview`/`update_preview` accumulation
  (`component-methods.md:101-108`) — *opposite* behaviors by design (M-2's whole point). The only
  genuinely shareable surface is the Result-finalize + char-safe truncation, which is small. The
  "shared helper" is therefore mostly aspirational; the design should either name the *specific*
  shared function (e.g. a `render_*` + `delete_preview→reply` finalize) or drop the claim. As
  stated it reads as a guarantee the code shape cannot honor. Not blocking, but it weakens the
  "no drift" consequence cited under ADR-1/ADR-4.

- **M-5 (`/btw` mid-turn injection interaction unaddressed).** The engine supports `/btw` to inject
  text into a running agent while the session is locked (`mod.rs:617-636`). For tmux+hook, `/btw`
  send-keys into the same pane mid-turn; the resulting Stop fires `turn_done.notify_waiters()`,
  which would unblock the engine's `await turn_done` for the *original* prompt prematurely (the
  `/btw` reply's Stop arrives first). With `Notify`, a `notify_waiters()` with no current waiter is
  also lost, and a single Notify cannot distinguish "which turn finished." The design must specify
  turn-done as a counted/sequenced signal or explicitly scope out `/btw` for tmux+hook. Not in the
  current docs at all.

### Minors

- **m-1' (no live progress in batch-1 Stop-only mode).** See m-1 above: dropping the heartbeat
  leaves long single-turn tasks with zero feedback until Stop in batch 1 (PostToolUse progress
  only lands in batch 2). Acceptable if intentional; should be stated as a batch-1 UX limitation.
- **m-4 (turn_done timeout constant undefined vs `EVENT_IDLE_TIMEOUT`).** functional-design must set
  it above realistic turn duration; note the existing 300s reference point and that a too-short
  value false-times-out long tasks (the heartbeat previously *reset* the 300s timer; nothing resets
  the new one).
- **m-5 (`Notify` lost-wakeup / ordering).** `tokio::sync::Notify::notify_waiters()` only wakes
  *currently registered* waiters; a Stop that arrives between unlock and the next `notified()` is
  lost. The engine must register the `notified()` future *before* send-keys, or use
  `notify_one()` semantics deliberately. Worth a sentence in functional-design.

### What is genuinely sound

The consumer/output redesign is a real improvement and resolves the core R1 blockers honestly: the
binder-owned channel (B-1), the always-on consumer (B-2 output side), the full poll-loop gating
enumeration (B-3), and the dissolution of source-discrimination by routing hook progress entirely
outside `events.rs` (M-1/M-2). These are grounded in the real types and are buildable. The single
thing standing between this and READY is ADR-7: the engine-turn-coordination model (backend fork
point + binder lifecycle + Notify semantics) is named but not made buildable against the real
`process_and_drain`/`drain_pending_messages` control flow, and one plausible implementation
deadlocks every turn. Close that (pin the fork point, the C-7 spawn hook, and the turn-done signal
semantics) and this is READY.
