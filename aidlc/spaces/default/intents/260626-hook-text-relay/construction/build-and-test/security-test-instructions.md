# Security Test Instructions

## SR-1 localhost only
验证接收端只绑 127.0.0.1(代码 hook_receiver::start 绑 127.0.0.1;非外部接口)。

## SR-2 门控(关键)
单测 + e2e 已验:未绑定 cwd 的 hook 被丢弃,不发任何消息。手动:另起一个非桥接目录的 cc,触发 hook,确认 Discord 无消息。

## SR-3 防御性解析
畸形 payload 不 panic(全 Option serde + 永远 200)。

## 无新攻击面
零新依赖(无供应链新面)、无密钥处理、localhost 无网络暴露。
