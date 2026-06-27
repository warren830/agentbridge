<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is maintained by the orchestrator during stage execution. Add observations at the gate ritual, not by editing here directly.

## Interpretations
- 2026-06-27T00:00:00Z — 自主模式。读了真实代码(event.rs/webhook.rs/tmux session.rs/engine mod.rs)接地设计。核心集成缝:TmuxSession 持有 event_tx: mpsc::Sender<AgentEvent>(poll loop 用它发事件),引擎 take_events() 取 receiver 跑 process_agent_events。hook 接收端需一个 (cwd/session)→event_tx clone 的注册表,收到 payload 时注入 AgentEvent 到对应 session 既有 channel。
- 2026-06-27T00:00:00Z — 跳过 refined-mockups(无 UI,与跳 rough-mockups 同理)。

## Deviations

## Tradeoffs
- 2026-06-27T00:00:00Z — architecture-reviewer R1 NOT-READY,抓到根本缺陷:原 ADR-1"clone TmuxSession event_tx 注入既有 channel"不成立(B-1 sender 故意不可 clone;B-2 process_agent_events 每轮一次性、空闲会话无消费者)。重设计为"每绑定会话一个常驻消费者 task,binder 自建 channel"(ADR-1/ADR-7)。该模型顺带消解 M-1/M-2(hook 路径自带 dispatcher,claude/acp 的 events.rs 完全不碰,无需区分来源)。这是真实架构修正,非措辞。reviewer 价值极高:我读了代码但没读透 session.rs:26-34 的所有权注释。

## Open questions
- 2026-06-27T15:00:00Z — architecture-reviewer R2(达 max 2 轮)仍 NOT-READY,新 blocking B-4:R1 三 blocker(B-1/B-2/B-3/M-1/M-2)全确认解决,但 ADR-7 把缺陷转移到 turn 驱动器 —— process_and_drain 对所有 backend 统一硬接 take_events/process_agent_events,且无"bind"生命周期钩子(/attach 只写 tmux_session,session 首条消息才懒加载)。一种读法(复用 process_and_drain)会在 300s idle 死锁每一轮。
- 2026-06-27T15:00:00Z — reviewer 达迭代上限,按协议带 finding 进门;但自主模式要驱动到可建代码,故我自己做 revision-3 坐实 B-4。**关键简化**:不再让引擎 turn loop 与异步消费者协调(那是跟架构对抗)。改为:hook 事件喂进 **TmuxSession 自己的 events_rx**(binder 在构造 TmuxSession 时把 hook_tx 作为该 session 的 event sender 之一交给它,非 clone —— 是构造期注入),让既有 process_agent_events 原样消费;ADR-7 的"按 backend 分流 turn"取消(B-4.1 解,不碰 process_and_drain/drain_pending_messages 的 race 逻辑)。空闲/agent-initiated 场景(电脑端直接驱动、无 engine turn)显式划为已知限制/后续增强,不在 MVP 强求(诚实缩范围而非过度工程)。
- 2026-06-27T15:00:00Z — 该简化代价:失去"纯空闲会话也能推送"(B-2 的 idle 子情形)。但 MVP 主场景是"手机发消息→引擎 turn 在跑→Stop 回来",这条 process_agent_events 能消费。idle-only 推送记为 v2。这是范围诚实。
- 2026-06-27T15:30:00Z — revision-3 完成,5 份产物全部一致更新。reviewer 已达 2 轮上限(§12a 带 finding 进门),但 revision-3 直接采纳 reviewer 的"close the fork point / pin spawn hook"建议 —— 用"消除 fork"而非"指定 fork"解决:hook 事件喂既有 channel、turn 协调零改、binder=start_session_for_entry 真实落点、C-4 在 cleanup_agent_session 移除。B-4.1/B-4.2/M-4/M-5 全消解;m-1/m-4(idle 超时常量)+ idle-only 推送(v2)留作 functional-design / 已知范围。自主模式下带 revision-3 过门是采纳 reviewer 自己的解法,非跳过未决 blocker。
- 2026-06-27T15:30:00Z — 候选 learning(自主记录):读代码必须读所有权/生命周期注释与函数的"每轮 vs 常驻"语义,不能只确认类型存在 —— 两轮架构 review 的根因都是我没读透 session.rs 的 channel 所有权和 process_agent_events 的每轮性。值得沉淀 project practice。
- 2026-06-27T00:00:00Z — 候选 learning(自主模式记此,不打断):reviewer 模式两次抓到 builder 乐观偏差(R1 限速逻辑/现成度;架构 R1 channel 所有权/消费者生命周期)。"读代码要读所有权与生命周期注释,不能只看类型存在"值得沉淀成 project practice。
