# Code Summary — Walking Skeleton (U-1..U-8 Stop path)

> 由 aidlc-developer-agent 实现,orchestrator 亲自 cargo check/test + live e2e 验证(自测纪律)。分支 `feat/hook-text-relay`。

## 新增文件

| 文件 | 内容 |
|------|------|
| `src/hook_route.rs` | **U-2** `HookRouteRegistry`(`std::sync::Mutex<HashMap<canonical_work_dir, mpsc::Sender<AgentEvent>>>`),new/bind/unbind/resolve。resolve 规范化 cwd + 组件级前缀匹配(ADR-6 m-2),最长前缀胜。6 单测。 |
| `src/hook_receiver.rs` | **U-1+U-3** HookPayload(全 Option serde)、纯 `map_hook`(Stop→Result、空→None BR-1、PostToolUse→None BR-16、未知→None)、localhost axum server(`POST /hook-event`,仅 127.0.0.1,永远 200 BR-7,resolve 未命中丢弃 BR-5)。4 map_hook 单测 + 1 live e2e(#[ignore])。 |
| `scripts/agentbridge_hook.py` | **U-7** ~55 行 stdlib-only。端口 argv/$AGENTBRIDGE_HOOK_PORT/9123。全 try/except、2s 超时、任何错误 exit 0(BR-10)。 |

## 修改文件

| 文件 | 改动 |
|------|------|
| `src/config/mod.rs` | **U-4e** `HookReceiverConfig{port}`(可选 hook_receiver 字段)+ `default_hook_receiver_port()=9123`;`TmuxConfig` 加 `hook_relay: bool`。 |
| `src/agent/tmux/session.rs` | **U-4a** move 进 poll task 前 clone event_tx;TmuxSession 存 `hook_sender` + `pub fn hook_sender()`;替换 stale 注释(说明 clone 故意为 hook 注入,死会话检测改靠 alive+idle timeout)。**U-5** poll_loop 加 hook_relay 参 + 纯 helper should_emit_poll_output/should_emit_heartbeat;hook 模式门控 Text/Image/settle-Result + 可见 Thinking 心跳(BR-14),保留 permission。1 门控单测。 |
| `src/agent/registry.rs` | **U-4b** start_session_for_entry 加 `hook_route: &Arc<HookRouteRegistry>` 参;tmux 分支 hook_relay 开时 bind(work_dir → session.hook_sender())。 |
| `src/engine/mod.rs` | **U-4b/c** Engine 加 hook_route 字段 + 访问器;经现有调用链穿线;cleanup_agent_session 调 unbind。**process_and_drain/drain_pending_messages 的 race-free 解锁逻辑零改动**(架构硬约束)。 |
| `src/main.rs` | **U-4d** 建一个共享 HookRouteRegistry 注入所有 engine + 启动接收端。**U-8** hook-install 子命令:写脚本到 ~/.agentbridge/、合并进 ~/.claude/settings.json 的 hooks.Stop/PostToolUse(合并非覆盖、幂等、原子写)。5 merge 单测。 |

## 关键决策/取舍

- **U-4a**:用 field+accessor(非返回元组),clone 在 move 前,同源保证。死会话检测退化由 alive flag + idle timeout 兜底(ADR-1 接受)。
- **hook_route 穿线**:作为普通参数穿过现有调用链(~8 签名),**不碰** process_and_drain 内部 race 逻辑(byte-for-byte 不变)。
- **U-5 静默保活推迟**:hook 模式全抑制心跳 Thinking(否则可见+detach preview)。干净的静默 timer-reset 需 events.rs 改动 + 新事件变体,超批次1 范围 → 留 `// TODO(batch)`。后果:Stop-only 长轮 >300s 可能触 idle timeout(已知 batch-1 限制,符 arch-review m-1')。

## 验证(orchestrator 亲自跑)

- `cargo check --all-targets`:Finished,0 warning。
- `cargo test`(全):71 lib + 277 bin + 其余套件全绿,0 failed;16 个新单测全过。
- **live e2e**(`hook_relay_end_to_end --ignored`):真实 axum server + 真实 agentbridge_hook.py 子进程 → Stop payload("live reply 你好",**含 CJK**)经 cwd 绑定 channel 投递为 `AgentEvent::Result`;未绑定 cwd 被丢弃(脚本仍 exit 0)。**1 passed。**

## 已知 batch-1 限制(诚实记录)

- Stop-only 长轮 >300s 可能触 idle timeout(静默保活留后续)。
- PostToolUse 进度、移除截图 = 批次2/3。
- idle-only 推送(无 engine turn)= v2(ADR-1 范围)。
- `/btw` 在 hook 模式不支持(BR-15)。

## Code Review

> **Reviewer:** Architecture Reviewer (independent; first look at the implemented code).
> **Round:** 1 of max 2.
> **Verdict:** READY
> **Scope:** correctness / design-conformance of the engine integration + the channel-ownership
> refactor (the surface flagged blocking in two prior design reviews). Verified against authoritative
> **rev-3** `decisions.md` (ADR-1/2/3/4/5/6/8; the trailing `## Architecture Review` block is the
> stale R2 review of rev-2 and was ignored per instruction) and `business-rules.md` (BR-1..16).
> I read every named file end-to-end, cross-referenced every load-bearing claim against the real
> types, and ran the tests myself: `cargo test --bin agentbridge hook_` → **16 passed, 1 ignored**;
> `hook_relay_end_to_end --ignored` → **1 passed** (real axum server + real `agentbridge_hook.py`
> subprocess, CJK Stop payload delivered as `AgentEvent::Result`, unbound cwd dropped, script exit 0).

### 1. U-4a channel-clone correctness (the core design risk) — CORRECT

This is genuinely the SAME channel `process_agent_events` drains, not a separate one. Verified the
single-channel topology in `session.rs:222-253`:
- `let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(128)` creates ONE channel.
- `let hook_sender = event_tx.clone()` (line 227) clones the sender **before** the original
  `event_tx` is moved into the poll task (line 243) — so the clone is a sibling sender on the
  identical channel, exactly as ADR-1 step 1 requires.
- `event_rx` is moved into `TmuxSession.events_rx` (line 250); `hook_sender` is stored in
  `TmuxSession.hook_sender` (line 252). `hook_sender()` (line 42-44) hands out further clones.
- `take_events()` (line 82-85) yields that same `events_rx` to `process_and_drain` →
  `process_agent_events`. So a hook `Result` sent via `hook_sender` lands in the exact `events_rx`
  the engine is draining for the live turn. No second channel, no separate consumer. The live e2e
  test confirms end-to-end delivery into the bound receiver.

The documented degradation (poll task exit no longer yields `recv()->None` because the clone keeps
the channel open) is real, correctly commented at `session.rs:30-35`, and matches the ADR-1
consequence — dead-session detection now leans on the `alive` flag + idle timeout. Accepted by rev-3.

### 2. ADR-1 conformance — CONFIRMED

No separate consumer task, no `Notify`, no binder actor, no ADR-7 resurrection anywhere. `grep` for
`Notify`/`turn_done` in the new code finds nothing. The hook path is purely: receiver → `resolve(cwd)`
→ `tx.send(event)` (`hook_receiver.rs:139`) into the existing channel. `process_and_drain` /
`drain_pending_messages` are byte-for-byte unchanged on the take_events/process_agent_events/
put_events_back contract — confirmed by reading the full functions (`mod.rs:814-1356`); the only edits
are the added `hook_route` parameter threaded through the signatures. The "binder" is `start_session_for_entry`'s
tmux branch (`registry.rs:136-138`), the real lifecycle point ADR-1 names — not a new actor.

### 3. BR conformance — ALL FAITHFUL

- **BR-1 (empty Stop → None):** `map_hook` Stop arm trims and returns `None` on empty/whitespace
  (`hook_receiver.rs:65-69`). Test `map_stop_empty_message_yields_none` covers both `"   \n  "` and
  `None`. Load-bearing: because the empty check is in `map_hook`, no empty `Result` ever reaches
  the engine's `events.rs:345` guard (which would otherwise fall through to `final_text()` when a
  preview was active). Correct.
- **BR-5 (resolve miss → dropped, no message):** `handle_hook_event` returns `200` with only a
  `tracing::warn` on a resolve miss (`hook_receiver.rs:123-130`); nothing is sent. The gating sits in
  `HookRouteRegistry::resolve` returning `None`. Tests `resolve_miss_returns_none` +
  the e2e unbound-cwd assertion confirm. Key safety rule honored.
- **BR-7 (always 200):** `handle_hook_event` returns `StatusCode::OK` on every path — miss, unmapped,
  empty, and channel-closed-on-send (`hook_receiver.rs:129,134,143`). The send-error case logs at
  `debug` and still returns 200 (line 139-143). Correct.
- **BR-10 (script exit 0 on any error):** `agentbridge_hook.py` wraps everything in
  `try/except Exception: pass` + `finally: sys.exit(0)` (lines 22-49), 2s POST timeout, stdlib-only.
  Empty stdin returns early. Verified the script exits 0 even against an absent receiver via the
  e2e unbound branch. Correct.
- **BR-14 (no visible Thinking heartbeat in hook mode, no preview detach):** `should_emit_heartbeat(hook_relay)`
  returns `!hook_relay` (`session.rs:783-785`); the heartbeat `Thinking` send at `session.rs:436-457`
  is gated by `busy_ticks >= HEARTBEAT_TICKS && should_emit_heartbeat(hook_relay)`, so in hook mode it
  never fires — and `Thinking` is what `events.rs:159` would render as `🧠` + `freeze_and_detach_preview`.
  Test `hook_mode_gates_off_output_and_heartbeat` asserts both gates. Correct. BR-16 (PostToolUse → None
  in batch 1) also honored (`hook_receiver.rs:78`, test `map_post_tool_use_is_none_in_batch_1`).

### 4. Constraint compliance — CLEAN

- **Zero new deps:** `git status` shows `Cargo.toml`/`Cargo.lock` unmodified. axum/serde/tokio/anyhow
  all pre-existing.
- **No `unsafe`:** none in any new/modified file.
- **No `unwrap`/`expect` outside tests:** every `unwrap`/`expect` in `hook_route.rs` (137,185,189) and
  `hook_receiver.rs` (217-274) is inside a `#[cfg(test)]` module. The production poison-recovery
  `lock().unwrap_or_else(|e| e.into_inner())` (`hook_route.rs:53,61,78`) is the correct
  non-panicking pattern, not a bare unwrap. `map_hook` uses `clone().unwrap_or_default()`.
- **`tracing` not `println`:** all new lib code uses `tracing`. The only `eprintln!`/`println!` are
  inside `#[ignore]` live tests and CLI subcommands (`main.rs` doctor/init, allowed at the binary
  boundary).
- **Char-boundary-safe slicing:** `map_hook` does no byte slicing; `is_path_prefix` operates on
  whole-component prefixes via `strip_prefix` + `starts_with('/')`, never indexes mid-char. The CJK
  e2e payload round-trips intact.
- **Race logic untouched:** `process_and_drain`/`drain_pending_messages` unlock-while-holding-state-mutex
  guard (`mod.rs:1204-1213`) is unchanged; only the `hook_route` param was threaded in.
- **Engine doesn't branch on platform name:** the hook path is backend-keyed (`entry.backend == "tmux"`
  in `registry.rs:101`), never platform-name-keyed; the engine treats the hook as just another event
  source on the existing channel.

### 5. Threading correctness — CORRECT

`hook_route: &Arc<HookRouteRegistry>` is threaded through `handle_message` → `process_and_drain` →
`start_agent_session_for_key` → `registry::start_session_for_entry`, and also through
`drain_pending_messages`, `drain_orphaned_queue`, `handle_command_message`, `cleanup_agent_session`
— all as a plain borrowed Arc, never held across the try-lock/queue critical sections. `main.rs:211`
creates ONE shared registry and injects it into every engine via `set_hook_route` **before**
`start()` (line 218), so all project engines + the single receiver (line 231) share one map — correct
for cross-project routing. `cleanup_agent_session` genuinely unbinds: `hook_route.unbind(&work_dir)`
(`mod.rs:1557`) using the same work_dir resolution as the bind site (per-channel `/dir` override else
project default), and it is invoked on `/new|/switch|/delete|/dir|/cd|/attach|/resume`, `/stop`, and
idle-reset (`mod.rs:740,1650,1703`). The unbind runs before the state lock, so no ordering hazard.

### 6. Bugs / races / nonconformance the passing tests would not catch — none blocking

- **No missing gating emission site.** I enumerated every `tx.send` in `poll_loop` against ADR-5's
  table: Text (`session.rs:535`) and Image (`session.rs:507`) sit behind `should_emit_poll_output`
  (line 494); the settle `Result` (line 542-552) is inside the same post-gate block so it is also
  suppressed; the heartbeat `Thinking` (line 455) is behind `should_emit_heartbeat`; `PermissionRequest`
  (line 397) and the session-death `Error` (lines 360,373) are intentionally NOT gated (permission must
  still work; a dead session must still surface). This exactly matches ADR-5. No second `Result` source
  competes with the hook in hook mode.
- **Prefix-match has no sibling false-positive.** `is_path_prefix` is component-aware: `/a/foo` does
  NOT match `/a/foobar` (test `sibling_dir_is_not_a_prefix`), and longest-prefix-wins is implemented
  (`hook_route.rs:84-93`) so nested bindings route to the nearest enclosing dir. Correct.
- **Installer does not corrupt settings.json.** `merge_hook_event` preserves foreign settings and
  foreign hooks, is idempotent (command-match dedup), bails on non-object root / non-object `hooks`
  (leaves foreign shapes alone), and writes atomically via temp-file + rename (`main.rs:707-711`).
  Five tests cover empty/idempotent/preserve/both-events/non-object. Invalid existing JSON is reported,
  not silently overwritten (`main.rs:691-692`). Correct.

**Minor observations (non-blocking, no action required for batch-1 READY):**
- **m-1 (cosmetic): `busy_ticks` grows unbounded in hook mode.** Because the heartbeat block at
  `session.rs:436` short-circuits on `should_emit_heartbeat(hook_relay)==false` BEFORE the
  `busy_ticks = 0` reset, `busy_ticks` increments every busy tick and never resets during a hook-mode
  turn. Harmless (a `u32` at 150ms/tick overflows only after ~20 years of continuous busy), and the
  value is unused in hook mode. Cleaner to reset it regardless, but not a defect.
- **m-2 (already documented): Stop-only turn >300s trips the idle timeout.** With the heartbeat gated
  off in hook mode there is nothing resetting `EVENT_IDLE_TIMEOUT` (events.rs 300s) on a long
  tool-less turn. This is the `// TODO(batch)` at `session.rs:441-443` and is honestly logged as a
  known batch-1 limitation (matches functional-design m-1'/m-4). Silent keepalive is correctly deferred.
- **m-3 (already documented): `/btw` in tmux+hook is undefined.** The `/btw` path (`mod.rs:638-661`)
  is backend-agnostic — it `send`s into the pane regardless of backend, so in hook mode the resulting
  Stop→Result would `break` the original turn early (events.rs:397). This is exactly BR-15's documented
  MVP scope-out; the code matches the limitation rather than doing something worse. Acceptable.
- **process note (not a code issue): the feature is in the working tree, not committed.**
  `git diff main...HEAD` is empty; all changes show as modified/untracked. This is fine for a
  walking-skeleton review but the branch tip does not yet contain the work.

### Assessment

A developer could build on this without architectural questions — and in fact the build is already
done and verified. The single highest-risk item (the U-4a channel clone) is implemented exactly as
rev-3 ADR-1 prescribes: one `mpsc::channel`, sender cloned before the original moves into the poll
task, the clone registered in the route table, the receiver drained by the unchanged
`process_agent_events`. The rev-3 model (no separate consumer, no Notify, no binder actor) is honored
throughout; the retired ADR-7 leaves no trace in the code. The four focal business rules (BR-1, BR-5,
BR-7, BR-10) and BR-14/BR-16 are each faithful and test-covered, with BR-1 and BR-14 guarding the two
real traps (active-preview empty-Result fall-through; rendered-Thinking keepalive spam). The
load-bearing race-free unlock logic in `process_and_drain`/`drain_pending_messages` is untouched.
Constraints all hold: zero new deps, no unsafe, no production unwrap/expect, tracing-only,
char-boundary-safe, no platform-name branching. The three minors are all pre-documented batch-1
limitations, not nonconformance. **READY.**
