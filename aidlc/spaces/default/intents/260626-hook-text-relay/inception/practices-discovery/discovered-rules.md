# Discovered Rules — agentbridge

> 矫正性、agent-facing。源自 CLAUDE.md 硬约束 + 确认答案。一行一规则。

## Mandated

ALWAYS use async I/O on hot paths (tokio::fs / tokio::process / tokio::time)
ALWAYS use anyhow::Result at application boundaries (engine, platform adapters, binaries)
ALWAYS use thiserror for library-level / reusable modules that export typed errors
ALWAYS use tracing with structured fields for logging
ALWAYS interact with chat platforms only through the Platform / ReplyCtx capability traits
ALWAYS go through the AgentSession trait for any agent backend
ALWAYS run cargo check after a non-trivial edit and cargo test green before claiming a feature done
ALWAYS ask before adding a new Cargo dependency; prefer reusing anyhow/tokio/tracing/serde/reqwest/axum
ALWAYS write inline #[cfg(test)] tests alongside implementation; gate live/external tests behind #[ignore]
ALWAYS use --no-verify on git push (global rule)
ALWAYS use char-boundary-safe string slicing in Rust (never raw byte slicing on potentially-CJK text)
ALWAYS define a throttle/coalesce policy for any per-tool/per-event chat relay before shipping

## Forbidden

NEVER use unsafe
NEVER use std::fs or std::thread::sleep on hot paths
NEVER use unwrap()/expect() outside tests and build.rs
NEVER use println!/eprintln! in library/engine code (main.rs CLI excepted)
NEVER branch on platform name in the engine (no `if name == "telegram"`); the engine must not downcast
NEVER special-case a new agent type inside the engine; go through AgentSession
NEVER replace the session try-lock + bounded-queue pattern with a blocking mutex or unbounded channel without discussing first
NEVER add Co-Authored-By lines to commit messages
NEVER use --no-verify on git commit (fix the hook root cause)
NEVER use cargo test --test-threads=1 to paper over a flaky test (fix the race)
NEVER add cross-project reference comments ("matches project X's Y")
