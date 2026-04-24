# Discord Setup Guide

This guide walks you through connecting **agentbridge** to Discord, so you can chat with your local Claude Code or Kiro CLI via a Discord bot.

## Prerequisites

- A Discord account
- A machine that can run agentbridge (no public IP needed)
- Claude Code **or** Kiro CLI installed and configured

> 💡 **Advantage**: Uses Gateway (WebSocket) — no public IP, no domain, no reverse proxy needed.

---

## Step 1: Create a Discord Application

### 1.1 Open the Developer Portal

Go to [Discord Developer Portal](https://discord.com/developers/applications) and sign in.

### 1.2 Create a New Application

1. Click "New Application" (top right)
2. Enter an application name (e.g. `agentbridge`)
3. Agree to the Terms of Service
4. Click "Create"

---

## Step 2: Create a Bot User

### 2.1 Go to Bot Settings

In the left sidebar, click "Bot".

### 2.2 Add a Bot

1. Click "Add Bot"
2. Confirm the action

### 2.3 Configure Bot Info

| Field | Suggested Value |
|-------|----------------|
| Username | `agentbridge` |
| Avatar | Upload an icon you like |

---

## Step 3: Get the Bot Token

### 3.1 Generate Token

On the Bot page:

1. Click "Reset Token"
2. Enter a 2FA code if prompted
3. Click "Copy"

> ⚠️ The token is only shown once — save it immediately! Format: `MTk4NjIyNDgzNDcOTY3NDUxMg.G8vKqh.xxx...`

### 3.2 Lost Your Token?

Click "Reset Token" again. The old token is invalidated immediately.

---

## Step 4: Configure Privileged Intents (Important!)

### 4.1 What Are Intents?

Intents control which events your bot can receive from Discord's Gateway.

### 4.2 Enable Required Intents

On the Bot page, under "Privileged Gateway Intents", enable:

| Intent | Purpose | Required? |
|--------|---------|-----------|
| **Message Content Intent** | Read message content | ✅ **Required** |
| Server Members Intent | Read server members | Optional |
| Presence Intent | Read user status | Optional |

> ⚠️ **Message Content Intent is mandatory** — without it, the bot will connect but receive empty message bodies.

### 4.3 Save Changes

Click "Save Changes" at the bottom.

---

## Step 5: Configure agentbridge

Edit `~/.agentbridge/config.yaml` (create it with `agentbridge init` if it doesn't exist yet):

### 5.1 Minimal config (Claude Code only)

```yaml
language: en
projects:
  - name: my-project
    work_dir: /path/to/your/project
    agent:
      mode: yolo
      max_turns: 30
    platforms:
      - type: discord
        options:
          token: "MTk4NjIyNDgzNDcOTY3NDUxMg.G8vKqh.xxx..."
          group_reply_all: true        # reply to every message in the guild
          thread_isolation: true       # new Discord thread per session
          guild_id: "1234567890"       # restrict to one server (optional)
    allow_from: "*"
```

### 5.2 Multi-agent config (Claude + Kiro)

```yaml
language: en
projects:
  - name: my-project
    work_dir: /path/to/your/project
    agents:
      - name: claude
        backend: claude
        mode: yolo
        max_turns: 30
      - name: kiro
        backend: acp
        acp:
          command: kiro-cli
          args: ["acp"]
          display_name: "Kiro"
    default_agent: kiro
    platforms:
      - type: discord
        options:
          token: "MTk4NjIyNDgzNDcOTY3NDUxMg.G8vKqh.xxx..."
          group_reply_all: true
          thread_isolation: true
          guild_id: "1234567890"
    allow_from: "*"
```

Use `/agent` in chat to list or switch agents at runtime.

### 5.3 Discord platform options

| Option | Default | Meaning |
|--------|---------|---------|
| `token` | (required) | Bot token from the Developer Portal |
| `guild_id` | unset | Restrict to messages from this guild (server). Useful if the bot is in multiple servers. |
| `group_reply_all` | `false` | When `true`, the bot replies to every message in the guild (no @mention needed). When `false`, bot only replies when @mentioned. |
| `thread_isolation` | `false` | When `true`, each agent session runs in its own Discord thread. Recommended. |
| `allow_from` | `"*"` | (Project-level) Comma-separated Discord user IDs, or `"*"` for anyone. |

> agentbridge declares the Intents it needs (MESSAGE_CONTENT, GUILD_MESSAGES, DIRECT_MESSAGES) automatically.
> With `thread_isolation = true`, agentbridge creates or reuses a Discord thread for each session, keyed by thread channel ID.

---

## Step 6: Generate an Invite Link

### 6.1 Go to OAuth2 Settings

In the left sidebar, click "OAuth2" → "URL Generator".

### 6.2 Select Scopes

Under "Scopes", check:
- ✅ `bot`
- ✅ `applications.commands`   *(required for `/agent` and other slash commands)*

### 6.3 Select Permissions

Under "Bot Permissions", check:

| Permission | Purpose |
|------------|---------|
| Read Messages/View Channels | Read messages |
| Send Messages | Send replies |
| Create Public Threads | New thread per agent session (if `thread_isolation` is on) |
| Send Messages in Threads | Reply inside threads |
| Read Message History | Show previous messages when resuming sessions |
| Embed Links | Rich progress embeds |
| Use Slash Commands | Allow `/agent`, `/new`, etc. |

### 6.4 Copy the Invite Link

The generated link appears at the bottom. Click "Copy".

---

## Step 7: Invite the Bot to Your Server

1. Open the copied URL in a browser
2. Select your target server from the dropdown
3. Review the permissions and click "Authorize"
4. Complete the CAPTCHA if prompted

---

## Step 8: Run agentbridge

### 8.1 Validate configuration

```bash
agentbridge doctor
```

Expected output:

```
  Config file (~/.agentbridge/config.yaml) is valid YAML
  claude CLI found: /usr/local/bin/claude
  [my-project] Default agent: kiro (backend: acp, command: kiro-cli)
  [my-project/kiro] ACP command 'kiro-cli' found
  [my-project/discord] Discord: @agentbridge reachable

All checks passed.
```

### 8.2 Launch

```bash
agentbridge                # foreground
agentbridge -c /path/to/config.yaml
```

Or run as a systemd user service (see bottom of this doc).

### 8.3 Verify Connection

Expected startup log:

```
INFO agentbridge::platforms::discord: discord: gateway connected
INFO agentbridge::platforms::discord: discord: READY bot_user_id=1492784059896696862
INFO agentbridge::platforms::discord: discord: commands registered count=30 scope="guild (instant)"
INFO agentbridge::engine: engine started project=my-project
INFO agentbridge: project ready name=my-project
```

If you see `commands registered` the slash-command menu (including `/agent`) is live.

---

## Step 9: Start Chatting

### 9.1 Channel usage

Send a message in any channel where the bot has permissions. If `group_reply_all` is enabled, the bot replies to every message. Otherwise, @mention the bot.

### 9.2 Slash commands

Type `/` in any channel and you'll see the full command menu populated by agentbridge:

- `/agent` — list agents, or `/agent kiro` to switch
- `/new [name]` — start a new session
- `/list` — list sessions for the current agent
- `/switch <id>` — switch session
- `/stop` — cancel the current turn
- `/help` — full command list

### 9.3 Direct message

1. Click the bot's avatar → Message
2. Send a DM — same behavior as channels

### 9.4 Switching agents mid-chat (multi-agent only)

```
/agent                  # show current agent + list available
/agent kiro             # switch to Kiro (starts a new session)
/agent claude           # switch back
```

Each agent maintains its own independent session list. `/list` only shows sessions for the currently active agent.

---

## Usage Example

```
User: Analyze the current project structure

agentbridge: 🤔 Thinking...
agentbridge: 🔧 Bash(ls -la)
agentbridge: Here's the project structure…
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Discord Cloud                           │
│   User Message ──→ Discord Gateway ◄── WebSocket             │
└─────────────────────────────────────────────────────────────┘
                          │ WebSocket (no public IP needed)
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Your Local Machine                        │
│   agentbridge ──► claude  OR  kiro-cli ──► Your Project    │
└─────────────────────────────────────────────────────────────┘
```

---

## Discord Gateway Features

| Feature | Details |
|---------|---------|
| **Connection** | WebSocket to `wss://gateway.discord.gg` |
| **Public IP** | ❌ Not needed |
| **Heartbeat** | Sent automatically at the interval advertised by Discord |
| **Reconnection** | Auto-reconnect on disconnect |
| **Intents** | Declared at connect time (Message Content is mandatory) |
| **Message limit** | 2000 chars per message (auto-split by agentbridge) |
| **Markdown** | Full native support |

---

## FAQ

### Q: Bot can't read message content

**Most common cause**: Message Content Intent is not enabled.

Fix:
1. Developer Portal → your app → Bot
2. Enable **Message Content Intent**
3. Save
4. Restart agentbridge

### Q: Bot connects, then goes silent after a while

Discord gateway occasionally times out. If you don't see any log activity for a while but the bot is still "Online" in Discord, restart agentbridge:

```bash
systemctl --user restart agentbridge.service
```

### Q: `/agent` doesn't show in the slash command menu

1. Did you include `applications.commands` scope when generating the invite URL? If not, kick the bot and re-invite with the correct scope.
2. Restart agentbridge after editing config. The `commands registered count=30` log line confirms the menu was re-published.
3. Discord caches slash commands — wait up to a minute after restart for the menu to refresh.

### Q: Slash command `/agent` returns nothing, but text `/agent` works

Discord's slash command UI requires permissions both on the server and on the bot. Check the bot has the `Use Slash Commands` permission in the channel (Server Settings → Integrations → your bot).

### Q: Bot doesn't appear in the server

Re-generate an invite link (Step 6) and invite again.

### Q: How to regenerate the token?

Developer Portal → your app → Bot → Reset Token. Update `~/.agentbridge/config.yaml` and restart.

### Q: Kiro-cli says authentication required

Kiro-cli uses its own auth, separate from agentbridge config. On the machine running agentbridge:

```bash
kiro-cli login --license free       # Builder ID
# or
kiro-cli login --license pro --identity-provider https://your-org.awsapps.com/start --region us-east-1
# headless server:
kiro-cli login --license free --use-device-flow
```

Credentials live under `~/.aws/sso/` and `~/.kiro/` of the Linux user running agentbridge.

---

## systemd user service (optional)

Create `~/.config/systemd/user/agentbridge.service`:

```ini
[Unit]
Description=agentbridge
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/agentbridge
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=agentbridge=info
Environment=HOME=/home/youruser
Environment=PATH=/home/youruser/.local/bin:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=default.target
```

Then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now agentbridge.service
loginctl enable-linger $USER   # keep running after logout
journalctl --user -u agentbridge.service -f
```

---

## References

- [Discord Developer Portal](https://discord.com/developers/applications)
- [Discord API Documentation](https://discord.com/developers/docs/intro)
- [Gateway Intents](https://discord.com/developers/docs/topics/gateway#privileged-intents)
- [OAuth2 Scopes](https://discord.com/developers/docs/topics/oauth2#shared-resources-oauth2-scopes)

---

## See Also

- [Telegram Setup](./telegram.md)
- [Kiro CLI Setup](./kiro-cli.md)
- [Back to README](../README.md)
