# Technology Stack — agentbridge

## Language & Build

- **Rust**(单 cargo crate,bin + lib),`tokio` 异步运行时。
- 构建:`cargo`。无 workspace、无 feature flags、无 `build.rs`。

## Libraries (Cargo.toml)

| 库 | 版本 | 用途 |
|----|------|------|
| tokio | 1 (full) | 异步运行时;所有热路径 I/O(process/fs/time/sync) |
| serde / serde_json / serde_yaml | 1 / 1 / 0.9 | 配置(YAML)、wire(JSON)、stream-json 解析 |
| reqwest | 0.12 (json,stream,multipart) | Telegram/Discord REST、网关 fetch、doctor |
| axum | 0.8 (ws) | 网关 server、webhook 接收端 |
| tower-http | 0.6 (fs,cors,limit) | 网关 CORS/静态/body-limit |
| tokio-tungstenite | 0.24 (native-tls) | Discord Gateway WS、网关 client |
| futures / futures-util | 0.3 | Sink/Stream 组合 |
| async-trait | 0.1 | async trait 对象 |
| tracing / -subscriber / -appender | 0.1 / 0.3 / 0.2 | 结构化日志(唯一日志机制) |
| clap | 4 (derive,env) | CLI |
| uuid | 1 (v4) | 会话 ID、权限 request ID |
| chrono | 0.4 (serde) | 时间戳 |
| thiserror / anyhow | 2 / 1 | 错误处理(库级/应用边界) |
| rusqlite | 0.34 (bundled) | 网关消息历史(WAL) |
| dirs | 6 | home 解析 |
| cron | 0.15 | cron 表达式 |
| hostname | 0.4 | 网关 instance_id |
| tempfile | (dev) | 测试临时目录 |

## Runtime External Deps

- `claude`(Claude Code CLI)—— claude/tmux backend 调用。
- `tmux` —— tmux backend。
- `python3` + Pillow —— 当前 tmux 截图渲染(`render_term.py`,**本 intent 将移除**)。
- `ssh`/`rsync` —— sync 子命令。

## 本 intent 对栈的影响

**零新依赖**(CLAUDE.md 要求加依赖先问)。hook 接收端复用 `axum`,payload 反序列化用 `serde`/`serde_json`,均已在栈内。移除 `python3`/Pillow 运行时依赖(随截图路径删除)。
