# Phase Boundary Verification — INCEPTION → CONSTRUCTION

> Traceability check before entering CONSTRUCTION. Per stage-protocol §12.

## 链路完整性:Requirements → Design → Units → Bolts

| FR | Design(component/ADR) | Unit | Bolt |
|----|----------------------|------|------|
| FR-1 hook 脚本 | C-1 | U-7 | Bolt 1 |
| FR-2 接收端 | C-2/C-3 | U-1, U-3 | Bolt 1 |
| FR-3 门控 | C-4 / ADR-1/6 | U-2, U-4 | Bolt 1 |
| FR-4 Stop 文字 | ADR-1/3 | U-6 | Bolt 1 |
| FR-5 PostToolUse | C-5 / ADR-4 | U-9 | Bolt 2 |
| FR-6 移除截图 | ADR-5 | U-5(门控), U-10(删) | Bolt 1(门控)/ Bolt 3(删) |
| FR-7 安装 | C-6 / ADR-8 | U-8 | Bolt 1 |

## 所有 unit 有 Bolt 归属

U-1..U-8 → Bolt 1;U-9 → Bolt 2;U-10 → Bolt 3。无孤儿 unit。

## 设计决策全部下沉到 unit/bolt

ADR-1(channel 上移)→U-4a;ADR-3(Stop→Result)→U-1;ADR-4(per-turn bool)→U-9;ADR-5(poll 门控+静默保活)→U-5;ADR-6(cwd 前缀)→U-2;ADR-8(端口)→U-4e/U-7/U-8。arch-review 的 B-1/B-2/B-3/M-1/M-2 均有 unit 缓解 + 测试。

## 关键风险均有 Bolt + 缓解 + 测试

见 `risk-and-sequencing-rationale.md` 风险登记表。每条风险落到具体 Bolt 的自测验收。

## 自测纪律就位

project ## Corrections 的"自测再叫人"贯穿所有 Bolt;Bolt 1 含 live 端到端 + 长轮测试。

## Result

✅ **PASS.** Requirements→Design→Units→Bolts 全链路一致;无孤儿;所有设计决策(含 reviewer 发现的 B/M)下沉到 unit + Bolt + 测试;walking skeleton(Bolt 1)定义清晰。可进入 CONSTRUCTION。
