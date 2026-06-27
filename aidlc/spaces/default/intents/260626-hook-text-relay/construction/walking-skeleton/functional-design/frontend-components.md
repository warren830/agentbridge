# Frontend Components — Walking Skeleton

**N/A — 本 feature 无前端/UI。**

agentbridge 是后端桥接服务。本 feature 的"界面"是既有聊天客户端(Discord / 飞书),由既有 `Platform` capability traits 驱动,本 feature 不新增任何 UI 组件、不改任何渲染。

回传的文字消息经既有 `MessageUpdater`/`reply` 路径发出 —— 那是平台适配器既有能力,非本 feature 的前端产物。

(此 CONDITIONAL 产物按 stage 规约在无 UI 时标 N/A,见 functional-design.md Step 5。)
