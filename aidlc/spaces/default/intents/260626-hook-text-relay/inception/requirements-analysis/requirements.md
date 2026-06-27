# Requirements — Hook-Based Real-Time Text Relay

> 上游:`intent-statement.md`、`scope-document.md`、codekb(`business-overview`/`architecture`/`code-structure`)、`team-practices.md`。深度 Standard。

## Functional Requirements

### FR-1 Hook 脚本(注册 + 上报)
- **FR-1.1** 提供一个薄 hook 脚本(python3 或 shell),注册为 Claude Code 的 Stop hook 与 PostToolUse hook。
- **FR-1.2** 脚本从 hook stdin JSON 读取 payload,POST 到 agentbridge 本地 HTTP 接收端。
- **FR-1.3** 脚本携带 `session_id` 与 `cwd`(payload 已含)用于路由/门控。
- **FR-1.4** 脚本**永不阻塞 cc**:任何错误(POST 失败、超时、解析异常)一律 `exit 0`,不影响 cc 主流程。
- **验收**:在受控 cc 里触发 Stop/PostToolUse,接收端收到对应 POST;杀掉接收端时 cc 不卡、脚本静默退出 0。

### FR-2 本地 HTTP 接收端
- **FR-2.1** agentbridge 内提供一个仅监听 localhost 的 HTTP 接收端(复用 `axum`,参考 `webhook.rs` 结构),接收 hook payload。
- **FR-2.2** 用 `serde` 防御性反序列化:字段缺失/格式变化不 panic,降级处理或丢弃并 `tracing` 记录(warn)。
- **FR-2.3** 接收端把 payload 映射成 `AgentEvent` 并投入现有事件管线(`engine/events.rs`),**不触碰 platform 注册、不按平台名分支**。
- **验收**:畸形 payload 不致崩溃,记 warn;合法 Stop payload 产出一个 `AgentEvent` 进入管线。

### FR-3 session → channel 绑定与门控
- **FR-3.1** agentbridge 桥接某会话时,登记 `(cwd, tmux_session) → channel` 绑定(复用 `core/session.rs` 的 `tmux_session` 字段)。
- **FR-3.2** 接收端按 **cwd 优先、session_id 兜底** 解析 hook 属于哪个 channel。
- **FR-3.3** 查不到绑定 → **丢弃**该 hook(门控),确保非桥接的本地 cc 不会误发。
- **验收**:桥接会话的 hook 路由到正确 channel;未桥接目录的 cc 触发 hook 时被丢弃(无消息发出)。

### FR-4 Stop 文字回传(MVP 核心)
- **FR-4.1** Stop hook 的 `last_assistant_message` → `AgentEvent::Result`(或 `Text`),经现有管线 → `Platform` 发出一条干净文字回复。
- **FR-4.2** 文字内容与 cc 该轮 transcript 一致(直读 `last_assistant_message`,不截断、不串轮)。
- **验收**:cc 答完一轮,手机端收到与终端一致的一条文字(非截图)。

### FR-5 PostToolUse 逐工具进度(第二批)
- **FR-5.1** PostToolUse 的 `tool_name`/`tool_response`/`duration_ms` → `AgentEvent::ToolUse`(或轻量进度变体,事件变体选择见 OQ-1,functional-design 定)→ 管线 → Platform。
- **FR-5.2** **逐工具实时可见但不刷屏**:工具进度通过**就地编辑的进度消息**(复用现有 `MessageUpdater` preview / `StreamPreview` 机制,即 Text 流式回复同款)呈现 —— 一轮内每个工具调用都实时更新到**同一条**进度消息(显示当前/最新工具及累计),而非每工具发一条新消息。这同时满足用户的"看到每个工具进度"(Q1)与"无刷屏"(成功指标 #6 / ic-throttle 约束)。`outgoing_ratelimit` 作为编辑频率的二级兜底。
  - **设计依据(校正)**:`MessageUpdater::{send,update,delete}_preview` + `StreamPreview` 节流编辑机制现成存在(`core/platform.rs` / `core/streaming.rs`,Text 流式回复在用)。**但当前 `engine/events.rs` 的 `ToolUse` 走的是相反路径** —— `freeze_and_detach_preview`(reset 成新消息)+ `reply`,即每工具发**新**消息。因此"工具进度复用同一 preview 就地编辑"是**新接线**(复用现成 primitive,但改变 events.rs 现有 ToolUse 行为),不是现成路径直接拿来用。freeze 与 Stop 文字的衔接由 OQ-3 在 functional-design 落定。
- **FR-5.3** 可配置开关(类比现有 `display.tool_messages`,见 `src/config/mod.rs`)。
- **验收**(消息条数有界,可测):
  - 一轮含 N 个工具调用(N 取 1 / 5 / 40)时,该轮新发到 channel 的**进度消息条数 ≤ 一个小常数**(目标 1,允许因 preview freeze/重建产生的少量额外条),**绝不随 N 线性增长**。
  - 每个工具调用在进度消息里**可见**(就地编辑可观察到工具名/状态变化)。
  - 编辑频率不突破 `outgoing_ratelimit` 每频道限速。

### FR-7 Hook 安装 / 注册(接管已在跑的 cc)
- **FR-7.1** 提供把 Stop + PostToolUse hook 注册进 **全局** `~/.claude/settings.json` 的安装方式(因接管已在跑的 cc 无法事后注入 `--settings`,见约束 HC-4)。
- **FR-7.2** 安装为**合并而非覆盖**:保留用户 `~/.claude/settings.json` 已有的 hook/配置,重复安装幂等(参考 remote-claude-control 的 setup 合并语义)。
- **FR-7.3** 全局 hook 经 FR-3 的 cwd/session 门控,确保只对 agentbridge 桥接的会话生效,不影响用户其他纯本地 cc。
- **FR-7.4**(范围标注)agentbridge 自动起 cc 时用 `claude --settings <注入配置>` 的路径为**可选增强**,本批不实现(scope out-of-scope 已注明)。
- **验收**:安装后全局 settings 含 hook 且原有配置保留;在桥接目录的 cc 触发 hook 被处理,在非桥接目录的 cc 触发 hook 被 FR-3 门控丢弃(无消息)。

### FR-6 移除截图路径(第三批,最后)
- **FR-6.1** 移除 tmux backend 的 `render_screenshot`、截图 poll 分支、`scripts/render_term.py`、`AgentEvent::Image`(若仅截图用)及相关 `find_python_with_pil`/`RENDER_SCRIPT`/settle 检测。
- **FR-6.2** 保留 tmux **输入** 路径(`send-keys`)不变。
- **FR-6.3** 移除前临时双轨(截图仍在 + 文字已加),Stop 文字验证通过后才删(cutover 规约)。
- **验收**:截图代码删除后,文字回路仍完整工作;`cargo build` 无 python/Pillow 运行时依赖残留引用。

## Non-Functional Requirements

| NFR | 目标 |
|-----|------|
| NFR-1 延迟 | Stop 文字从 cc 答完到手机收到 **< 2s(本地条件下 p95)** |
| NFR-2 不刷屏(消息条数有界) | 一轮工具进度产生的**新消息条数与工具数 N 解耦**(目标每轮 ~1 条就地编辑的进度消息,不随 N 线性增长);编辑频率受 `outgoing_ratelimit` 二级兜底。这是 ic-throttle 约束的满足方式(就地编辑而非合并/丢弃) |
| NFR-3 资源 | 接收端轻量:localhost HTTP,无持久化,async(tokio/axum) |
| NFR-4 正确性 | 文字与 transcript 一致;门控确保不串 channel、不误发 |
| NFR-5 CJK 安全 | 所有文字切片用 char-boundary 安全方法(project 硬约束;含 StreamPreview 既有风险点) |
| NFR-6 健壮性 | hook 永不阻塞 cc;接收端防御性解析不 panic |
| NFR-7 工程约束 | 零新 crate 依赖;无 unsafe;anyhow 边界;tracing 结构化;引擎不分支平台 |

## Traceability

| 需求 | scope proto-unit | 批次 |
|------|------------------|------|
| FR-1, FR-2 | PU-1 传输层 | MVP |
| FR-3 | PU-2 门控 | MVP |
| FR-4 | PU-3 Stop 文字 | MVP |
| FR-5 | PU-4 PostToolUse+节流 | 批次2 |
| FR-6 | PU-5 移除截图 | 批次3 |
| FR-7 | PU-1/PU-2(安装+门控前置) | MVP |

## Open Questions(交给 functional-design)

- **OQ-1** 事件变体选择:Stop 文字用 `AgentEvent::Result` 还是 `Text`?工具进度复用 `ToolUse` 还是加轻量进度变体?(FR-4.1 / FR-5.1 暂留双选,functional-design 定夺。)
- **OQ-2** "Stop 文字验证通过"(FR-6.3 的 cutover gate)具体判据 = 成功指标 #2 / FR-4 验收通过(手机端收到与终端一致的文字)。functional-design 把它落成可执行的验证步骤。
- **OQ-3** 进度消息就地编辑的具体 UX:显示"当前工具"还是"已跑工具列表累计"?freeze 时机如何与 Stop 文字衔接?(FR-5.2 已定"同一条 preview",细节 functional-design 定。)

<!-- Revision 1 (2026-06-27): addressed reviewer round-1 NOT-READY.
  blocking FR-5.2/NFR-2 → reworked to in-place-edited progress message (reuse MessageUpdater/StreamPreview),
    message-count decoupled from tool count N; satisfies both "see each tool" (Q1) and "no spam" (metric #6 / ic-throttle).
  major (untestable) → FR-5.2 acceptance now bounds new-message-count ≤ small const, independent of N.
  major (missing install FR) → added FR-7 (global ~/.claude/settings.json merge-install + gating).
  minor (event-variant ambiguity) → moved to OQ-1.  minor (NFR-1 fuzzy) → firm "<2s p95".  minor (FR-6.3 gate) → OQ-2 ties to metric #2/FR-4. -->

## Review

**Reviewer:** Product Lead — quality gate
**Round:** 2 of 2 (max)
**Verdict:** READY

### Round-1 findings — disposition

**[Blocking] FR-5.2/NFR-2 — no-spam contradiction → RESOLVED.**
Round 1 rejected this because `outgoing_ratelimit` only *paces* edits/sends, it never *reduces* the message count, so "see each of N tools" inevitably produced N messages and the anti-spam value was not delivered. The rework changes the *mechanism*, not just the wording: tool progress is now an **in-place edit of a single progress message** (one `PreviewHandle`, repeated `update_preview`), so message count is decoupled from N rather than paced. That genuinely dissolves the contradiction — visibility comes from edits to one message, "no spam" comes from there being one message. NFR-2 correctly frames it as "就地编辑而非合并/丢弃," which is exactly the mechanism that delivers the value, and it traces cleanly to success metric #6 ("节流/合并策略") and the ic-throttle constraint.

I verified the design grounding against the codebase, and it holds at the capability level: `src/core/platform.rs` exposes `MessageUpdater::{send_preview, update_preview, delete_preview}` against a `PreviewHandle`; `src/core/streaming.rs` `StreamPreview` is a real Idle→Active→Frozen→Finished state machine with throttled edits (MIN_INTERVAL 1500ms / MIN_DELTA 30 chars) and `freeze`/`unfreeze`/`reset`; both Telegram and Discord implement edit-in-place. The building blocks the requirement leans on exist. This is a satisfiable requirement, not a wish — RESOLVED.

One correction the builder should carry into functional-design (not a blocker, a precision issue): the FR-5.2 phrase "Text 流式回复同款" and the "**设计依据**" citing codekb `architecture.md` overstate the grounding. The cited `architecture.md` and the *current* `src/engine/events.rs` do the **opposite** of what FR-5.2 specifies: today `AgentEvent::ToolUse` calls `freeze_and_detach_preview(...)` (which `reset()`s the preview into a fresh message) and then `platform.reply(...)` — i.e. freeze the text preview, then send a **new** message per tool. So FR-5.2 is a *new behavior* (reuse one preview message for tool progress, edit-in-place instead of freeze+new-reply), not an existing path being reused as-is. The requirement is still sound — the mechanism is feasible from existing primitives — but functional-design must own the freeze/text-handoff interaction (which OQ-3 already flags) and not assume the wiring is free. Logging here so the builder doesn't carry "it already does this" into design.

**[Major] FR-5.2 untestable acceptance → RESOLVED.** The acceptance now binds the observable: for N = 1/5/40, *new* progress messages per turn ≤ a small constant and never linear in N, AND each tool is observable as an edit (tool name/state change) in that message, AND edit frequency stays within `outgoing_ratelimit`. QA can write all three: count messages produced per turn at three N values, assert the count curve is flat not linear, and assert per-tool visibility via edit observation. Testable. The "allow少量额外条 due to preview freeze/重建" escape hatch is acceptable because it is bounded ("小常数") rather than open-ended.

**[Major] Missing hook-install FR → RESOLVED.** New FR-7 covers global `~/.claude/settings.json` merge-install (FR-7.2 — non-destructive, idempotent), gating reuse via FR-3 (FR-7.3), and explicitly fences the `--settings` injection path as deferred optional enhancement (FR-7.4). It traces to scope item 7 and constraint HC-4 (verified in scope-document.md and constraint-register.md), and carries its own acceptance covering both the bridged (handled) and non-bridged (gated, no message) cases. Adequate. Traceability table updated with the FR-7 row.

**[Minor] Event-variant ambiguity → RESOLVED** (moved to OQ-1, correctly deferred to functional-design rather than left as a silent gap in FR-4.1/FR-5.1).
**[Minor] NFR-1 fuzzy → RESOLVED** (firm "< 2s p95, 本地条件下").
**[Minor] FR-6.3 cutover gate → RESOLVED** (OQ-2 ties the gate to success metric #2 / FR-4 acceptance, with functional-design to make it an executable verification step).

### Assessment

All three blocking/major round-1 findings are resolved with mechanism changes, not wordsmithing — the no-spam requirement now describes something achievable and measurable, the acceptance is QA-writable, and the install gap is closed and scoped. Every FR carries acceptance criteria; traceability to proto-units and batches is intact; in/out/deferred boundaries are explicit (FR-7.4, FR-6 cutover, scope Out-of-Scope). The three open questions are genuine design decisions correctly pushed to functional-design, not requirements gaps masquerading as questions.

The single residual — FR-5.2's overstated "已有/同款" grounding — is a design-handoff accuracy note, not a requirements defect. Engineering can start without coming back to ask what to build; they will discover during functional-design that the tool-progress preview is new wiring rather than a reused path, and OQ-3 already routes that work. That does not rise to NOT-READY. Verdict stands: **READY**.

