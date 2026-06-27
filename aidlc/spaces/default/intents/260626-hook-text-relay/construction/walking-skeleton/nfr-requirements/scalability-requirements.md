# Scalability Requirements — Walking Skeleton

**大部分 N/A — 单机单用户工具。**

- 无水平扩展需求:一个 agentbridge 进程、一个用户、少量并发会话。
- 路由表用 `Arc<Mutex<HashMap>>`,绑定数量级 = 用户同时桥接的会话数(个位数),无扩展压力。
- hook 接收端 async(tokio),天然处理少量并发 POST。
- 无数据库扩展、无分片、无负载均衡。

唯一相关边界:`mpsc::channel(128)` 缓冲 —— 若 hook 事件远快于 process_agent_events 消费会满,但实际每轮 Stop 1 个 + 工具进度受节流,128 远够。承自既有 channel 容量(`session.rs:211`)。

(承自 `requirements.md` NFR-3 资源轻量;此 feature 不改变 agentbridge 的扩展特性。)
