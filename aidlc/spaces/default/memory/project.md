# Project-Level Rules

> Project-specific overrides and corrections. Overrides aidlc-team.md
> and aidlc-org.md. Populated by practices-discovery and the
> self-learning loop.
>
> Use sparingly: most teams don't need a project layer. Reach for it
> only when this specific project deviates from team-wide practice in a
> stable, durable way (e.g., "this monorepo project rebases even though
> our team default is squash"; "this legacy project skips the test
> floor because the existing suite is unsalvageable and we accept
> that").

## Way of Working

<!-- Project-specific override. Example: -->
<!-- This monorepo project rebases instead of squash-merging because -->
<!-- the per-package commit history is the audit trail we depend on -->
<!-- for partial-rollback decisions. Override applies to this project -->
<!-- only. -->

## Walking Skeleton

<!-- Project-specific override. Example: -->
<!-- This project skips the walking skeleton because we're rewriting -->
<!-- an existing service in-place — there's no greenfield bootstrap -->
<!-- to gate. -->

## Testing Posture

<!-- Project-specific override. -->

## Deployment

<!-- Project-specific override. -->

## Code Style

<!-- Project-specific override. -->

## Tech Stack

<!-- Technology choices locked for this project. -->

## Decided

<!-- Decisions made in earlier stages that should not be re-asked. -->
<!-- Format: DECIDED: [decision] (Stage [slug], [date]) -->

## Scope Overrides

<!-- Custom scope rules for this project. -->

## Forbidden

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: NEVER [behavior] (affirmed [date]) -->
<!-- Example: NEVER throw exceptions across service layer boundaries (affirmed 2026-05-17) -->

NEVER use unsafe (affirmed 2026-06-26)
NEVER use std::fs or std::thread::sleep on hot paths (affirmed 2026-06-26)
NEVER use unwrap()/expect() outside tests and build.rs (affirmed 2026-06-26)
NEVER use println!/eprintln! in library/engine code (main.rs CLI excepted) (affirmed 2026-06-26)
NEVER branch on platform name in the engine (no `if name == "telegram"`); the engine must not downcast (affirmed 2026-06-26)
NEVER special-case a new agent type inside the engine; go through AgentSession (affirmed 2026-06-26)
NEVER replace the session try-lock + bounded-queue pattern with a blocking mutex or unbounded channel without discussing first (affirmed 2026-06-26)
NEVER add Co-Authored-By lines to commit messages (affirmed 2026-06-26)
NEVER use --no-verify on git commit (fix the hook root cause) (affirmed 2026-06-26)
NEVER use cargo test --test-threads=1 to paper over a flaky test (fix the race) (affirmed 2026-06-26)
NEVER add cross-project reference comments ("matches project X's Y") (affirmed 2026-06-26)
## Mandated

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: ALWAYS [behavior] (affirmed [date]) -->
<!-- Example: ALWAYS use Result<T,E> for fallible operations in service layer (affirmed 2026-05-17) -->

ALWAYS use async I/O on hot paths (tokio::fs / tokio::process / tokio::time) (affirmed 2026-06-26)
ALWAYS use anyhow::Result at application boundaries (engine, platform adapters, binaries) (affirmed 2026-06-26)
ALWAYS use thiserror for library-level / reusable modules that export typed errors (affirmed 2026-06-26)
ALWAYS use tracing with structured fields for logging (affirmed 2026-06-26)
ALWAYS interact with chat platforms only through the Platform / ReplyCtx capability traits (affirmed 2026-06-26)
ALWAYS go through the AgentSession trait for any agent backend (affirmed 2026-06-26)
ALWAYS run cargo check after a non-trivial edit and cargo test green before claiming a feature done (affirmed 2026-06-26)
ALWAYS ask before adding a new Cargo dependency; prefer reusing anyhow/tokio/tracing/serde/reqwest/axum (affirmed 2026-06-26)
ALWAYS write inline #[cfg(test)] tests alongside implementation; gate live/external tests behind #[ignore] (affirmed 2026-06-26)
ALWAYS use --no-verify on git push (global rule) (affirmed 2026-06-26)
ALWAYS use char-boundary-safe string slicing in Rust (never raw byte slicing on potentially-CJK text) (affirmed 2026-06-26)
ALWAYS define a throttle/coalesce policy for any per-tool/per-event chat relay before shipping (affirmed 2026-06-26)
## Corrections

<!-- Project-specific corrections from human feedback. -->
<!-- Format: NEVER/ALWAYS [behavior] (learned [date]) -->
- Any per-tool / per-event relay to a chat channel (e.g. PostToolUse progress) must define a throttle or coalesce policy before shipping — unbounded per-event sends re-introduce the screenshot-era chattiness the relay is meant to kill (learned 2026-06-26) <!-- cid:intent-capture:ic-throttle -->
- Before claiming any relay/bridge work complete, run the full chain end-to-end yourself (hook to local HTTP to platform) and verify the live result — do not use the user as a debugging harness (learned 2026-06-26) <!-- cid:intent-capture:ic-selftest -->
- For replace/cutover features (swapping one mechanism for another), schedule the removal of the OLD mechanism as the LAST batch — keep both paths alive (temporary dual-track) until the new path is verified working, so there is never a window with no usable path (learned 2026-06-26) <!-- cid:scope-definition:sd-cutover -->
- When truncating a string tail or taking a substring in Rust, use char-boundary-safe methods (char_indices, floor_char_boundary, or chars().rev().take()) — never raw byte slicing like &s[s.len()-N..], which panics on multibyte (CJK) content. agentbridge handles CJK chat text, so this is load-bearing (learned 2026-06-26) <!-- cid:reverse-engineering:re-charboundary -->
- When designing against existing Rust code, read the OWNERSHIP and LIFECYCLE semantics (who holds a channel sender/receiver, is a function per-turn or long-lived, what do the comments say about why), not just whether a type exists. Two architecture-review rounds both root-caused to assuming a clonable sender that was deliberately moved away, and assuming a per-turn function was a daemon consumer (learned 2026-06-26) <!-- cid:application-design:ad-ownership -->
- Reusing an existing AgentEvent variant inherits its FULL existing behavior in the event dispatcher, not just the semantics you want. E.g. AgentEvent::Thinking is rendered as a visible message AND triggers freeze_and_detach_preview — it cannot double as a silent keepalive. Before reusing a type/variant for a new purpose, check every branch that already handles it in events.rs (learned 2026-06-26) <!-- cid:units-generation:ug-reuse-behavior -->
