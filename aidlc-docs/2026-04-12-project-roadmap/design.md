# AgentPush 完整实施路线图

> 决策日期：2026-04-12
> 状态：已批准
> 技术栈：Rust + tokio
> Agent：仅 Claude Code
> 分发：二进制 + npm wrapper

---

## 架构总览

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI (clap)                           │
│  agentbridge / init / doctor / daemon / cron / send           │
├─────────────────────────────────────────────────────────────┤
│                    Management API (axum)                     │
│  REST + WebSocket + Web Dashboard 静态资源                    │
├─────────────────────────────────────────────────────────────┤
│                      Engine (per-project)                    │
│  消息路由 · 命令分发 · 会话管理 · 权限控制 · 流式输出           │
├──────────────────────┬──────────────────────────────────────┤
│   Agent 层            │         Platform 层                  │
│  ┌─────────────────┐ │  ┌──────────┐ ┌──────────┐          │
│  │ Claude Code     │ │  │ Telegram │ │ Discord  │ ...      │
│  │ (subprocess)    │ │  └──────────┘ └──────────┘          │
│  │ (SDK future)    │ │                                      │
│  │ (Remote SSH/WS) │ │  ┌──────────────────────────┐       │
│  └─────────────────┘ │  │ Bridge (WebSocket 外接)   │       │
│                       │  └──────────────────────────┘       │
├───────────────────────┴──────────────────────────────────────┤
│  基础设施：Session · Cron · i18n · RateLimit · Speech/TTS    │
└─────────────────────────────────────────────────────────────┘
```

---

## 分阶段实施计划

### Phase 0：修复 MVP 阻塞项（Day 1-2）

**目标：** Telegram 收到消息 → Claude Code 处理 → 回复文本

| # | 任务 | 文件 | 说明 |
|---|------|------|------|
| 0.1 | 修复 Telegram poll_loop 未启动 | platforms/telegram/mod.rs | `start()` 中 `tokio::spawn(Arc::clone(&self).poll_loop())` |
| 0.2 | 修复 handler 未调用 | platforms/telegram/mod.rs | `process_update()` 中调用 handler 回调 |
| 0.3 | 接通 Agent 调用 | engine.rs | `handle_message()` 中调用 `agent.send()` 并流式回复 |
| 0.4 | 端到端测试 | - | 发 Telegram 消息 → 收到 Claude Code 回复 |

**交付物：** 可以在 Telegram 上和 Claude Code 对话的最小产品。

---

### Phase 1：核心功能完善（Week 1-2）

**目标：** 达到日常可用水平

| # | 任务 | 优先级 | 说明 |
|---|------|--------|------|
| 1.1 | 流式预览 | P0 | Telegram `editMessageText` 实时更新消息 |
| 1.2 | Session 管理命令 | P0 | /new /list /switch /current /delete |
| 1.3 | 模型切换 | P0 | /model 命令，配置多模型 |
| 1.4 | 权限模式切换 | P0 | /mode default/yolo/plan/auto |
| 1.5 | 工具调用权限审批 | P0 | Telegram inline button 允许/拒绝 |
| 1.6 | Typing 指示器 | P1 | 处理中显示「正在输入」 |
| 1.7 | 图片/文件收发 | P1 | 接收图片转发给 Agent，Agent 生成的文件回传 |
| 1.8 | 访问控制 | P1 | allow_from + admin_from 白名单 |
| 1.9 | 速率限制 | P1 | 滑动窗口限流 |
| 1.10 | 错误处理改进 | P1 | 统一错误类型，用户友好提示 |
| 1.11 | init 交互式向导 | P1 | 引导用户配置 Telegram token + work_dir |
| 1.12 | doctor 诊断 | P2 | 检查 claude CLI、网络、配置 |

---

### Phase 2：高级功能（Week 3-4）

**目标：** 补齐核心功能，加入差异化特性

| # | 任务 | 分类 | 说明 |
|---|------|------|------|
| 2.1 | Cron 定时任务系统 | 核心 | cron 表达式 + prompt/exec 模式 |
| 2.2 | 自定义命令 | 核心 | 模板化 prompt 命令 + exec 命令 |
| 2.3 | 命令别名 | 核心 | 中文触发词 → 命令映射 |
| 2.4 | 语音转文字 (STT) | 核心 | 集成 OpenAI Whisper / Groq |
| 2.5 | 文字转语音 (TTS) | 核心 | 集成 Qwen TTS / OpenAI TTS |
| 2.6 | Bot-to-bot Relay | 核心 | 多项目间消息转发 |
| 2.7 | 自动压缩 | 核心 | token 超阈值时自动 /compact |
| 2.8 | 多项目支持 | 核心 | 单进程多个 project engine |
| 2.9 | 违禁词过滤 | 安全 | banned_words 匹配拦截 |
| 2.10 | i18n 完善 | 体验 | en/zh/ja 全面覆盖 |
| 2.11 | **权限审批 UI（差异化）** | 差异化 | Telegram button 审批，比纯文本回复更好 |

---

### Phase 3：Discord 平台 + 运维（Week 5-6）

**目标：** 第二平台上线 + 生产级运维

| # | 任务 | 分类 | 说明 |
|---|------|------|------|
| 3.1 | Discord Gateway 适配 | 平台 | WebSocket 连接，消息收发 |
| 3.2 | Discord 流式预览 | 平台 | editMessage 实时更新 |
| 3.3 | Discord slash 命令注册 | 平台 | Application Commands API |
| 3.4 | Discord thread 隔离 | 平台 | 每个用户/话题独立 session |
| 3.5 | Daemon 守护进程 | 运维 | systemd / launchd 集成 |
| 3.6 | 日志轮转 | 运维 | 文件日志 + 大小限制 |
| 3.7 | 自动更新 | 运维 | `agentbridge update` 自更新 |
| 3.8 | Webhook 端点 | 运维 | 外部触发 (git hooks, CI) |
| 3.9 | npm wrapper 分发 | 分发 | `npm install -g agentbridge` |

---

### Phase 4：差异化 — Web Dashboard（Week 7-8）

**目标：** 可视化管理界面

| # | 任务 | 说明 |
|---|------|------|
| 4.1 | Management REST API | axum HTTP 服务器，CRUD 项目/session/cron |
| 4.2 | 实时 WebSocket 推送 | 消息流实时推送到前端 |
| 4.3 | Web 前端（React/Vue） | 配置管理、session 浏览、cron 管理 |
| 4.4 | 用量统计面板 | token 消耗、消息计数、延迟图表 |
| 4.5 | 在线配置编辑 | 可视化编辑 config.yaml |
| 4.6 | QR 码扫描配置 | 飞书/微信的 QR 引导流程 |

---

### Phase 5：差异化 — 远程 Agent + 多租户（Week 9-12）

**目标：** 团队使用场景

| # | 任务 | 说明 |
|---|------|------|
| 5.1 | **远程 Agent 协议** | 定义 Agent ↔ AgentPush 的 WebSocket/SSH 通信协议 |
| 5.2 | Agent 代理模式 | agentbridge 运行在服务器，Claude Code 运行在开发者笔记本 |
| 5.3 | SSH tunnel 自动建立 | `agentbridge agent connect user@host:/path/to/project` |
| 5.4 | **多租户引擎** | 多用户各自独立 agent session，统一 bot 入口 |
| 5.5 | 用户注册/邀请 | 团队成员通过 bot 自助注册 |
| 5.6 | 租户隔离 | 每个用户独立 work_dir、权限、配额 |
| 5.7 | 管理员面板 | 查看所有租户状态、用量、审计日志 |
| 5.8 | 计费/配额系统 | 按用户 token 消耗限额 |

---

### Phase 6：扩展平台（Week 13+，按需）

| # | 平台 | 连接方式 | 优先级 |
|---|------|----------|--------|
| 6.1 | 飞书 (Feishu) | WebSocket | 高（国内市场） |
| 6.2 | 钉钉 (DingTalk) | Stream | 中 |
| 6.3 | Slack | Socket Mode | 中（海外团队） |
| 6.4 | 微信 (Weixin) | 长轮询 ilink | 高（个人用户） |
| 6.5 | 企业微信 (WeCom) | WebSocket | 中 |
| 6.6 | LINE | Webhook | 低 |
| 6.7 | QQ | OneBot WS | 低 |
| 6.8 | Bridge 协议 | WebSocket | 中（让社区自己接） |

---

## 技术决策记录

| 决策 | 选择 | 原因 |
|------|------|------|
| 语言 | Rust | 性能好，单二进制分发 |
| 异步运行时 | tokio | Rust 生态标准 |
| HTTP 框架 | axum | Management API + Web Dashboard |
| 配置格式 | YAML | 比 TOML 更直观，嵌套结构清晰 |
| Agent 通信 | subprocess (claude --print --stream-json) | 官方 CLI 接口，成熟可靠 |
| 远程 Agent | WebSocket over SSH tunnel | 安全，不暴露公网端口 |
| 前端 | React (Vite) 嵌入二进制 | embed 静态资源，零依赖部署 |
| 分发 | GitHub Release + npm wrapper | 覆盖两个群体：Rust/CLI 用户 + Node.js 生态用户 |
| 持久化 | JSON 文件（v1）→ SQLite（v2） | 简单开始，后续可迁移 |

---

## 功能规划概览

| 功能 | agentbridge 计划 | 状态 |
|------|----------------|------|
| 多 Agent 支持 | 仅 Claude Code | 设计选择（深度 > 广度） |
| 多平台 | 先 Telegram + Discord，后续按需 | Phase 1 + 3 |
| 流式预览 | ✅ | Phase 1 |
| 权限审批 | **Inline Button UI** | Phase 1（差异化） |
| Session 管理 | ✅ | Phase 1 |
| Cron 定时 | ✅ | Phase 2 |
| Bot Relay | ✅ | Phase 2 |
| 语音 STT/TTS | ✅ | Phase 2 |
| Web Dashboard | **✅** | Phase 4（差异化） |
| 远程 Agent | **✅** | Phase 5（差异化） |
| 多租户 | **完整多租户** | Phase 5（差异化） |
| Daemon 模式 | ✅ | Phase 3 |
| npm 分发 | ✅ | Phase 3 |
| 自动更新 | ✅ | Phase 3 |
| 卡片消息 | Phase 6 | 按需 |
| Bridge 协议 | Phase 6 | 按需 |

---

## 里程碑总结

| 里程碑 | 时间 | 交付物 |
|--------|------|--------|
| **M0: MVP** | Day 2 | Telegram ↔ Claude Code 文本对话可用 |
| **M1: 可用** | Week 2 | 流式预览 + session + 权限审批 + 图片 |
| **M2: 功能完整** | Week 4 | Cron + Relay + 语音 + 自定义命令 |
| **M3: 双平台** | Week 6 | Discord 上线 + Daemon + npm 发布 |
| **M4: 差异化** | Week 8 | Web Dashboard 上线 |
| **M5: 团队版** | Week 12 | 远程 Agent + 多租户 |
| **M6: 生态** | Week 13+ | 更多平台按需接入 |

---

## 下一步行动

立即开始 **Phase 0**：修复 3 个 MVP 阻塞 bug，让 Telegram → Claude Code 链路跑通。
