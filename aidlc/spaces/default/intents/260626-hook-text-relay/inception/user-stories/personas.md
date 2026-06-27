# Personas — Hook-Based Real-Time Text Relay

## P-1 远程操控者(主 persona,也是唯一)

- **谁**:agentbridge 的作者本人,单用户自部署。
- **场景**:人不在 Mac 前(在手机端,先 Discord 后飞书),Mac 上有一个**正在跑的 cc 会话**(如 nova-bidding 目录)。
- **目标**:像坐在 Mac 终端前一样,远程看到 cc 的文字交互并继续驱动它 —— 不是看一张张截图。
- **痛点**:截图看不清、刷屏、节奏失控、与原生终端体验割裂。
- **技术水平**:高(项目作者),能装全局 hook、能用 tmux,接受"hook 装进 ~/.claude/settings.json"这类设置。

> 单用户工具,无次要 persona。无对外用户、无团队协作面。
