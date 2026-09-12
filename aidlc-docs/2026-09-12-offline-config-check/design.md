# Design: 离线 config 校验 (`agentbridge config check` + `--json`)

Issue: warren830/agentbridge#3

## Requirements

### 目标
提供一个**纯离线**的 config 校验命令，让 CI、pre-commit hook 和部署脚本能在不接触
宿主机环境的情况下回答一个问题：**这份 config.yaml 能不能被 agentbridge 加载？**
校验结论用退出码表达，诊断信息用一个稳定的 JSON 对象表达。

### 功能需求
- **`agentbridge --config PATH config check`**：文本输出，人读。
- **`--json`**：stdout 恰好一个 JSON 对象（合法/非法两种结果都是），字段固定、schema 有版本号。
- **复用 `config::load`/`validate` 语义**：不新增第二套规则，legacy `agent:` 与多 agent
  `agents:` 两种形态都按启动时的同一套规则判定，避免 "check 过了但启动失败"。
- **退出码**：0 = 合法；1 = 任何 config 错误（文件缺失/不可读、YAML 语法错、字段类型错、语义错）。
- **零副作用**：不联网、不查 PATH 上的 agent 可执行文件、不写文件、不起后台服务、不抢单实例锁。
- **诊断不泄漏配置值**：错误里只有类别 + 结构化位置，不含 token、密钥、项目名、路径值。

### 非目标
- **不替代 `doctor`**：`doctor` 检查宿主机（`claude` 在不在 PATH、ACP command 可达性、
  hook 装没装）；`config check` 只看文件本身。两者定位不同，都保留。
- **不做 schema 建议/自动修复**：只报第一个问题，不给 "你是不是想写 X" 之类的猜测。
- **不做多错误聚合**：沿用 `validate` 的 fail-fast 语义（第一个问题即返回），
  避免为了聚合而改动启动路径的行为。
- **不校验业务可达性**：dummy token、宿主机上不存在的 `work_dir`、没装的 agent
  可执行文件都必须**通过**校验 —— 这正是它能在 CI 里跑的前提。
- **不新增依赖**：`serde`/`serde_yaml`/`serde_json`/`clap` 都已在 tree 里。

**Security baseline**: ENABLED

---

## Architecture

### 改动范围

```
src/
  config/
    mod.rs          # 修改：validate() 委托给 validation；挂 check/validation 两个子模块
    validation.rs   # 新增：结构化语义校验（IssueKind + ValidationIssue）
    check.rs        # 新增：CheckReport / CheckError / CheckErrorKind + check()
  main.rs           # 修改：Commands::Config { Check { json } } + run_config_check
README.md           # 修改：CLI 表 + "Offline config check"（schema/退出码/字段表）
```

### 组件单元

- **U1 结构化校验**（`src/config/validation.rs`）
  把原先散在 `mod.rs` 里的 `validate`/`validate_agents` 规则原样搬过来，返回
  `Result<(), ValidationIssue>` 而不是 `anyhow::Error`。`ValidationIssue` 同时携带两级
  披露内容：
  - `kind: IssueKind` + `location: String` —— 可公开；`location` 只由字段名和下标拼成
    （`projects[0].agents[1].acp`）。
  - `detail: String` —— 私有字段，只能通过 `detail()` 取；就是**原来那句**含项目名/agent 名的
    人读消息，给 `config::load` 用。

- **U2 报告类型 + 检查流程**（`src/config/check.rs`）
  `check(path) -> CheckReport`（返回报告而非 `Result`：合法与非法都是检查的正常产出，
  由调用方把 `valid` 翻成退出码）。三步：`read_to_string` → `serde_yaml::from_str`
  → `validation::validate_config`。`CheckErrorKind` 是唯一的对外类别枚举：
  三个 I/O/解析类 + 九个语义类（`From<IssueKind>` 显式映射，加变体时编译器会逼着补全）。
  渲染函数 `to_json()` / `to_text()` 返回 String，不打印 —— 库层不 `println!`。

- **U3 CLI 接线**（`src/main.rs`）
  `config` 子命令 + `check` 动作 + `--json` flag。`--config` 本来就是 `global = true`，
  所以 `--config PATH config check` 和 `config check --config PATH` 都能用。
  `run_config_check` 是同步 fn（全程只有一次文件读，没有 async 工作），打印后
  `std::process::exit(1)`（与 `run_sync`/`run_relay` 的既有做法一致）。

### 数据流

```
CLI --config PATH config check [--json]
      │
      ▼
config::check::check(Some(path))
      │  std::fs::read_to_string          → Err(NotFound)  → config_not_found
      │                                   → Err(其它)      → config_unreadable
      │  serde_yaml::from_str::<AppConfig> → Err           → invalid_yaml (仅 line/column)
      │  validation::validate_config       → Err(issue)    → issue.kind → CheckErrorKind
      ▼
CheckReport { schema, valid, config_path, projects, error }
      │
      ├─ --json → to_json() → stdout 单个 JSON 对象
      └─ 默认   → to_text() → 人读三/五行
      ▼
exit 0 (valid) / exit 1 (invalid)
```

### 为什么把规则搬出 `mod.rs`

`load` 与 `check` 必须给出**同一个结论**，但**不同的措辞**。若各写一套判断，两边迟早漂移；
若 `check` 直接复用 `load` 返回的 `anyhow` 字符串，就会把 `Project 'x': duplicate agent
name 'y'` 这类值原样喷进机器可读输出。中间层 `ValidationIssue` 同时解决这两点：结论只有
一份（`validate` 现在只是它的一个渲染器），值只走人读那条路。

### JSON schema（`agentbridge.config-check.v1`）

```json
{ "schema": "...", "valid": true,  "config_path": "...", "projects": 2, "error": null }
{ "schema": "...", "valid": false, "config_path": "...", "projects": 1,
  "error": { "kind": "duplicate_agent_name",
             "location": "projects[0].agents[1].name",
             "message": "duplicate agent name within a project" } }
```

- 五个 key 恒定存在，`error` 用 `null` 表示成功 —— 消费者不必按结果分支挑字段。
- `config_path` 来自**调用方**（`--config` 或默认路径），不是文件内容，因此可以回显。
- `projects` 是解析出的项目数；文件读不到/解析不了时为 0。
- `kind` 是 snake_case 闭集，`message` 是每个 kind 的固定句子（无插值）。
- `location` 只回答"在哪"：解析错给 `line N, column M`，语义错给结构路径，无位置时 `null`。
- 字段语义变更要 bump `schema` 版本号，消费者应该 pin 它。

---

## NFR Plan

### Medium -- quick scan
- [x] 响应时间：一次文件读 + 一次 YAML 解析，无网络无 subprocess，毫秒级
- [x] 并发：无共享状态，不取单实例锁（可与运行中的 bridge 同时跑）
- [x] 数据保留：只读，不产生任何文件
- [x] 日志：本路径不发 tracing 事件 —— 保证 stdout 只有那一个 JSON 对象
- [x] 缓存：无需

### Security
- **值泄漏是主要威胁模型**：这份报告的用途就是被 CI 日志、dashboard、bot 消息转载，
  所以任何来自 config 文件内部的字符串都不许进入输出。
  - `serde_yaml` 的报错本身会引用出错的标量（`invalid type: string "not-a-port"`），
    因此**丢弃整条 parser message**，只留 `location()` 的行列号。
  - 语义错的固定句子由 `IssueKind` 提供，`message` 全是 `&'static str`，构造上无法插值。
  - `location` 只由字段名和 `usize` 下标拼接。
- **不做可执行文件发现**：不 `which`、不 spawn，避免把"校验一份 config"变成执行宿主机上
  由 config 指定的命令。
- **只读一个文件**：不跟随 include、不解析相对路径引用，攻击面就是那一次 `read_to_string`。
- **`read_to_string` 一次搞定"存在"与"可读"**：不用 `exists()` 预判，杜绝 TOCTOU 式的
  "报告说在、实际读不到"。

---

## Error/Rescue Map

| 什么会失败 | `error.kind` | 所属单元 | 系统行为 | 用户感知 |
|-----------|--------------|---------|---------|---------|
| `--config` 指的文件不存在 | `config_not_found` | U2 | 报告 valid=false，退出 1 | 打印类别，附检查过的路径 |
| 无权限 / 路径是目录 / 非 UTF-8 | `config_unreadable` | U2 | 同上 | 同上（不回显 io error 文本） |
| YAML 语法错 | `invalid_yaml` | U2 | 同上，`location` 给行列 | "not valid YAML or wrong field type" + 行列 |
| 字段类型错（如 `port: "abc"`） | `invalid_yaml` | U2 | 同上 | 同上；**不回显 `"abc"`** |
| `projects` 为空 | `no_projects` | U1 | 同上 | 类别 + `projects` |
| 项目名为空串 | `empty_project_name` | U1 | 同上 | 类别 + `projects[i].name` |
| 项目无 platforms | `no_platforms` | U1 | 同上 | 类别 + `projects[i].platforms` |
| 同时有 `agent:` 与 `agents:` | `agent_and_agents_conflict` | U1 | 同上 | 类别 + `projects[i]` |
| agent name 为空串 | `empty_agent_name` | U1 | 同上 | 类别 + `projects[i].agents[j].name` |
| agents 里 name 重复 | `duplicate_agent_name` | U1 | 同上 | 类别 + 重复项的下标 |
| `backend: acp` 缺 `acp:` | `missing_acp_config` | U1 | 同上 | 类别 + `projects[i].agents[j].acp` |
| `backend: tmux` 缺 `tmux:` | `missing_tmux_config` | U1 | 同上 | 类别 + `...tmux` |
| `default_agent` 不在 agents 里 | `unknown_default_agent` | U1 | 同上 | 类别 + `projects[i].default_agent` |
| 报告序列化失败（理论不可达） | — | U3 | `to_json()` 返回 `Err`，main 以 anyhow 冒泡 | stderr 一行 `Error:`，退出 1 |

---

## 测试策略

### 单元测试
- **U1**（`config/validation.rs`）：合法多 agent 配置无 issue；`kind` + `location` 精确到下标
  （含第二个项目、缺 tmux 段）；同一 issue 的 `detail()` 含名字而 `safe_message()`/`location` 不含。
- **U2**（`config/check.rs`）：
  - 合法：legacy `agent:` 配置（dummy token + 不存在的 `work_dir` + 不存在的 agent 命令）、
    多 agent 配置（claude + acp + tmux，两个项目）。
  - 非法：文件缺失、路径是目录、YAML 语法错、字段类型错、空 projects、重复 agent name、
    未知 `default_agent`、缺 `acp:`、缺 `tmux:`。
  - 泄漏防线：含 token / 密码 / 项目名 / agent 名 / 私有路径的配置在 JSON **与**文本两种渲染
    里都搜不到这些串；字段类型错的报告里搜不到那个非法标量。
  - schema 契约：合法与非法两种报告都恰好 5 个 key、error 恰好 3 个 key、`kind`/`location`
    取值精确；JSON 往返相等；文本渲染写明结论。
  - 与 loader 一致：同一份文件 `check().valid` 与 `load().is_ok()` 同向。
- **U3**（`main.rs::cli_tests`）：`--config PATH config check` 解析出 config 路径且 `json`
  默认关；`--json` 置位；`--config` 放在子命令之后也认；`config` 缺动作/给未知动作报错。

### 手动验证清单
- [x] `--config good.yaml config check` → 文本 `config check: ok`，`echo $?` = 0
- [x] `--config good.yaml config check --json` → 单行 JSON，`valid: true`，退出 0
- [x] `--config bad-type.yaml config check --json` → `invalid_yaml` + 行列，无非法标量，退出 1
- [x] `--config absent.yaml config check --json` → `config_not_found`，退出 1
- [x] 非法情况下 stdout 仍恰好一个对象（`wc -l` = 1）

---

## 向后兼容矩阵

| 升级前状态 | 升级后行为 |
|---|---|
| 现有 `config::load` 的报错文案 | 逐字不变（`validate` 仍渲染同一句），既有单测断言全部保留 |
| 现有 `validate` 的判定顺序 | 不变（规则原样搬迁，fail-fast 顺序一致） |
| 只有 legacy `agent:` 的 config | `config check` 判定合法，与启动一致 |
| 已有 `doctor` 用法 | 不变；`config check` 是新增命令，不改 `doctor` 输出 |

---

## 开放问题（非 blocker）

- **多错误聚合**：目前只报第一个问题。要一次列全，得让 `validate_config` 返回
  `Vec<ValidationIssue>` 并给报告加 `errors` 数组 —— 那会同时改动启动路径的语义，
  留给后续 spec（届时 bump `schema` 到 v2）。
- **`doctor --json`**：宿主机检查也值得机器可读输出，可以复用这里的 report 模式，
  但字段集完全不同（PATH、端口占用、hook 状态），不塞进 `config-check` schema。
- **`init` 生成后自动 check**：`init` 向导写完文件后跑一次 check 会更闭环，本次不做。
