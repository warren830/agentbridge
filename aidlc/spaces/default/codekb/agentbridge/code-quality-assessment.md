# Code Quality Assessment — agentbridge

## Testing

- **位置**:全部 inline `#[cfg(test)] mod tests`;无顶层 `tests/`(lib.rs 预留了集成测试公开面但暂未用)。
- **框架**:stock `#[test]`/`#[tokio::test]` + `tempfile::TempDir`,无 mock/assertion 第三方。
- **重点覆盖**:`core/session.rs`(~30 测:try_lock/队列/CRUD/持久化)、`agent/mod.rs`(工具摘要/base64/assistant 解析)、`agent/tmux/session.rs`(reply-block 抽取/footer/权限菜单)、`engine/skills.rs`、`config/mod.rs`(校验矩阵)、`engine/mod.rs`(allow-list/banned-words/agent 流)。
- **`#[ignore]` live 测**:`tmux/session.rs::tmux_live_screenshot`(需 tmux+claude+Pillow)、`acp/session.rs` 两个(live ACP 子进程)。
- 无覆盖率插桩。

## Code Quality Indicators

- **Tracing**:一致的结构化字段(`session_key = %key`),库/引擎无 `println!`(仅 main.rs CLI 用,合理)。
- **错误处理**:`anyhow` 在应用边界(engine/platforms/binaries/gateway),`thiserror` 为依赖;边界纪律合规。
- **无 `unsafe`**:确认 `src/` 中零。
- **`unwrap`/`expect`**:约 20 文件含,主要在 `#[cfg(test)]` 内或 `std::sync::Mutex::lock().unwrap()`(SessionManager 内部可变,已知模式);热路径 async 用 `?`。
- **Session 纪律(load-bearing)**:try-lock+队列完整(AtomicBool CAS、RAII guard、持锁解锁闭合竞态、有界队列 MAX=5、UnlockGuard 兜底)。
- **CLAUDE.md 守则遵守**:引擎只经 capability traits;agent 经 AgentSession;engine 已拆 commands/events/skills。

## Technical Debt

| # | 债务 | 严重度 |
|---|------|--------|
| TD-1 | **大量未提交改动**(10 文件,+1245/−159):tmux 截图+settle+auto-restart 重写、AgentEvent::Image、/attach、/resume、Discord 代理隧道、移除 build_sender_prompt。需在 construction/ship 收束 | 高(范围管理) |
| TD-2 | tmux 截图路径(screenshot flag/render_screenshot/find_python_with_pil/RENDER_SCRIPT/STABLE_TICKS)—— **本 intent 移除目标** | 中(计划内) |
| TD-3 | **无中央 platform factory map** —— CLAUDE.md 描述的 factory HashMap 未落地,实为 engine 硬编码 match。agent backend 反而有运行时分派 | 中(设计偏离) |
| TD-4 | StreamPreview `display_text` 按字节切片,CJK 多字节尾部可能 panic-slice。hook 文字成主路径后风险上升 | 中(本 intent 应修) |
| TD-5 | 3 个 TODO:discord ACP perm option_id 未接(总映射 allow)、gateway token 未走 URL、gateway 前端订阅无 per-session ACL(转发所有事件给所有前端) | 低-中 |
| TD-6 | `FileSender` 契约定义但无适配器实现(dead) | 低 |
| TD-7 | gateway client 重连是平 5s sleep(无 backoff),不如 Discord/Telegram 的抖动指数退避 | 低 |
| TD-8 | resume 路径历史不稳(process_and_drain 有 retry-on-empty 补丁;sdk-cli entrypoint 会话不进原生 /resume) | 低-中 |

## 对本 intent 的启示

- **TD-2** 是本 intent 的移除目标(scope PU-5)。
- **TD-4** 应在本 intent 顺带修(hook 文字是新主路径)。
- **TD-3** 决定了 hook 接收端必须经事件管线而非 platform 注册。
- **TD-1** 的截图相关部分由本 intent 移除批次覆盖;其余(proxy/attach/resume)范围外(RAID I-1)。
