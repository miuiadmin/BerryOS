# 法证：TLS 原位方案崩因归因（T-1 段⓪，2026-10-28）

> T-1「std 线程面平台解锁刀」的段⓪ 法证实录：不改码、不改 ABI、零 QEMU，
> 只回答一个问题——**uu_sort/uu_du 的 `run_spawn_hooks` 崩点，根因到底是什么。**
> 产出 = 本文 + `docs/挂账-C2c深坑件.md` §⑤ 归因勘误 + T-1 后续刀形依据。
> 方法：audit 构建（strip=false）+ 反汇编对位 + 槽字节亲读 + 双路独立收敛后对抗复核。

## 结论速览

**classification = load-reloc-face（TLS 提供面），confidence = high。**

崩因不是「std 线程面平台缺口」，而是 **berry-libc crt0 原位 TLS 方案不健全**：
`.tbss` 零初始化契约被三方共违约，TLS 胞元初值 = 活 `.data.rel.ro` 文件字节
（vtable/函数指针），`std::thread::spawn` 的钩子链把 .text 指针当 `SPAWN_HOOKS`
链表头消费 → 8 字节原子打 4-mod-8 地址 → ISS=0x21 对齐故障 D-abort。

**排除面（四方无罪）**：std 代码（钩子链语义正确）、outline-atomics
（`__aarch64_have_lse_atomics` 恒 0 在 cortex-a72 是正确值、ldxr 回退路功能完好）、
berry-ld（静态 ELF 零 `.rela`/`.dynamic`——RELRO 字节即链接终值，重定位错不存在）、
libc 桩（spawn 路径无任何返回堆/野指针的桩）。

## 崩因机制：三方共违约 .tbss 零初始化契约

TLS 惯例：`.tbss`（SHT_NOBITS）承诺「运行期为零」。Linux 靠运行期 TLS 拷贝
（每线程一块，tdata 拷贝 + tbss 余零）兑现。本仓三方叠加把这纸契约撕了：

1. **lld 标准布局**：`.tbss` 与 `.data.rel.ro` 同 `sh_addr`（NOBITS 不占文件字节，
   下一个文件背书段从同址开始）——lld 没错，这是 ELF 惯例形态；
2. **berry-exec 零 PT_TLS 感知**（`user/berry-exec/src/lib.rs:322-352`）：spawn 只按
   PT_LOAD filesz 泵字节、单 VMO RIGHT_READ|WRITE 整窗映射——装载器也没错，
   PT_TLS 本就不是装载义务；
3. **crt0 原位 TP**（`user/berry-libc/src/crt0.rs:49-67`）：`berry_tls_setup` 免拷贝直用
   镜像原位（TPIDR_EL0 = PT_TLS.p_vaddr − 0x10），头注 :6-9 断言「.tbss 孔洞由
   装载端铺零页覆盖」**为假**——该窗住着 `.data.rel.ro` 本尊。

结果：`.tbss` 窗 = 活 RELRO 数据。全部 TLS 访问（TLSLE，`mrs TPIDR_EL0` 直取——
两枚二进制实测 21 处、零 `__tls_get_addr`/emutls/TLSDESC）读到的"零初值"
全是 vtable 区文件字节；全部 TLS 写都在腐蚀活 RELRO。

## 证据链 E1-E10

- **E1 era 实录核对**（`docs/阶段沿革.md:605-611`）：du 崩 D-abort ISS=0x21
  PC=0x306918 FAR=0x26c68c、x1==FAR、崩点早于 pthread_create；:202 era sort 崩
  FAR=0x24（低址小整数当指针——与今日 audit sort head=4 同族）。两处 P4 判决
  均以「std 线程面平台缺口」归档——本刀复核对象。
- **E2 TLS 符号与段布局**（audit sort+du 亲测）：`SPAWN_HOOKS` 为 .tbss TLS 符号
  （du st_value=0x258、sort 0x280，皆 ≥ filesz=0x258）；.tbss 与 .data.rel.ro 同址
  （du@0x307e50、sort@0x434330、发行版 du@0x3081e0）；cell = PT_TLS 基+filesz
  = .data.rel.ro+0（du）/ +0x28（sort）。
- **E3 槽字节亲读**（零初始化假设逐字节证伪）：audit du cell → head=0x26ca44
  （.text 内、%8=4）、state=0xfb；audit sort cell → head=4、state=0x04；发行版
  release du（与 rootfs bin/du 逐字节同）cell → head=0x26c85c（.text
  [0x262da0,0x306d88) 内、%8=4）、state=0x54。
- **E4 消费点反汇编**：`mrs TPIDR_EL0 → add x19,#0x268 → ldrb state → 状态机`
  （==1 直取值 / ==2 destroyed-panic / 其余含 0 与垃圾 → `strb #1` 后落入取值，
  **init 只写状态字节、永不写值字节**）→ `ldr head → str xzr 清槽 → bl
  __aarch64_ldadd8_relax`。崩点在 Thread::new 之后的 spawn 机体、早于 pthread_create。
- **E5 helper 反汇编**：adrp+ldrb 读 have_lse（.bss）→ cbz → LSE `ldadd` /
  回退 +0x18 = `ldxr x0,[x1]` 循环。ABI = 值 x0、指针 x1 ⟹ era x1==FAR 即原子目标；
  era PC=0x306918 恰落 ldxr（今日产物 helper@0x3067c0 结构同型）。
- **E6 故障类别自洽**：ISS=0x21 = 对齐故障，8B 原子打 %8=4 必发且不受 SCTLR.A
  豁免；整窗 RW 映射排除「未映射」解释；sort head=4（< 首个 LOAD 0x200000）
  为同根异值的翻译错误型。
- **E7 源码定根因**：crt0.rs:49-67 原位 TP + 头注假断言；berry-exec lib.rs:322-352
  仅泵 PT_LOAD filesz + 零 PT_TLS 感知。
- **E8 桩面排除**：pthread_self→1 / pthread_create→EAGAIN / getspecific→静态零数组
  / join→ESRCH（stubs.rs:875-898）——无野指针源；uu_du-0.12.0 src/du.rs:1170
  无条件 `thread::spawn`（注册表源对拍）——崩点确在打印线程 spawn 路径。
- **E9 家族面**：stdhello .tbss@0x24e690 == .data.rel.ro 同址、槽内两个活指针
  （0x211cf0/0x2136bc）——带毒存活实证；hexyl .tbss@0x2a3838 落 RX LOAD filesz
  尾外零区——**结构性干净**（非垃圾非运气）。
- **E10 纪律合规**：git 零变化、零 QEMU、零 cargo——只读既有 audit 产物
  （/tmp/t1-forensic-target）与生产 cargo-target 只读抽件。

## 对抗复核（12 条：upheld 5 / refuted 4 / unverifiable 3）

被**驳倒**的竞争假设（每条都值得记录——它们是「线程面缺口」话术的来源）：

- ❌ TLS GD 模型 `__tls_get_addr` 桩返垃圾：两枚二进制零 GD 符号/调用，全 TLSLE；
- ❌ libc 桩谎报喂野指针：spawn 路径无指针返回桩，野指针与文件偏移内容逐字节同源；
- ❌ thread-face proper（std 钩子链真依赖线程运行时）：崩点在 TLS 胞元读取+原子，
  早于 pthread 桩触达；std 代码语义正确（Cell take/clone + Arc 引用计数原子）；
- ❌（半驳）R-B 字节级读数：偏移映射差 8，真 cell head=0x26c85c（R-A 读数对）。

**upheld** 载荷断言：根因归属（crt0 原位方案）、era 预测（release du 仍将
FAR=0x26c85c 同签名崩——状态机+对齐逻辑链闭合）、E7 RELRO 腐化面、E9 家族面。
**unverifiable**（era 二进制已被重建覆盖，今日同槽复现作结构同型论据，不必追）：
era 悬垂指针 4/static@0x432078 字样、era helper 基址 0x306900 数值、era du
cell=drr+0 数值。

## 家族全扫（2026-10-28 主会话补做——open question ② 收口）

host 零 QEMU 扫现役 rootfs.cpio（C-4 收官终值 29,247,432B）全部 38 件 bin ELF：
判定法 = .tbss 窗 `[p_vaddr+filesz, p_vaddr+memsz)` 是否落在某 PT_LOAD filesz
文件背书内 **且** 窗内存在非零字节。

**结果（38 件 = 24 带毒 + 13 结构性干净 + 1 免疫）**：

- ☠️ **带毒 24 件**（uu 家族 23 件全军覆没 + stdhello）：cat/cp/cut/date/df/du/
  echo/env/false/head/ls/mkdir/mv/pwd/rm/rmdir/sort/tail/touch/tr/true/uniq/wc
  ——窗首 u64 全是 .text/.rodata 指针，其中 13 件 %8=4（cut/date/df/du/head/mv/
  pwd/rm/rmdir/sort/touch/tr/wc——**sort/du 同款对齐崩形态**），11 件 %8=0
  （cat/cp/echo/env/false/ls/mkdir/tail/true/uniq/wc + stdhello——原子不炸但
  静默腐化）；
- ✅ **干净 13 件**：全部 filesz=0、memsz=0x18（纯 .tbss 无 .tdata），.tbss 落
  PT_LOAD filesz 尾外 memsz-only 零页区——与 hexyl 判定两路独立同论；
- ➖ **免疫 1 件**：bin/hello 无 PT_TLS——本雷面不适用。

**规律**（机制支撑的经验律）：有真 .tdata（filesz>0）⟹ .tbss 与后续文件背书段
同址 ⟹ 带毒；纯 .tbss（filesz=0）⟹ PT_TLS 落尾外零区 ⟹ 结构性干净。
**带毒 ≠ 今日必炸**：引爆需程序真读/写相应 TLS 胞元——sort/du 的 SPAWN_HOOKS
恰在无条件 `thread::spawn` 路径上；其余件今日存活 = 胞元未消费（布局轮盘），
不是无雷证明。du 窗首 u64=0x26c85c 与法证发行版读数**逐位一致**（同槽同值铁证）。

## 修法方向（T-1 第一刀裁定依据）

1. **第一刀 = TLS 提供面（crt0 拷贝式，倾向案）**：`berry_tls_setup` 改堆上分配
   memsz+0x10 零块 → 拷 filesz（tdata）→ tbss 余零 → TP=块首−0x10。
   TLSLE `tprel` 随 TP 平移，**全部既有代码零改动**；解除全家族 RELRO 腐化面。
   需核 .tdata 直址消费者（`addr_of!` 类裸取址）在搬迁后仍在。
2. **顺手立 lint 门**：rootfs ELF `.tbss ∩ 非零文件字节` 静态扫描挂
   mkcpio/test-host（本文扫描器即原型）——防未来 crate 布局掷骰回潮。
3. **不可走「装载器跳写 [filesz,memsz)」**：该窗住着 .data.rel.ro 本尊，
   跳写 = 毁 vtable。
4. **链接脚本挪 .tbss 出文件背书区**：每工作区 link script × 全部 std 程序，脆，
   不推荐。

## T-1 范围盒重开（次序倒转）

原案主体（pthread/futex/0x35）降为**第二层**：TLS 修复后 du/sort 才首次真正
抵达 `pthread_create=EAGAIN` → unwrap panic（panic=abort → brk → user_fault_exit），
届时线程面真实缺口清单（pthread_create 真实现 + 0x35 thread_start + per-thread
TLS 拷贝 + guard + sigaltstack/mprotect 面）才可测——刀量可能反而小于原案。

两份 P4「不可行」判决（C-2c 挂账刀 + C-2 刀 2）：崩点**前提已被改写**——崩点
定位不变（确在 run_spawn_hooks、早于 pthread_create），但根因归「TLS 提供面」
非「线程面」；结论「挂账维持」仍对，`docs/挂账-C2c深坑件.md` §⑤ 已随本文勘误。

## 余留 open questions

- ① **一步定验（未跑）**：拷贝式 crt0 改完后重链 audit du 跑一次——预期原子崩
  消失、死点前移 pthread_create=EAGAIN → unwrap panic，一次实验同时验证两层归因；
- ③ pthread 第二层缺口尺寸：TLS 修复后 `thread::spawn` 除 pthread_create 外还
  顺序踩哪些桩——决定 T-1 后半刀量；0x35 thread_start 在册未实现是硬前置；
- ④ 修法二选一终裁：拷贝式 vs 链接脚本——本文倾向拷贝式，`.tdata 直址消费者`
  核查随刀首段定案。
