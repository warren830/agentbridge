# Intent Capture Questions — Hook-Based Real-Time Text Relay

> 这是"用 Claude Code 的 hook 实时返回**文字**、彻底干掉 tmux 截图"这个改造的意图捕获。
> 我已经根据我们之前的对话**预填了推荐答案**(每题的 A 选项)。你只需确认,或改成别的选项 / X (Other) 自己写。
> 模式见文件末尾。每题最后一个选项都是 `X. Other`。

---

## Q1. 我们要解决的核心业务问题是什么?

当前 tmux backend 靠每 150ms 截一次终端屏幕 + Pillow 渲染 PNG 发回聊天软件。问题:截图啰嗦、节奏难控、CJK/字体/对齐 bug 多、是三种方案里最弱的一种。

- A. 用 Claude Code 官方 hook(Stop + PostToolUse)把 cc 每轮的**纯文字回复**和**逐工具进度**实时发回聊天软件,彻底取代截图 —— 文字干净、节奏天然(事件驱动而非轮询)、无渲染 bug。
- B. 保留截图,只是额外加一路文字(双轨并存)。
- C. 只解决节奏问题(降低截图频率),不引入 hook。
- D. 换成 SDK/print 模式重写(牺牲"同一个 cc")。
- X. Other (please specify)

[Answer]: A

---

## Q2. 谁是用户?他们在什么场景下用、痛在哪?

- A. 就是我本人(单用户、自用)。场景:人在手机端(先 Discord 验证,最终飞书),远程控制我 Mac 上**已经在跑的那个 cc**(如 nova-bidding),想看到跟在 Mac 终端前一样的文字交互 —— 而不是一张张截图。痛点:截图看不清、刷屏、和原生体验不一致。
- B. 团队多人共享一个 bot。
- C. 给别人用的产品(对外)。
- X. Other (please specify)

[Answer]: A

---

## Q3. 成功长什么样?哪些指标重要?

- A. 端到端验证(我自己 live 测过,不靠你当调试器):①手机发消息→敲进 Mac 上正在跑的 cc;②cc 每轮答完→聊天软件收到一条**干净文字回复**(非截图);③跑工具时→收到逐工具进度("⏺ 正在跑 npm test…");④全程是同一个 cc(带我的 CLAUDE.md/MCP/skills);⑤截图代码路径被移除。无"截图刷屏""节奏失控"问题。
- B. 只要文字能回来就行,不要求逐工具进度。
- C. 还要加逐字流式(像网页 ChatGPT)。
- X. Other (please specify)

[Answer]: A

---

## Q4. 为什么现在做这个(触发点)?

- A. 已学习两个姊妹项目(happy、remote-claude-control),确认 remote-claude-control 用 Stop hook 读 transcript 是成熟做法;且我已亲手验证 Claude Code 的 Stop hook 直接带 `last_assistant_message`(纯文字)、PostToolUse 带 tool_name/tool_response —— 技术路径已 de-risk,该把最弱的截图方案换掉了。
- B. 截图方案出了线上故障,必须紧急换。
- C. 没什么特别触发,就是想优化。
- X. Other (please specify)

[Answer]: A

---

## Q5. 初步范围信号(这是多大的活)?

- A. feature 规模、brownfield 改造:新增极薄 hook 脚本(~30行,带 cwd/session_id 门控)+ agentbridge 内新增本地 HTTP 接收端(复用已有 axum)+ hook payload→AgentEvent 映射 + 砍掉截图轮询/Pillow。不碰 tmux **输入**路径。架构上不破坏"引擎不分支平台"约束(hook 只 POST 给 agentbridge,由 Platform trait 发出)。
- B. 比这更小(纯重构,只换截图为文字)。
- C. 比这更大(顺带重做整个 tmux backend / 加飞书)。
- X. Other (please specify)

[Answer]: A

---

## Q6. 实时粒度(你在岔路口已选,这里确认)?

- A. Stop + PostToolUse 两个都要:每轮一条干净文字回复(Stop)+ 长任务中途的逐工具进度(PostToolUse)。
- B. 只要 Stop(每轮一条,最简)。
- C. 还要 Notification hook(权限弹窗也转发到手机并能远程批准/拒绝)。
- X. Other (please specify)

[Answer]: A

---

## Q7. hook 安装策略(决定 hook 装哪,你在岔路口已选)?

- A. 接管已在跑的 cc:hook 装进全局 `~/.claude/settings.json`,用 tmux session / cwd 门控,只对 agentbridge 桥接的会话生效,不影响我别的纯本地 cc。
- B. agentbridge 自己用 `claude --settings <注入配置>` 起 cc(不碰任何文件,最干净),不接管已在跑的。
- C. 两者都支持(自动起的用 --settings,接管的用全局 hook + 门控)。
- X. Other (please specify)

[Answer]: A

---

## 回答方式

- **Guide me** — 我逐题带你过(每题已预填 A,你确认或改)
- **I'll edit the file** — 你直接改这个文件,改完发 "done"
- **Chat** — 我们随便聊,我从对话里抽答案

(所有答案我都预填成了 A = 我们对话里已确立的方向。如果全部认同,直接说"全部确认"/"approve"即可;某题想改,告诉我题号 + 新选项。)
