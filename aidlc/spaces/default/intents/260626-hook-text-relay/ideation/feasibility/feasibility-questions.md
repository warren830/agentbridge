# Feasibility Questions — Hook-Based Real-Time Text Relay

> 可行性与约束分析。本会话已**亲手验证**大量技术事实,所以多数题不是"开放未知",而是"确认我对约束/取舍的理解准不准"。每题已预填推荐答案。
> 标准的 integration / 合规 / 云 问题对这个纯本地单人工具大多 N/A,我如实标注。

---

## Q1. 这个改造要和哪些现有系统/组件集成?

- A. 三个集成点,全部已在代码库中存在:①Claude Code 的 hook 系统(Stop/PostToolUse,已验证 payload);②agentbridge 现有的事件管线(`AgentEvent` → `Platform` trait → Discord/飞书);③tmux backend 的**输入**路径(`send-keys`,保持不变)。新增的唯一"集成面"是 hook 脚本 → agentbridge 的本地传输(localhost HTTP)。无外部系统、无云、无第三方 API。
- B. 还要集成别的(请说明)。
- X. Other (please specify)

[Answer]: A

---

## Q2. hook 技术路径的可行性已验证到什么程度?

- A. 已端到端验证(本会话亲手跑):用 throwaway `claude --print --settings <test>` 触发了全部 hook,捕获真实 payload —— 确认 ①Stop 带 `last_assistant_message`(纯文字);②PostToolUse 带 `tool_name`/`tool_input`/`tool_response`(含 stdout/stderr)/`duration_ms`,且每个工具调用后实时触发;③所有 hook 带 `session_id`+`cwd`;④`claude --settings <path>` 可注入 hook 而不改项目文件。技术路径无"我猜"成分。
- B. 只验证了一部分(请说明哪些还没验)。
- C. 还没验证,纯设计推断。
- X. Other (please specify)

[Answer]: A

---

## Q3. hook → agentbridge 的传输方式怎么选?

- A. 本地 HTTP(localhost),复用项目已有的 `axum` 依赖(CLAUDE.md 已列为允许的现有依赖)。hook 脚本是个极薄的 POST 客户端。理由:进程间、语言无关(hook 脚本可以是 python/bash)、不引入新依赖。
- B. Unix domain socket / 命名管道(更轻,但 hook 脚本侧实现更繁)。
- C. 写文件 + agentbridge 监听文件(像 rcc 的 signal 文件,但增加轮询/inotify 复杂度)。
- X. Other (please specify)

[Answer]: A

---

## Q4. 门控机制怎么定(让非桥接的本地 cc 不乱发)?

- A. agentbridge 在桥接某会话时,把"哪个 session_id / cwd 属于哪个聊天 channel"的绑定登记下来;hook 脚本带 session_id+cwd POST 过来,agentbridge 查不到绑定就丢弃。门控逻辑在 agentbridge 侧(hook 脚本保持极薄、无状态)。
- B. 门控在 hook 脚本侧(脚本自己判断 cwd 是否匹配,像 rcc 的物理目录隔离)。
- C. 两层都做(脚本粗筛 + agentbridge 精确路由)。
- X. Other (please specify)

[Answer]: A

---

## Q5. PostToolUse 逐工具进度的"节奏"风险怎么控(这是已沉淀的 project 约束)?

- A. 必须有节流/合并策略(已写进 project.md ## Corrections)。初步方向:合并连续的同类工具事件、或只对长耗时工具(`duration_ms` 超阈值)单独报、或对一轮内的工具调用做摘要式聚合。具体策略留到 NFR/functional design 定,但**可行性上确认这是可控的**(我们手里有 tool_name + duration_ms,足够做判断)。
- B. 不节流,逐个发(接受刷屏)。
- C. 干脆不要 PostToolUse,只保留 Stop(回到"每轮一条")。
- X. Other (please specify)

[Answer]: A

---

## Q6. 合规 / 云 / 数据出境 / 团队技能 等标准可行性维度?

- A. 大多 N/A 并说明:**无云资源**(纯本地 localhost),**无数据出境到新方**(文字经已有的聊天通道发出,和现状一致),**无合规面**(单人自用、无 PII 处理变化)。**团队技能**:Rust+tokio+axum 都是项目现有栈,hook 脚本可用 python(系统自带)。**预算/时间线**:单人项目,无硬约束。唯一真实约束是 CLAUDE.md 的工程约束(异步热路径、无 unsafe、加依赖先问、平台不分支)。
- B. 有我没考虑到的合规/云/数据约束(请说明)。
- X. Other (please specify)

[Answer]: A

---

## 回答方式

- **Guide me** — 逐题带你过
- **I'll edit the file** — 你直接改,改完发 "done"
- **Chat** — 随便聊,我抽答案

(全部预填 A = 已验证事实 + 已确立方向。认同就说"全部确认";要改告诉我题号。)
