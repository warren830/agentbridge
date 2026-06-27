# User Stories — Hook-Based Real-Time Text Relay

> persona P-1(远程操控者)。每条 story 映射 requirements.md 的 FR,带 INVEST 验收。

## US-1 收到干净文字回复(取代截图)— MVP 核心

> 作为远程操控者,当我手机发消息给正在跑的 cc、它答完一轮后,我要收到**一条与终端一致的干净文字回复**(而非截图),以便我能看清、能复制、体验和坐在 Mac 前一样。

- **验收**:cc 答完一轮 → 手机端收到一条文字(内容 = transcript 的 `last_assistant_message`),非图片;CJK 正常显示。
- **健壮性(显式归属,不被 happy-path 吞掉)**:FR-1.4(杀掉接收端时 cc 不卡、hook exit 0)与 FR-2.2(畸形 payload 不 panic、记 warn 降级)由各自 FR 验收覆盖,**不**以"US-1 happy path 通过"代替验证 —— build-and-test 必须单独验这两条非happy-path 保证。
- **映射**:FR-4(+ FR-1/FR-2 传输前置)。批次:MVP。INVEST:独立可验、有用户价值、可测。

## US-2 只看到属于我这个会话的回复(不串、不漏、不误发)

> 作为远程操控者(可能同时有别的纯本地 cc 在跑),我要只收到**我桥接的那个会话**的 hook 回传,以便别的 cc 不会把消息发到我的聊天频道、也不会发错频道。

- **验收**:桥接会话的 hook 路由到正确 channel;未桥接目录的 cc 触发 hook 时无任何消息发出(门控丢弃)。
- **映射**:FR-3(cwd 优先 session 兜底)。批次:MVP。

## US-3 看到逐工具实时进度,但不被刷屏

> 作为远程操控者,当 cc 在跑一串工具时,我要**实时看到它正在做什么**(当前/最新工具),但**不想被几十条通知刷屏**,以便我既有掌控感又不被打扰。

- **验收**:含 N 个工具的一轮(N=1/5/40)→ 新发到 channel 的进度消息条数 ≤ 小常数(目标 1 条就地编辑),不随 N 线性增长;每个工具在该消息里可见。
- **映射**:FR-5(就地编辑进度 preview)+ NFR-2。批次:批次2。

## US-4 远程控制的是同一个 cc(带我的全套配置)

> 作为远程操控者,我要确认手机驱动的就是 Mac 上**那个正在跑的 cc**(带我的 CLAUDE.md / MCP / skills),以便远程和本地行为一致,不是另起一个"裸" cc。

- **验收(本 feature 的在范围价值)**:回传文字反映该 cc 的**全套配置上下文**(CLAUDE.md/MCP/skills),证明桥接的是用户既有的、已配置的 cc(由 FR-7 全局 hook + FR-3 门控保证)。
- **回归守卫(非本 feature 交付物)**:手机消息经 tmux `send-keys` 进入既有 cc 的输入路径**保持不变** —— 这是既有行为的回归守卫,QA 不需在本 feature 新建/验证输入路径。
- **映射**:FR-7 安装+门控保证接管的是既有 cc;输入路径既有、范围外。批次:MVP(安装/门控前置)。

## US-5 一键安装,不弄乱我的其他 cc

> 作为远程操控者,我要能把 hook 装进全局设置且**保留我已有的 ~/.claude/settings.json 配置**,以便接管已在跑的 cc,同时不影响我别的纯本地 cc。

- **验收**:安装为合并而非覆盖,幂等;装后原有 hook/配置保留;非桥接 cc 不受影响(经门控)。
- **映射**:FR-7。批次:MVP。

## US-6 平滑切换,不出现"啥都收不到"的空窗

> 作为远程操控者,在从截图切到文字的过程中,我**不希望出现既没截图也没文字的空窗**,以便切换期间始终有可用回路。

- **验收**:截图路径在 Stop 文字验证通过(成功指标 #2 / FR-4 验收)前保留;验证通过后才移除截图。切换全程至少一条回路可用。
- **映射**:FR-6(cutover 规约)+ project cutover 约束。批次:批次3。

## 覆盖检查

每条 FR 都有对应 story(FR-1/2→US-1 前置,FR-3→US-2,FR-4→US-1,FR-5→US-3,FR-6→US-6,FR-7→US-4/US-5)。无孤儿 FR,无无 FR 支撑的 story。

## Review

**Reviewer:** Product Lead — quality gate
**Round:** 1 of 2 (max 2)
**Verdict:** READY

### Coverage (FR-1..FR-7 → story)

Verified each FR maps to at least one story, and each story traces back to an FR — no orphans either direction.

| FR | Story | Note |
|----|-------|------|
| FR-1 Hook 脚本 | US-1 (前置) | Transport prerequisite, not the user-visible subject — acceptable (see minor below). |
| FR-2 HTTP 接收端 | US-1 (前置) | Same — internal plumbing, no standalone story. |
| FR-3 门控 | US-2 | Direct, primary subject. |
| FR-4 Stop 文字 | US-1 | Direct, MVP core. |
| FR-5 PostToolUse 进度 | US-3 | Direct. |
| FR-6 移除截图 | US-6 | Direct (framed as cutover/no-gap value). |
| FR-7 安装+门控 | US-4, US-5 | FR-7.1/7.2 → US-5 (merge-install), FR-7.3 → US-2/US-4 (gating-to-same-cc). |

No FR is unstoried; no story lacks FR backing. Coverage is complete.

### Findings

**[Minor] FR-1/FR-2 are coverage-by-prerequisite, not by a value story.** US-1's acceptance ("手机收到一条文字 = `last_assistant_message`") does exercise the transport end-to-end, so the FRs are *tested* through US-1. But there is no story whose acceptance independently asserts the two FR-specific robustness behaviors that have no other home: FR-1.4 (hook 永不阻塞 cc — 杀掉接收端时 cc 不卡、脚本 exit 0) and FR-2.2 (畸形 payload 不 panic,记 warn 降级). These are exactly the "non-happy-path" guarantees a product lead worries get dropped because they ride inside a happy-path story. For a single-user internal tool this is acceptable — the requirements own the acceptance and functional-design will carry it — but the builder should NOT let "US-1 passes" stand in for "FR-1.4 / FR-2.2 verified." Recommend a one-line note in US-1 (or US-2) that robustness/degradation is covered by the FR-1.4/FR-2.2 acceptance, not by the story's happy path. Not blocking.

**[Minor] US-4 conflates "same cc" with input-path correctness that is out of scope.** US-4's acceptance asserts "手机发的消息经 tmux `send-keys` 进入既有 cc(输入路径不变)." The input path is real and unchanged (verified: `src/agent/tmux/session.rs` `tmux_send_keys`), but this feature does not touch it — so this clause is asserting a *pre-existing* behavior, not something this work delivers. The story's genuine, in-scope value is the second clause: "回传文字反映该 cc 的全套配置上下文" (the bridged cc is the user's configured cc, guaranteed by FR-7 global-hook + FR-3 gating). Keep US-4, but the testable bound that belongs to *this* feature is the config-context clause; the send-keys clause is a regression guard at best. Tighten so QA doesn't think they must build/verify the input path here. Not blocking.

### INVEST

Stories are correctly written as vertical slices (phone→cc→hook→back), not horizontal layers — the assessment's vertical-slice claim checks out. Independence is honestly qualified: US-3 and US-1 both note the transport dependency rather than pretending it away, and US-4's negotiability caveat (no char-level streaming, hook-event granularity, user-confirmed) is the right kind of disclosure. No story is a disguised task. All six are estimable and small for a single-user tool.

### Acceptance criteria — US-3 (the load-bearing one)

This is the story most likely to be a wish instead of a requirement, and it passes. The acceptance binds the observable correctly: for N = 1/5/40, *new* progress messages per turn ≤ a small constant and explicitly NOT linear in N, AND each tool observable as an edit in that one message. That is the right testable shape — message count decoupled from N, not merely paced. QA can write it: drive a turn at three N values, assert the count curve is flat, assert per-tool visibility via edit observation. The bound is independent of N as required. This resolves the same no-spam contradiction the upstream requirements review flagged, and the story inherits the resolution faithfully (in-place edit of one preview, not freeze+new-message-per-tool). One precision carry-forward, not a defect: events.rs today does `freeze_and_detach_preview` + reply per ToolUse (verified at `src/engine/events.rs:159/174/214/243`), i.e. the OPPOSITE of US-3's mechanism — so US-3 is new wiring, and the assessment's 风险/注记 already says exactly that. Good that the story-set carries the caveat rather than assuming the path is free.

### Persona

Single persona P-1 (远程操控者 = the author, single-user self-deploy) is appropriate and not lazy. There is no second-party user, no team-collaboration surface, no operator/admin split — the tool relays one person's own already-running cc to their own phone. No meaningful stakeholder is missing. (A reviewer could argue for an implicit "bystander" stakeholder — the user's OTHER purely-local cc sessions that must NOT receive messages — but that concern is already captured as acceptance in US-2/US-5 gating, which is the correct place for it; it does not need its own persona.) Proportionate to a single-user internal tool — no enterprise persona ceremony demanded or needed.

### MVP slice

MVP = US-1 + US-2 + US-4 + US-5 is a coherent walking skeleton, not a horizontal layer: transport + gating + same-cc + install together deliver "from my phone I get one clean text reply from MY bridged cc, and only mine." That is genuinely end-to-end and demoable on day one. US-3 (progress) deferred to batch 2 and US-6 (screenshot removal/cutover) to batch 3 is the right sequencing — the screenshot path stays as the fallback until Stop-text is proven (US-6 acceptance ties removal to FR-4 verification / metric #2), so there is no "nothing works" window. The cut respects the upstream traceability table's batch assignments. Sound.

### Assessment

Coverage is complete with no orphans, the persona is right-sized for a single-user tool, the MVP is a real vertical skeleton with a safe cutover, and the one acceptance criterion that could have been a wish (US-3 no-spam-but-visible) is bound to an N-independent, QA-writable bound. The two minor findings are tightening notes — FR-1.4/FR-2.2 robustness should be explicitly attributed so it isn't swallowed by US-1's happy path, and US-4's send-keys clause should be reframed as a regression guard rather than in-scope deliverable. Neither requires a round trip with engineering. Engineering can start. **READY.**
