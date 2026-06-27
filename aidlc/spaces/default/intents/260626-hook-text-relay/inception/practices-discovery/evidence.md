# Evidence — Practices Discovery

> 各视角发现汇总 + freshness trail。

## 扫描方式(deviation note)

未派 4 个并行 subagent(stage prose 的默认)。理由:CLAUDE.md 已明文写死编码/错误/测试/架构约束,且本 intent 刚完成全库 RE。conductor 自行补齐唯一缺口(分支/CI/lint 证据),直接综合。见 memory.md Deviations。

## pipeline-deploy 视角(分支/部署)

- 分支:`feat/tmux-backend`、`worktree-task-03-observability` → PR(#1)合并到 main。GitHub Flow。
- Commit:祈使句、conventional-ish(`feat:`/`fix(scope):`)、<72 字符。
- CI:无(无 `.github/workflows`、`.gitlab-ci.yml`、`Jenkinsfile`、`.circleci`)。
- 部署:`daemon.rs` systemd user-service。单人自部署。

## quality 视角(测试)

- 全 inline `#[cfg(test)] mod tests`;stock `#[test]`/`#[tokio::test]` + tempfile;无 mock 库。
- 重覆盖:core/session(~30)、agent/mod、tmux/session、skills、config、engine。
- live 测 `#[ignore]` 门控(tmux_live_screenshot、ACP 两个)。
- 无覆盖率插桩。CLAUDE.md:完成前 cargo test 全绿、不用 --test-threads=1。

## developer 视角(编码/架构边界)

- anyhow 应用边界 / thiserror 库级;无 unsafe;hot-path 用 ?。
- capability traits 解耦平台;AgentSession 解耦 agent;core 不依赖 agent/platform。
- session try-lock+队列(load-bearing)。StreamPreview CJK 字节切片风险(已记 codekb)。

## devsecops 视角(lint/安全/供应链)

- 无 rustfmt/clippy 配置文件(用默认)。
- 无 SAST/DAST/secret-scan/dependabot(单人项目)。
- 配置 mode 0600(~/.agentbridge/config.yaml 存 bot token)。

## Freshness

来源:CLAUDE.md(in-context)+ codekb(commit 25c8c3a 全库 RE)+ git/CI 自查(2026-06-26)。
