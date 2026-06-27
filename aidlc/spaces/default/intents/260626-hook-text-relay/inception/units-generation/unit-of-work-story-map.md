# Unit of Work → Story Map — Hook-Based Real-Time Text Relay

> Units ⇄ User Stories ⇄ FR ⇄ scope proto-units 的双向追溯。

| Unit | User Stories | FR | proto-unit | 批次 |
|------|--------------|----|-----------|----|
| U-1 映射 | US-1 | FR-2.2, FR-4, FR-5, ADR-3 | PU-1 | MVP |
| U-2 路由表 | US-2 | FR-3 | PU-2 | MVP |
| U-3 接收端 | US-1 | FR-2 | PU-1 | MVP |
| U-4 引擎集成 | US-2, US-4 | FR-3, ADR-1 | PU-1/PU-2 | MVP |
| U-5 poll 门控 | US-6(cutover 期) | FR-6(部分), ADR-5 | PU-5 前置 | MVP |
| U-6 Stop 端到端 | US-1, US-2, US-4, US-5 | FR-4 | PU-3 | MVP |
| U-7 hook 脚本 | US-1, US-4 | FR-1 | PU-1 | MVP |
| U-8 安装器 | US-5 | FR-7 | PU-1/PU-2 | MVP |
| U-9 PostToolUse 进度 | US-3 | FR-5, NFR-2 | PU-4 | 批次2 |
| U-10 移除截图 | US-6 | FR-6 | PU-5 | 批次3 |

## 覆盖核对

- **每个 US 都有 unit**:US-1→U-1/3/6/7、US-2→U-2/4、US-3→U-9、US-4→U-4/6/7、US-5→U-6/8、US-6→U-5/10。✅ 无孤儿 US。
- **每个 FR 都有 unit**:FR-1→U-7、FR-2→U-1/3、FR-3→U-2/4、FR-4→U-6、FR-5→U-9、FR-6→U-5/10、FR-7→U-8。✅ 无孤儿 FR。
- **每个 unit 都有 US/FR 支撑**:✅ 无无源 unit。

## MVP 验收映射(U-6 收口时验)

| 成功指标(intent-statement) | 验证 unit |
|------|-----------|
| ①手机发消息→敲进正在跑的 cc | U-4(send-keys 既有路径不变)+ U-6 |
| ②每轮一条干净文字(非截图) | U-1/U-3/U-6(Stop→Result→reply) |
| ④同一个 cc(带配置) | U-4/U-8(接管既有 cc + 门控) |
| ⑤截图代码移除 | U-10(批次3) |
| ③逐工具进度 | U-9(批次2) |
| ⑥无刷屏 | U-9(消息条数与 N 解耦) |
