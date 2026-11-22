# Agent Note: fsproto 请求号 txid——公共头 5 字节回显甄别混流应答（路线图 P1-1）

Status: implemented

## Problem

berry-fsproto 协议请求/应答不带请求号。拓扑是**共享 channel**：svcmgr 持 `channel_pair()` 的 fs_a，全部 fs client（bsh、stdhello、uu ktest 子、GUI app）拿的都是同一 fs_a 的 SEND|RECV dup——请求发进同一 fsmgr 收队列，应答回同一 fs_a 队列，并发 client 交叠时回复可混流。现状靠两道兜底：svcmgr 以时序纪律保证「fs_a 队列单活跃读者」（机械排队避免交叠）；client 侧任一收侧超时即**毒化整个会话**（v1 无请求号，迟到应答会被下一 RPC 错配成自己的应答——GET 面是静默错数据，只能以作废会话换确定性失败）。

## Decision

- **公共头加 txid（u32 LE）**：请求与应答同构头 `[op|st u8][txid u32]` 恰 5 字节（fsproto `HDR`）。请求方编码时落号，应答方原样回显，双向同偏移 1..5 由 [`hdr_txid`] 单点读取。
- **client 自增取号**（berry-fsclient `rpc`）：静态 `AtomicU32` 发号器从 1 起，**0 保留**——fsmgr 对取不出 txid 的截断帧（<5B）回显 0，真请求永不取 0，错配即弃。
- **收侧匹配弃收**：应答帧 txid 与本请求不符 = 他者/迟到应答，弃收续等本请求的应答；帧 <5B 取不出 txid = 协议破缺，上交 op 解析面拒（Rpc）。
- **txid 是甄别位不是路由位**：共享队列里被错收的他者应答弃收后其真主仍收不到（丢失变超时——诚实失败优于静默错数据）；多 client 全并发须 per-client channel（v2 再议，见 fsproto 模块头「诚实缺口」注）。
- **会话毒化保留零漂移**：txid 落地后迟到应答已可甄别、理论上可解封毒化，但解封是行为变更，本刀不动（解除随 v2 裁决）。

## Alternatives considered

- **per-client channel**（svcmgr 按 client 建独立 fs 通道对）：根治混流且免协议演进，但改 svcmgr 拓扑与内核通道面，超出 P1-1 协议刀的改动面（本刀只裁 berry-fsproto + berry-fsclient + fsmgr）；作为全并发的终局形态挂 v2。
- **pipeline-only txid**（每 client 独立通道前提下的批量请求号）：诊断（svcmgr 注释「fsproto 无请求号，交叠即混流」）表明拓扑真问题是共享队列混流而非同 client pipeline，收益面错位。
- **顺手解封毒化位**：改行为语义，违反本刀「纯增量」约束。

## Consequences

- **wire format 全量 +4B**：状态应答 1→5 字节、GET 应答头 7→11、LIST 应答头 2→6、STAT 应答 6→10、STATFS 应答 21→25（`STATFS_REPLY_LEN`）；PUT 请求 worst = 6+48+5+192 = 251B ≤ `CHAN_MSG_MAX` 256，帽内无截断面。
- **编码器/解析器签名全加 txid 参数**（请求编码器与应答编码器；解析函数返回形态不变——txid 由会话层经 `hdr_txid` 先取）；fsmgr 分派前单独取号，**畸形请求的 ST_BAD 应答也回显**（取不出回 0）。
- **消费面零改动**：bsh 与 berry-fstree 只引 fsproto 常量；berry-libc 只引 fsclient 会话 API（`get_chunk`/`put_chunk`/`commit`/`stat` 等签名未动），txid 生成封闭在 `rpc` 内部。
- **BCTS T2 静态对账不受影响**：`reconcile_t2` 以正则锚 `pub const OP_*` 名值对账（tools/bcts-run.sh），十个 op 名值未动。
- **验证（2026-11-13）**：fsproto host 35 测 + fsclient host 53 测全绿；`just build-user` 全链绿；QEMU（virt/TCG）三轮复现 `FS ok n=2156`（64B + 2156B 多片写读经 txid 全链）与 BSH/PIPE/VBLK/BLKD/DNS/TCP/TCPS/EVS 等前后邻标记。`FAT ok`/`FATW ok` 在本机不可达：bsh 自验序 skt（netd 域，先在于本刀）卡死挡住其后的 fat 族——A/B 基线（stash 本刀重编重跑）同样卡死于同点且基线 FS ok 同样通过，判定与 txid 无关；CI（Linux SLIRP）不受此环境影响。
- **环境教训**：QEMU 验证须内核与 userland 同批构建——旧内核（内嵌旧 svcmgr）+ 新 rootfs 是未定义形态（实证症状：fsmgr `fat init failed: rpc` + bsh 早死，全部 FS 族标记假性缺失）。

## Related

- 路线图：[系统优化路线图-v1.0](../../../docs/系统优化路线图-v1.0.md) §2 P1-1（本刀落 txid 半件；「服务并行预拉」半件未动，状态待仓主更新）。
- 协议面单源：`user/berry-fsproto/src/lib.rs` 模块头（txid 布局与诚实缺口注）；会话面：`user/berry-fsclient/src/rpc.rs` 模块头。
