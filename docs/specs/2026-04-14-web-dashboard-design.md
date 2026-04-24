# AgentPush Web Dashboard Design

**Date:** 2026-04-14
**Status:** Approved
**Complexity:** Heavy

---

## Overview

A Web dashboard for managing multiple agentbridge instances. Team members can view all sessions across instances, monitor agent activity in real-time, and inject messages into any session — with replies continuing through the original platform (Discord/Telegram).

## Architecture

```
┌──────────────┐     HTTPS/WSS      ┌─────────────────┐     WSS (reverse)   ┌──────────────┐
│  Nuxt 前端   │ ◄──────────────────► │  agentbridge      │ ◄────────────────► │  agentbridge   │
│  (browser)   │   REST + WebSocket  │  gateway mode   │   instance connects │  instance A  │
└──────────────┘                     │                 │ ◄────────────────► ├──────────────┤
                                     │  - auth (token) │                    │  instance B  │
                                     │  - route/relay  │ ◄────────────────► ├──────────────┤
                                     │  - static files │                    │  instance C  │
                                     └─────────────────┘                    └──────────────┘
```

### Three modes, one binary

| Command | Role |
|---------|------|
| `agentbridge run` | Normal bridge (existing) |
| `agentbridge run --gateway wss://gw:9900 --gateway-token xxx` | Bridge + register to gateway |
| `agentbridge gateway --port 9900 --token xxx` | Gateway mode |

### Monorepo structure

```
agentbridge/
├── src/              # Rust: agentbridge + gateway
├── Cargo.toml
├── web/              # Nuxt frontend
│   ├── package.json
│   ├── nuxt.config.ts
│   ├── pages/
│   ├── components/
│   └── layouts/
└── ...
```

- Dev: `cargo run -- gateway` + `cd web && npm run dev` (separate hot reload)
- Deploy: `npm run build` → `web/.output/public/`, gateway serves static files

## Communication Protocol

### Instance → Gateway (reverse WebSocket)

Instance initiates WebSocket connection to gateway on startup.

```jsonc
// Registration (instance → gateway, on connect)
{
  "type": "register",
  "instance_id": "warren-macbook",
  "instance_name": "Warren's MacBook",
  "projects": [
    {
      "name": "agentbridge",
      "work_dir": "/home/warren/agentbridge",
      "sessions": [
        {
          "session_key": "discord:1234567",
          "session_id": "abc-123",
          "name": "Fix UTF-8 bug",
          "agent_session_id": "claude-xxx",
          "updated_at": "2026-04-14T10:00:00Z",
          "is_busy": true
        }
      ]
    }
  ]
}

// Real-time event relay (instance → gateway)
{
  "type": "event",
  "instance_id": "warren-macbook",
  "session_key": "discord:1234567",
  "event": {
    "type": "text",        // text | thinking | tool_use | tool_result | result | error | permission_request
    "content": "Let me check..."
  }
}

// Session state update (instance → gateway)
{
  "type": "session_update",
  "instance_id": "warren-macbook",
  "sessions": [...]  // full session list refresh
}

// Heartbeat (bidirectional, every 30s)
{ "type": "ping" }
{ "type": "pong" }
```

### Gateway → Instance (commands)

```jsonc
// Send message to a session
{
  "type": "send_message",
  "session_key": "discord:1234567",
  "text": "check the logs",
  "from": "web:warren"
}

// Execute command
{
  "type": "command",
  "session_key": "discord:1234567",
  "command": "stop"  // stop | new | dir | compress | name | ...
}

// Permission response (from web UI)
{
  "type": "permission_response",
  "session_key": "discord:1234567",
  "request_id": "perm-123",
  "decision": "allow"  // allow | deny | allow_all
}
```

### Frontend → Gateway (REST + WebSocket)

**REST API:**

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/instances` | List all connected instances with projects/sessions |
| POST | `/api/instances/:id/send` | Send message to session `{ session_key, text }` |
| POST | `/api/instances/:id/command` | Execute command `{ session_key, command, args }` |
| POST | `/api/instances/:id/permission` | Respond to permission `{ session_key, request_id, decision }` |

**WebSocket `/api/ws`:**

```jsonc
// Subscribe to session events (frontend → gateway)
{ "type": "subscribe", "instance_id": "warren-macbook", "session_key": "discord:1234567" }
{ "type": "unsubscribe", "instance_id": "warren-macbook", "session_key": "discord:1234567" }

// Receive events (gateway → frontend)
{
  "type": "event",
  "instance_id": "warren-macbook",
  "session_key": "discord:1234567",
  "event": { "type": "text", "content": "..." }
}

// Instance connect/disconnect notifications
{ "type": "instance_online", "instance_id": "warren-macbook", ... }
{ "type": "instance_offline", "instance_id": "warren-macbook" }

// Session state changes
{ "type": "session_update", "instance_id": "warren-macbook", "sessions": [...] }
```

## Frontend Design

### Layout

```
┌─────────────────────────────────────────────────────────┐
│  AgentPush                                   [warren] 🔓 │
├───────────────┬─────────────────────────────────────────┤
│               │  📁 agentbridge › discord:thread-123       │
│ ▼ 🖥 macbook  │  ───────────────────────────────────     │
│   ▼ agentbridge │  🧑 修复UTF-8 bug                       │
│    ▶ thread-1 │  🤖 好的，让我看看源码...                │
│    ▶ thread-2 │     ⚡ Read › src/discord/mod.rs        │
│   ▼ opsagent  │     ⚡ Edit › src/discord/mod.rs        │
│    ▶ deploy   │  🤖 已修复，改了3处字符串截断...         │
│               │     [tokens: in=4521 out=892]            │
│ ▼ 🖥 prod     │                                          │
│   ▼ api-svc   │  ┌────────────────────────────────────┐ │
│    ▶ monitor  │  │ 输入消息...                [发送]  │ │
│               │  └────────────────────────────────────┘ │
│               │  [/stop] [/new] [/dir] [/compress]       │
└───────────────┴─────────────────────────────────────────┘
```

### Pages

| Route | Description |
|-------|-------------|
| `/login` | Token input |
| `/` | Main dashboard (left sidebar + chat panel) |

### Key interactions

- **Click session** → Subscribe to its event stream, show message history
- **Send message** → POST to gateway → relay to instance → inject into engine
- **Reply appears** → On original platform (Discord/Telegram) AND mirrored to Web via event stream
- **Permission request** → Show Allow/Deny/Allow All buttons in Web chat
- **Command buttons** → /stop /new /dir /compress as toolbar shortcuts
- **Instance offline** → Grey out in sidebar, show "offline" badge

### State management

- Pinia store for instances/sessions/events
- WebSocket connection with auto-reconnect
- Event buffer per session (last 100 events in memory)
- Active subscriptions tracked to minimize gateway load

## Authentication

- Gateway config: `api_token: "your-secret-token"`
- Frontend login: user enters token, stored in localStorage
- All REST: `Authorization: Bearer <token>` header
- WebSocket: token sent in first message after connect
- Instance registration: `gateway_token` in instance config must match

## Data Flow: User sends message from Web

```
1. User types in Web chat → POST /api/instances/macbook/send
2. Gateway authenticates → finds instance "macbook" WebSocket
3. Gateway sends { type: "send_message", session_key, text } to instance
4. Instance engine receives → injects as synthetic message (like cron)
5. Engine processes: dedup → access → session lock → agent dispatch
6. Agent responds → events flow: instance → gateway → Web (mirror)
7. Agent response also sent to Discord (original platform reply)
8. Web shows the mirrored response in real-time
```

## Implementation Phases

### V1: Minimum Viable Dashboard
- [ ] Gateway subcommand (axum WebSocket server)
- [ ] Instance registration protocol (reverse WebSocket)
- [ ] REST API (instances, send, command)
- [ ] WebSocket event relay (subscribe/unsubscribe)
- [ ] Token auth
- [ ] Nuxt frontend: login, sidebar tree, chat panel, send message
- [ ] Instance side: `--gateway` flag, connect + register + relay events
- [ ] Static file serving (gateway serves Nuxt build output)

### V2: Enhanced Interaction
- [ ] Permission buttons in Web (Allow/Deny/Allow All)
- [ ] Command toolbar (/stop /dir /new /compress)
- [ ] Session search and filter
- [ ] Instance offline detection + auto-reconnect
- [ ] Typing indicator in Web
- [ ] Stream preview (live text updates)

### V3: Team Features
- [ ] Multi-user auth (username + visible scope per user)
- [ ] File browser (view agent work directory)
- [ ] Log viewer
- [ ] Monitoring dashboard (token usage, response times)
- [ ] Mobile responsive layout

## Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Gateway tech | Rust (axum + tokio-tungstenite) | Same binary, shared types |
| Frontend | Nuxt 3 | User preference, SSR optional |
| Connection direction | Instance → Gateway (reverse) | NAT/firewall friendly |
| Auth | Token-based (V1) | Simple, sufficient for small team |
| Session ownership | Web mirrors existing platform sessions | No reply hijacking |
| Reply routing | Original platform (Discord/Telegram) | Web is transparent proxy |
| Repo structure | Monorepo | One repo, coordinated releases |
