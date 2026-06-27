# Code Generation Plan — Walking Skeleton (U-1..U-8 Stop path)

> 实现批次1(Stop 文字端到端)。承自 units `unit-of-work.md`、`business-rules.md`、`decisions.md`(rev-3)。

## 实现顺序(按依赖,纯→集成)

1. **U-1/U-2(纯,先测)**:`src/hook_route.rs`(HookRouteRegistry)+ `src/hook_receiver.rs` 的 HookPayload/map_hook。
2. **U-3**:hook_receiver.rs 的 axum server。
3. **U-4a(最高风险)**:session.rs 暴露 event_tx clone。
4. **U-4b/c/d/e**:registry.rs bind、engine cleanup unbind、main 启动接收端、config 端口。
5. **U-5**:poll_loop hook 模式门控。
6. **U-7**:scripts/agentbridge_hook.py。
7. **U-8**:main.rs hook-install 子命令。

## 约束清单(CLAUDE.md)

零新依赖 / 无 unsafe / 无 unwrap(测试外)/ anyhow 边界 / tracing / char-boundary 安全 / 不碰 session try-lock / 引擎不分支平台 / 不碰 process_and_drain race 逻辑。

## 自测要求(project ## Corrections)

cargo check 全绿 + cargo test 全绿 + **live 端到端**(真实 axum server + 真实 python 脚本,验 Stop→Result 投递 + 门控丢弃 + CJK)。
