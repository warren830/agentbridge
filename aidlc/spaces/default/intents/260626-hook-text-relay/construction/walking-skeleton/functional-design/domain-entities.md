# Domain Entities — Walking Skeleton (U-1..U-8)

> 本 feature 是传输/桥接,**无业务域实体**。仅有数据传输结构。引用 `component-methods.md`。

## 数据结构(非业务实体,是传输 DTO)

### `HookPayload`(U-1,新增,`src/hook_receiver.rs`)
- serde `Deserialize`;所有字段 `Option<T>`(防御性,FR-2.2)。
- 字段:`hook_event_name`、`session_id`、`cwd`、`last_assistant_message`(Stop)、`tool_name`/`tool_input`/`tool_response`/`duration_ms`(PostToolUse)。
- 生命周期:请求级,无持久化。

### `HookRouteRegistry`(U-2,新增,`src/hook_route.rs`)
- `Arc<Mutex<HashMap<normalized_work_dir: String, mpsc::Sender<AgentEvent>>>>`。
- 生命周期:进程级共享状态;条目随 session 绑定增删。

### `AgentEvent`(既有,`core/event.rs`,不改)
- 复用既有 `Result` / `ToolUse` 变体(ADR-3,不扩枚举)。

## 关系

```
HookPayload --map_hook--> Option<AgentEvent> --send--> HookRouteRegistry 中的 mpsc::Sender
                                                              |
                                              (既有 session event channel)
```
<!-- Text fallback: HookPayload 经 map_hook 转成 Option<AgentEvent>,经 HookRouteRegistry 里查到的 mpsc Sender 投入既有 session event channel。 -->

## 配置实体(U-4e,`config/mod.rs`)
- `TmuxConfig` 加输出模式字段(screenshot|hook)。
- 新增 `hook_receiver.port`(默认 9123)。

## 无实体生命周期状态机

本 feature 无有状态业务实体;唯一的"状态"是 StreamPreview(既有,批次2 U-9 复用)的 Idle→Active→Frozen→Finished,不在本 unit 新建。
