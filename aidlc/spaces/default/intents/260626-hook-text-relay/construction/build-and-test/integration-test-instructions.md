# Integration Test Instructions

## Live 端到端(#[ignore],需手动)
```
cargo test --bin agentbridge hook_relay_end_to_end -- --ignored --nocapture
```
真实 axum server + 真实 python 脚本子进程,验证 Stop→Result 投递(含 CJK)+ 门控丢弃 + 脚本 exit 0。

## 真机 live(MVP 验收,手动)
1. tmux 起一个跑 cc 的 session(hook_relay=true 的项目)。
2. `agentbridge hook-install`,启动 agentbridge。
3. Discord 发消息 → cc 答完 → Discord 收到干净文字(非截图)。
4. 验证未桥接目录的 cc 触发 hook 不发消息(门控)。
