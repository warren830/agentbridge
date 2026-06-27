# Risk & Sequencing Rationale — Hook-Based Real-Time Text Relay

## 为什么这个 Bolt 顺序

**风险优先 + cutover 安全**:

1. **Bolt 1 先打通传输 + Stop 文字**:最高不确定性集中在这里 —— U-4a 的 channel 所有权重构(改 TmuxAgent::start_session)、hook→既有 channel 注入是否真被 process_agent_events 消费、门控是否正确。这些是整个架构的成败点(arch-review 两轮都聚焦于此)。先验证骨架,后续 Bolt 才有意义。
2. **Bolt 2(进度)在骨架后**:依赖 Bolt 1 的 channel 通路;且 U-9 的就地编辑 preview 复用 Bolt 1 验证过的派发路径。
3. **Bolt 3(删截图)最后**:cutover 规约(project ## Corrections 已沉淀)—— 替换型改造把"删旧机制"放最后,文字回路验证可用前保留截图作 fallback,避免空窗(US-6)。

## 关键风险登记(承自 feasibility RAID + arch-review)

| 风险 | Bolt | 缓解 |
|------|------|------|
| U-4a channel 重构破坏 TmuxSession 死会话检测(B-1 衍生) | Bolt 1 | event_tx clone 与 poll task sender 同源测试;死会话检测退化为 idle-timeout 兜底(已在 ADR-1 记录可接受) |
| 静默保活做错→刷屏/打碎 preview(B-2) | Bolt 1/2 | U-5b 用 timer-reset-only 分支(不 reply 不 freeze);Bolt 2 回归断言零 🧠 |
| poll 输出门控不全→双 Result 竞争(B-3) | Bolt 1 | U-5 枚举 Text/Result/Image 全关,permission 保留;单测断言 |
| /btw 提前 break turn(M-2) | Bolt 1 | MVP 显式划 /btw 在 hook 模式不支持(U-6 文档化),v2 处理 |
| 长轮误触 idle timeout(M-1) | Bolt 1 | U-5b 静默保活;U-6 必含 >300s 长轮测试 |
| claude/acp 回归(U-9 per-turn bool) | Bolt 2 | false 路径零改 + 回归测 |
| idle-only 推送缺失(B-2 设计取舍) | — | 显式缩到 v2(ADR-1),MVP 不承诺 |

## 自测纪律(贯穿所有 Bolt)

project ## Corrections 已沉淀:**声称完成前 AI 自跑全链路 live 测,不拿用户当调试器**。每个 Bolt 完成前:`cargo test` 全绿;Bolt 1 额外 live 端到端(真实 cc+tmux+hook+Discord)。
