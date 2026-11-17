# recv/send 用户参数 I/O 合并单窗（W3-Q92）

Status: implemented
<!-- W3-Q92；PR 本批同开（2026-11-17） -->

## 背景

02 号 IPC 审计（`research/内核深度进化/02-IPC快路径-Top5优化机会-v1.0.md` 机会一）实锤：recv 每发 10 次 walk_leaf、6 次 PT 锁取放、6 次 irq_window 开包——copy_in_u64×2（容量双读）、access_ok×4（出参预扫）、copy_to_user×3（载荷/新值拷出）、copy_out_u64×2（双 u64 回写）各带自己的 PT 锁窗与解蔽窗。10 次走查里 8 次是重复验同一两页（nbytesp/nhandlesp/bytes/handles 典型同栈同页）。send 侧同形两窗（hbuf/bbuf 拷入各一窗）。全成本由 EL0 用户服务（svcmgr/echo/talker 及后续生态）承担。

机会四（rights_of 单查折叠）已随 W3-Q91（PR #449）收口；本刀是 02 号首刀。

## 改法

- 新窗内辅助 `copy_in_u64_locked`/`copy_out_u64_locked`（kernel/src/syscall.rs）：vmo 族 S-4 4-2 判例形态（`copy_from_user_locked` 同款纪律）——调用方已持本 AS PT 锁，窗内做同页跳过走查 + volatile 读/写。
- send：hbuf/bbuf 双拷窗并成单一 PT 锁窗（语句序/错误码/先序逐位保持——bbuf 拷仍在 probe/distinct 之后，坏 handle 先拒）；send 全 read 相，单 last_page 缓存无读写相混面。bbuf 拷毕显式 `drop(_pt)` 收窗——PT 锁不带进 take/push/唤醒链（fast_wake/wake_matching/try_preempt 取 SQ/RQ 锁，锁序零新边）。
- recv：容量双读 + 预扫四段 + 载荷/新值/双 u64 回写全链并成单一 PT 锁窗（6 锁 6 窗 → 1 锁 1 窗）；中段 insert_ref 令牌入表零用户内存触点留窗外（表无锁面不进 PT 窗）。尾段毕 `drop(_pt)` 后进唤醒链。

## 权限相分离（本刀的设计点）

`last_page` 同页缓存**不分读写权限**。recv 是读写双相号（容量读 + 出参写）：若读相与写相共用缓存，读走查过的页会使写走查被跳过——RO 页读得过、写不过（正是 SEC-C 预扫要抓的缺口形状）；写走查被跳过后原始 store 落 RO 页 = EL1 权限 fault。故读相（`lp_read`：容量双读）与写相（`lp_write`：预扫 + 拷出 + 回写）各持独立缓存。vmo 族先例单窗单权限相（read/write 各一窗）无此面；send 全 read 相亦无此面。

## 为什么语义逐位保持

错误码与回滚面不变：容量读错 -EFAULT、容量门拒 -EINVAL、预扫拒 -EFAULT（SEC-C：载荷零拷出）、拷出/回写错 -EFAULT + recv_rollback（take 回 n 令牌 + 压回队首）——每步的返回码、回滚参数与原序逐位一致。载荷拷出从「insert_ref 之前」移到「尾段同窗」：insert_ref 零用户内存触点、失败路径回滚面同构（原序载荷拷出失败回滚 `&[]`，新序在 tail 失败统一回滚 `&newvals[..n]`——tail 内载荷已是第一写，失败时零拷出与原序等价）。段守卫（g3-1c-2）与 §A113 walk+store 同窗铁律原样成立（窗更大、窗数更少）。

## 验证

- kernel aarch64-unknown-none check 绿，警告 62=62 持平（零新警）。
- test-host 257 组 2551 全绿 EXIT=0（0 failed；selftest_guard PASS 60 件 0 漏接线）。
- check-docs 413 篇 0 告警；abi-check PASS。
- unsafe 台账随源重产 1121→1123（+2：copy_in_u64_locked read_volatile 块 + 行号漂移吸收；密度 9.53%/10.59%）。
- ktest H 段（H8-H10 SEC-C 预扫负例/随行 handle 迁移闭环）+ I 段 wait 面 + FAST L6 收发回归全在 test-host 之外由 smoke CI 执法——本刀不改错误面，负例自动回归。

## Related

- [沿革三百八十七](../../../docs/阶段沿革.md)
- [02 号 IPC 审计](../../../research/内核深度进化/02-IPC快路径-Top5优化机会-v1.0.md) 机会一
