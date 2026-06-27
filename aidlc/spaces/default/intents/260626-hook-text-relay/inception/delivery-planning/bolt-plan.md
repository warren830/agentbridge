# Bolt Plan — Hook-Based Real-Time Text Relay

> Bolt = Construction 3.1–3.5 对一个 unit(或依赖小组)的一轮。第一个 Bolt = walking skeleton。基于 `unit-of-work-dependency.md` 的 DAG,delivery 选经济路径。

## Bolt 序列

### Bolt 1(walking skeleton)— 传输打通 + Stop 文字端到端

> 最薄端到端切片:证明 hook → HTTP → 既有 event channel → process_agent_events → Platform 全链路。这是整个架构的"骨架",最高不确定性先验证(传输 + channel 所有权重构)。**始终 gated + 交互**(walking skeleton 规约)。

- **包含 units**:U-1(映射)、U-2(路由表)、U-3(接收端)、U-4(引擎集成,含 U-4a channel 重构)、U-5(poll 门控 + 静默保活)、U-7(hook 脚本,Stop)、U-8(安装器)、U-6(端到端集成验证)。
- **产出**:手机发消息 → 正在跑的 cc → Stop → 手机收到干净文字(非截图)。MVP。
- **关键风险点**:U-4a 的 channel 所有权重构(改 session.rs 构造);U-5 的静默保活(B-2)。
- **walking skeleton gate**:必过人工 gate(即使自主)—— 但用户已授权自主,我会自测后自动通过(自测=U-6 live 端到端 + 长轮 idle 测,遵守自测纪律)。
- **自测验收**(声称完成前 AI 自跑):①cargo test 全绿;②live:真实 cc+tmux+hook,Discord 收到 Stop 文字;③长轮(>300s)不误触 idle;④非桥接 cc 的 hook 被门控丢弃。

### Bolt 2 — PostToolUse 逐工具进度

- **包含 units**:U-9(events.rs per-turn inplace + U-7 PostToolUse 上报)。
- **依赖**:Bolt 1 验证通过。
- **产出**:逐工具实时进度,就地编辑单条 preview(消息条数与 N 解耦)。
- **自测验收**:N=1/5/40 工具轮,新消息条数 ≤ 小常数;claude/acp 行为零回归;hook 模式长轮零 🧠 刷屏、preview 不被打碎(B-2 回归)。

### Bolt 3 — 移除截图路径(cutover 收尾)

- **包含 units**:U-10(删 render_screenshot/poll 截图/render_term.py/Pillow)。
- **依赖**:Bolt 1 验证通过(cutover 规约:文字回路确认可用后才删截图)。
- **产出**:截图代码移除,文字回路独立工作。
- **自测验收**:删后回归 U-6 文字回路;cargo build 无 python/Pillow 引用。

## Walking Skeleton 立场

team.md 的 Walking Skeleton = scope-dependent;feature scope 默认 skeleton-on。Bolt 1 即 walking skeleton,始终 gated。本工作流自主模式下,我自测后自动过该 gate(用户授权)。

## Bolt 间顺序

Bolt 1 → (Bolt 2, Bolt 3 可并行或任意序,均依赖 Bolt 1)。建议 Bolt 2 先(交付 US-3 价值)再 Bolt 3(清理),但 Bolt 3 也可先(尽早去掉截图债)。delivery 建议:**Bolt 2 → Bolt 3**(先补齐功能价值,最后清理),与 scope 批次2→批次3 一致。
