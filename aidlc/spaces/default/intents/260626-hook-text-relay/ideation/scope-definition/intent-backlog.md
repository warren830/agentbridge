# Intent Backlog — Hook-Based Real-Time Text Relay

> Proto-units(后续 units-generation 会正式化)。优先级 MoSCoW;排序 = 风险优先 + 依赖优先。引用 `scope-document.md` 的 MVP 边界。

## Prioritized Proto-Units

| # | Proto-Unit | MoSCoW | 批次 | 依赖 | 风险/价值理由 |
|---|-----------|--------|------|------|---------------|
| PU-1 | **传输层**:薄 hook 脚本(Stop 先行)+ localhost HTTP 接收端(axum)+ payload 反序列化 | Must | 批次 1(MVP) | 无 | 最不确定的新集成面,先打通验证 |
| PU-2 | **session→channel 绑定 + 门控**:agentbridge 登记桥接会话,hook 按 session_id/cwd 路由,查不到即丢 | Must | 批次 1(MVP) | PU-1 | MVP 必需(否则不知道发给谁 / 误触其他 cc) |
| PU-3 | **Stop 文字回传**:`last_assistant_message` → `AgentEvent::Text` → Platform 发出 | Must | 批次 1(MVP) | PU-1, PU-2 | **MVP 核心价值** —— 每轮一条干净文字取代截图 |
| PU-4 | **PostToolUse 进度 + 节流/合并**:工具事件 → 节流 → 进度事件 → Platform | Must | 批次 2 | PU-1, PU-2 | 用户明确要的逐工具实时;依赖传输先通 + 需节流(R-1) |
| PU-5 | **移除截图路径**:删 `render_screenshot` / 截图 poll / `render_term.py` | Must | 批次 3 | PU-3 验证通过 | 放最后,避免"截图已删、文字未通"空窗(I-2) |

## MVP = 批次 1(PU-1 + PU-2 + PU-3)

达成后:手机端每轮收到干净文字回复。这是可独立验证、可独立交付的最小端到端切片(walking skeleton 性质:打通了 hook→HTTP→Platform 全链路)。

## 依赖 DAG(拓扑,仅形状;经济路径由 delivery-planning 定)

```
PU-1 (传输层)
  |
  +--> PU-2 (门控)
  |      |
  +------+--> PU-3 (Stop 文字) [MVP 完成]
                |
                +--> PU-4 (PostToolUse + 节流)
                |
                +--> PU-5 (移除截图)
```
<!-- Text fallback: PU-1 传输层是根;PU-2 门控依赖 PU-1;PU-3 Stop文字依赖 PU-1+PU-2,完成即 MVP;PU-4 PostToolUse 和 PU-5 移除截图都依赖 PU-3。PU-4 与 PU-5 互不依赖,可并行或任意序。 -->

## 非本 backlog(范围外,仅标记)

- 逐字流式、远程权限审批、飞书适配器实现 —— out of scope。
- proxy/attach/resume 等无关旧改动 —— 留到 construction/ship 决定(RAID I-1)。
