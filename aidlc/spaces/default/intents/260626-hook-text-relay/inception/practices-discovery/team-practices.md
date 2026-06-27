# Team Practices — agentbridge

> 描述性、团队声音。五区对应 aidlc-team.md 标题。综合 CLAUDE.md + 全库 RE + git/CI 证据 + 确认答案。

## Way of Working

GitHub Flow:功能分支(`feat/*`、`fix/*`,含 `worktree-task-*` 并行分支)→ Pull Request → 合并到 `main`。Commit 主题用祈使句、保持在 72 字符内,采用 conventional-ish 前缀(`feat:`/`fix(scope):`)。`git push` 用 `--no-verify`(全局规则);`git commit` **不**用 `--no-verify`(hook 失败要修根因);提交信息从不加 `Co-Authored-By`。

## Walking Skeleton

按情况(scope-dependent)。对新集成类 feature,先实现最薄的端到端切片以验证架构贯通(本 intent 的 MVP —— 传输层+Stop文字打通 hook→HTTP→Platform —— 即是一例),但不作为团队铁律。引擎对 feature scope 默认 skeleton-on,符合该立场。

## Testing Posture

测试随实现一起写(test-alongside)、单元为主。测试一律 inline `#[cfg(test)] mod tests`,用 stock `#[test]`/`#[tokio::test]` + `tempfile`,无 mock 框架;关键模块密集覆盖(如 session 约 30 个测试)。需真实外部环境的端到端测用 `#[ignore]` 门控(如 hook→HTTP→平台 live 测、ACP/tmux live 测)。声明功能完成前必须 `cargo test` 全绿;非平凡改动后 `cargo check`。绝不用 `--test-threads=1` 掩盖 flaky —— 修竞态根因。

## Deployment

单人自部署:`cargo build` + systemd user-service(`agentbridge daemon` 安装/启停/日志,见 `daemon.rs`)。无 CI/CD 流水线(单人项目,无 `.github/workflows` 等)。部署须注意 daemon 运行环境与 `run` 子命令不同(尤其 proxy 等环境变量在 systemd 下可能静默失效)。

## Code Style

遵循 CLAUDE.md:Rust + tokio,所有热路径 I/O 异步(`tokio::fs`/`process`/`time`,禁 `std::fs`/`std::thread::sleep`);`anyhow::Result` 在应用边界、`thiserror` 在库级模块;测试外不 `unwrap()`/`expect()`;只用 `tracing`(结构化字段),禁 `println!`/`eprintln!`(main.rs CLI 除外);**无 `unsafe`**;注释写 why 不写 what、不加跨项目引用(中文注释仅限 aidlc-docs/);加新依赖前先问、优先复用现有 crate;平台只经 `Platform`/`ReplyCtx` capability traits、引擎不按平台名分支;`engine/mod.rs` 保持只做路由,拆 commands/events/skills。无 rustfmt/clippy 配置文件(用默认)。
