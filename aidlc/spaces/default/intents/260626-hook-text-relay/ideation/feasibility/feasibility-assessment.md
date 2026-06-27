# Feasibility Assessment — Hook-Based Real-Time Text Relay

> 上游输入:`intent-statement.md`(intent-capture)。market-research 被跳过(单人内部工具,build-vs-reuse 结论已在 intent-statement 的 trigger 段记录)。

## Technical Viability

**判定:高度可行,且核心机制已端到端验证(本会话亲手跑通,非纸面推断)。**

三个集成点全部已在系统中存在,改造不引入新的外部依赖面:

| 集成点 | 现状 | 本 feature 的动作 | 风险 |
|--------|------|------------------|------|
| Claude Code hooks(Stop / PostToolUse) | Claude Code 原生能力 | 注册薄 hook,读 payload | 低 —— payload 已验证 |
| agentbridge 事件管线(`AgentEvent` → `Platform` trait) | 已存在,截图也走这条 | 新增 `AgentEvent::Text` / 进度事件入口 | 低 —— 复用现有管线 |
| tmux `send-keys` 输入 | 已存在 | **不改** | 无 |
| hook → agentbridge 传输 | 新增 | localhost HTTP(复用 `axum`) | 低 —— 进程内本地 |

**已验证的关键事实**(`claude --print --settings <test>` 触发,真实 payload 捕获):

1. **Stop hook** 每轮触发一次,payload 直接含 `last_assistant_message`(纯文字回复)。比 remote-claude-control 还省一步(它需读 transcript 反扫 JSONL 抠文字)。
2. **PostToolUse hook** 每个工具调用后**实时**触发,含 `tool_name` / `tool_input` / `tool_response`(`stdout`/`stderr`/`interrupted`)/ `duration_ms`。
3. 全部 hook payload 含 `session_id` + `cwd` —— 门控与路由的依据。
4. `claude --settings <path>` 可注入 hook 配置而不改项目 `.claude/settings.json`(`claude --help` 确认);接管已在跑的 cc 则用全局 `~/.claude/settings.json` + 门控。

## Risk Analysis

| 风险 | 级别 | 缓解 |
|------|------|------|
| PostToolUse 在工具密集轮里刷屏(重现"截图发太勤快") | **中** | 必须节流/合并(已沉淀为 project.md 约束);可行性上可控 —— 有 `tool_name`+`duration_ms` 做判断依据 |
| 接管已在跑的 cc:全局 hook 误触其他本地 cc | 中 | session_id/cwd 门控;非桥接会话 agentbridge 查不到 channel 绑定即丢弃 |
| 逐字流式做不到(hook 是事件级非字符级) | 低(已接受) | 用户已确认不需要逐字流式;Stop+PostToolUse 的粒度足够 |
| hook 脚本出错阻塞 cc | 低 | 遵循 rcc 模式:hook 永不阻塞,任何错误 `exit 0` |
| transcript/payload 格式随 Claude Code 升级变化 | 低 | 用 Stop 的 `last_assistant_message` 直读字段(比解析 transcript 更稳);加防御性解析 |

## AWS / Platform Perspective (aws-platform-agent 视角)

**N/A。** 纯本地架构:hook 脚本在本机、HTTP 接收端在 localhost、文字经已有聊天通道(Discord WebSocket gateway / 未来飞书 WebSocket 长连接)发出。无 EC2/Lambda/任何云资源新增,无 Well-Architected 评估面。(代理 daemon 的部署问题见 [[proxy-daemon-breakage]],但那是现有部署的已知项,不在本 feature 范围。)

## Compliance Perspective (compliance-agent 视角)

**N/A。** 单人自用;无 PII 处理方式变化;无数据出境到新的第三方(文字走的是和现状(截图)相同的聊天通道);无 PCI/HIPAA/SOC2 触点。

## Conclusion

**可行性确认:GO。** 技术路径已验证、无新外部依赖、无云/合规阻碍。唯一需在后续设计阶段坐实的是两件工程取舍:①PostToolUse 节流策略(NFR/functional design);②session→channel 绑定与门控机制(application design)。两者都已识别、均无可行性障碍。
