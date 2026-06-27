# Feedback Loop — Hook Text Relay (Feature Closeout)

## 已交付(MVP / Bolt 1,已验证)
手机发消息 → 正在跑的 cc(同一个,带配置)→ Stop hook → 手机收到**干净文字**(取代截图)。门控防串扰。安装器一键合并装 hook。分支 `feat/hook-text-relay`,未提交(待用户决定)。

## 反馈来源 = 用户实际使用
单用户工具,反馈闭环 = 作者本人在 Discord 真机试用。建议下一步:
1. 在一个 tmux+cc 项目设 `hook_relay: true`,跑 `agentbridge hook-install`,启动 agentbridge。
2. Discord 发消息,验证收到干净文字(而非截图)。

## 已延后(诚实记录,非本工作流交付)
- **Bolt 2**(U-9):PostToolUse 逐工具实时进度(就地编辑 preview,消息条数与 N 解耦)。
- **Bolt 3**(U-10):移除截图代码路径(cutover 规约:验证文字回路可用后)。
- **v2**:idle-only 推送(无 engine turn 时);`/btw` 在 hook 模式支持;长轮 >300s 静默保活。

## 优化候选(用后再定)
- 静默保活实现(消除 >300s idle 缺口)。
- PostToolUse 进度的 UX 细节(当前工具 vs 累计列表)。
