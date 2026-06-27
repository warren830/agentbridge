//! Hook route registry: maps a bridged session to the sender half of its agent
//! event channel, keyed by two independent identifiers.
//!
//! The hook receiver looks up an inbound hook payload here to find the bridged
//! session it belongs to. A hook that matches nothing is dropped — this is the
//! gating that stops non-bridged Claude Code instances from leaking messages
//! into chat (BR-5).
//!
//! Two keys, because an *attached* tmux session commonly runs in a directory
//! unrelated to agentbridge's configured work_dir, so cwd can never be matched
//! reliably (proven live; ADR-6 m-2):
//!   1. **tmux session name** — the reliable key for the tmux backend. The hook
//!      script discovers it (`tmux display-message`) and sends it as
//!      `tmux_session`; the bind side knows it from config / the derived name.
//!      Matched first, exact.
//!   2. **work_dir / cwd prefix** — the fallback for cases with no tmux session
//!      name (and the original keying). Prefix match so a subdirectory routes to
//!      its enclosing binding.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::core::event::AgentEvent;

/// Canonicalize a path string for stable keying / prefix matching.
///
/// Equivalent paths (relative, symlinked, trailing slash) must map to the same
/// key so a session bound under one spelling resolves a hook reported under
/// another. Falls back to the raw string when the path cannot be resolved (e.g.
/// the directory no longer exists), matching `SessionManager::new`.
fn normalize(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// Maps a bridged session to its event sender by two keys: tmux session name
/// (exact, preferred) and canonicalized work_dir (prefix, fallback).
///
/// A `std::sync::Mutex` is used because every access is a short, non-awaiting
/// map operation; the lock is never held across an `.await`.
#[derive(Default)]
pub struct HookRouteRegistry {
    by_work_dir: Arc<Mutex<HashMap<String, mpsc::Sender<AgentEvent>>>>,
    by_tmux_session: Arc<Mutex<HashMap<String, mpsc::Sender<AgentEvent>>>>,
}

impl HookRouteRegistry {
    pub fn new() -> Self {
        Self {
            by_work_dir: Arc::new(Mutex::new(HashMap::new())),
            by_tmux_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Bind a session's event sender under both its work_dir (canonicalized for
    /// prefix matching) and, when known, its tmux session name (exact). The
    /// tmux name is the reliable key for an attached session whose cwd differs
    /// from the configured work_dir.
    pub fn bind(&self, work_dir: &str, tmux_session: Option<&str>, tx: mpsc::Sender<AgentEvent>) {
        let key = normalize(work_dir);
        // A poisoned lock here only means a prior holder panicked while the map
        // was momentarily inconsistent; recovering the guard is safe for a plain
        // HashMap and lets the bridge keep working rather than aborting.
        {
            let mut map = self.by_work_dir.lock().unwrap_or_else(|e| e.into_inner());
            tracing::info!(work_dir = %key, "hook route bound (work_dir)");
            map.insert(key, tx.clone());
        }
        if let Some(sess) = tmux_session.map(str::trim).filter(|s| !s.is_empty()) {
            let mut map = self.by_tmux_session.lock().unwrap_or_else(|e| e.into_inner());
            tracing::info!(tmux_session = %sess, "hook route bound (tmux session)");
            map.insert(sess.to_string(), tx);
        }
    }

    /// Remove both bindings for a session (called when it is torn down).
    pub fn unbind(&self, work_dir: &str, tmux_session: Option<&str>) {
        let key = normalize(work_dir);
        {
            let mut map = self.by_work_dir.lock().unwrap_or_else(|e| e.into_inner());
            if map.remove(&key).is_some() {
                tracing::debug!(work_dir = %key, "hook route unbound (work_dir)");
            }
        }
        if let Some(sess) = tmux_session.map(str::trim).filter(|s| !s.is_empty()) {
            let mut map = self.by_tmux_session.lock().unwrap_or_else(|e| e.into_inner());
            if map.remove(sess).is_some() {
                tracing::debug!(tmux_session = %sess, "hook route unbound (tmux session)");
            }
        }
    }

    /// Resolve an inbound hook to a bound session's event sender.
    ///
    /// Tries the tmux session name first (exact match — the reliable key for an
    /// attached session), then falls back to cwd prefix matching: a bound
    /// work_dir matches when it is a path-component prefix of the canonicalized
    /// `cwd`, so a cc running in a subdirectory still routes correctly. Returns
    /// a clone of the sender on a hit, `None` otherwise.
    pub fn resolve(
        &self,
        tmux_session: Option<&str>,
        cwd: Option<&str>,
    ) -> Option<mpsc::Sender<AgentEvent>> {
        // 1. tmux session name — exact, the dependable key.
        if let Some(sess) = tmux_session.map(str::trim).filter(|s| !s.is_empty()) {
            let map = self.by_tmux_session.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(tx) = map.get(sess) {
                return Some(tx.clone());
            }
        }
        // 2. cwd prefix — fallback.
        let cwd = normalize(cwd?.trim());
        if cwd.is_empty() {
            return None;
        }
        let map = self.by_work_dir.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = map.get(&cwd) {
            return Some(tx.clone());
        }
        let mut best: Option<(&String, &mpsc::Sender<AgentEvent>)> = None;
        for (dir, tx) in map.iter() {
            if is_path_prefix(dir, &cwd) {
                match best {
                    Some((b, _)) if b.len() >= dir.len() => {}
                    _ => best = Some((dir, tx)),
                }
            }
        }
        best.map(|(_, tx)| tx.clone())
    }
}

/// True when `prefix` is a path-component prefix of `path`. Component-aware so
/// `/a/foo` is not treated as a prefix of `/a/foobar`.
fn is_path_prefix(prefix: &str, path: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_end_matches('/');
    if prefix == path {
        return true;
    }
    match path.strip_prefix(prefix) {
        Some(rest) => rest.starts_with('/'),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drainable() -> (mpsc::Sender<AgentEvent>, mpsc::Receiver<AgentEvent>) {
        mpsc::channel(8)
    }

    #[test]
    fn bind_then_resolve_exact() {
        let reg = HookRouteRegistry::new();
        // Use a directory that exists so canonicalize succeeds identically for
        // bind and resolve; /tmp is present on every supported platform.
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy().to_string();
        let (tx, _rx) = drainable();
        reg.bind(&dir, None, tx);
        assert!(
            reg.resolve(None, Some(&dir)).is_some(),
            "exact cwd match must resolve"
        );
    }

    #[test]
    fn resolve_by_tmux_session_name() {
        // The real-world fix: cc runs in a directory unrelated to the bound
        // work_dir, but the tmux session name matches exactly.
        let reg = HookRouteRegistry::new();
        let (tx, _rx) = drainable();
        reg.bind("/Users/me/warren_ws", Some("nova-bidding"), tx);
        // cwd is a totally different tree (the cc's real cwd) — cwd alone misses.
        assert!(
            reg.resolve(None, Some("/Users/me/Documents/Novastar/bidding"))
                .is_none(),
            "unrelated cwd must not match by cwd"
        );
        // ...but the tmux session name routes it correctly.
        assert!(
            reg.resolve(Some("nova-bidding"), Some("/Users/me/Documents/Novastar/bidding"))
                .is_some(),
            "tmux session name must resolve regardless of cwd"
        );
    }

    #[test]
    fn resolve_subdir_prefix_match() {
        let reg = HookRouteRegistry::new();
        // Canonicalize a real base, then append a (non-existent) subdir. The
        // subdir won't canonicalize, so it falls back to the raw concatenation —
        // which still shares the canonicalized base as a path-component prefix.
        let base = std::env::temp_dir().canonicalize().unwrap();
        let base_str = base.to_string_lossy().to_string();
        let sub = base.join("ab-hook-subdir-xyz/inner");
        let sub_str = sub.to_string_lossy().to_string();
        let (tx, _rx) = drainable();
        reg.bind(&base_str, None, tx);
        assert!(
            reg.resolve(None, Some(&sub_str)).is_some(),
            "a cwd inside the bound dir must resolve via prefix match"
        );
    }

    #[test]
    fn resolve_miss_returns_none() {
        let reg = HookRouteRegistry::new();
        let (tx, _rx) = drainable();
        reg.bind("/some/bound/dir", Some("some-session"), tx);
        assert!(reg
            .resolve(Some("other-session"), Some("/a/totally/different/dir"))
            .is_none());
        assert!(reg.resolve(None, None).is_none());
        assert!(reg.resolve(Some(""), Some("")).is_none());
    }

    #[test]
    fn unbind_then_resolve_none() {
        let reg = HookRouteRegistry::new();
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        let (tx, _rx) = drainable();
        reg.bind(&dir, Some("sess-x"), tx);
        assert!(reg.resolve(Some("sess-x"), None).is_some());
        reg.unbind(&dir, Some("sess-x"));
        assert!(
            reg.resolve(Some("sess-x"), Some(&dir)).is_none(),
            "unbound session must miss on both keys"
        );
    }

    #[test]
    fn sibling_dir_is_not_a_prefix() {
        // /a/foo must NOT match /a/foobar — guard against substring prefixing.
        assert!(is_path_prefix("/a/foo", "/a/foo/bar"));
        assert!(is_path_prefix("/a/foo", "/a/foo"));
        assert!(!is_path_prefix("/a/foo", "/a/foobar"));
        assert!(!is_path_prefix("/a/foo", "/a"));
    }

    #[tokio::test]
    async fn resolved_sender_delivers_to_bound_receiver() {
        let reg = HookRouteRegistry::new();
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        let (tx, mut rx) = drainable();
        reg.bind(&dir, None, tx);
        let resolved = reg.resolve(None, Some(&dir)).expect("resolve hit");
        resolved
            .send(AgentEvent::Text { content: "hi".into() })
            .await
            .expect("send to bound channel");
        match rx.recv().await {
            Some(AgentEvent::Text { content }) => assert_eq!(content, "hi"),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
