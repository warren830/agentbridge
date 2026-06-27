# Quality Gates — Hook Text Relay

> 本项目质量门是本地的(无 CI)。本 feature 已全部满足:

| 门 | 状态 |
|----|------|
| `cargo check` 0 warning | ✅ |
| `cargo clippy` 0 warning | ✅(修 too_many_arguments) |
| `cargo test` 全绿 | ✅(277 bin + 71 lib + 16 新) |
| live 端到端 | ✅(hook_relay_end_to_end,含 CJK + 门控) |
| 独立 review | ✅(arch-review READY 零 blocking) |
| 零新依赖 | ✅ |
| 无 unsafe / 无 unwrap(测试外) | ✅ |

后续 Bolt 2/3 沿用同一组本地门。
