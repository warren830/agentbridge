# Constraint Register — Hook-Based Real-Time Text Relay

> 引用上游 `intent-statement.md` 的 scope signal 与 guardrails。

## Technical Constraints (硬约束 — 来自 CLAUDE.md)

| # | 约束 | 对本 feature 的影响 |
|---|------|---------------------|
| TC-1 | Rust + tokio,所有热路径 I/O 异步(`tokio::fs`/`process`/`time`,无 `std::fs`/`thread::sleep`) | HTTP 接收端必须 async(axum 天然满足);hook payload 处理在 tokio 任务里 |
| TC-2 | 应用边界用 `anyhow::Result`;库级模块用 `thiserror` | HTTP 接收端/事件映射在引擎边界 → `anyhow`;若抽出可复用模块 → `thiserror` |
| TC-3 | 不在测试外 `unwrap()`/`expect()` | payload 解析全部走 `?` / 显式错误处理 |
| TC-4 | 只用 `tracing`,结构化字段(如 `session_key = %key`) | 接收端日志用 `tracing::info!(session_id = %id, ...)` |
| TC-5 | 无 `unsafe` | 不触及 |
| TC-6 | 加新依赖前先问;优先复用 `anyhow`/`tokio`/`tracing`/`serde`/`reqwest`/`axum` | 传输用 axum(已有),payload 反序列化用 serde(已有)—— **零新依赖** |
| TC-7 | 平台适配器只经 `Platform`+`ReplyCtx` capability traits;引擎不下钻、不按平台名分支 | hook payload 转成 `AgentEvent` 后走现有管线;**接收端绝不 `if platform == ...`** |
| TC-8 | 注释写 *why* 不写 *what*,不加跨项目引用 | 解释 hook 门控/节流的"为什么",不写"参考 rcc 的做法" |
| TC-9 | session 纪律:`src/core/session.rs` 的 try-lock+队列模式是 load-bearing,不擅自换 | hook 事件注入若触及 session,沿用现有模式,不引入阻塞 mutex |
| TC-10 | agent 边界:新 agent 走 `AgentSession` trait,不在引擎里特判 | tmux backend 仍是同一个 `AgentSession`;hook 是其"出"路径的新实现,不新增 agent 类型 |

## Hook-Contract Constraints (来自 Claude Code,已验证)

| # | 约束 | 影响 |
|---|------|------|
| HC-1 | hook 是事件级触发,非字符级 → 无法逐字流式 | 已接受;粒度 = 每轮(Stop)+ 每工具(PostToolUse) |
| HC-2 | hook 不得阻塞 cc,出错应 `exit 0` | hook 脚本极薄、容错、永不抛 |
| HC-3 | payload 形状由 Claude Code 定义,可能随升级变 | 防御性解析;优先用稳定字段(`last_assistant_message`) |
| HC-4 | 接管已在跑的 cc 无法事后注入 `--settings` | 该路径必须用全局 `~/.claude/settings.json` + 门控 |

## Organizational Constraints

| # | 约束 | 影响 |
|---|------|------|
| OC-1 | 单人项目,无团队/预算/时间线硬约束 | 节奏自定;每个 gate 由本人批准 |
| OC-2 | 验证纪律(project.md):声称完成前 AI 自测全链路 | 实现后由 AI 自己 live 跑 hook→HTTP→平台,不拿用户当调试器 |

## Regulatory Constraints

**无。** 纯本地、单用户、无 PII 变化、无数据出境到新方、无 PCI/HIPAA/SOC2 触点。
