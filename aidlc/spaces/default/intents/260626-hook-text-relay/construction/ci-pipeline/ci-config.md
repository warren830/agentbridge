# CI Config — Hook Text Relay

**N/A — 单人项目,无 CI/CD 流水线**(practices-discovery 证据:无 .github/workflows、.gitlab-ci.yml、Jenkinsfile、.circleci)。

质量保证 = 本地门(team.md Deployment / Testing Posture):
- 提交前:`cargo check` + `cargo test` 全绿 + `cargo clippy` 0 warning。
- 关键改动:live 端到端自测(本 feature 已做)。

若未来引入 CI,建议最小 GitHub Actions:`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test`。本 feature 不引入。
