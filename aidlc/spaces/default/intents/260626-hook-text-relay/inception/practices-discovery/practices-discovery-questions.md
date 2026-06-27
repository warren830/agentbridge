# Practices Discovery Questions — agentbridge

> 五个实践区。证据来自:CLAUDE.md 明文约束 + 全库 RE + git/CI 自查。多数已能从证据确定(预填),只有 Walking Skeleton 立场和 Testing Posture 需你判断(代码里看不出团队意图)。
> 确认后会首次填充 team.md 五区,并把硬约束提升到 project.md ## Mandated/Forbidden。

---

## Q1. Way of Working(协作/分支方式)?

证据:`feat/` `fix(scope):` 分支 + PR 合并(#1 from feat/tmux-backend)、有 worktree-task 分支、conventional-ish commit(祈使句、<72 字符,CLAUDE.md 也明文要求)。

- A. GitHub Flow:feature 分支(`feat/*`、`fix/*`)→ PR → 合并到 main。commit 主题祈使句、<72 字符。push 用 `--no-verify`(全局规则),commit **不**用 `--no-verify`(项目规则)。不加 Co-Authored-By。
- B. 别的(请说明)。
- X. Other

[Answer]: A

---

## Q2. Walking Skeleton 立场?(代码里看不出,需你判断 —— 这会影响 construction 第一个 bolt 的 gate)

本 intent 的 MVP 本身就是个 walking skeleton(传输层+Stop文字,打通 hook→HTTP→Platform 全链路最薄切片)。

- A. **scope-dependent / 按情况**:对本 feature 这种新集成,先做最薄端到端切片验证(本 intent 的 MVP 即是);但不作为团队铁律。→ 引擎按 scope 默认(feature=skeleton-on)。
- B. always:每个 greenfield feature 都先走 walking skeleton。
- C. never:不搞 walking skeleton 仪式。
- X. Other

[Answer]: A

---

## Q3. Testing Posture(测试姿态)?

证据:全部 inline `#[cfg(test)] mod tests`,stock `#[test]`/`#[tokio::test]` + tempfile,无 mock 库;重模块覆盖密(session ~30 测);live 测用 `#[ignore]` 门控;CLAUDE.md:非平凡改动后 `cargo check`,声明完成前 `cargo test`,不用 `--test-threads=1` 掩盖 flaky,修竞态根因。

- A. **测试随写(test-alongside)+ 单元为主**:inline 单元测试紧跟实现;关键模块密集覆盖;需真实环境的端到端测用 `#[ignore]` 门控(如 hook→HTTP→平台 live 测)。声明完成前必须 `cargo test` 全绿。不靠单线程掩盖竞态。
- B. 严格 TDD(先写测试再写实现)。
- C. BDD(feature 文件驱动)。
- X. Other

[Answer]: A

---

## Q4. Deployment(部署)?

证据:`daemon.rs` 提供 systemd user-service 安装;无 CI 配置文件;单人自部署。memory 记录 [[proxy-daemon-breakage]](env-var proxy 在 systemd daemon 下静默失效)。

- A. 单人自部署:`cargo build` + systemd user-service(`agentbridge daemon`)。无 CI/CD 流水线(单人项目)。部署注意 daemon 环境与 `run` 不同(proxy env 等)。
- B. 别的(请说明)。
- X. Other

[Answer]: A

---

## Q5. Code Style(超出 linter 的团队约定)?

证据全部来自 CLAUDE.md(已明文)+ 代码观察:无 rustfmt/clippy 配置文件(用默认)。

- A. 按 CLAUDE.md:Rust+tokio 异步热路径(`tokio::fs`/`process`/`time`,禁 `std::fs`/`thread::sleep`);`anyhow` 在应用边界、`thiserror` 库级;测试外不 `unwrap()`/`expect()`;只用 `tracing` 结构化字段、禁 `println!`/`eprintln!`;**无 `unsafe`**;注释写 why 不写 what、不加跨项目引用;加依赖先问;平台只经 capability traits、引擎不 `if name==`;session try-lock+队列不可换阻塞 mutex/无界 channel。
- B. 还有别的约定(请补充)。
- X. Other

[Answer]: A

---

## 回答方式

- **Guide me** / **I'll edit the file** / **Chat**

(全部预填 A。重点确认 Q2/Q3 是你的真实意图。认同就说"全部确认"。)
