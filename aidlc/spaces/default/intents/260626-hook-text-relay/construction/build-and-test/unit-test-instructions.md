# Unit Test Instructions

```
cargo test                          # 全量
cargo test --bin agentbridge hook_  # 仅 hook 相关单测(16 个)
```

## 覆盖(新增 16 单测)
- `hook_route`(6):bind→resolve exact / subdir prefix / miss None / unbind None / sibling-dir 非前缀 / sender 投递到绑定 receiver。
- `map_hook`(4):Stop+text→Result / 空 Stop→None(BR-1)/ PostToolUse→None(BR-16)/ 未知→None。
- gating(1):hook 模式门控 output+heartbeat。
- hook_install(5):空 settings 加条目 / 保留既有 / 幂等 / 双事件 / 非 object 容错。

Standard test strategy(team.md Testing Posture:test-alongside、单元为主、inline #[cfg(test)])。
