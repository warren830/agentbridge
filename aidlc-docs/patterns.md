# Project Patterns
Last updated: 2026-04-12

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

## Stack Decisions
- HTTP framework for Management API: axum (decided 2026-04-12)
- Distribution: single binary via cargo + npm wrapper (decided 2026-04-12)
- No SQLite yet - JSON files for v1 persistence (revisit at Phase 4)
