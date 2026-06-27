<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is maintained by the orchestrator during stage execution. Add observations at the gate ritual, not by editing here directly.

## Interpretations
- 2026-06-26T00:00:00Z — Pre-filled all 7 questions with proposed answers drawn from the prior conversation rather than interrogating from a blank slate; the problem, verified hook facts (Stop carries last_assistant_message, PostToolUse carries tool_name/tool_response — personally triggered and captured), and architecture were already established this session. User confirmed all as A. Honors the AI-DLC ritual without re-deriving settled facts.

## Deviations

## Tradeoffs
- 2026-06-26T00:00:00Z — Chose hook-based text relay over (a) keeping screenshots, (b) SDK/print rewrite. Hooks keep "same running cc" (tmux still drives input) AND give clean structured text; SDK rewrite would sacrifice the same-cc property the user cares most about. remote-claude-control proves the Stop-hook path in production.

## Open questions
- 2026-06-26T00:00:00Z — PostToolUse per-tool progress could re-introduce the original "截图发太勤快" chattiness on tool-heavy turns; design stage must decide a throttling/coalescing policy (e.g. collapse consecutive tool events, or only surface long-running tools). Carry into NFR/functional design.
- 2026-06-26T00:00:00Z — Transport contract: every hook payload carries session_id + cwd, but agentbridge must map that to the correct platform channel (which Discord/Feishu chat to send to). Need a registry/binding (session_id or cwd → channel) — parallels the existing per-channel tmux session derivation. Resolve in application design.
- 2026-06-26T00:00:00Z — Gating mechanism for the global hook: cwd-based vs tmux-session-based vs a marker file agentbridge writes. Affects how a non-bridged local cc stays silent. Resolve in application design.
