# Telegram Setup Guide

This guide walks you through connecting **agentbridge** to Telegram, so you can chat with your local Claude Code or Kiro CLI via a Telegram bot.

## Prerequisites

- A Telegram account
- A machine that can run agentbridge (no public IP needed)
- Claude Code **or** Kiro CLI installed and configured

> 💡 **Advantage**: Uses Long Polling mode — no public IP, no domain, no reverse proxy needed.

---

## Step 1: Create a Telegram Bot

### 1.1 Open BotFather

Search for **@BotFather** in Telegram (the official bot manager) and start a chat.

> ⚠️ Make sure it's the verified official BotFather — don't use third-party imitations.

### 1.2 Create a New Bot

Send the command `/newbot`. BotFather will ask you to provide a name and username.

### 1.3 Set the Bot Name

Enter a **display name** for your bot (e.g. `agentbridge`).

### 1.4 Set the Bot Username

Enter a **username** (must end with `bot`, e.g. `agentbridge_bot`).

> 💡 **Naming rules:**
> - Must end with `bot` (case-insensitive)
> - Only letters, numbers, and underscores
> - Must be globally unique

### 1.5 Get the Bot Token

After creation, BotFather will reply with something like:

```
Done! Congratulations on your new bot...
Use this token to access the HTTP API:
1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-123456

Keep your token secure...
```

> ⚠️ Save this token immediately — it's only shown once! If lost, use `/mybots` → select bot → `API Token` → `Revoke current token` to regenerate.

---

## Step 2: Configure agentbridge

Edit `~/.agentbridge/config.yaml` (create it with `agentbridge init` if it doesn't exist yet):

### 2.1 Minimal config (Claude Code only)

```yaml
language: en
projects:
  - name: my-project
    work_dir: /path/to/your/project
    agent:
      mode: yolo           # default | yolo | plan | auto
      max_turns: 30
    platforms:
      - type: telegram
        options:
          token: "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-123456"
    allow_from: "*"        # "*" = anyone, or "id1,id2" = restrict
```

### 2.2 Multi-agent config (Claude + Kiro)

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
    default_agent: kiro    # which agent new sessions use
    platforms:
      - type: telegram
        options:
          token: "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-123456"
    allow_from: "*"
```

Use `/agent` in chat to list or switch agents at runtime.

> **Where does `allow_from` go?** It's a **project-level** field, a sibling of `platforms`. Placing it inside `options:` is silently ignored.
>
> To find your Telegram user ID, send any message to **@userinfobot**.

---

## Step 3: Get Chat ID (Optional)

If you want to restrict the bot to specific users/groups, you'll need the Chat ID.

### 3.1 Get Your Personal Chat ID

1. Send any message to your bot
2. Visit the following URL (replace `{{TOKEN}}` with your token):

```
https://api.telegram.org/bot{{TOKEN}}/getUpdates
```

3. Find the `chat.id` field in the returned JSON

### 3.2 Get a Group Chat ID

1. Add the bot to a group
2. Send a message mentioning @your_bot in the group
3. Check the `getUpdates` URL — group Chat IDs are usually negative numbers

---

## Step 4: Set Bot Commands (Optional)

agentbridge registers the full command list with Telegram automatically on startup — you'll see the `/` menu populated with `/new`, `/list`, `/switch`, `/agent`, `/stop`, and more.

If you want to override the menu manually via BotFather:

### 4.1 Set Command Menu

In BotFather, send:

```
/setcommands
```

Select your bot, then enter the command list:

```
help - Show available commands
new - Start a new session
list - List sessions
agent - Switch agent backend (claude / kiro)
stop - Stop current task
```

### 4.2 Set Bot Description

```
/setdescription
```

Enter a description — users will see this when they first open the bot.

---

## Step 5: Run agentbridge

### 5.1 Validate configuration

```bash
agentbridge doctor
```

You should see:

```
  Config file (~/.agentbridge/config.yaml) is valid YAML
  claude CLI found: /usr/local/bin/claude
  [my-project] Default agent: kiro (backend: acp, command: kiro-cli)
  [my-project/kiro] ACP command 'kiro-cli' found
  [my-project/telegram] Telegram: @agentbridge_bot reachable

All checks passed.
```

### 5.2 Launch

```bash
agentbridge                # foreground
agentbridge -c /path/to/config.yaml     # custom config path
```

Or run as a systemd user service:

```bash
# Create ~/.config/systemd/user/agentbridge.service (see bottom of this doc)
systemctl --user enable --now agentbridge.service
journalctl --user -u agentbridge.service -f
```

### 5.3 Verify Connection

Expected startup log:

```
INFO agentbridge: agentbridge v0.1.0 starting projects=1
INFO agentbridge::platforms::telegram: telegram: long-poll started
INFO agentbridge::engine: engine started project=my-project
INFO agentbridge: project ready name=my-project
```

---

## Step 6: Start Chatting

### 6.1 Direct Message

1. Search for your bot's username in Telegram
2. Click "Start"
3. Send a message — agentbridge routes it to your default agent

### 6.2 Group Chat

1. Create or open a group
2. Add your bot
3. Send messages

> ⚠️ By default, Telegram bots in groups only see messages that @mention them or reply to them. To read all group messages, talk to BotFather: `/setprivacy` → select bot → `Disable`.

### 6.3 Switching agents mid-chat (if multi-agent is configured)

```
/agent            # show current agent + list available
/agent kiro       # switch to Kiro (starts a new session for kiro)
/agent claude     # switch back to Claude Code
```

Each agent has its own independent session list. `/list` only shows sessions for the currently active agent.

---

## Usage Example

```
User: What's this project about?

agentbridge: 🤔 Thinking...
agentbridge: 🔧 Bash(ls -la)
agentbridge: This project is a Rust CLI that bridges Claude Code to
             messaging platforms — Telegram, Discord, and more.
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Telegram Cloud                          │
│   User Message ──→ Telegram Bot API ◄── Long Polling         │
└─────────────────────────────────────────────────────────────┘
                           │ HTTPS (no public IP needed)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    Your Local Machine                        │
│   agentbridge ──► claude  OR  kiro-cli ──► Your Project    │
└─────────────────────────────────────────────────────────────┘
```

---

## Long Polling vs Webhook

| Feature | Long Polling (default) | Webhook |
|---------|-----------------------|---------|
| Public IP | ❌ Not needed | ✅ Required |
| Domain | ❌ Not needed | ✅ Required |
| HTTPS cert | ❌ Not needed | ✅ Required |
| Complexity | Simple | More complex |
| Latency | Low | Low |
| Best for | Local dev, private networks | Production / shared deployment |

agentbridge currently uses **long polling** and does not require any inbound network exposure.

---

## FAQ

### Q: Bot doesn't respond to messages

Check in order:

1. Is the agentbridge process running? `systemctl --user status agentbridge.service`
2. Is the token correct and not revoked?
3. Look at `journalctl --user -u agentbridge.service -f` while you send a message — if you don't see `handle_message: received`, the message didn't reach agentbridge.
4. If this is a group chat, did you disable Group Privacy in BotFather?

### Q: How do I regenerate the token?

1. Send `/mybots` to BotFather
2. Select your bot → `API Token` → `Revoke current token`
3. Copy the new token into `~/.agentbridge/config.yaml`
4. Restart agentbridge

### Q: Bot doesn't respond in groups

Most likely cause is Telegram's **Group Privacy mode**. Disable it:

```
@BotFather → /mybots → select bot → Bot Settings → Group Privacy → Turn off
```

Then kick and re-invite the bot.

### Q: `/agent` returns "Unknown command"

You're on an agentbridge build that predates the multi-agent feature. Upgrade to a version that ships the `/agent` command.

### Q: Kiro-cli says authentication required

Kiro-cli uses its own auth, separate from agentbridge config. On the machine running agentbridge:

```bash
kiro-cli login --license free       # Builder ID (personal)
# or
kiro-cli login --license pro --identity-provider https://your-org.awsapps.com/start --region us-east-1
# or, on a headless server:
kiro-cli login --license free --use-device-flow
```

Credentials are stored under `~/.aws/sso/` and `~/.kiro/` for the Linux user running agentbridge.

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
loginctl enable-linger $USER    # keep the service running after logout
```

---

## References

- [Telegram Bot API](https://core.telegram.org/bots/api)
- [BotFather Guide](https://core.telegram.org/bots#botfather)

## See Also

- [Discord Setup](./discord.md)
- [Kiro CLI Setup](./kiro-cli.md)  *(if you haven't installed it yet)*
- [Back to README](../README.md)
