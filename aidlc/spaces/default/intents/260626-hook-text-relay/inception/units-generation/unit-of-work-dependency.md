# Unit of Work Dependency — Hook-Based Real-Time Text Relay

## 依赖 DAG(拓扑;构建顺序由 delivery-planning 定)

```
U-1 映射(纯函数) ──┐
                    ├──> U-3 接收端 ──> U-4 引擎集成 ──> U-5 poll 门控 ──┐
U-2 路由表 ─────────┘                      |                           ├──> U-6 Stop 端到端(MVP)
                                           |                           |
U-7 hook 脚本(Stop) ──────────────────────┘                           |
U-8 安装器 ────────────────────────────────────────────────────────────┘
                                                                        |
                                                                        v
                                                          U-9 PostToolUse 进度(批次2)
                                                                        |
                                                                        v
                                                          U-10 移除截图(批次3)
```
<!-- Text fallback: U-1 映射 和 U-2 路由表 无依赖,可最先并行建。U-3 接收端依赖 U-1+U-2。U-4 引擎集成依赖 U-3。U-5 poll 门控依赖 U-4。U-7 hook 脚本(Stop)依赖 U-3 端口契约,可并行。U-8 安装器依赖 U-7。U-6 Stop 端到端(MVP 完成点)集成 U-1..U-5+U-7+U-8。U-9 PostToolUse 进度(批次2)依赖 U-6。U-10 移除截图(批次3)依赖 U-6 验证通过。 -->

## 依赖表

| Unit | 直接依赖 | 可并行起点? |
|------|----------|--------------|
| U-1 映射 | core/event(既有) | ✅ 最先 |
| U-2 路由表 | 无 | ✅ 最先(与 U-1 并行) |
| U-7 hook 脚本 | U-3 的端口契约(ADR-8,可先定) | ✅ 可并行(契约对齐) |
| U-3 接收端 | U-1, U-2 | — |
| U-4 引擎集成 | U-3 | — |
| U-8 安装器 | U-7 | ✅ 可早 |
| U-5 poll 门控 | U-4 | — |
| U-6 Stop 端到端 | U-1..U-5, U-7, U-8 | MVP 收口 |
| U-9 PostToolUse | U-6 | 批次2 |
| U-10 移除截图 | U-6(验证通过) | 批次3 |

## 关键路径

`U-1/U-2 → U-3 → U-4 → U-5 → U-6` 是 MVP 关键路径。U-7/U-8 可与 Rust 侧并行(端口契约 ADR-8 先定即可),在 U-6 收口前就位。

## 风险标注(给 delivery-planning / construction)

- **U-4** 最高风险 unit:含 **U-4a channel 所有权重构**(改 `TmuxAgent::start_session` 返回/构造,暴露 event_tx clone —— arch-review B-1 指出的真实重构,触碰 `session.rs`),外加 registry.rs / mod.rs / config。不碰 process_and_drain 的 race 逻辑。
- **U-4 与 U-5 共享 `session.rs`(m-3)**:U-4a 改 channel 构造,U-5 改 poll_loop;DAG 边 U-4→U-5 已串行化,U-5 在 U-4a 重构后的签名上构建。construction 须知二者同文件、有先后。
- **U-5** 改 poll_loop —— 门控 Text/Result/Image 三发射点,**保留 permission 检测 + 保留心跳 Thinking 作 idle 保活(M-1)**,测试两条路径(screenshot/hook)。
- **U-9** 改 events.rs —— per-turn bool 的 false 路径必须零回归(claude/acp 测试)。
- **端口字面量** 9123(ADR-8)由 U-4e 定义,U-3/U-7/U-8 共享,勿漂移(m-5)。
