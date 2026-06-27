# User Stories Assessment — Hook-Based Real-Time Text Relay

## INVEST 评估

| Story | Independent | Negotiable | Valuable | Estimable | Small | Testable |
|-------|:-:|:-:|:-:|:-:|:-:|:-:|
| US-1 收到干净文字 | ✅(传输前置后独立) | ✅ | ✅ 核心价值 | ✅ | ✅ | ✅ |
| US-2 会话隔离/门控 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| US-3 逐工具进度不刷屏 | ✅(依赖 US-1 传输) | ✅ | ✅ | ✅ | ✅ | ✅ 条数有界可测 |
| US-4 同一个 cc | ✅ | ⚠️ 受 hook 契约约束 | ✅ | ✅ | ✅ | ✅ |
| US-5 合并安装 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ 幂等可测 |
| US-6 平滑切换 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

## MVP 切片

MVP = US-1 + US-2 + US-4 + US-5(传输 + 门控 + 同一 cc + 安装)→ 手机端收到干净文字、只收自己会话、确认是既有 cc。
批次2 = US-3(进度)。批次3 = US-6(切换/移除截图)。

## 风险/注记

- US-3 是关键价值与关键风险并存(逐工具 vs 不刷屏的张力,已由 FR-5 就地编辑方案化解;但 events.rs 现有 ToolUse 是 freeze+新消息,需新接线 —— 见 requirements OQ-3)。
- US-4 的 "negotiable" 受限:逐字流式做不到(hook 事件级),已与用户确认接受。

## 可垂直切片性

所有 story 都是端到端纵切(手机→cc→hook→回手机),非水平分层,符合 vertical-slice 原则。
