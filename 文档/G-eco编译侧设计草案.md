# G-eco 编译侧设计草案（认领半：编译侧 + svcmgr 消费端）

> 状态：草案随实施修订（2026-10-16 创建；同日随入机侧第一刀 9ff9258 修订；**G-eco-0/G-eco-a 已落地（2026-10-16），§0/D1 按实施发现修订为 stable 免 nightly 路线**；**G-eco-b 已落地（2026-10-16）——D3 定型为动态布局 + §3 增补 console 发送形态契约修正**）。对口会话分工：**B 半会话认领入机侧内核半**（rootfs 递送：QEMU **fw_cfg 具名文件** `-fw_cfg name=opt/berry/rootfs` / DTB `/chosen` 双源 → 内核 static `ROOTFS`（16MiB no-panic 帽）→ 下一步包**只读 VMO（rights 仅 READ）boot 直插 svcmgr 表**——svcmgr 是 boot 手搭进程不走 process_start bootstrap，零 ABI 变化；handle 定值 boot 断言后硬编，同 console endU 模式）；**本草案 = 编译侧 + svcmgr 消费半**。两侧 seam = 「内核交付一个可读的 rootfs VMO」，可并行。四层实证（9ff9258）：QEMU 非 Linux ELF `-kernel` 直加载不走 Linux boot 协议（x0=0 / RAM 无 DTB / `-initrd` 整体不加载）——**`-initrd` 死路勿再用**，入机载体 = `just run-rootfs`。
>
> 硬验收锚（`路线图.md` G-eco 生态关）：**未经修改的第三方 Rust 程序（ripgrep/hexyl 级）交叉编译后在 BerryOS 上跑通**。全部死于没人写程序——本关是「系统从 demo 变平台」的临界点。

## 0. 事实基线（写码前对齐）

- **ABI 合约源**：`系统调用ABI设计.md` §8.3 原型 + §8.4 Rust FFI 镜像（`用户态/berry-sys`）。G2 起冻结、**add-only**。~~`vmo_get_size` 缺口~~——**v1 不需要**：cpio newc 以 `TRAILER!!!` 条目自终止，walk 天然停；VMO 尺寸（= rootfs 实长）即 `vmo_read` 硬界（越界报错 = 防御性第二停点）——**零新 syscall、零 ABI 变化**。
- **模板机制**：`内核/src/user.rs` `UserProcess::build(fa, code: &[u8], …)` 收**原始字节**——asm 字符串只是当前产字节的方式，换编译产物是即插即用。**单页断言（code.len() ≤ PAGE_SIZE）是 Rust 化的第一道墙**（见 D3）。
- ~~**custom target ⇒ nightly**~~（**G-eco-a 实施期修订：stable 免 nightly 路线**）：builtin `aarch64-unknown-none`（预编译 core）+ 各 bin 自带 build.rs 绝对路径注入 `link-user.ld`（基址 0x1000）即可产平像；G-eco-d 的 std 面走 builtin `aarch64-unknown-linux-musl`（预编译 std）同样免 nightly。custom target JSON **推迟到确需 target 级差异再上**（届时才接受 nightly——本机现存 nightly 为 2022 古董不可用）。平像抽取本机无任何 objcopy（brew llvm 未装）——`工具链/elf2flat.py` 纯 python 解 PT_LOAD 铺平 + 补零到页（盖 .bss）+ `--max` 单页墙执法（教训：取整按**相对长度**，按绝对地址取整会把 200B 代码吹成 2 页）。**内核 stable 纪律不动，且用户程序链也 stable**。
- **根 `.cargo/config.toml` 锁了 `build.target=aarch64-unknown-none`**：与用户程序目标同值——`用户态/` 无需自带 config 中和（原方案作废）；链接脚本差异经各 bin 的 build.rs 注入（内核 build.rs 同款习惯），lib rlib 天然忽略 link-arg 不互扰。

## 1. 拆步与验收

| 步 | 内容 | 验收 |
|----|------|------|
| **G-eco-0** ✅ | `用户态/berry-sys` raw 绑定 crate（no_std，§8.4 手写镜像：`extern "C"`，每个 `berry_*` 内封 `svc #0`——编号只在私有 nr 模块，用户不写编号）。IDL 单源生成是终态（§8.4 引言），v1 手写欠债挂注 | ~~crate 编译过 + 符号面对账~~ **已过**（nm 实测 27/27 符号导出；vmar_map wrapper 折叠 options、exit 双号 `-> !` 自旋留桩） |
| **G-eco-a** ✅ | ~~target spec~~（修订为 builtin target + build.rs 注入链接脚本，见 §0）+ `工具链/elf2flat.py` + `just build-user` + **首个编译用户程序**（no_std + berry-sys）替换 abuser asm 模板 | ~~既有 smoke 全绿~~ **已过**：smoke 23/23 零回归 + EL1 120s 'S'×38 稳态流动（编译产物运行等价活证）+ EL2 `21` 前缀 + host 11 passed + size assert-flip OK |
| **G-eco-b** ✅ | **svcmgr Rust 化**：cpio newc + ELF64 解析塞手写 asm 不合理——svcmgr 整体换成编译 flat binary（boot 流程零改：模板本来就是字节）；`UserProcess::build` 放宽多页代码（D3）——已落：`用户态/bins/svcmgr` + echo/talker 同批换轨（两遍构建）+ asm 模板 user.rs 911–1663 行整块退役 | **已过**：`SVC ok`/`DEMO ok` 零回归（smoke 23/23 + EL1 75s 手动全标记/金丝雀/死者基线 + EL2 + host 11 passed + size assert-flip OK——新基线 rom 212.8KiB/.text 130604B）；**契约修正**：svcmgr 本体 console 发送 = 自旋重发非阻塞（误统一阻塞即启动期饿死，见 §3） |
| **G-eco-c** | svcmgr 消费 rootfs VMO（boot 直插表的定值 handle）：`vmo_read` 流式扫 **cpio newc**（110B 头 + 4 对齐名/数据，`TRAILER!!!` 自终止，VMO 尺寸硬界兜底）→ **ELF64 加载器**（e_ident 校验 → 逐 PT_LOAD：vmo_create/commit/write 段字节 → vmar_map@p_vaddr → 栈页 → process_start(entry=e_entry, bootstrap=console 别名)）。**cpio/ELF 解析器写成纯函数、host 单元测试盖**（AI 测试友好化：恶意/截断/对齐野值面） | 打 `ECO ok`（**smoke 第 24 标记**，尾插随合并序——`ROOTFS ok` 若先入 smoke 名单则顺延其后）= 首个 rootfs 程序经全链（`just run-rootfs` fw_cfg → 内核 static → 只读 VMO → cpio → ELF → EL0 → console）跑通出字 |
| **G-eco-d** | **std 面**（真·未经修改验收）：`aarch64-unknown-linux-musl` target 复用 + 自建静态 **libc shim**（std 的 unix 后端调 libc 符号——`write`/`read`/`exit` 等经 berry-sys 映射 console/channel，无映射面返 ENOSYS）→ hexyl 级 std 程序直接链接跑通；argv 由 runtime crt0 合成（bootstrap 消息协议） | hexyl 在 BerryOS 出 hex dump（对照 Mac 直跑输出一致） |

> 排序理由：G-eco-0/a 完全不依赖对面的内核半，立即可做；G-eco-c 的交付形状已通报（rootfs VMO 只 READ / boot 直插表 / 定值 handle），host 单测 + 伪造 VMO 字节可先行，联调只等对面第二刀落地；G-eco-d 独立于前四步的时序但有符号面风险（见 D4 kill criteria），故压轴。

## 2. 关键技术决策

- **D1 target 形态（G-eco-a 修订）**：**builtin `aarch64-unknown-none` 复用**——bin crate 各自 build.rs 注入 `link-user.ld`（`-T` 绝对路径 + `-zmax-page-size=4096`：rust-lld AArch64 默认 64K max-page 会把 PT_LOAD 对齐抬到 0x10000，0x1000 基址表达不了）。**不先做 PIE/动态链接**（v1 无加载基址随机化诉求；ASLR 留 v2）；custom `aarch64-unknown-berry` JSON 推迟到确需 target 级差异（届时接受 nightly）。
- **D2 flat binary 约定**：`.text/.rodata/.data` 进文件；**`.bss` 由 `工具链/elf2flat.py` 补零到页再交付**（解 PT_LOAD 铺平，尾部按 `max(p_vaddr+p_memsz)` 相对长度取整页）——VMO 页虽零初始化，但把 bss 覆盖兑现在文件侧最直白、`UserProcess::build` 零特判。链接基址 0x1000（`USER_CODE_VA`）；单页程序栈约定不变（0x3000 顶），多页镜像 data/stack 随代码页数上移（D3）。
- **D3 多页代码（G-eco-b 已定型）**：`build()` 断言从 `≤ PAGE_SIZE` 放宽到 `USER_CODE_MAX`=64 KiB（elf2flat `--max 65536` 同值执法；断言防手滑传整段 ELF）。**动态布局**：code 占 `ceil(len/页)*页` @0x1000、data/stack 顺次上移、新 `pub stack_top` 字段（svcmgr spawn 改读字段，其余单页进程仍读常量）；**单页程序与旧固定三页布局逐位相同** = 既有 8 进程零扰动。实测 svcmgr 平像 3 页（9975B→页补零 12288B，data@0x4000、stack [0x5000,0x6000)）。**只动 UserProcess 这一条路**，syscall 面（vmar_map）不动。
- **D4 std 路线三选一**：①musl target + 自建 libc shim（**主选**：不 fork std，Redox-lite；先审计 std 链接符号面，`nm` 清单进本草案附录）；②vendored std 打 patch（Redox 全量模式，维护成本高，**备选**）；③std 面整体推迟 P5 POSIX（**fallback**）。**kill criteria**：若 hexyl 链接拖进无法 shim 的符号（线程/futex/mmap 语义级依赖），①降级 ③、拆「no_std+alloc 第三方库」为 P4 验收降档并回报路线图修订。
- **D5 解析器即产品**：cpio newc / ELF64 解析是**用户态服务逻辑**（微内核原则），写成 `#![no_std]` 纯函数 crate（`用户态/berry-loader`），host 单元测试全盖（截断/坏魔数/奇名长/多 PT_LOAD/p_vaddr 未对齐）——这是「AI 测试友好化」在生态关的落点。
- **D6 编排（G-eco-a 已落）**：用户程序 crate 在 `用户态/bins/*`（独立工作区成员）；`just build-user`（stable + builtin target + elf2flat.py）产出 `内核/src/userbins/*.bin`（gitignored）→ 内核 `include_bytes!`；**`build`/`check`/`run-release`/`size-release` 依赖 `build-user`**（缺产物 = include_bytes 编译期炸响，不静默）。CI（smoke）自动覆盖；`just test-host` 增 `berry-loader` 面（G-eco-c 起）。**两遍构建（G-eco-b 起）**：pass1 先编 leaf bin（echo/talker）平像落 `用户态/userbins/` 中转区，pass2 全 workspace 编 svcmgr（include_bytes! 内嵌两平像）——svcmgr build.rs rerun-if-changed 显式盖全（link-user.ld + 自身源 + 两 staged 平像；**任何 rerun-if 指令都替换 cargo 默认全量重编行为，漏列 = 改了子程序、内核还嵌旧字节静默上线**）。

## 3. 与既有面的零回归承诺

- smoke 23 标记零扰动；`ECO ok` 尾插第 24。svcmgr Rust 化（G-eco-b）**功能等价替换**：验收链（process_create → vmo/vmar → channel → process_start → SVC ok → talker/echo2 → DEMO ok）逐字保留——它是 4-5b/4-6 的活证，不许降级。
- 前存 bug（waiter/pinger 早死 + abuser/spawner console 自旋，29cac7a 基线同症状）**不在 svcmgr 替换面**（模板各自独立）——单独修或随各自模板 Rust 化收口，不挂 G-eco 关卡。
- **契约修正（G-eco-b 实施发现，最重要的一条）**：asm svcmgr 本体的 console 发送（`SVC ok`/稳态 'v'）是**纯自旋重发**（`b 6b`/`b 14b` 无 wait），asm talker 的 console 发送才是 EBUSY→wait WRITABLE 阻塞——上文「abuser/spawner console 自旋」家族归纳把 svcmgr 也带进去了，实为 svcmgr 的**正确契约**。机制：启动期 console 队列饱和窗口内，阻塞等待者被唤醒后须等一整圈轮转（~16 任务 × ~200ms TCG slice ≈3.2s）才拿到片，自旋友商先 refill 腾出的槽 → 三场竞速耗尽 10s 帽 → -ETIMEDOUT 自陷死（初版 Rust svcmgr 即死于此：SVC ok 从未落串口 + echo1 连带饿死）；自旋者由轮转兜底必达（EL0 可抢占 = 自旋有界）。Rust 版以 `send_spin` 保形、通道发送保持阻塞（asm 同形）。**换轨纪律：重试形态（自旋 vs 阻塞）是契约的一部分——逐发送点核对 asm 的 EBUSY 分支去向，不能按家族归纳。**
- PI 面：svcmgr/talker/echo2/新程序全 EL0 每进程私有表、prio 0——零新增受捐面（4-5 纪律自动满足）。

## 4. 开放问题

- **Q1**（已答，随 9ff9258 + VMO 形状通报）：rights **只 READ** 确认（v1 copy 经 vmo_read，MAP 面 v2）；交付 = **boot 直插 svcmgr 表**（非 bootstrap 通道，零 ABI 变化），handle 定值待对面 boot 断言后硬编。
- **Q2**（本侧）：第三方程序的 argv/stdin 协议形状（bootstrap 消息即「环境块」）——G-eco-d 前定，草案届时随实现细化。
