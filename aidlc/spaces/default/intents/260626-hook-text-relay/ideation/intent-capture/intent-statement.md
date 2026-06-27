# Intent Statement — Hook-Based Real-Time Text Relay

## Problem Statement

agentbridge 的 tmux backend 目前靠**每 150ms 截一次终端屏幕 + Pillow 渲染 PNG** 把 Claude Code 的活动发回聊天软件。这条路是三种 backend 里最弱的:

- **啰嗦/刷屏** —— 截图节奏靠 idle-debounce、stable_ticks、saw_busy 等启发式逼近"一轮结束",难以稳定做到"一轮一条"。
- **渲染脆弱** —— CJK 显示成豆腐块、字体/列宽对齐、PIL 路径探测等一连串 bug。
- **信息有损** —— 截图是像素,不是结构化文本;手机端看不清、无法复制、和 Mac 终端的原生体验不一致。

要解决的核心问题:**把"截图回传"换成"用 Claude Code 官方 hook 实时回传纯文字",在不牺牲"操作的是同一个正在跑的 cc"这一前提下,让手机端看到干净、结构化、节奏自然的文字交互。**

技术路径已 de-risk(本会话亲手验证):
- **Stop hook** 每轮触发一次,payload 直接带 `last_assistant_message`(纯文字回复本身)—— 比 remote-claude-control 还省一步(它需读 transcript 反扫 JSONL)。
- **PostToolUse hook** 每个工具调用后实时触发,带 `tool_name` / `tool_input` / `tool_response`(含 stdout/stderr)/ `duration_ms`。
- 每个 hook payload 都带 `session_id` + `cwd`,可用于门控与路由。
- `claude --settings <path>` 可注入 hook 配置而不改项目文件(已 `claude --help` 确认)。

## Target Customer

**单用户、自用**(项目作者本人)。

- **场景**:人在手机端(先用 Discord 验证,最终目标飞书),远程控制 Mac 上**已经在跑的那个 cc 会话**(例如 nova-bidding 目录里的 cc)。
- **核心诉求**:"手机和电脑看到的跟 Mac 里面一模一样" —— 同一个 cc(带本人的 CLAUDE.md / MCP / skills 全套配置),但回传的是干净文字而非截图。
- **当前痛点**:截图看不清、刷屏、节奏失控、与原生终端体验割裂。

## Success Metrics

成功 = **作者本人 live 端到端验证通过**(遵循 [[feedback-self-test-before-asking]]:自己测通再叫人,不拿用户当调试器):

| # | 可验证结果 |
|---|-----------|
| 1 | 手机发消息 → 经 agentbridge `tmux send-keys` 敲进 Mac 上**正在跑的** cc(输入路径不变) |
| 2 | cc 每轮答完 → 聊天软件收到**一条干净文字回复**(来自 Stop hook 的 `last_assistant_message`,非截图) |
| 3 | cc 跑工具时 → 收到**逐工具进度**("⏺ 正在跑 npm test…"),来自 PostToolUse |
| 4 | 全程是**同一个 cc**,带本人 CLAUDE.md / MCP / skills |
| 5 | **截图代码路径被移除**(render_screenshot / poll 截图轮询 / Pillow 脚本) |
| 6 | 无"截图刷屏""节奏失控"问题(进度事件有节流/合并策略) |

## Initiative Trigger

**为什么现在**:

1. 已系统学习两个姊妹项目(`~/warren_ws/happy`、`~/warren_ws/remote-claude-control`),确认 remote-claude-control 的 **Stop-hook 读文字**是生产级成熟做法,且我们当前的截图方案是三者里最弱的。
2. 本会话**亲手触发并捕获了全部 hook 的真实 payload**,确认 Stop 直接给纯文字、PostToolUse 给结构化工具结果 —— 技术路径不再有"我猜"的部分。
3. 作者明确表达"很不喜欢截图这个方案",意图清晰、时机成熟。

## Initial Scope Signal

**feature 规模、brownfield 改造**(已确认 scope=feature):

- **改动范围**:
  - 新增极薄 hook 脚本(~30 行),带 `cwd`/`session_id` 门控,只把 payload POST 给本地 agentbridge —— 不自己发消息、不截图。
  - agentbridge 内新增**本地 HTTP 接收端**(复用已有 `axum` 依赖)。
  - hook payload → `AgentEvent`(`Text` + 新增的工具进度事件)映射,走现有事件管线。
  - **移除**截图轮询 / `render_screenshot` / `render_term.py`(Pillow)。
- **不改**:tmux backend 的**输入**路径(`send-keys`)保持不变。
- **架构约束**:hook 只 POST 给 agentbridge,由现有 `Platform` trait 发出 —— 不破坏 CLAUDE.md 中"引擎绝不 `if platform == ...` 分支"的约束;以后加飞书零改动。
- **hook 安装策略**:接管已在跑的 cc —— hook 装进全局 `~/.claude/settings.json`,用 tmux session / cwd 门控,只对 agentbridge 桥接的会话生效,不影响其他纯本地 cc。

## Constraints & Guardrails (from CLAUDE.md)

- Rust + tokio,所有热路径 I/O 异步(`tokio::fs`/`process`/`time`)。
- `anyhow` 在应用边界,`thiserror` 在库级;不在测试外 `unwrap()`/`expect()`。
- 只用 `tracing`,结构化字段。无 `unsafe`。
- 注释写 *why* 不写 *what*,不加跨项目引用。
- 加新依赖前先问(优先复用 `axum`/`reqwest`/`tokio`/`serde`)。
- 非平凡改动后跑 `cargo check`,声明完成前跑 `cargo test`。
- 平台适配器只经 `Platform` + `ReplyCtx` capability traits 与引擎交互;引擎不下钻、不按平台名分支。
