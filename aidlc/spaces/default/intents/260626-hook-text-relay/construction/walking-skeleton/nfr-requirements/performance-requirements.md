# Performance Requirements — Walking Skeleton

> 承自 `requirements.md` NFR-1/NFR-2。引用 `business-logic-model.md`、`technology-stack.md`。

## PR-1 延迟(NFR-1)
- Stop 文字从 cc 答完到手机收到 **< 2s(本地 p95)**。
- 路径开销:hook 脚本 POST(localhost,~ms)+ map_hook(纯函数,μs)+ mpsc send(μs)+ 既有 process_agent_events → Platform reply(主要延迟 = 聊天 API)。本地部分可忽略,瓶颈在 Platform API。

## PR-2 消息条数(NFR-2,批次2 U-9 主验)
- 批次1(Stop only):每轮 1 条文字。
- 批次2(PostToolUse):每轮进度消息条数 ≤ 小常数,与工具数 N 解耦(就地编辑 preview)。

## PR-3 接收端开销
- localhost axum,async(tokio),无持久化;每请求 O(1) 路由查找(HashMap 前缀匹配,绑定数极小)。
- hook 脚本短超时(~2s)不阻塞 cc。

## 验收
- live 端到端测 Stop 文字延迟(主观 < 2s);批次2 测 N=1/5/40 消息条数曲线平。
