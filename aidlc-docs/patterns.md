# Project Patterns
Last updated: 2026-07-05

## Conventions
- Language: Rust, async with tokio
- Config format: YAML (~/.agentbridge/config.yaml)
- Platform adapters: each in src/platforms/{name}/mod.rs, implements Platform trait
- Agent: only Claude Code, spawns `claude --print --output-format stream-json`
- Session persistence: JSON files in ~/.agentbridge/sessions/{project}/
- Error handling: anyhow for app-level, thiserror for library-level
- Logging: tracing crate with env-filter

## Anti-Patterns
- Do NOT use compile-time plugin registration (global init() side effects). Use runtime dynamic dispatch.
- Do NOT put all engine logic in one file. Keep engine.rs focused on routing, split commands/streaming/etc.
- Do NOT block the tokio runtime with synchronous I/O. Use tokio::fs for file operations in hot paths.
- Do NOT hold two SessionManager std mutexes at once except in the global order
  `sessions → active_keys → session_keys` — the reverse order deadlocked the whole
  bridge (AB-BA, reproduced by `no_deadlock_between_getters_and_persist`). Prefer
  clone-then-release over holding two locks at all. (2026-07-05)
- Do NOT hold the global `interactive_states` tokio mutex across sleeps, agent
  spawns, subprocess sends, or platform network calls — it gates message intake
  for EVERY session on the bridge. (2026-07-05)
- Do NOT leave any `.await` unbounded on the turn path: the per-session busy lock
  is only released when `process_and_drain` returns, so a parked await strands the
  lock forever (no panic, no log — just a dead channel). Every subprocess call and
  every platform HTTP call on that path needs a timeout. (2026-07-01)
- Do NOT shadow a session binding with a block-scoped `let session = ...` when the
  intent is to REBIND for later use — the idle-reset shadow bug left a freshly
  locked session unreachable and the channel permanently wedged. (2026-07-05)
- Do NOT treat `broadcast::RecvError::Lagged` as loop-terminal (`while let Ok`):
  it is recoverable; treating it as EOF silently kills the consumer forever. (2026-07-05)
- Do NOT construct `reqwest::Client::new()` for anything on a message path —
  reqwest has NO default timeout. Use a builder with `.timeout` + `.connect_timeout`,
  or per-request `.timeout()` when the client is shared with a long-poll. (2026-07-05)

## Stack Decisions
- HTTP framework for Management API: axum (decided 2026-04-12)
- Distribution: single binary via cargo + npm wrapper (decided 2026-04-12)
- No SQLite yet - JSON files for v1 persistence (revisit at Phase 4)
