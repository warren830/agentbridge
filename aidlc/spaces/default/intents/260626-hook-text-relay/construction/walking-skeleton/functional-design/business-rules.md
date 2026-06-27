# Business Rules — Walking Skeleton (U-1..U-8)

> 引用 `requirements.md`(FR-1..7)、`component-methods.md`。

## 映射规则(U-1)

- **BR-1** Stop 事件:`last_assistant_message` 为空 → 映射结果 **必须为 None**(不发空轮消息)。【强制,单测覆盖】
- **BR-2** 只接 `Stop`(批次1)/ `Stop`+`PostToolUse`(批次2);其余 hook 事件类型 → None。
- **BR-3** Stop → `AgentEvent::Result`(非 Text);语义=一轮收尾(ADR-3)。token 字段填 0。

## 路由/门控规则(U-2/U-4)

- **BR-4** `resolve` 用规范化 cwd 前缀匹配绑定的 work_dir;命中返回 sender clone,未命中返回 None。
- **BR-5** 未命中 = 丢弃 hook(门控):非桥接 cc 的 hook **绝不**产生消息。【FR-3.3,关键安全规则】
- **BR-6** 绑定在 `start_session_for_entry`(tmux 分支)登记,在 `cleanup_agent_session` 注销。

## 接收端规则(U-3)

- **BR-7** 接收端**永远返回 200**(即使门控丢弃 / 映射 None / 解析失败)—— hook 脚本不应重试或报错。
- **BR-8** 畸形 payload → 200 + `tracing::warn`,**不 panic**(防御性反序列化)。
- **BR-9** 仅监听 localhost(无鉴权/TLS,单机单用户)。

## hook 脚本规则(U-7)

- **BR-10** 脚本任何异常(读 stdin / 网络 / 超时)→ `exit 0`,**永不阻塞 cc**。【FR-1.4,关键】
- **BR-11** POST 用短超时(~2s);失败静默放过。

## 安装规则(U-8)

- **BR-12** 安装 = **合并而非覆盖** ~/.claude/settings.json;保留既有 hooks/配置;重复安装幂等。
- **BR-13** hook command 内嵌实际端口(默认 9123,ADR-8)。

## 保活规则(U-5b)

- **BR-14** hook 模式的 idle 保活信号**绝不**产生用户可见输出、**绝不** detach/reset preview(timer-reset-only)。【B-2 关键】

- **BR-15**(arch-review m-3)tmux+hook 模式下 `/btw` mid-turn 注入**不支持**(MVP):其产生的 Stop→Result 会在单消费者模型里 `break`(`events.rs:397`)原 turn 早退,行为未定义;完整 turn 序列化留 v2(U-6 已记)。
- **BR-16**(arch-review m-1)**批次1** 的 `map_hook` PostToolUse 分支返回 **None**(批次1 不接工具进度);**批次2(U-9)** 才改为返回 `ToolUse`。BR-2 的"只接"按批次解读。

## 不变量

- **INV-1** 任一时刻只有一个消费者读 session event channel(既有 process_agent_events;hook 只是另一事件源)。
- **INV-2** tmux 输入路径(send-keys)不被本 feature 改动。
- **INV-3** claude/acp backend 行为零回归(per-turn bool false 路径不变)。

## Review

**Reviewer:** Architecture Reviewer (independent, first look at this functional-design stage)
**Round:** 1 of max 2
**Verdict:** READY

Verified the business rules and flows against the authoritative **revision-3** application design
(`decisions.md` ADR-1/2/3/4/5/6/8 + the rev-3 narrative that RETIRES ADR-7/C-7/binder/Notify),
`component-methods.md` (rev-3), `unit-of-work.md` (R2-resolved), and `requirements.md` (READY).
I cross-checked the load-bearing claims against the real source: `src/engine/events.rs`,
`src/core/event.rs`, `src/agent/tmux/session.rs`. Note: the `## Architecture Review` block at the
bottom of `decisions.md` is the **stale R2 review of revision-2** (it critiques ADR-7/C-7/binder/Notify,
all retired by rev-3); I judged this design against rev-3, exactly as `unit-of-work.md` did.

This is a single-user backend transport/bridge feature with no domain model and no UI. `domain-entities.md`
(DTOs only) and `frontend-components.md` (N/A) are legitimately lean — that is correct, not a gap.

### Faithfulness of the four focal rules — verified against source

- **BR-1 (empty→None) — FAITHFUL and load-bearing.** Verified the necessity directly in code:
  the engine's empty-resume guard (`events.rs:345-352`) only skips a Result when
  `input_tokens==0 && output_tokens==0 && content.is_empty() && **!preview.was_active()**`. A hook
  Result always carries token 0/0, but if it arrives while a preview IS active, the guard does NOT
  fire and the Result falls through to `events.rs:360`, which would emit `preview.final_text()`
  instead of dropping the empty turn. So the empty check genuinely must happen in `map_hook`, not
  rely on the engine guard. BR-1 ("必须为 None … 单测覆盖") + ADR-3 M-3 capture this precisely.
  `business-logic-model.md` algorithm 1 (`if text.is_empty(): return None`) matches.

- **BR-5 (gating safety, FR-3.3) — FAITHFUL.** "未命中 = 丢弃,非桥接 cc 的 hook 绝不产生消息"
  matches ADR-6 + C-2 step 1 (`resolve …else { warn+return 200 }`). Correctly marked 关键安全规则.

- **BR-10 (hook never blocks cc, FR-1.4) — FAITHFUL.** `exit 0` on any error matches ADR-2/FR-1.4.

- **BR-14 (silent keepalive) — FAITHFUL and the single most important rule to get right.** This is
  the rev-3 B-2 fix (the unit-of-work R2 blocker). I verified the danger it guards against:
  `AgentEvent::Thinking` is rendered by default — `events.rs:158` gates only on
  `display.thinking_messages` (**default true**), and when true it calls `freeze_and_detach_preview`
  (`events.rs:159`) **and** `platform.reply(ctx, "🧠 …")` (`events.rs:166`). The poll-loop heartbeat
  (`session.rs:418-430`) emits a non-empty `Working…` status every ~12s. So reusing `Thinking` as a
  keepalive would spam the user and detach the live preview. BR-14's hard constraint —
  "绝不产生用户可见输出、绝不 detach/reset preview(timer-reset-only)" — is exactly the correct
  capture and forecloses the trap. Matches `unit-of-work.md` U-5b option (a). Good.

### map_hook + Stop e2e consistency with ADR-1 rev-3 — verified

- The Stop e2e flow in `business-logic-model.md` ("投入既有 channel … 正在跑的 process_agent_events
  recv 到 Result → reply → 见 Result break turn(既有路径)") is faithful to ADR-1 rev-3 and to the
  real consumer: `events.rs:339-397` handles `AgentEvent::Result`, emits the reply, and `break`s the
  loop at `events.rs:397` — the same path the old poll-loop settle Result used. No new consumer, no
  Notify, no backend turn-fork. Correct.
- BR-3 (Stop→`Result`, token 0) matches `AgentEvent::Result { content, session_id, input_tokens,
  output_tokens }` in `core/event.rs:60-65` and resolves OQ-1 (Result over Text). The token-0 → no
  `[ctx ~N%]` consequence is honestly recorded as a capability loss (ADR-3 M-3); verified the
  indicator is gated on `input_tokens > 0` at `events.rs:369`. Consistent.
- `domain-entities.md` `HookPayload` field set matches C-2 exactly; `HookRouteRegistry`
  (`Arc<Mutex<HashMap<work_dir, mpsc::Sender<AgentEvent>>>>`) matches C-4. No drift.
- INV-1/INV-2/INV-3 correctly restate the rev-3 single-consumer / send-keys-unchanged /
  claude-acp-zero-regression invariants.

### Findings

**Minors (non-blocking; do not gate READY):**

- **m-1 — PostToolUse batch-boundary inconsistency between the two docs (could mildly mislead
  codegen, but BR-2 disambiguates).** `business-logic-model.md` algorithm 1 shows the `PostToolUse`
  arm returning `ToolUse {…}` with a comment "批次2 才接;批次1 可先返回 None"; `component-methods.md`
  C-3 shows it returning `Some(ToolUse{…})` unconditionally. BR-2 resolves the intent correctly
  ("只接 Stop(批次1)/ Stop+PostToolUse(批次2)"), and U-1/U-9 split PostToolUse into batch-2.
  Since this is the walking-skeleton (batch-1) and U-9 owns the PostToolUse wiring, a one-line note
  in `business-logic-model.md` clarifying "批次1 的 map_hook PostToolUse 分支返回 None;批次2(U-9)
  改为 ToolUse" would remove the only spot where a codegen agent could guess. Not blocking — BR-2 +
  unit-of-work already carry the authoritative answer.

- **m-2 — `resolve` signature in pseudocode omits the `Option<&str>` shape and the warn-on-None
  side.** `business-logic-model.md` algorithm 2 writes `resolve(cwd)`; the real contract
  (C-4) is `resolve(&self, cwd: Option<&str>) -> Option<mpsc::Sender<AgentEvent>>`, and C-2 warns
  before returning 200 on a None resolve. The pseudocode is fine as algorithm-level intent (it is
  explicitly "聚焦算法/流程"), and the canonical signature lives in C-4 — so this is a precision
  note, not a contradiction. The `summarize(payload)` in algorithm 1 vs `summarize_tool(p)` in C-3
  is the same harmless naming drift in batch-2-only code.

- **m-3 — `/btw` scope-out is decided upstream but not echoed in business-rules.** `unit-of-work.md`
  U-6 explicitly scopes `/btw` as UNSUPPORTED for tmux+hook in MVP (its Stop→Result would `break`
  the single consumer early at `events.rs:397`). This is a real, named decision that a developer
  reading only the functional-design rules could miss. A one-line rule (e.g. "BR-15: tmux+hook 模式
  下 `/btw` 不支持(MVP);mid-turn 注入的 Stop 行为未定义,v2 处理") would make the rule set
  self-contained. Non-blocking because it is a documented batch-1 limitation, not new behavior.

- **m-4 — keepalive timeout constant not pinned.** BR-14 owns the "silent" property correctly, but
  no rule states the idle-timeout reference point (`EVENT_IDLE_TIMEOUT = 300s`, `events.rs:27`) or
  that the keepalive must fire faster than it. `unit-of-work.md` U-5b carries the mechanism; a
  pointer would help, but this is implementation detail for U-5, not a business rule. Non-blocking.

### Assessment

The business rules (BR-1..14) faithfully and completely capture the rev-3 design decisions, and the
four focal rules (BR-1 empty→None, BR-5 gating, BR-10 never-block, BR-14 silent keepalive) are each
correct against the real code — BR-1 and BR-14 in particular guard against traps I verified would
otherwise bite (the active-preview empty-Result fall-through, and the rendered-`Thinking` keepalive
spam). The map_hook algorithm and the Stop e2e flow are consistent with ADR-1 rev-3: feed the
existing channel, unchanged `process_agent_events` consumes the Result and breaks the loop exactly
like the old poll settle. No rule contradicts rev-3; no rule would mislead code-generation into the
retired ADR-7/C-7/binder/Notify model. The four minors are precision/self-containment improvements
(PostToolUse batch boundary, resolve signature, `/btw` echo, keepalive constant), none of which
blocks a developer from building the walking skeleton — the authoritative answers all exist upstream.
Proportionate to a single-user backend bridge with no domain model and no UI. **READY.**
