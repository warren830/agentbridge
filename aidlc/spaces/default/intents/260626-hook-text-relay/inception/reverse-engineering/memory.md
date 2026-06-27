<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is maintained by the orchestrator during stage execution. Add observations at the gate ritual, not by editing here directly.

## Interpretations
- 2026-06-26T15:40:00Z — 用户选择"完整跑"全库扫描而非精简,理由是 codekb 是 space 级、跨 intent 复用。单 repo(workspace root 即唯一 repo),codekb 写入 aidlc/spaces/default/codekb/agentbridge/(engine 解析,未手拼路径)。freshness commit = 25c8c3a。

## Deviations

## Tradeoffs

## Open questions
- 2026-06-26T15:45:00Z — 扫描发现:没有中央 platform factory map(CLAUDE.md 描述的设计未落地),实际是 engine::create_platform_capabilities 里硬编码 match("telegram"/"discord")。hook HTTP 接收端必须经事件管线(event_broadcast + capability traits)注入,绝不碰 platform 注册。application-design 据此设计。
- 2026-06-26T15:45:00Z — engine/events.rs 已有我们正需要的 dispatch:Text→MessageUpdater preview(节流)、ToolUse→reply("⚡ tool › …")、Result→finalize。hook 接收端只需把 payload 映射成 AgentEvent::Text / ToolUse 投入同一管线,映射工作量比预想小。Stop 的 last_assistant_message→Result/Text;PostToolUse→ToolUse(经节流)。
- 2026-06-26T15:45:00Z — StreamPreview 的 display_text 按字节切片(&text[len-MAX..]),CJK 多字节尾部可能 panic-slice。hook 文字成为新主路径后,长 CJK 回复必经此路 → 必须修成 char-boundary 安全切片。functional/code 阶段处理。
- 2026-06-26T15:45:00Z — build_sender_prompt 已被移除(未提交改动),prompt 现逐字发送。与本 feature 的 send-keys 输入路径"不改"一致。
