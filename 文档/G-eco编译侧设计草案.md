# G-eco 编译侧设计草案（认领半：编译侧 + svcmgr 消费端）

> 状态：草案（2026-10-16，4-6 双半收官后；同日随入机侧第一刀 9ff9258 修订）。对口会话分工：**B 半会话认领入机侧内核半**（rootfs 递送：QEMU **fw_cfg 具名文件** `-fw_cfg name=opt/berry/rootfs` / DTB `/chosen` 双源 → 内核 static `ROOTFS`（16MiB no-panic 帽）→ 下一步包**只读 VMO（rights 仅 READ）boot 直插 svcmgr 表**——svcmgr 是 boot 手搭进程不走 process_start bootstrap，零 ABI 变化；handle 定值 boot 断言后硬编，同 console endU 模式）；**本草案 = 编译侧 + svcmgr 消费半**。两侧 seam = 「内核交付一个可读的 rootfs VMO」，可并行。四层实证（9ff9258）：QEMU 非 Linux ELF `-kernel` 直加载不走 Linux boot 协议（x0=0 / RAM 无 DTB / `-initrd` 整体不加载）——**`-initrd` 死路勿再用**，入机载体 = `just run-rootfs`。
>
> 硬验收锚（`路线图.md` G-eco 生态关）：**未经修改的第三方 Rust 程序（ripgrep/hexyl 级）交叉编译后在 BerryOS 上跑通**。全部死于没人写程序——本关是「系统从 demo 变平台」的临界点。

## 0. 事实基线（写码前对齐）

- **ABI 合约源**：`系统调用ABI设计.md` §8.3 原型 + §8.4 Rust FFI 镜像（`用户态/berry-sys`）。G2 起冻结、**add-only**。~~`vmo_get_size` 缺口~~——**v1 不需要**：cpio newc 以 `TRAILER!!!` 条目自终止，walk 天然停；VMO 尺寸（= rootfs 实长）即 `vmo_read` 硬界（越界报错 = 防御性第二停点）——**零新 syscall、零 ABI 变化**。
- **模板机制**：`内核/src/user.rs` `UserProcess::build(fa, code: &[u8], …)` 收**原始字节**——asm 字符串只是当前产字节的方式，换编译产物是即插即用。**单页断言（code.len() ≤ PAGE_SIZE）是 Rust 化的第一道墙**（见 D3）。
- **custom target ⇒ nightly**：自定义 target JSON 无预编译 core，no_std 也得 `-Zbuild-std=core`；std 级程序则 `build-std=std`。**nightly 只进用户程序编译链，内核 stable 纪律不动**（前置补 `rustup toolchain install nightly && rustup component add rust-src --toolchain nightly`）。
- **根 `.cargo/config.toml` 锁了 `build.target=aarch64-unknown-none` + 全局 target-dir**：`用户态/` 需自带 `.cargo/config.toml`（nearest-wins）指向 berry target + 独立 target-dir，防内核工作区被污染。

## 1. 拆步与验收

| 步 | 内容 | 验收 |
|----|------|------|
| **G-eco-0** | `用户态/berry-sys` raw 绑定 crate（no_std，§8.4 手写镜像：`extern "C"` + `#[repr(C)]`，每个 `berry_*` 内封 `svc #0`——编号只在内核，用户不写编号）。IDL 单源生成是终态（§8.4 引言），v1 手写欠债挂注 | crate 编译过 + 符号面与 §8.3 逐一对账（doc-test 式清单）；无 QEMU 面 |
| **G-eco-a** | target spec `aarch64-unknown-berry`（JSON）+ 链接脚本（0x1000 基址，同现模板约定）+ `just build-user` + **首个编译用户程序**（no_std + berry-sys，经 console channel 打一行）以 flat binary 嵌入替换一个现有 asm 模板 | 既有 smoke 全绿（零回归）+ 新程序经 console 落串口（编译产物与 asm 模板运行等价的活证）；nightly 边界就位 |
| **G-eco-b** | **svcmgr Rust 化**：cpio newc + ELF64 解析塞手写 asm 不合理——svcmgr 整体换成编译 flat binary（boot 流程零改：模板本来就是字节）；`UserProcess::build` 放宽多页代码（D3） | `SVC ok`/`DEMO ok` 零回归（svcmgr 功能等价）；内核镜像增量入 `just size` 基线 |
| **G-eco-c** | svcmgr 消费 rootfs VMO（boot 直插表的定值 handle）：`vmo_read` 流式扫 **cpio newc**（110B 头 + 4 对齐名/数据，`TRAILER!!!` 自终止，VMO 尺寸硬界兜底）→ **ELF64 加载器**（e_ident 校验 → 逐 PT_LOAD：vmo_create/commit/write 段字节 → vmar_map@p_vaddr → 栈页 → process_start(entry=e_entry, bootstrap=console 别名)）。**cpio/ELF 解析器写成纯函数、host 单元测试盖**（AI 测试友好化：恶意/截断/对齐野值面） | 打 `ECO ok`（**smoke 第 24 标记**，尾插随合并序——`ROOTFS ok` 若先入 smoke 名单则顺延其后）= 首个 rootfs 程序经全链（`just run-rootfs` fw_cfg → 内核 static → 只读 VMO → cpio → ELF → EL0 → console）跑通出字 |
| **G-eco-d** | **std 面**（真·未经修改验收）：`aarch64-unknown-linux-musl` target 复用 + 自建静态 **libc shim**（std 的 unix 后端调 libc 符号——`write`/`read`/`exit` 等经 berry-sys 映射 console/channel，无映射面返 ENOSYS）→ hexyl 级 std 程序直接链接跑通；argv 由 runtime crt0 合成（bootstrap 消息协议） | hexyl 在 BerryOS 出 hex dump（对照 Mac 直跑输出一致） |

> 排序理由：G-eco-0/a 完全不依赖对面的内核半，立即可做；G-eco-c 的交付形状已通报（rootfs VMO 只 READ / boot 直插表 / 定值 handle），host 单测 + 伪造 VMO 字节可先行，联调只等对面第二刀落地；G-eco-d 独立于前四步的时序但有符号面风险（见 D4 kill criteria），故压轴。

## 2. 关键技术决策

- **D1 target 形态**：G-eco-a 起自建 `aarch64-unknown-berry`（bare-metal user：EL0、无 redzone 依赖、PIC 不需要——静态链接固定基址）。**不先做 PIE/动态链接**（v1 无加载基址随机化诉求；ASLR 留 v2）。
- **D2 flat binary 约定**：`.text/.rodata/.data` 进文件；**`.bss` 由 justfile 后处理补零到 `_end` 符号再交付**（ld 抄 `_end`，脚本 pad 对齐页）——VMO 页虽零初始化，但**映射长度 = 文件长度**决定 bss 是否被覆盖，pad 进文件最直白、`UserProcess::build` 零特判。链接基址 0x1000（`USER_CODE_VA`），栈约定不变（0x3000 顶）。
- **D3 多页代码**：`build()` 断言从 `≤ PAGE_SIZE` 放宽到 `≤ 64 KiB`（VMO/map 按页对齐取整；断言防手滑传整段 ELF）。**只动 UserProcess 这一条路**，syscall 面（vmar_map）不动。
- **D4 std 路线三选一**：①musl target + 自建 libc shim（**主选**：不 fork std，Redox-lite；先审计 std 链接符号面，`nm` 清单进本草案附录）；②vendored std 打 patch（Redox 全量模式，维护成本高，**备选**）；③std 面整体推迟 P5 POSIX（**fallback**）。**kill criteria**：若 hexyl 链接拖进无法 shim 的符号（线程/futex/mmap 语义级依赖），①降级 ③、拆「no_std+alloc 第三方库」为 P4 验收降档并回报路线图修订。
- **D5 解析器即产品**：cpio newc / ELF64 解析是**用户态服务逻辑**（微内核原则），写成 `#![no_std]` 纯函数 crate（`用户态/berry-loader`），host 单元测试全盖（截断/坏魔数/奇名长/多 PT_LOAD/p_vaddr 未对齐）——这是「AI 测试友好化」在生态关的落点。
- **D6 编排**：`just build-user`（nightly + target json + objcopy + pad）产出 `内核/src/userbins/*.bin`（gitignored）→ 内核 `include_bytes!`；**`build` 依赖 `build-user`**（缺产物 = include_bytes 编译期炸响，不静默）。CI（smoke）自动覆盖；`just test-host` 增 `berry-loader` 面。

## 3. 与既有面的零回归承诺

- smoke 23 标记零扰动；`ECO ok` 尾插第 24。svcmgr Rust 化（G-eco-b）**功能等价替换**：验收链（process_create → vmo/vmar → channel → process_start → SVC ok → talker/echo2 → DEMO ok）逐字保留——它是 4-5b/4-6 的活证，不许降级。
- 前存 bug（waiter/pinger 早死 + abuser/spawner console 自旋，29cac7a 基线同症状）**不在 svcmgr 替换面**（模板各自独立）——单独修或随各自模板 Rust 化收口，不挂 G-eco 关卡。
- PI 面：svcmgr/talker/echo2/新程序全 EL0 每进程私有表、prio 0——零新增受捐面（4-5 纪律自动满足）。

## 4. 开放问题

- **Q1**（已答，随 9ff9258 + VMO 形状通报）：rights **只 READ** 确认（v1 copy 经 vmo_read，MAP 面 v2）；交付 = **boot 直插 svcmgr 表**（非 bootstrap 通道，零 ABI 变化），handle 定值待对面 boot 断言后硬编。
- **Q2**（本侧）：第三方程序的 argv/stdin 协议形状（bootstrap 消息即「环境块」）——G-eco-d 前定，草案届时随实现细化。
