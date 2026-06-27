# Business Logic Model — Walking Skeleton (U-1..U-8)

> 权威功能行为已在 `inception/application-design/component-methods.md` + `services.md` 指定。本文聚焦 walking-skeleton 的算法/流程,引用上游 `unit-of-work.md`、`requirements.md`、`components.md`、`component-methods.md`、`services.md`。

## 核心算法 1:hook payload → AgentEvent 映射(U-1,纯函数)

```
map_hook(payload):
    match payload.hook_event_name:
        "Stop":
            text = payload.last_assistant_message or ""
            if text.is_empty(): return None        # 空轮不发(ADR-3 M-3 强制)
            return Result { content: text, session_id, input_tokens: 0, output_tokens: 0 }
        "PostToolUse":                              # 批次2 才接;批次1 可先返回 None
            return ToolUse { id, tool: tool_name, input: summarize(payload) }
        _: return None                             # 不接的事件
```

决策树:事件类型 → 取对应字段 → 空值守卫 → 构造 AgentEvent。无副作用、无 I/O。

## 核心算法 2:hook 路由解析(U-2,门控)

```
resolve(cwd):
    norm = canonicalize(cwd)
    for (work_dir, tx) in registry:               # 前缀匹配(ADR-6 m-2,容子目录)
        if norm.starts_with(work_dir): return Some(tx.clone())
    return None                                    # 未命中 → 接收端丢弃(门控 FR-3.3)
```

## 核心流程:Stop 文字端到端(U-6,MVP)

```
1. 手机消息 → Engine try-lock → send-keys → cc(既有,不改)
2. Engine: take_events + process_agent_events(drain session event_rx,既有)
3. cc 答完 → Stop hook → hook 脚本 POST /hook-event
4. 接收端 handle_hook_event:
     tx = registry.resolve(cwd)?                   # 门控
     ev = map_hook(payload)?                       # 映射
     tx.send(ev).await                             # 投入既有 channel
     return 200                                    # 永远 200
5. 正在跑的 process_agent_events recv 到 Result → reply(content) → 见 Result break turn(既有路径)
6. 手机收到干净文字
```

## 数据转换

- hook stdin JSON →(serde 防御性)→ `HookPayload`(全 Option 字段)→(map_hook)→ `AgentEvent` →(既有 events.rs)→ Platform reply。
- 无持久化、无状态累积(批次1)。

## 错误/边界处理

- payload 缺字段 / 非法 JSON → 接收端 200 + tracing::warn,不 panic(FR-2.2)。
- resolve 未命中 → 丢弃(门控)。
- tx.send 失败(channel 满/关)→ 静默(尽力投递)。
- hook 脚本任何错误 → exit 0(永不阻塞 cc,FR-1.4)。
- 长轮 >300s → U-5b 静默保活防 idle timeout 误触。
