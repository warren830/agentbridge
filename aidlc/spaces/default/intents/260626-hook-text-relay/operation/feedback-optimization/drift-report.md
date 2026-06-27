# Drift Report — Hook Text Relay

**基础设施漂移 N/A**(无 IaC、无云资源)。

代码层面漂移防护(已设计):
- codekb(reverse-engineering @25c8c3a)在本 feature 大改 tmux backend 后应刷新。
- 派发逻辑:C-7 取消后只有 events.rs 一份派发(无双份漂移,M-4 已解)。
- hook payload 格式漂移:U-1 防御性解析兜底。
