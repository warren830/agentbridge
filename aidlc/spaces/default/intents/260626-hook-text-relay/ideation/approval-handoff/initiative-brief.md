# Initiative Brief — Hook-Based Real-Time Text Relay

> IDEATION 一页纸总结。上游:`intent-statement.md`、`scope-document.md`、`intent-backlog.md`、`feasibility-assessment.md`、`constraint-register.md`。

## Intent & Problem

agentbridge 的 tmux backend 靠每 150ms 截屏 + Pillow 渲染回传,是三种 backend 里最弱的(刷屏、渲染脆弱、信息有损)。本 initiative:**用 Claude Code 官方 hook(Stop + PostToolUse)实时回传纯文字,彻底替代截图**,且保持"操作的是同一个正在跑的 cc"(带本人 CLAUDE.md/MCP/skills)。

## Market Validation

N/A(单人自用内部工具)。唯一相关维度是 build-vs-reuse:已研究姊妹项目 happy(SDK wrapper)和 remote-claude-control(Stop-hook 读 transcript),结论是**在 agentbridge 内自建**——因要保留"同一个 cc"+ 复用现有 `Platform` trait/事件管线;remote-claude-control 的 Stop-hook 模式是已验证参考。

## Feasibility & Risk Highlights

**判定 GO。** 核心机制本会话**亲手验证**:Stop 带 `last_assistant_message`(纯文字)、PostToolUse 实时带 `tool_name`/`tool_response`/`duration_ms`、全带 `session_id`+`cwd`、`--settings` 可注入。

Top 风险及缓解:PostToolUse 刷屏 → 强制节流(已为 project 约束);全局 hook 误触 → session/cwd 门控;payload 漂移 → 防御性解析;hook 阻塞 → 永不阻塞/exit 0。

## Scope Boundary

- **In**:薄 hook 脚本 + localhost HTTP 接收端(axum)+ payload→AgentEvent 映射 + session→channel 门控 + PostToolUse 节流 + 移除截图路径。
- **Out**:逐字流式、远程权限审批、飞书适配器实现、无关旧改动、多机/远程 HTTP。
- **MVP**:传输层 + Stop 文字(walking skeleton,打通 hook→HTTP→Platform)。

## Concept Visuals

N/A —— 纯后端改造,无 UI。(回传通道本身是 Discord/飞书既有聊天界面。)

## Team Plan

N/A —— 单人项目。执行 = 作者本人 + AI agent personas(architect/developer/quality 等)经 AI-DLC 编排。

## Go/No-Go Recommendation

**GO。** 技术 de-risked(已验证)、零新依赖、无云/合规阻碍、范围清晰、MVP 小而聚焦。进入 INCEPTION 坐实两个工程取舍:①PostToolUse 节流算法;②session→channel 绑定与门控机制。
