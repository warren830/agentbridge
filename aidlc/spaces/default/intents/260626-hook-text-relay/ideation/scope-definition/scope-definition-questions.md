# Scope Definition Questions — Hook-Based Real-Time Text Relay

> 范围定义:把"用 hook 替代截图"切成 in/out 边界 + 有优先级的 backlog。已基于 intent-statement 和 feasibility 预填。

---

## Q1. 交付价值的最小可行范围(MVP 边界)是什么?

- A. MVP = **传输层 + Stop 文字回传**:hook 脚本(带门控)→ localhost HTTP 接收端 → `AgentEvent::Text` → 现有 Platform 管线发出。达成后,手机端每轮能收到一条干净文字回复(取代截图的核心价值)。PostToolUse 进度、截图移除放 MVP 之后。
- B. MVP 必须**一次性**包含 Stop + PostToolUse + 截图移除(全做完才算)。
- C. MVP 更小:只验证 hook→HTTP 能通(打印到日志),先不接管道。
- X. Other (please specify)

[Answer]: A

---

## Q2. PostToolUse 逐工具进度算 must-have 还是 nice-to-have?

- A. **Must-have,但在 MVP 之后的第二批**。它是你明确要的(逐工具实时进度),但依赖传输层先通、且需要节流策略;所以排在 Stop 文字之后单独成一个 unit。不砍,只是排序。
- B. Nice-to-have,可以以后再说。
- C. 跟 Stop 一起进 MVP(同批做)。
- X. Other (please specify)

[Answer]: A

---

## Q3. 移除截图代码(render_screenshot/poll/Pillow)什么时候做?

- A. **最后一批**。等 hook 文字回路验证可用后再移除,避免中途出现"截图已删、文字还没通"的空窗。移除前 = 双轨临时并存(截图仍在,文字已加);验证通过后一次性删截图路径。
- B. 第一批就删(破釜沉舟,逼自己把文字做通)。
- C. 永久保留截图作为可选 fallback(配置开关)。
- X. Other (please specify)

[Answer]: A

---

## Q4. backlog 排序偏好?

- A. **风险优先 + 依赖优先**混合:①传输层(最不确定的新集成面,先验证)→ ②Stop 文字(MVP 核心价值)→ ③PostToolUse + 节流 → ④移除截图。
- B. 价值优先(先做用户最快能感知的)。
- C. 我有别的顺序偏好(请说明)。
- X. Other (please specify)

[Answer]: A

---

## Q5. 工作区里那一大堆未提交的旧改动(截图模式/proxy/attach/resume)怎么处理 —— 算这次范围内吗?

- A. **范围外,但需先收束**:本 feature 聚焦 hook 文字回传。旧的截图相关改动会被本 feature 的"移除截图"那批覆盖;proxy/attach/resume 等与本 feature 无关的改动,留到 construction/ship 阶段单独决定(提交、丢弃、还是另开 intent)。范围定义只**标记**这个 issue(已在 RAID I-1),不在这里解决。
- B. 算范围内:本 feature 要顺带把工作区清干净。
- C. 先把旧改动全提交了再开始本 feature(前置动作)。
- X. Other (please specify)

[Answer]: A

---

## 回答方式

- **Guide me** / **I'll edit the file** / **Chat**

(全部预填 A。认同就说"全部确认";要改告诉我题号。)
