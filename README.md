# agentbridge

Bridge Claude Code to your favorite chat apps. Control your AI coding agent from Telegram, Discord, and more.

## Quick Start

### Install

**From npm (recommended):**

```bash
npm install -g agentbridge
```

**From source:**

```bash
git clone git@github.com:anthropics/agentbridge.git
cd agentbridge
cargo build --release
```

### Configure

```bash
agentbridge init
```

This creates `~/.agentbridge/config.yaml`. Or create it manually:

```yaml
language: en
projects:
  - name: my-project
    work_dir: "/path/to/your/project"
    agent:
      mode: "default"       # default | yolo | plan | auto
      # model: "claude-sonnet-4-20250514"  # optional model override
    platforms:
      - type: telegram
        options:
          token: "123456:ABC-DEF"
      - type: discord
        options:
          token: "your-discord-bot-token"
          # guild_id: "123456789"  # optional: respond to all messages in this guild
    allow_from: "*"          # or comma-separated user IDs
    rate_limit:
      max_messages: 20
      window_secs: 60
    # banned_words: ["secret"]
    # commands:
    #   - name: test
    #     description: "Run tests"
    #     prompt: "Run the test suite and report results"
```

### Run

```bash
agentbridge           # start the bridge
agentbridge run       # same as above
```

## Features

- **Multi-platform**: Telegram and Discord adapters with a pluggable architecture
- **Streaming responses**: Live message editing shows Claude's output as it generates
- **Session management**: Multiple named sessions per user, persist across restarts
- **Model/mode switching**: Change models and permission modes on the fly via chat commands
- **Cron jobs**: Schedule recurring prompts (e.g., daily status summaries)
- **Custom commands**: Define project-specific slash commands with prompt templates
- **Rate limiting**: Per-user sliding window rate limiter
- **Access control**: Restrict which users can interact with the bot
- **Image support**: Send photos to Claude (Telegram)
- **Banned words filter**: Silently drop messages matching configured words
- **i18n**: English, Chinese, Japanese UI strings
- **Daemon mode**: Run as a systemd user service

## Chat Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/new [name]` | Create a new session |
| `/list` | List all sessions |
| `/switch <id>` | Switch to a different session |
| `/current` | Show current session info |
| `/delete` | Delete current session |
| `/model [name]` | Show or change the active model |
| `/mode [mode]` | Show or change the permission mode (default/yolo/plan/auto) |
| `/cron add <schedule> <prompt>` | Add a scheduled task |
| `/cron list` | List scheduled tasks |
| `/cron del <id>` | Delete a scheduled task |
| `/commands` | List custom commands |
| `/status` | Show project status |

## CLI Commands

```
agentbridge               Start the bridge (default)
agentbridge run            Same as above
agentbridge init           Interactive setup wizard
agentbridge doctor         Check configuration health
agentbridge daemon install Install as systemd user service
agentbridge daemon start   Start the background service
agentbridge daemon stop    Stop the background service
agentbridge daemon status  Show service status
agentbridge daemon logs    View service logs
agentbridge daemon uninstall  Remove the service
```

## Platform Setup

### Telegram

1. Create a bot via [@BotFather](https://t.me/BotFather)
2. Copy the bot token into your config
3. (Optional) Set `allow_from` to your Telegram user ID for access control

### Discord

1. Create an application at the [Discord Developer Portal](https://discord.com/developers/applications)
2. Create a bot under the application
3. Enable **MESSAGE CONTENT** intent in the bot settings
4. Generate an invite URL with `bot` scope and `Send Messages` + `Read Message History` permissions
5. Copy the bot token into your config
6. The bot responds to:
   - Direct messages
   - Guild messages where the bot is @mentioned
   - All messages in the configured `guild_id` (if set)

## License

MIT
