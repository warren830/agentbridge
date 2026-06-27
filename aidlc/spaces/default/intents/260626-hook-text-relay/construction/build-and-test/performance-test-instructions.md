# Performance Test Instructions

## NFR-1 延迟(< 2s p95,本地)
手动:Discord 发消息计时到收到 Stop 文字。本地路径(POST+mpsc+既有管线)开销 ms 级,瓶颈在 Platform API。

## NFR-2 消息条数(批次2 主验)
批次1:每轮 1 条文字。批次2(U-9)需测 N=1/5/40 工具轮新消息条数 ≤ 小常数(就地编辑)。

## 已知限制
Stop-only 长轮 >300s 可能触 idle timeout(静默保活留批次)。
