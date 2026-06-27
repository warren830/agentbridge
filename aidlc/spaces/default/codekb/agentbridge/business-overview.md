# Business Overview — agentbridge

## Domain & Purpose

agentbridge 是一个 **Rust + tokio 桥接服务**,把 **Claude Code** 接到聊天软件(Telegram、Discord),让用户从手机/聊天端远程驱动 Claude Code 会话。核心价值:把"在终端前操作 Claude Code"的体验搬到任何聊天客户端。

## Key Functionality

- **多平台适配**:Telegram(长轮询)、Discord(Gateway WS),经统一的 `Platform` capability traits 接入,引擎不感知具体平台。
- **三种 agent backend**(经 `AgentSession` trait 统一):
  - `claude` —— 原生 `claude --print --output-format stream-json`,结构化事件流(最干净)。
  - `acp` —— JSON-RPC over stdio,接 Kiro/Cursor/Gemini。
  - `tmux` —— 经 `send-keys`/`capture-pane` 驱动 tmux 里跑的 cc,屏幕抓取式(最弱,本 intent 正在改造)。
- **会话管理**:每项目多会话,JSON 持久化,try-lock + 有界队列保证并发安全。
- **富交互**:流式预览(逐步编辑消息)、工具调用展示、权限审批按钮、语音转文字、定时提示注入。
- **网关 dashboard**:可选的 web 控制台,经 WS 反向连接 fan-out 多实例。

## Active Initiative

**Hook-based text relay**(本 intent):用 Claude Code 的 Stop/PostToolUse hook 实时回传纯文字,替代 tmux backend 的屏幕截图路径。详见 `intents/260626-hook-text-relay/`。

## Users

主要为单用户自部署(作者本人),手机端远程控制 Mac 上运行的 Claude Code。
