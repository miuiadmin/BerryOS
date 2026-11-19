# berry-lifecycle —— 生命周期三口纯函数底座（app-2）

BerryOS 应用生命周期回调三口（规范 §4.1-5：注册 pause 回调 / 写 state /
读 state）的纯函数底座，`docs/app-2实施设计草案-v1.0.md` §2.2/§3 立法：

- **32B blob 容器头 v1**：magic `BTMB` / ver=1 / hlen=32 / counter /
  payload_len / crc32（§3.1 布局）；读侧按 hlen 分派（v0=12B 旧档兼容读、
  v1=32B 正路径、坏档四型 → BadFmt，§3.2 兼容矩阵）。
- **CRC32**：bitwise 实现（poly `0xEDB88320`），`tools/berry/src/png.rs`
  `Crc32` 同算法镜像——两处单源问题以 host 测向量钉板
  （`"123456789" → 0xCBF43926` + 金件 blob 逐字节快照）。
- **8B 帧编解码**：帧⇄(tag, counter)——tag u8 + pad 3B 恒 0 +
  counter u32 LE，与 svcmgr `tomb_selftest` 的 wire 面逐字节同源。
- **counter 代持状态机**：Restored(N)→N+1 / beat→+1 / save→写头取当拍值。

syscall 胶水面（句柄获取 / channel·VMO 收发 / 幕序编排）在 berry-libc
`lifecycle.rs`；本 crate 零依赖 no_std、host 可测（berry-tls/berry-signal
同款「纯面外迁」形态）。
