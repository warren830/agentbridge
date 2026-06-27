<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is maintained by the orchestrator during stage execution. Add observations at the gate ritual, not by editing here directly.

## Interpretations
- 2026-06-27T00:00:00Z — 用户在本阶段中途授权"一直跑下去,不要再问我"。视为显式自主授权(最高优先级,覆盖 AI-DLC 逐门审批默认)。后续:自动决策+自动过门+跳过 learnings 人类提问(候选记入日记不丢失)+construction 真写代码并自测。仅工作流完成或真实失败才停。

## Deviations
- 2026-06-27T00:00:00Z — 自主模式下不再调 AskUserQuestion;审批门用 report --result approved --user-input 注明"用户授权自主";不加 --test-run(非测试,不可误标审计)。§13 learnings 人类提问跳过(像 test-run 跳过),候选记日记。

## Tradeoffs
- 2026-06-27T00:00:00Z — Q1 矛盾裁决:用户选"每工具实时报一条",与之前"图片发太勤"+ic-throttle 硬约束冲突。裁决=每工具一条 + 经现有 outgoing_ratelimit 每频道限速兜底。解读:文字远轻于截图,限速兜底即满足 ic-throttle 的"节流策略"要求(不是删约束,是"限速兜底"算合规的节流实现)。functional-design 据此设计。
- 2026-06-27T00:00:00Z — Q2 裁决:cwd 优先、session_id 兜底路由到 channel(用户确认)。复用 session.rs 的 tmux_session 字段。

## Open questions
- 2026-06-27T00:00:00Z — reviewer(product-lead)两轮:R1 NOT-READY(blocking:限速兜底只 pace 不减条数,违反无刷屏;major:验收不可测、缺安装 FR)。R2 READY,修法=工具进度改"就地编辑单条 preview"(条数与 N 解耦)+ 验收改"条数≤小常数不随 N 线性"+ 加 FR-7 安装。
- 2026-06-27T00:00:00Z — reviewer 精度校正(已采纳进 FR-5.2):当前 events.rs 的 ToolUse 是 freeze+新 reply(每工具新消息),"复用一条 preview 编辑工具进度"是新接线而非现成路径。functional-design 必须自己设计 freeze/text 衔接(OQ-3),别假设现成。这是 builder 乐观偏差,reviewer 两次都抓到(R1 限速逻辑漏洞、R2 现成度夸大)—— 自主模式下 reviewer 是关键对抗校验。
