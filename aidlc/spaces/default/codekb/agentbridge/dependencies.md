# Dependencies — agentbridge

## External Dependencies

见 `technology-stack.md` 的库表。运行时外部进程:`claude`、`tmux`、`python3`+Pillow(截图,将移除)、`ssh`/`rsync`(sync)。

## Internal Cross-Module Dependencies

```
main ──> engine ──> core (platform/event/session/streaming/message)
          │  │  └──> agent (mod/registry ──> {claude, acp/*, tmux/*})
          │  └─────> platforms (discord/telegram)  [仅经 core::platform traits]
          ├──> config
          ├──> gateway (server/client/protocol/db)
          ├──> webhook / cron / relay / sync / speech / dedup / ratelimit / lock
          └──> skills
```
<!-- Text fallback: main 起 engine;engine 依赖 core 契约层、agent backend、platforms(只经 trait);config/gateway/webhook/cron 等是 main 直接挂载的旁路组件。core 是最底层,不依赖 agent/platform。 -->

## 关键依赖方向(架构约束)

- **core 不依赖 agent/platform** —— 它是纯契约层,反转依赖。
- **engine 不依赖具体 platform 类型** —— 只经 `core::platform` 的 capability traits(CLAUDE.md:引擎不 `if name==`)。例外:`engine::create_platform_capabilities` 的 match 是唯一的平台名分支(已隔离)。
- **engine 不依赖具体 agent 类型** —— 只经 `AgentSession`;`agent/registry.rs` 做运行时字符串分派。

## 本 intent 的依赖影响

- 新增:HookReceiver(挂在 main/engine 旁,像 webhook)→ 经 `core::event::AgentEvent` + 事件管线注入,**不新增对 platform 的依赖**。
- 移除:tmux backend 对 `python3`/Pillow 的运行时依赖。
- 零新 crate 依赖。
