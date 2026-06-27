# Decision Log — IDEATION Phase

> 记录 IDEATION 阶段所有关键决策。

| # | 决策 | 阶段 | 理由 |
|---|------|------|------|
| D-1 | scope = feature(Standard 深度) | intent-capture | 多新模块 + 改动现有架构,非纯重构 |
| D-2 | 用 hook 实时文字替代截图(非保留双轨/非 SDK 重写) | intent-capture | hook 保留"同一个 cc" + 给干净结构化文字;SDK 重写会牺牲同-cc 属性 |
| D-3 | 跳过 market-research | scope routing | 单人内部工具无商业市场;build-vs-reuse 结论已记于 intent-statement |
| D-4 | 传输 = localhost HTTP(复用 axum),非 socket/文件 | feasibility | 进程间、语言无关、零新依赖 |
| D-5 | 门控在 agentbridge 侧(session→channel 绑定),hook 保持薄 | feasibility | hook 无状态、可移植;路由逻辑集中在引擎 |
| D-6 | PostToolUse 必须节流(沉淀为 project 约束) | feasibility / scope | 防止重现"截图刷屏";有 tool_name+duration_ms 可做判断 |
| D-7 | MVP = 传输层 + Stop 文字;PostToolUse 第二批;移除截图最后 | scope-definition | 风险优先(先打通传输)+ 避免空窗(最后删截图) |
| D-8 | 替换型改造"删旧机制放最后一批"沉淀为 project practice | scope-definition | 复用价值:临时双轨直到新路验证 |
| D-9 | 无关旧改动(proxy/attach/resume)范围外,留到 construction/ship | scope-definition | 聚焦本 feature;截图相关旧改动由移除批次覆盖 |
| D-10 | 跳过 team-formation + rough-mockups | scope routing | 单人项目无团队;纯后端无 UI |
| D-11 | Go,进入 INCEPTION | approval-handoff | 技术已验证、风险有缓解、范围清晰 |
