# External Dependency Map — Hook-Based Real-Time Text Relay

## 外部依赖

| 依赖 | 类型 | Bolt | 状态 | 风险 |
|------|------|------|------|------|
| Claude Code hook 系统(Stop/PostToolUse) | 平台能力 | Bolt 1/2 | ✅ 已验证(本会话亲手触发) | payload 升级漂移 → U-1 防御性解析 |
| `claude --settings` / 全局 `~/.claude/settings.json` hooks 段 | 平台契约 | Bolt 1 | ✅ 已验证 | 格式变化 → U-8 合并写需容错 |
| tmux(`send-keys`/`capture-pane`) | 外部进程 | 全程 | ✅ 已在用(既有 tmux backend) | 不变(输入路径保留) |
| python3(hook 脚本运行时) | 运行时 | Bolt 1 | ✅ 系统自带 | Pillow **不再需要**(Bolt 3 移除) |
| axum / serde / tokio | crate | Bolt 1 | ✅ 已在 Cargo.toml | 零新依赖 |
| Discord(验证平台)/ 飞书(目标) | 出口平台 | Bolt 1 | ✅ Discord 已接;飞书经 Platform trait 零改动可加(范围外) | — |

## 无新增外部依赖

零新 crate(ADR-2/技术栈核对)。移除一个运行时依赖(Pillow,Bolt 3)。无云、无网络服务、无第三方 API。

## 阻塞依赖

无硬阻塞。所有依赖已就位或已验证。Bolt 1 可立即开工。
