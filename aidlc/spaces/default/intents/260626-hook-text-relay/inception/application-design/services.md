# Services / Processes — Hook-Based Real-Time Text Relay

> 本 feature 不引入新的可部署进程(deployable service);它在既有 agentbridge 进程内新增一个后台监听器 + 一个独立的 cc 侧脚本。

> **修订 3 取代下图的 C-7 模型**:取消常驻消费者 task 与引擎分流。hook 事件经 C-4 持有的 `event_tx` clone 喂进 session **既有 event channel**,由**原样不变的 `process_agent_events`** 在 engine turn 内消费。channel 由 `start_session_for_entry` 创建。下面"常驻消费者/turn_done Notify"的描述已废弃,仅以本注记修正;数据流见本节末"数据流(修订3)"。

## 进程拓扑(修订 2,部分被修订3取代 —— 见上方注记)

```
+----------------------------------------------------------------+
|  agentbridge 进程(既有)                                        |
|                                                                |
|  Engine ── tmux AgentSession ──> tmux send-keys ───────────────┼──> [正在跑的 cc]
|    |  (输入:try-lock 序列化)                                    |          |
|    |  await turn_done Notify <----------------------+          |          | Stop / PostToolUse
|    |                                                |          |          v
|  binder: 建 (hook_tx,hook_rx) + spawn C-7 消费者    |          |   [C-1 hook 脚本]
|                                                     |          |          |
|  [C-7 常驻消费者 task]── capability traits ──> 手机  |          |          |
|     ^  hook_rx.recv()         (reply/preview)       | notify   |          |
|     |                                               |          |          |
|  [C-2 hook 接收端 :PORT] ── hook_tx.send ───────────+          |<── POST ─┘ (localhost)
|     uses C-4 registry (cwd 前缀 -> hook_tx)                    |
+----------------------------------------------------------------+
```
<!-- Text fallback: Engine 经 tmux AgentSession 用 send-keys 送输入(try-lock 序列化输入)。binder 在桥接会话时新建 hook channel(hook_tx/hook_rx)并 spawn C-7 常驻消费者 task,把 hook_tx 注册进 C-4。cc 触发 Stop/PostToolUse → C-1 脚本 POST → C-2 接收端用 cwd 前缀匹配查 C-4 拿 hook_tx → send AgentEvent → C-7 消费者 recv 并经 capability traits 派发到手机。C-7 收到 Stop/Result 时 finalize preview 并 notify 引擎 turn_done。空闲会话(无 engine turn)C-7 照常派发。channel 由 binder 自建,不 clone TmuxSession 的 event_tx;不依赖 process_agent_events。 -->

## 进程/组件清单

| 单元 | 类型 | 进程位置 | 备注 |
|------|------|----------|------|
| C-1 hook 脚本 | 独立脚本(python3) | cc 侧(被 Claude Code 调起) | 无状态、永不阻塞、出错 exit 0 |
| C-2 hook 接收端 | 后台 tokio task | agentbridge 进程内 | axum,仅 localhost,镜像 webhook.rs |
| C-3 映射 | 纯函数 | C-2 内 | 无 I/O;空轮→None |
| C-4 路由注册表 | 共享状态 | agentbridge 进程内 | `src/hook_route.rs`;Arc<Mutex<HashMap<work_dir, hook_tx>>> |
| C-5 进度协调器 | C-7 消费者内逻辑 | agentbridge 进程内 | 就地编辑 preview,**不碰 events.rs** |
| C-6 安装器 | CLI 子命令 | 一次性运行 | 写 ~/.claude/settings.json(合并) |
| C-7 常驻消费者 | 每会话长寿命 task | agentbridge 进程内 | 持 hook_rx,经 capability traits 派发;ADR-1/ADR-7 |

## 数据流(MVP / US-1,修订)

1. binder 桥接会话时:建 `(hook_tx, hook_rx)`、`registry.bind(work_dir, hook_tx)`、spawn C-7 消费者(持 hook_rx)。
2. 手机消息 → Engine try-lock → tmux `send-keys` → cc(既有路径,不改)→ 引擎 `await turn_done`。
3. cc 答完 → Claude Code 触发 Stop hook → C-1 读 payload → POST localhost C-2。
4. C-2 解析 → C-4 用 cwd 前缀匹配找 `hook_tx` → C-3 映射 `Result` → `hook_tx.send`。
5. C-7 消费者 `recv` → finalize 进度 preview → `cap.reply(content)` → 手机收到文字 → `turn_done.notify`(引擎解锁、drain 队列)。
6. **空闲场景(agent-initiated)**:用户在电脑端直接驱动 cc,引擎没发 prompt、没锁 → cc 触发 Stop → C-1→C-2→C-7 照常派发 → 手机也看到("两边都看到")。

## 失败/边界

- C-1 POST 失败 → 静默(cc 不受影响);该轮文字丢失但 cc 正常 —— 可接受(尽力投递)。
- C-2 消费者已退(hook_rx drop)→ `send` 返回 Err → 静默丢弃。
- C-4 未命中(非桥接 cc / cwd 不在任何绑定 work_dir 前缀下)→ 丢弃(门控)。
- 畸形 payload → warn + 200。
- cc 卡死/Stop 永不来 → 既有 `EVENT_IDLE_TIMEOUT`(300s,`events.rs:27`)兜底(process_agent_events 本就有此超时);hook 模式下 poll 心跳关闭,故长任务需确认超时常量 > 实际轮时长(m-1/m-4,functional-design)。

## 数据流(修订3 —— 最终)

1. `start_session_for_entry`(tmux 分支)建 `(event_tx, event_rx)`:`event_tx` 给 poll 任务 + **一个 clone 注册进 C-4**(keyed by work_dir);`event_rx` 给 TmuxSession。
2. 手机消息 → Engine try-lock → tmux `send-keys` → cc;引擎照常 `take_events`+`process_agent_events`(drain `event_rx`)。
3. cc 答完 → Stop hook → C-1 POST → C-2 → C-4 用 cwd 前缀匹配拿 `event_tx` clone → C-3 映射 `Result` → `send` 进**同一条 channel**。
4. 正在跑的 `process_agent_events` `recv` 到该 `Result` → finalize preview(工具进度若 `tool_progress_inplace`)→ `Platform.reply` → 手机收到文字 → 见 Result 即结束该轮(同旧 poll settle Result 的路径,`process_and_drain` 零改)。
5. cleanup 时 C-4 移除 clone。
6. **idle-only(无 engine turn)缩到 v2**:此时无人 drain `event_rx`,hook Result 滞留 channel 至下一轮 drain_stale —— MVP 不支持纯空闲推送。

## NFR 对应

- NFR-1 延迟:本地 POST + mpsc + 既有管线,< 2s(主要受 Platform API)。
- NFR-3 资源:C-2 无持久化、async、轻量。
- NFR-6 健壮性:每层失败都静默降级、不 panic、不阻塞 cc。
