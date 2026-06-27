# Scope Document — Hook-Based Real-Time Text Relay

> 上游:`intent-statement.md`、`feasibility-assessment.md`、`constraint-register.md`。

## In Scope

1. **薄 hook 脚本**(~30 行,python 或 shell):注册为 Claude Code 的 Stop + PostToolUse hook,带 `session_id`/`cwd` 门控,只把 payload POST 到本地 agentbridge —— 不自己发消息、不截图。
2. **本地 HTTP 接收端**(agentbridge 内,复用现有 `axum`):接收 hook payload,反序列化(serde),按 session→channel 绑定路由。
3. **payload → `AgentEvent` 映射**:
   - Stop 的 `last_assistant_message` → `AgentEvent::Text`(每轮一条干净文字)。
   - PostToolUse 的 `tool_name`/`tool_response`/`duration_ms` → 工具进度事件(经节流/合并)。
4. **session → channel 绑定与门控**:agentbridge 登记桥接会话的 session_id/cwd↔channel,查不到即丢弃 hook。
5. **PostToolUse 节流/合并策略**(具体算法留到 NFR/functional design,但属本范围)。
6. **移除截图路径**:`render_screenshot`、截图轮询 poll loop、`scripts/render_term.py`(Pillow)。
7. **hook 安装**:接管已在跑的 cc → 全局 `~/.claude/settings.json` + 门控;(自动起 cc 的 `--settings` 注入路径为可选增强,见 out-of-scope 边界说明)。

## Out of Scope

- **逐字流式**(字符级):hook 是事件级,做不到;用户已确认不需要。
- **远程权限审批**(Notification hook 转发弹窗 + 远程批准):未来可能,不在本次。
- **飞书适配器实现**:本次先用 Discord 验证;飞书因架构"不分支平台"约束而零改动可加,但其适配器本身不在本范围。
- **工作区里与本 feature 无关的未提交改动**(proxy / `/attach` / `/resume` 等):留到 construction/ship 阶段单独决定(见 RAID I-1)。截图相关的旧改动由 in-scope 第 6 项的移除批次覆盖。
- **多机 / 远程 HTTP**:本次 localhost 单机;若未来多机需重审 A-6 假设(鉴权/TLS)。

## MVP Boundary

**MVP = In-Scope 第 1–4 项的最小组合 + 仅 Stop 文字**:
hook(仅 Stop)→ HTTP 接收端 → `AgentEvent::Text` → Platform 发出 + session→channel 门控。

达成 MVP 即可在手机端收到"每轮一条干净文字回复"取代截图 —— 这是核心价值。PostToolUse 进度(第 5 项节流)和截图移除(第 6 项)在 MVP 之后分批叠加。

## Value Stream

```
手机消息 --> [tmux send-keys] --> 正在跑的 cc(同一个,带 CLAUDE.md/MCP/skills)
                                       |
                          cc 跑完一轮 --> [Stop hook] --> POST --> [HTTP 接收端] --> AgentEvent::Text --> [Platform trait] --> 手机收到干净文字
                          cc 跑工具   --> [PostToolUse hook] --> POST --> [节流/合并] --> 进度事件 --> [Platform trait] --> 手机收到 "⏺ ..."
```
<!-- Text fallback: 手机消息经 tmux send-keys 进入正在跑的 cc;cc 每轮结束触发 Stop hook,POST 到本地 HTTP 接收端,转成 Text 事件经 Platform trait 发回手机;cc 跑工具时 PostToolUse hook 经节流后发进度事件。输入路径不变,输出从截图换成 hook 文字。 -->

## Scope vs Constraints 验证

- 与 `constraint-register.md` 一致:零新依赖(axum/serde 已有)、引擎不分支平台(经 Platform trait)、异步热路径(axum 天然 async)。
- 与 timeline 无冲突(单人项目无硬 deadline)。
- 无 scope/risk 矛盾:MVP 刻意小、风险高的传输层先行。
