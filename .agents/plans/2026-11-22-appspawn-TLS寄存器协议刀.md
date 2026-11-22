# appspawn TLS 寄存器协议刀（X-4e 根治·装载侧自描述）

Type: spec
Version: 1
Status: done
Updated: 2026-11-22

一句话说明：根治 [appdemo X-4e 胎死](../research/2026-11-22-appdemo-X-4e-胎死诊断与修复三岔口.md)——owner 裁决（2026-11-22）「最彻底最优秀最先进、不计代价」，取案 D 升级形：**装载侧解析 + 寄存器协议**——svcmgr spawn 核心（自测/应用/rootfs 执行三路共单点）在 spawn 时从 ELF 字节解析首 PT_TLS 四元组，经 `process_start` 新增第 6 参（x6=调用方 32B 描述符指针）入内核，内核装配子任务上下文 x2..x5；crt0 `_start` 直读寄存器装配 TLS，`__ehdr_start` 走查整体退役——PT_TLS 段本体成为唯一事实源（一事实一归属），任何链接布局下成立，X-4e 失败类按设计灭绝。

## 裁决记录

- 三岔口（research 文）案 A/B/C 全数弃用：A 动内核取 entry 双面风险；B 构建期烘焙 = 事实第二家（漂移面）；C 与 0x1000 平像几何冲突。案 D 为唯一满足「单源 + 全布局成立 + 失败类灭绝」的形。
- 寄存器位选定：x1 已有语义（env 旗标，syscall.rs:2205 在案）不动；TLS 四元组占 **x2..x5**（vaddr/filesz/memsz/align，filesz==0&&memsz==0=无 TLS 哨兵）；x6 = process_start 调用方描述符指针（0=缺席）。

## 实施面（五件）

### 件 1：内核 ABI 延伸（kernel/src/syscall.rs `sys_process_start`）

- 第 6 参 `tf.regs[7]`（x6）= 调用方 TLS 描述符指针：0=缺席（x2..x5 置 0）；非 0 = `access_ok` 读窗 + `copy_from` 32B（四 u64：vaddr/filesz/memsz/align），坏指针 `-EINVAL` 拒。
- **前置检查（兼容坑，必做）**：0x32 现签名 5 参，x6 此前未定义——盘点全部现役调用方（user.rs 自测 global_asm 序列、berry-libc spawn 包装）确认 x6 是零还是垃圾；垃圾者同批清零，否则新执法把旧调用方打红。
- 描述符四值透传 `berry_sched::try_prepare_user`（延 4 参或打包结构），子上下文 x2..x5 落值；`spawn_user_env` boot 路不解析（svcmgr 本体无 TLS 平像），x2..x5 恒 0——协议成文进 syscall.rs 与 ABI 文档（docs/系统调用ABI设计）`process_start` 条目（add-only 注记）。

### 件 2：svcmgr spawn 核心解析（user/bins/svcmgr/src/main.rs）

- 单点：spawn 服务核心（`app_serve_spawn` 与 rootfs exec 共路处）读得完整 ELF 字节后，解析 ELF64 phdr 首 PT_TLS → 栈上 32B 描述符 → x6 递 `process_start`；非 ELF/无 PT_TLS/none 平像 → x6=0。
- 解析纯函数落 `user/berry-tls/src/lib.rs`（`find_ptls` 既有面延展——签名改食字节切片），host 可测；与 `tools/tlsscan.py` 对拍（python/rust 双份算术义务注记两侧同步）。

### 件 3：crt0 退役 ehdr 走查（user/berry-libc/src/crt0.rs）

- `_start` asm：`mov x2..x5` → `berry_tls_setup(vaddr, filesz, memsz, align)` 四参直传；哨兵（filesz==0&&memsz==0）早退同今「无 PT_TLS」形。
- `extern "C" { static __ehdr_start }` 与 find_ptls 走查调用删除；四模板全局（BERRY_TLS_*）照写（第二层 pthread 消费面不变）；头注重写（前置条件从「链接器自供 ehdr」改「装载协议供 x2..x5」）。
- `berry-tls` 的 `find_ptls` 本体保留（tlsscan/host 测与 svcmgr 解析共用），仅 crt0 不再调。

### 件 4：门与测试

- `tools/tlsscan.py`：X-4e 判据改写——std+crt0 族不再要求「头在 LOAD 内」，改为「PT_TLS 可由装载侧解析提取」（解析成功即过）；0x1000 平像族豁免条款随 ehdr 走查退役一并删。
- ktest 三测：x6 坏指针拒 / x6=0 哨兵透传（x2..x5==0）/ 描述符四元组入子上下文。
- berry-tls host 测：字节切片解析对拍 tlsscan.py 同 ELF 输出恒等。

### 件 5：验收链

- `just image` 全绿（隔离 target dir）→ `just gui8-smoke` 双靴过点（含点击段 `app APPDEMO up` 恰 1 行）→ run-rootfs 五标记链回归（stdhello/bsh 族不回归）。
- 既有挂账不受本刀约束：svcmgr PC=0（子进程死亡对账路）与 inputd h=0 时变形另案在册（research 文遗留节）。

## 边界与不做

- 不动 link-user.ld、不动 0x1000 平像几何、不动 access_ok 下界、不改 manifest.toml 形。
- 不顺手修 svcmgr PC=0——appdemo 活后该路径必被合法 app 退出触达，属独立刀（已在册）。
