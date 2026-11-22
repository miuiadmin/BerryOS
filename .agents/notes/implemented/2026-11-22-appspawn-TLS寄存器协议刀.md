# appspawn TLS 寄存器协议刀实施（X-4e 根治）

Status: implemented

一句话说明：按 [刀谱](../../plans/2026-11-22-appspawn-TLS寄存器协议刀.md)（owner 裁决「最彻底最优秀最先进、不计代价」）五件全落地——内核 `process_start`（0x32）add-only 第 6 参 x6 = 调用方 32B TLS 描述符指针（界检拷入 + vaddr 顶界门，坏指针 -EINVAL），四元组经任务上下文 r[4..7]（x23..x26）→ `user_entry_trampoline` 落子入口 x2..x5；crt0 `_start` 直传四参给 `berry_tls_setup`，`__ehdr_start` 自走查整体退役；装载侧单源 = `berry-loader::elf_head` 首 PT_TLS 提取（`ElfHead.tls` 新域 + 几何门），`berry-exec::spawn_rootfs_elf` 经 x6 递入（svcmgr 28 处 + bsh/berry-term/berry-verify 全 std spawn 单点覆盖）；tlsscan F3 门换轨 v2（「PT_TLS 装载侧可提取」——phdr 首窗 + filesz≤memsz + vaddr+memsz≤USER_TOP，旧 exec 族/0x1000 平像豁免条款退役）；X-4e 失败类按设计灭绝——任意链接布局下协议恒成立，PT_TLS 段本体是唯一事实源。

## 裁决与判例

- **寄存器协议成文**：子入口 x0=bootstrap、x1=env 旗标（既有不动）、x2..x5=TLS 四元组（vaddr/filesz/memsz/align；filesz==0&&memsz==0=无 TLS 哨兵）。x2..x5 此前是 switch 期垃圾值——定义它们是纯收紧（add-only），无旧消费者可破坏。
- **解析家宅偏离刀谱一字**：刀谱写「解析纯函数落 berry-tls」；实施落在 `berry-loader::elf_head` 的 phdr 走查内（`ElfHead.tls` 新域）。理由：elf_head 本就逐 phdr 走查装载首窗——PT_TLS 提取安在它家免第二套走查（一事实一归属：装载器的 phdr 走查是 PT_TLS 运行期唯一真源）；berry-tls 的 `find_ptls`（ehdr 走查形）保留供 tlsscan 对拍/host 测，crt0 不再消费。
- **ktest 范围偏离**：刀谱列三测（x6 坏指针拒/哨兵透传/四元组入上下文）；实施取 TaskContext r[4..7] 逐位断言（ktest FP 测块内）+ e2e 双保险（gui8 点击链 x6 真描述符全链 + run-rootfs 五标记 std 族回归）——syscall 级三测需在 ktest 语境手搭用户 AS，与其测的拷贝小面不成比例；e2e 面覆盖同一断言链。
- **门判据换轨**：tlsscan F3 从「ELF 头在映像内（exec 族）」换为「PT_TLS 装载侧可提取」——镜像 berry-loader elf_head 新面（phend ≤ 4096 首窗 / filesz ≤ memsz / vaddr+memsz ≤ USER_TOP），python/rust 双份算术义务面同步成对；selftest 换五例（无头无 TLS 过/头段内过/平像形过/filesz>memsz 拒/越 TOP 拒/phdr 出窗拒）。

## 实施面清单

- arch/aarch64/src/task.rs：`TaskContext::new_user` +`tls: [usize;4]` 参（r[4..7]=x23..x26）；trampoline 四行 `mov x2..x5, x23..x26`。
- sched/src/{task,sched}.rs：`new_user_with_as`/`try_new_user_with_as`/`try_prepare_user`/`try_spawn_user` 全链 +参；boot 双 wrapper（spawn_user/spawn_user_env）恒 [0;4]。
- kernel/src/syscall.rs：`sys_process_start` x6 读入（`copy_from_user` 界检 + `USER_VA_END` 顶界门）；`sys_thread_start` 恒哨兵；文档头块 x6 契约成文。
- kernel/src/ktest.rs：FP 测块 TLS 四元组 r[4..7] 断言；F8 两处调用点随签名。
- user/berry-loader/src/lib.rs：`ElfHead.tls: Option<TlsSeg>` + `TlsSeg` 四元组 + phdr 循环内 PT_TLS 提取（首段胜 + BadSegment 几何门）；host 测 +3（提取/缺席/几何门）。
- user/berry-sys/src/lib.rs：`berry_process_start` +`tls_desc: *const u64` 参（x6 透传）。
- user/bins/svcmgr/src/main.rs：九处自测 spawn 补 `core::ptr::null()`（none 平像族 = 哨兵缺席）。
- user/berry-exec/src/lib.rs：`spawn_rootfs_elf` 组描述符（`h.tls` 有则栈上四 u64 指针、无则 null）递 x6。
- user/berry-libc/src/crt0.rs：重写——`_start` 四行 x2..x5→x0..x3 换位、`berry_tls_setup(vaddr, filesz, memsz, align)` 四参直传、哨兵早退、`__ehdr_start` extern 与走查调用删除、头注协议化重写。
- tools/tlsscan.py：F3 v2 + selftest 五例换轨。
- docs/系统调用ABI设计-v1.0.md：0x32 表行 + 原型注释 add-only 记 x6；docs/规则/06 §A143 行尾 2026-11-22 裁决修订注（不变式降格为链接卫生建议）。

## 验证

- 构建面：`cargo check`（berry-sched/berry-kernel）+ `cargo test -p berry-loader`（66 测 = 63 旧 + 3 新）+ `tlsscan --selftest` 全绿；`just image` rc=0（target-campaign 隔离目录；build-user 尾 tlsscan --check 对真产物执法过）；`abi-check` 五面单源一致（abi.toml 源 + `just abi-gen` 再生，手改文档会被门拒——实施期实踩一次后归正）。
- 静态铁证：新烘 APPDEMO 平像 `_start` 反汇编 = `mov x0,x2 / mov x1,x3 / mov x2,x4 / mov x3,x5 / bl berry_tls_setup`——旧 `adrp+ldr` GOT 载 `__ehdr_start`（烘焙 0）形态消失。
- 内核时序零回归铁证：rootfs 靴 `DPGPRESS ok c=883625` 与刀前基线 883502 差 0.01%——dpg 供给路逐字节无漂移。
- 实施期两翻车与修复：① 自测 asm x6 残值（出参数组指针 0x2070 被当描述符界检拒 → spawner 自陷 → G1.5 watchdog 杀数 2 panic）——`mov x6, xzr` 同批清零，刀谱前置检查点名的坑应验；② ABI 文档手改撞 abi-check 门——改走 abi.toml 源。
- e2e 挂账（**存量雷非本刀**）：gui8 点击段与 rootfs 链深段（NET ok 后）均被两颗 15:36 合并存量雷堵死——svcmgr 深链暴毙（刀前 gui8 跑同族 PC=0 已在册，rootfs 链最后绿是合并前 10:05；本刀 rootfs 跑 boots 全靴标记+DPGPRESS+NET ok 后 svcmgr 亡）与 gui8 路径 dpg_press 相位赛假阳（同一内核 rootfs 形 DPGPRESS ok/gui8 形 panic——四形态免疫面外第五形态，挂 dpg 刀）；app 族点击段 e2e 证明随 svcmgr 刀修复后补跑——**已兑现（同日）**：svcmgr 双崩刀（58016d0f）落地后 gui8-smoke 全链 PASS rc=0（A: CLICK ok n=1 + LAYOUTW ok + 新 app APPDEMO up ×1；B: LAYOUT ok n=2 + 零重写 + canonical 109B）——TLS 寄存器协议全链（装载器 PT_TLS 提取→x6→x2..x5→crt0→main 首行）e2e 实证，X-4e 胎死类连同验证终态全闭。
