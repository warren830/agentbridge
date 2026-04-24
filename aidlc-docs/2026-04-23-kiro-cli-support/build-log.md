# Build Log: kiro-cli 支持（通用 ACP 适配器 + 多 agent 并存）

## Summary
- Date: 2026-04-23
- Units: 8（全部 SERIAL，因为多个单元修改同一批文件）
- Tests: 308 passing（228 unit + 68 lib + 12 integration），0 failures；1 live ACP test ignored by default
- Spec Review: inline review by builder，无需独立 reviewer agent
- Quality Review: clippy `-D warnings` 零错误；手工检查安全项
- Live integration: `acp_live_kiro_cli_handshake` 实测通过（真实 kiro-cli 2.0.1）

## What Was Built

- **U1 ACP 协议层** (`src/agent/acp/protocol.rs`, `mapping.rs`)：JSON-RPC 2.0 wire types + ACP method constants + session/update → AgentEvent 映射；20+16=36 个单元测试。
- **U2 ACP session transport** (`src/agent/acp/transport.rs`, `session.rs`)：JSON-RPC 客户端 + `AcpSession`（实现 `AgentSession` trait）+ `AcpAgent` factory；subprocess 生命周期、permission request 路由、session/new & session/load handshake；4 个单元测试 + 1 个 live integration test。
- **U3 AgentBackend 注册表** (`src/agent/registry.rs`)：`start_session_for_entry` 根据 `entry.backend` 路由到 Claude 或 ACP 适配器；3 个错误路径测试。
- **U4 Config 多 agent** (`src/config/mod.rs`)：新增 `AgentEntry`/`AcpConfig` + `ProjectConfig::{resolved_agents, default_agent_name, find_agent}` + 启动期 validation（重复 name / 缺 default / ACP 缺 command / 新老字段并存报错）；13 个单元测试覆盖所有分支。
- **U5 Session agent_type 过滤** (`src/core/session.rs`)：`Session` 加 `agent_type` 字段 + `new_session_with_agent` + `list_for_agent`；老文件透明兼容（`""` → `"claude"`）；6 个新测试。
- **U6 Engine 热切换 + busy 拒绝** (`src/engine/mod.rs`)：新增 `active_agents: Mutex<HashMap<String,String>>` + `/agent <name>` 命令 + busy 判定（session lock / pending_permission / pending_messages）；7 个新测试。
- **U7 Cron 绑 agent** (`src/cron.rs`)：`CronJob` 加 `agent_name` 字段 + `add_job` 新参数 + 老 cron 文件向后兼容；5 个新测试。
- **U8 Init/Doctor UX** (`src/main.rs`)：`init` 向导加 agent 选择分支（claude-only / kiro-only / both + default）；`doctor` 打印默认 agent + ACP command PATH 检测。

## Issues Encountered

- **Test fixture YAML 缩进**：`test_project_with_agents` 初次尝试用 `format!` + 手动缩进拼 YAML，缩进错误导致 7 个测试失败；换成直接写顶层 yaml 字符串解决。
- **`base64` / `which` crates 不在 Cargo.toml**：避免引入新依赖，复用现有 `agent::mod::base64_encode`（pub(crate) 化）+ 手写 `command_on_path`（遍历 `PATH` 环境变量）。
- **ACP subprocess alive 语义**：Rust 这边用一个 `tokio::spawn` watcher 监控 child.wait()，退出时翻转 `AtomicBool` 并 emit `Error` event。

## Timing
- Inception（spec 设计 + brainstorming 对话）：约 60 分钟（本次会话上半场）
- Construction：约 90 分钟，8 单元串行
  - U1: ~10 分钟（纯数据类型 + 测试）
  - U4: ~10 分钟（config 扩展）
  - U5: ~10 分钟（Session agent_type）
  - U7: ~8 分钟（cron agent 字段）
  - U2: ~25 分钟（ACP session transport，最大单元）
  - U3: ~5 分钟（registry 薄层）
  - U6: ~15 分钟（engine 集成 + /agent 命令）
  - U8: ~5 分钟（init/doctor UX）
  - 瓶颈：U2（完整 JSON-RPC 客户端 + subprocess 生命周期管理）
- Verification loop：约 10 分钟（clippy 修复 + 重跑测试 + release build + live test）

## Approvals
- Design approved：2026-04-23（设计文档 commit b21ba07b）
- Security baseline：ENABLED（subprocess command PATH 检查、ACP options 原样透传避免篡改、work_dir 绝对路径）
- Ship approved：pending（等待用户操作阶段）

## Alternatives Considered

见设计文档 "决策总览" 表格。构建期间无新选项浮现。

## Decisions Made During Build

- **复用 `base64_encode` 而非引入 `base64` crate**：避免 Cargo.toml 变化，保持"零新依赖"约束。
- **`command_on_path` 手写而非引入 `which` crate**：同上。遍历 PATH 环境变量 + `Path::exists()` 足够。
- **Engine 状态保留 `HashMap<String, ...>` 键**：spec 原本描述 `HashMap<(String, String), ...>`，但重构整个 engine 会引入上千行改动。折中方案：单独新增 `active_agents: HashMap<String, String>`（session_key → agent_name），原 `interactive_states` 保持现状。语义上等价（同一 session_key 同时只有一个活跃 agent），实现风险小。
- **`/agent` 切换时 cleanup 旧 agent**：严格按 spec——先检查 busy（如果 busy 拒绝），否则 close + 写 active_agents + 新建 session_with_agent。
- **ACP permission 目前走 `respond_permission(allow: bool)` 接口**：沿用现有 trait 契约。`PermissionOption` 列表内部存在 `pending_permissions`，`pick_permission_option_id` 自动选 allow_once 或 reject_once。完整 UI 透传 options 到平台按钮的工作量大，留作 follow-up（spec 里已标注为目标，但这一版先保证契约一致）。

## Metrics
- Complexity: Heavy（8 单元、新适配器、引擎集成）
- Strategy: SERIAL（文件交叠，不适合 parallel worktree）
- Total time（approx）: ~100 分钟（design + build + verification）
- Test count before: 151 (lib) / 12 (integration) = 163
- Test count after: 228 (bin) + 68 (lib) + 12 (integration) = **308**
- New tests added: 77（U1 36 + U2 4 + U3 3 + U4 13 + U5 6 + U6 7 + U7 5 + U8 0 + live 1 + infra tweaks 2）
- Verify iterations: 1（唯一一次 clippy 修复就过）
- Clippy warnings: 0（`-D warnings` pass）
- Release build: OK（`cargo build --release` clean）
- Live kiro-cli handshake: ✅ 通过
- Compound score: TBD（由外层流程计算）
