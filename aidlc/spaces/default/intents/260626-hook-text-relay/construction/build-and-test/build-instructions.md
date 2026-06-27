# Build Instructions — Hook Text Relay

## Build
```
cargo build            # dev
cargo build --release  # release
```
零新依赖(axum/serde/tokio/anyhow 已在 Cargo.toml)。无 build.rs、无 feature flag。

## Lint
```
cargo clippy --bin agentbridge   # 0 warning(默认 lints)
```

## 运行时依赖(hook 脚本)
- python3(系统自带,仅 stdlib)。Pillow 不再需要(批次3 移除截图后)。

## 启用 hook relay
1. config(~/.agentbridge/config.yaml):tmux backend 设 `hook_relay: true`;可选 `hook_receiver: { port: 9123 }`。
2. 安装 hook:`agentbridge hook-install`(合并写 ~/.claude/settings.json,幂等)。
3. 启动 agentbridge:接收端自动在配置端口监听 localhost。
