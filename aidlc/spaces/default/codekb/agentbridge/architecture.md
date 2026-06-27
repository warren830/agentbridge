# Architecture — agentbridge

## Architectural Style

**模块化单体**(单 cargo crate,bin + lib),围绕三组解耦边界组织:

1. **Platform 边界**(`core/platform.rs`)—— capability traits,引擎只经此与聊天平台交互。
2. **Agent 边界**(`agent/mod.rs`)—— `AgentSession` trait,引擎只经此与 Claude/ACP/tmux 交互。
3. **Engine 路由核心**(`engine/`)—— 居中调度,刻意保持"只做路由",拆成 commands/events/skills。

## Component Relationship Diagram

```
  +-------------+      IncomingMessage      +------------------+
  | Platform    | ------------------------> |  Engine          |
  | adapter     |                           |  (handle_message)|
  | (Discord/   | <-- capability calls ---- |                  |
  |  Telegram)  |    (reply/preview/etc)    +--------+---------+
  +-------------+                                    |
        ^                                            | drives
        | AgentEvent dispatch                        v
        | (engine/events.rs)                +------------------+
        +---------------------------------- |  AgentSession    |
                                            |  (claude/acp/    |
                                            |   tmux)          |
                                            +--------+---------+
                                                     |
                                            emits AgentEvent (mpsc)
                                                     |
                                            +------------------+
                                            | event_broadcast  | --> gateway forwarder
                                            +------------------+
```
<!-- Text fallback: Platform 适配器收到消息转 IncomingMessage 给 Engine;Engine 经 AgentSession 驱动某 backend;backend 经 mpsc 发 AgentEvent;engine/events.rs 把事件分发回 Platform 的 capability 调用(reply/preview/image/buttons),同时广播到 gateway。 -->

## Interaction Diagram — 一轮消息的处理(claude backend)

```
用户消息 --> Platform.start 回调 --> MessageHandler --> Engine.handle_message
   |
   +-- try_lock session (busy AtomicBool)
   |     成功 --> AgentSession.send(prompt)
   |     失败 --> 入有界队列(MAX_QUEUED=5)
   |
   +-- AgentSession 跑,发 AgentEvent 到 mpsc
   |
   +-- engine/events.rs::process_agent_events 循环:
   |     Text     --> StreamPreview.append --> MessageUpdater.update_preview(节流)
   |     ToolUse  --> freeze preview --> reply("⚡ tool › …")
   |     PermissionRequest --> InlineButtonSender.send_with_buttons --> 阻塞等决定(≤600s)
   |     Result   --> finalize preview + [ctx ~N%]
   |
   +-- SessionGuard drop / unlock --> drain 队列下一条
```
<!-- Text fallback: handle_message 先 try_lock session,成功则 AgentSession.send,失败入队;backend 发事件经 mpsc;events.rs 把 Text 转预览编辑、ToolUse 转 reply、PermissionRequest 转按钮并阻塞等待、Result 收尾;guard drop 后 drain 队列。 -->

## Key Patterns

- **Capability query interface**:`PlatformCapabilities` 用 `as_xxx() -> Option<&dyn Trait>` 让引擎运行时探测平台能力(更新预览/发图/按钮/typing),平台不支持就返回 `None`,引擎降级。
- **Try-lock + bounded queue session**(`core/session.rs`,load-bearing):`AtomicBool` CAS 抢锁,RAII `SessionGuard`,`unlock()` 在持有 interactive-state mutex 时调用以闭合队列/解锁竞态。**不可换成阻塞 mutex 或无界 channel**(CLAUDE.md)。
- **StreamPreview 状态机**(`core/streaming.rs`):Idle→Active→Frozen→Finished,节流编辑(MIN_INTERVAL=1500ms,MIN_DELTA=30 chars)。
- **事件广播**:每个 AgentEvent 既驱动平台调用,也 `event_broadcast.send` 给网关。

## 与本 intent(hook-relay)的关系

hook HTTP 接收端是 tmux backend 在"出"方向的新实现:它把 hook payload 映射成 `AgentEvent`(Text/ToolUse/Result),投入**同一条 `engine/events.rs` 管线**。**不新增 agent 类型、不碰 platform 注册** —— 复用现有 AgentSession→事件管线→capability traits 全链路。
