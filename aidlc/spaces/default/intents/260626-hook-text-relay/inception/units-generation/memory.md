<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is maintained by the orchestrator during stage execution. Add observations at the gate ritual, not by editing here directly.

## Interpretations
- 2026-06-27T00:00:00Z — 自主模式。基于 revision-3 应用设计切 units。units 比 scope 的 proto-units(PU-1..5)更细,对齐具体代码组件 + 可测边界。纯函数(C-3 映射)/独立模块(C-4)先行可测。

## Deviations

## Tradeoffs

## Open questions
- 2026-06-27T00:00:00Z — arch-review R1 NOT-READY:B-1(U-4 漏了真实 channel 创建点 session.rs:211 —— channel 在 TmuxAgent::start_session 建、非 start_session_for_entry,"上移"是真重构)+ M-1(hook 模式关心跳 → 长轮误触 300s idle timeout)+ M-2(/btw 的 Stop 提前 break turn)。已修:U-4 拆 U-4a..e 明确 session.rs channel 重构;U-5 改为保留心跳 Thinking 作保活;U-6 把 /btw 显式划 v2 不支持 + 长轮测试。其余 reviewer 确认 sound(覆盖/DAG/批次/可测性全过)。
- 2026-06-27T00:00:00Z — 关键修正认知:ADR-5 原说"hook 模式关心跳 Thinking",但心跳是 idle-timeout 保活信号(非用户输出),必须保留。这是设计 doc 与 unit 拆分协同发现的(reviewer 跨 events.rs 验证)。
- 2026-06-27T16:00:00Z — arch-review R2(达上限)NOT-READY,新 B-2:我用"保留心跳 Thinking 作保活"修 M-1,但 Thinking 在 events.rs 被渲染成可见 🧠 + 触发 freeze_and_detach_preview —— 用新 bug(刷屏+打碎进度 preview)换旧 bug。修正:U-5b 改为"真正静默的保活信号"(timer-reset-only 分支,不 reply 不 freeze)。reviewer 达 2 轮上限,按 §12a 我自行采纳 reviewer 的处方(静默保活)落定。
- 2026-06-27T16:00:00Z — 候选 learning(强烈):复用一个既有 event 类型 = 继承它的全部既有行为。AgentEvent::Thinking 不是中性保活,它绑定可见输出+preview-detach。三次 review 的反复教训:不能假设"复用类型只取我要的语义"。值得沉淀 project practice。
