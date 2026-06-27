# Security Requirements — Walking Skeleton

> 承自 `requirements.md` NFR-4/NFR-6/NFR-7。devsecops 视角。

## SR-1 接收端仅 localhost(NFR-4)
- hook 接收端绑 `127.0.0.1` only,不监听外部接口。单机单用户,无需鉴权/TLS。
- 风险面:仅本机进程能 POST。本机已是信任边界(用户自己的 Mac)。

## SR-2 门控防串扰(NFR-4,关键)
- C-4 resolve 未命中即丢弃:非桥接 cc 的 hook 绝不发到任何 channel(BR-5)。
- 防止"别的本地 cc 的输出泄漏到我的聊天频道"。

## SR-3 防御性解析(NFR-6)
- 畸形/恶意 payload → 不 panic、不崩溃(serde Option + 200 + warn)。本机来源,主要防意外非攻击。

## SR-4 无密钥处理
- 本 feature 不引入/不存储任何密钥。hook payload 不含敏感信息(就是 cc 的文字/工具名,与现有截图回传的信息面相同)。
- config 的 bot token 等既有密钥不受本 feature 影响。

## devsecops 视角小结
**N/A 项**:无 SAST/DAST 新面、无供应链新依赖(零新 crate)、无 secret-scan 触点、无网络暴露(localhost)。唯一实质安全规则是 SR-2 门控(已是核心功能)。
