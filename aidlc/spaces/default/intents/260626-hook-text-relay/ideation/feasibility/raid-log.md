# RAID Log — Hook-Based Real-Time Text Relay

> Risks / Assumptions / Issues / Dependencies。引用上游 `intent-statement.md`。

## Risks

| ID | 风险 | 概率 | 影响 | 缓解 | 责任阶段 |
|----|------|------|------|------|----------|
| R-1 | PostToolUse 在工具密集轮刷屏,重现"截图发太勤快"的痛点 | 中 | 中 | 强制节流/合并策略(已是 project.md 约束);用 `tool_name`+`duration_ms` 做聚合/阈值判断 | NFR / functional design |
| R-2 | 全局 hook 误触其他纯本地 cc(接管模式下) | 中 | 中 | session_id/cwd 门控;agentbridge 无 channel 绑定即丢弃 | application design |
| R-3 | Claude Code 升级改变 hook payload 形状 | 低 | 中 | 防御性解析;优先稳定字段;加单测覆盖解析 | code generation / build-and-test |
| R-4 | hook 脚本 bug 阻塞 cc 主流程 | 低 | 高 | hook 永不阻塞、出错 `exit 0`;脚本极薄无状态 | code generation |

## Assumptions

| ID | 假设 | 验证状态 |
|----|------|----------|
| A-1 | Stop hook 每轮带 `last_assistant_message`(纯文字) | ✅ 已亲手验证 |
| A-2 | PostToolUse 每工具实时触发,带 tool_name/response/duration_ms | ✅ 已亲手验证 |
| A-3 | 全部 hook 带 session_id + cwd | ✅ 已亲手验证 |
| A-4 | `claude --settings <path>` 可注入 hook(自动起场景) | ✅ `claude --help` 确认 |
| A-5 | 接管已在跑的 cc 只能靠全局 settings(无法事后 --settings) | ✅ 逻辑确认(进程已启动) |
| A-6 | 本机 localhost HTTP 对单用户场景足够(无需鉴权/TLS) | 假设成立(本地回环);若未来多机需重审 |

## Issues (当前已知,非阻塞)

| ID | 问题 | 状态 |
|----|------|------|
| I-1 | 大量未提交的旧改动(截图模式、proxy、/attach、/resume 等)还在工作区 | 未决 —— 本 feature 会移除截图路径,需决定旧改动如何收束(留到 construction/ship 阶段处理) |
| I-2 | 现有 tmux backend 的 `render_screenshot`/poll 截图/`render_term.py` 待移除 | 计划内 —— 本 feature 的 scope 明确包含移除 |

## Dependencies

| ID | 依赖 | 类型 | 状态 |
|----|------|------|------|
| D-1 | Claude Code hook 系统(Stop/PostToolUse) | 外部(平台能力) | ✅ 可用、已验证 |
| D-2 | agentbridge 现有 `axum` 依赖 | 内部 | ✅ 已在 Cargo.toml |
| D-3 | agentbridge 现有 `AgentEvent` / `Platform` trait / 事件管线 | 内部 | ✅ 已存在(截图也走这条) |
| D-4 | tmux `send-keys` 输入路径 | 内部 | ✅ 已存在,不改 |
| D-5 | 系统 `python3`(若 hook 脚本用 python)或纯 shell/curl | 外部(运行环境) | ✅ 本机可用 |
