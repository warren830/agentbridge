# Component Dependency — Hook-Based Real-Time Text Relay

> **修订 3(最终)**:取消 C-7 常驻消费者(ADR-7 退役)。hook 事件经 C-4 持有的 `event_tx` clone 喂进 session **既有 event channel**,由原样 `process_agent_events` 消费。channel 由 `start_session_for_entry` 创建(binder 真实落点)。下方 C-7 相关条目以本注记修正:C-7 取消,其位置由"既有 channel + 既有 process_agent_events"替代;C-5 进度回到 events.rs 由 per-turn bool 控制。构建依赖表中"C-7 消费者"行改读为"events.rs ToolUse/Result 分支加 per-turn inplace 子分支"。

## Dependency DAG(拓扑;构建顺序由 delivery-planning 定经济路径)

```
C-6 安装器 ─(部署期)──> 把 C-1 注册进 ~/.claude/settings.json(含端口 ADR-8)
C-1 hook 脚本 ──(运行时 POST)──> C-2 接收端
                                    |
                                    v  resolve(cwd 前缀)
                                  C-4 路由注册表 ──> hook_tx
                                    |                  ^
                                    | C-3 映射(→AgentEvent)
                                    v                  | bind(work_dir, hook_tx)
                                  hook_tx.send ──> [hook channel] <── binder 建 (hook_tx,hook_rx)
                                                        |                       (引擎桥接时)
                                                        v hook_rx.recv
                                                  C-7 常驻消费者 task
                                                    | (含 C-5 进度 preview)
                                                    v
                                          Platform capability traits (既有) ──> 手机
                                                    |
                                                    v Stop→notify
                                          引擎每轮 await turn_done(ADR-7)
```
<!-- Text fallback: C-6 部署期把 C-1 注册进全局 settings(含端口)。运行时 C-1 POST 给 C-2;C-2 经 C-3 映射成 AgentEvent、用 C-4 的 cwd 前缀匹配拿 hook_tx 并 send 进 hook channel。该 channel 由 binder(引擎桥接会话时)新建,binder 同时 spawn C-7 常驻消费者持 hook_rx。C-7 recv 事件,内含 C-5 进度 preview 逻辑,经既有 Platform capability traits 派发到手机;收到 Stop 时 notify 引擎的 await turn_done。不经 TmuxSession channel、不经 process_agent_events。 -->

## 构建依赖(供 units-generation / delivery-planning)

| 组件 | 依赖 | 可独立构建? |
|------|------|--------------|
| C-4 路由注册表 | 无(新 `hook_route.rs`,Arc<Mutex<HashMap>>) | ✅ 先建,可单测 |
| C-3 映射 | core/event(既有 AgentEvent) | ✅ 纯函数,可先写+测 |
| C-2 接收端 | C-4、C-3、axum(既有) | 依赖 C-4、C-3 |
| C-7 消费者 | C-4(channel)、core/platform(capability)、core/streaming、events.rs 派发 helper | 依赖 C-4;派发 helper 需从 events.rs 抽出 |
| C-5 进度逻辑 | 在 C-7 内、core/streaming(StreamPreview) | 随 C-7(批次2) |
| binder 集成 | 引擎(桥接点)、C-4、C-7、turn_done Notify、ADR-7 turn-await | 依赖 C-4、C-7 |
| C-1 hook 脚本 | C-2 端口契约(ADR-8) | ✅ 可独立写 |
| C-6 安装器 | C-1 脚本路径、端口(ADR-8) | ✅ 独立 |

## 与 scope 批次对齐

- **MVP(批次1)**:C-4 + C-3(Stop)+ C-2 + C-7(Stop 派发)+ binder/ADR-7 turn-await + ADR-5 关 poll 输出 + C-1(Stop)+ C-6 → US-1/2/4/5(Stop 文字端到端)。
- **批次2**:C-3(PostToolUse)+ C-5 进度 preview(在 C-7 内)→ US-3。
- **批次3**:移除截图代码 → US-6。

## 关键集成点(reviewer 重点)

修订 2 后,风险点转移:
1. **ADR-7 引擎 turn 协调**:tmux+hook 后端的一轮不再走 `process_agent_events`,改为 `send-keys`+`await turn_done`。引擎需按 backend 分流(claude/acp 走旧 process_agent_events,tmux+hook 走新路径)。这是最大的新接线,functional-design 必须坐实分流点。
2. **ADR-5 poll 输出全关**:枚举的 4 个发射点(Text/Result/Thinking/Image)在 hook 模式下确实关闭,permission 保留。
3. **派发 helper 抽取**:C-7 与 events.rs 共享 AgentEvent→capability 派发逻辑,避免两份漂移。
4. **idle 心跳缺口(m-1)**:Stop-only MVP 期无 PostToolUse 心跳,靠 ADR-7 turn-await 超时兜底。
