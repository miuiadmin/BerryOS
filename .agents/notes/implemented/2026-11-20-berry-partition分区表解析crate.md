# berry-partition：GPT/MBR 分区表解析 crate（存储 P0-2 刀 1）

Status: implemented

## Problem

[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md) P0-2 定谳「块设备栈太浅」的第一断层：全仓无分区表解析器——fsmgr 的卷定位是约定式探测（{LBA0, LBA16} 试 BPB 签名 + hidden 对账），vblk 把整盘当一个分区暴露，学生做不了「分区 → 格式化 → 多文件系统对照」这条最经典的存储实验线。刀谱裁决：解析器先行（纯函数 crate），vblk 消费面下一刀接线——本刀纯新增不动消费面。

## Decision

**方案 = 新增 `user/berry-partition`（no_std 纯函数 crate，「解析器即产品」家族——berry-fatfs/berry-blkproto 同款形态）**：

1. **读盘注入**：`parse(block_reader, block_size)` 收 `impl FnMut(u64, &mut [u8]) -> Result<()>` 闭包而非盘句柄——库零 IO 零 syscall 面，host 测试用内存镜像闭包全验（none-target 全量构建可编、musl 消费侧 alloc 由 std 提供）。块缓冲走 alloc Vec（512B..64KiB 帽——栈外单块驻留）。
2. **MBR 面**：LBA0 尾 0x55AA 失配即 `BadMbrSignature` loud 拒；4×16B 槽位（偏移 446）type=0 跳空槽、type=0xEE 即整面转 GPT（protective MBR 语义，其余 MBR 槽位无论有无内容全忽略）；`index` = 槽位号 1..=4 非发现序（空槽不压缩编号）。
3. **GPT 面**：LBA1 头按 UEFI 标准偏移取 my_lba(24)/partition_entry_lba(72)/num_entries(80)/entry_size(84)；签名 "EFI PART"、my_lba==1、entry_size==128、num_entries ∈ [1,512] 四门全 loud 拒；条目区逐条按块内偏移切窗（同块缓存免重读），type_guid 全零跳过；name 域 72B utf16le 解码（截 NUL、无效代理对折 U+FFFD 不拒——名字是展示面非结构面）。
4. **病态域防御**：分区条目 first_lba=0（覆盖表区）或 first>last → `BadEntrySpan`（MBR 面 sectors=0 同拒）；partition_entry_lba<2 或 +条目区宽溢出 u64 → `BadHeaderLba`；reader 失败透传（`Error::Reader` 由消费方构造返回）。
5. **明确不执法面**（教学盘像由宿主工具产出）：GPT 头 CRC32 与条目区 CRC32、分区 LBA 域对盘容量的对账（库不知盘尾）、hybrid MBR（0xEE 出现即整面转 GPT）。
6. **接线**：user/ workspace members 入列 + justfile test-host 显式行（host 16 测：MBR/GPT 镜像构造解析、0xEE 跳转、坏签名/病态域全拒面、4KiB 块、reader 错透传）。

## Alternatives considered

CRC32 校验入执法面（弃——教学镜像常由脚本手造，CRC 面会挡合法教学盘，留消费侧可选加强）；分区内嵌套解析（EBR 扩展分区链，弃——教学面直式布局，扩链留需求出现再上）；`index` 用发现序连续编号（弃——表内槽位号与 UEFI 工具显示的 Partition N 对齐，空槽跳号是结构事实不该抹平）；盘容量入参对账 last_lba 越盘尾（弃——库边界守「表像自洽」，盘容量真值归消费侧 vblk 持有）；GPT 条目区跨界拼块支持（弃——块长须整除 128 即无跨界，512/4096 教学块恒满足，怪块长 `BadBlockSize` loud 拒）。

## Consequences

`cargo test -p berry-partition --target aarch64-apple-darwin` 16 测全绿；none-target 构建（no_std 面）零警通过；user/Cargo.lock 随 member 入列增量 +18 行。消费面下一刀 = vblk 扫分区表暴露多分区（`Partition`/`PartitionKind` API 已冻结本形）；再下一刀 = fsmgr 按 `first_lba` 定卷替代 BPB 约定式探测（研究报 P0-2 第二断层）。Related：[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)（P0-2 缺口定谳）。
