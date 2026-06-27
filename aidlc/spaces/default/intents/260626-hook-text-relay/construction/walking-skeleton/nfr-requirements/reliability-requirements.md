# Reliability Requirements — Walking Skeleton

> 承自 `requirements.md` NFR-6。

## RR-1 hook 永不阻塞 cc(关键)
- hook 脚本任何失败 → exit 0(BR-10/FR-1.4)。cc 主流程绝不因 hook/agentbridge 故障而卡。

## RR-2 接收端容错
- 畸形 payload、未命中路由、channel 满/关 → 各自静默降级(200 / 丢弃 / drop),不 panic、不崩接收端(BR-7/8)。

## RR-3 死会话 / 清理
- C-4 在 cleanup_agent_session 注销绑定;event_tx clone 导致的死会话检测退化由 idle-timeout(300s)兜底(ADR-1 已记可接受)。

## RR-4 idle 保活不误杀长轮(M-1)
- U-5b 静默保活防 >300s 长轮误触 idle timeout(BR-14)。

## RR-5 尽力投递语义
- hook 文字是"尽力投递":POST 失败/channel 满 → 该轮文字可能丢失,但 cc 与 agentbridge 都不崩。可接受(非关键数据,用户可在 cc 端看到)。

## 验收
- 杀接收端 → cc 不卡、hook exit 0(RR-1)。
- 喂畸形 payload → 接收端不崩、记 warn(RR-2)。
- >300s 长轮 → 不误触 idle(RR-4)。
