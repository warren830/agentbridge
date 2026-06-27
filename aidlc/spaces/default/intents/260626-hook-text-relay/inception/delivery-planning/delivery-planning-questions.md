# Delivery Planning Questions — Hook-Based Real-Time Text Relay

> 自主模式(用户授权"一直跑")。Delivery 决策从 unit DAG + scope 批次推导,无需新问题。记录关键决策供追溯:

## D-1 Bolt 序列 — [Decided]
[Answer]: Bolt 1(walking skeleton:U-1/2/3/4/5/7/8/6,Stop 文字端到端)→ Bolt 2(U-9 PostToolUse 进度)→ Bolt 3(U-10 移除截图)。风险优先 + cutover 安全(删截图最后)。

## D-2 walking skeleton 立场 — [Decided]
[Answer]: scope-dependent(team.md),feature 默认 skeleton-on。Bolt 1 即 skeleton,始终 gated;自主模式下 AI 自测通过后自动过 gate(用户授权)。

## D-3 团队分配 — [Decided]
[Answer]: 单人项目,顺序执行,无 mob/并行 owner。

## D-4 Bolt 2/3 顺序 — [Decided]
[Answer]: Bolt 2 → Bolt 3(先补功能价值再清理),与 scope 批次2→3 一致;两者都只依赖 Bolt 1,顺序可调。
