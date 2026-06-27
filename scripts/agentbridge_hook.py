#!/usr/bin/env python3
"""agentbridge Claude Code hook relay.

Reads a Claude Code hook payload from stdin and POSTs it to the agentbridge
hook receiver running on localhost. Registered as a Stop / PostToolUse hook by
`agentbridge hook-install`.

Hard rule: this must NEVER block or fail Claude Code. Every error path exits 0
with no output, and the POST uses a short timeout. The receiver always answers
200, so there is nothing to retry. Stdlib only — no third-party imports.

Port resolution: argv[1] if given, else $AGENTBRIDGE_HOOK_PORT, else 9123.

The receiver routes a hook to the right chat channel by tmux session name, not
by cwd: an attached cc commonly runs in a directory unrelated to agentbridge's
configured work_dir, so cwd can never be matched reliably. Claude Code does not
know its tmux session, but this script — running as a descendant of the cc
process inside the pane — can ask tmux, and injects it as `tmux_session`.
"""

import json
import os
import subprocess
import sys
import urllib.request


def _tmux_session() -> str:
    # Resolve the tmux session this cc runs in. $TMUX being set means we're
    # inside tmux; `display-message -p '#S'` then resolves the session via the
    # attached client. Best-effort: any failure yields "" and the receiver
    # falls back to cwd matching.
    if not os.environ.get("TMUX", "").strip():
        return ""
    try:
        out = subprocess.run(
            ["tmux", "display-message", "-p", "#S"],
            capture_output=True, text=True, timeout=2,
        )
        return out.stdout.strip()
    except Exception:
        return ""


def main() -> None:
    try:
        port = 9123
        if len(sys.argv) > 1 and sys.argv[1].strip():
            port = int(sys.argv[1].strip())
        elif os.environ.get("AGENTBRIDGE_HOOK_PORT", "").strip():
            port = int(os.environ["AGENTBRIDGE_HOOK_PORT"].strip())

        raw = sys.stdin.read()
        if not raw.strip():
            return
        # Parse, augment with the discovered tmux session, re-serialize. The
        # receiver prefers tmux_session for routing and falls back to cwd.
        payload = json.loads(raw)
        if isinstance(payload, dict):
            sess = _tmux_session()
            if sess:
                payload["tmux_session"] = sess
        body = json.dumps(payload).encode("utf-8")

        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/hook-event",
            data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        # Short timeout: a slow/absent receiver must not stall the agent.
        urllib.request.urlopen(req, timeout=2).read()
    except Exception:
        # Any failure (no receiver, bad JSON, network, timeout) is swallowed —
        # the hook is best-effort and must never break the Claude Code turn.
        pass
    finally:
        sys.exit(0)


if __name__ == "__main__":
    main()
