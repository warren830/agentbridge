# Project Rules

## Code Style

- **Language:** Rust + tokio. All I/O on the hot path (engine, platform adapters, agent subprocess) must be async — no `std::fs`, no `std::thread::sleep`. Use `tokio::fs`, `tokio::time`, `tokio::process`.
- **Error handling:** `anyhow::Result` at application boundaries (engine, platform adapters, binaries). `thiserror` for library-level crates / reusable modules that export typed errors. Do not `unwrap()` or `expect()` outside tests and `build.rs`.
- **Logging:** `tracing` crate only — never `println!`/`eprintln!` in library code. Use structured fields (`tracing::info!(session_key = %key, "...")`) over string interpolation.
- **No `unsafe`.** The architecture rewrite explicitly eliminated it; if you think you need it, ask first.
- **Comments:** Write *why*, not *what*. Do not add "matches project X's Y" / "based on Z's approach" cross-references — comments should be self-contained. Chinese comments are fine in `aidlc-docs/`, English in source.

## Testing & Build

- Run `cargo check` after any non-trivial edit. Run `cargo test` before claiming a feature is done.
- **Never use `--no-verify`** to skip pre-commit hooks. If a hook fails, fix the root cause.
- **Never use `cargo test -- --test-threads=1` to paper over flaky tests.** Fix the race.
- Ask before adding a new dependency to `Cargo.toml`. Prefer reusing existing crates (`anyhow`, `tokio`, `tracing`, `serde`, `reqwest`, `axum`) over pulling in new ones.
- For cross-cutting refactors, verify both `cargo check` and `cargo test` pass before declaring done.

## Documentation & Commits

- **Never add `Co-Authored-By` lines to commit messages.** (Global rule.)
- **Use `--no-verify` when running `git push`.** (Global rule — does not apply to `git commit`.)
- Design docs live in `aidlc-docs/YYYY-MM-DD-<slug>/`. Each feature gets its own dated folder with a `design.md` and optionally `build-log.md`.
- Project-wide patterns and anti-patterns go in `aidlc-docs/patterns.md`.
- Commit messages: subject line in the imperative mood ("Fix X", "Add Y"), not past tense. Keep it under 72 chars.

## Architecture

- **Platform adapters** (`src/platforms/{telegram,discord,...}`) interact with the engine exclusively through the `Platform` + `ReplyCtx` capability traits in `src/core/platform.rs`. The engine must never downcast or `if name == "telegram"` branch.
- **No compile-time plugin registration.** No global `init()` side effects, no `inventory`-style auto-registration. Platforms are registered at runtime via a factory map.
- **Agent boundary:** only Claude Code is supported (subprocess `claude --print --output-format stream-json` or ACP). Any new agent goes through the `AgentSession` trait in `src/agent/mod.rs` — do not special-case inside the engine.
- **Session discipline:** the try-lock + queue pattern in `src/core/session.rs` is load-bearing. Do not replace it with a blocking mutex or an unbounded channel without discussing first.
- **Keep `src/engine/mod.rs` focused on routing.** Split out commands (`commands.rs`), events (`events.rs`), skills (`skills.rs`) — do not grow it into a monolith.
