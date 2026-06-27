# Tech Stack Decisions — Walking Skeleton

> 承自 `technology-stack.md`、`decisions.md` ADR-2/ADR-8。

## 复用既有栈(零新依赖,NFR-7)

| 关注点 | 选择 | 理由 |
|--------|------|------|
| HTTP 接收端 | `axum`(已有) | 镜像 webhook.rs;零新依赖 |
| 反序列化 | `serde`/`serde_json`(已有) | HookPayload Option 字段 |
| 异步 | `tokio`(已有) | 热路径 async(CLAUDE.md) |
| channel | `tokio::sync::mpsc`(已有) | 复用 session event channel |
| 日志 | `tracing`(已有) | 结构化字段 |
| 错误 | `anyhow`(应用边界) | 接收端/引擎集成在边界 |
| hook 脚本 | python3(系统自带) | 语言无关、Pillow 不再需要 |

## 决策

- **TSD-1** 零新 Cargo 依赖(CLAUDE.md:加依赖先问 —— 本 feature 无需加)。
- **TSD-2** 端口默认 9123(ADR-8),config 可覆盖,区隔 webhook 9111。
- **TSD-3** 移除运行时依赖 Pillow(批次3 随截图路径删)。
- **TSD-4** 无 unsafe(CLAUDE.md);所有切片 char-boundary 安全(NFR-5/project ## Corrections)。

## 验收
- `cargo build` 不新增依赖;`Cargo.toml` 无新增 crate。
