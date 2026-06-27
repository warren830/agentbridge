# Team Allocation — Hook-Based Real-Time Text Relay

## 团队

**单人项目。** 执行 = 作者本人 + AI agent personas(经 AI-DLC 编排)。无多人团队、无 mob 编组、无并行 owner 分配。

## Bolt 执行方式

- 所有 Bolt 顺序执行(单人,无并行人力)。
- Construction 各 Bolt 内:aidlc-developer-agent 主导 code-generation(3.5,subagent),aidlc-quality-agent 主导 build-and-test(3.6)。
- 自主模式:AI 驱动,作者在 Bolt 1 walking skeleton 自测通过后被告知(遵守自测纪律,不中途打断)。

## 容量现实核对

- 无 deadline、无容量约束(单人自用)。
- 唯一现实约束:每个 Bolt 声称完成前必须 `cargo test` 全绿 + 关键 Bolt(Bolt 1)live 自测(team.md Testing Posture)。
