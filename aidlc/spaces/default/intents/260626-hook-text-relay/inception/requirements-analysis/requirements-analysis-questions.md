# Requirements Analysis Questions — Hook-Based Real-Time Text Relay

> 把 hook-relay 写成结构化需求。多数已在前面阶段定了,这里问的是 feasibility 留下的两个工程取舍的**需求级**决策(节流策略、绑定细节)+ 几个之前没具体化的需求点。已预填。

---

## Q1. PostToolUse 节流策略 —— 需求级行为是什么?(feasibility 留的取舍 #1)

- A. **默认合并 + 长任务单独报**:一轮内的工具调用默认折叠成"进度脉冲"(不是每个工具一条消息);但单个 `duration_ms` 超阈值(比如 >10s)的工具单独实时报。最终具体算法/阈值留到 functional-design,但需求层面定为"默认合并、长任务突出"。可配置开关(像现有 display.tool_messages)。
- B. 每个工具调用都实时报一条(不合并)。
- C. 完全不报工具进度,只报 Stop 文字(等于砍掉 PU-4)。
- X. Other

[Answer]: X — 每工具实时报一条(不默认合并),但经现有 outgoing_ratelimit 每频道限速兜底防刷屏。矛盾裁决:文字远轻于截图,限速兜底即满足 ic-throttle 节流约束。

---

## Q2. session → channel 绑定 —— 需求级行为?(feasibility 留的取舍 #2)

- A. agentbridge 在桥接某会话时登记 `(cwd, tmux_session) → channel` 绑定;hook payload 带 `session_id`+`cwd`,接收端按 **cwd 优先、session_id 兜底** 查绑定;查不到 → 丢弃(门控)。复用现有 session.rs 的 tmux_session 字段。
- B. 只按 session_id 绑定(cwd 不参与)。
- C. 只按 cwd 绑定(session_id 不参与)。
- X. Other

[Answer]: A

---

## Q3. 用哪些 hook 事件 + 各自映射成什么?

- A. **Stop** → 取 `last_assistant_message` → `AgentEvent::Result`(收尾,带文字)或 `Text`;**PostToolUse** → 取 `tool_name`/`tool_response`/`duration_ms` → 经节流 → `AgentEvent::ToolUse`(或轻量进度变体,functional-design 定)。MVP 只接 Stop;PostToolUse 第二批。不接 PreToolUse/Notification(本次范围外)。
- B. 还要接 Notification(权限弹窗转发)。
- C. 还要接 UserPromptSubmit / SessionStart 等。
- X. Other

[Answer]: A

---

## Q4. hook 脚本的错误/健壮性需求?

- A. hook 脚本**永不阻塞 cc、任何错误 exit 0**(POST 失败也静默放过);接收端 payload 解析用防御性方式(字段缺失/格式变化不 panic,降级或丢弃并 tracing 记录)。接收端只监听 localhost。
- B. hook 失败要重试 / 要让用户知道。
- X. Other

[Answer]: A

---

## Q5. 非功能需求(NFR)目标?

- A. **延迟**:Stop 文字从 cc 答完到手机收到 < ~2s(本地,主要受聊天 API 限);**节流**:工具进度不超过现有 outgoing_ratelimit(每频道节流);**资源**:接收端轻量(本地 HTTP,无持久化);**正确性**:文字内容与 transcript 一致(直读 last_assistant_message,不丢不串);**CJK 安全**:所有文字切片 char-boundary 安全(已是 project 规约)。
- B. 有更严的目标(请说明)。
- X. Other

[Answer]: A

---

## 回答方式

- **Guide me** / **I'll edit the file** / **Chat**

(全部预填 A。Q1/Q2 是 feasibility 留的需求级取舍,值得你确认。)
