# Stakeholder Map — Hook-Based Real-Time Text Relay

## Key Stakeholders

| Stakeholder | Role | Interest | Decision power |
|-------------|------|----------|----------------|
| 项目作者(本人) | 用户 + 开发者 + 决策者 | 手机端远程控制正在跑的 cc,看到干净文字而非截图;体验与 Mac 终端一致 | **决策者**(单人项目,所有 gate 由本人批准) |
| Claude Code(被控 agent) | 受控系统 | 通过 Stop/PostToolUse hook 暴露每轮文字 + 逐工具进度;hook 不得阻塞、出错即 `exit 0` | 无(被动触发) |
| agentbridge 引擎 | 桥接核心 | 接收 hook payload,经 `Platform` trait 中立分发;不得按平台名分支 | 无(架构约束的承载者) |
| 平台适配器(Discord→飞书) | 出口 | 经 capability traits 收事件并发消息;新增接收端不得破坏 trait 边界 | 无 |

## Decision-Makers vs. Influencers

- **决策者**:作者本人 —— 单用户自用项目,需求、scope、设计、每个 approval gate 都由本人拍板。
- **影响者(技术约束,非人)**:
  - **CLAUDE.md 架构约束** —— "引擎不分支平台""平台只经 trait""无 unsafe""异步热路径""加依赖先问" 是硬性影响,设计必须遵守。
  - **Claude Code hook 契约** —— hook 事件集、payload 形状、触发时机由 Claude Code 定义(已验证),设计只能在其能力边界内取舍(如逐字流式做不到)。
  - **姊妹项目先例** —— remote-claude-control 的 Stop-hook 模式是已验证的参考实现。

## Communication Requirements

- **本人 ↔ 系统**:经聊天软件(Discord 验证 → 飞书生产)。这正是本 feature 要改造的回传通道本身。
- **验证沟通纪律**:遵循 [[feedback-self-test-before-asking]] —— 作者要求"自己端到端测通再叫人",不把本人当调试器。每个声称"完成"的环节,先由 AI 自己 live 测过(hook→HTTP→平台 全链路)。
- **审批节奏**:AI-DLC 各 stage 的 approval gate 是本人与系统的正式沟通检查点。
