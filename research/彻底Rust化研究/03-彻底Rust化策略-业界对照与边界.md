# 03 号底稿:彻底 Rust 化策略——业界对照与边界

> **采集日期**:2026-11-12
> **性质**:研究底稿(证据供给 + 裁定建议,非立法)。「彻底 Rust 化研究」系列第 03 篇;一切裁定语句均为**建议稿**,采纳与定名随 ADR;开放问题原样列于 §4,不替拍板。
> **消费面**:①「彻底 Rust 化」总裁决件(路线图 v1.x 语言边界条款 / AGENTS.md 身份行话术收敛)的证据基座;②01 号(现状盘点与 S/M/L 档位表)的业界对照补集——本篇 §3.4 估算待 01 号权威表落地后回填对齐;③后续实施序列件(逐刀排序)的优先级输入;④ADR #82 OEM 交付面(三包四层)的工具语言口径与 SDK-2 签名链工具语言归属。
> **输入**:仓库现状取证(`tools/` 逐件盘点(wc -l,2026-11-12)、`justfile` test-host 接线、`rust-toolchain.toml`、`user/berry-crypto`、`tools/berry`、`.github/workflows/ci.yml`)+ 外部权威文档直接抓取(Redox 官方手册、Fuchsia 语言政策原文、Linux 内核文档、seL4 源树 tools/ 目录清单、Python 官方文档;URL 见来源索引,均于 2026-11-12 实抓)。
> **方法**:外部事实一律给 URL + 抓取原文要点(引号内为原文英译中);内部事实给路径+行号;关键数值两源(内部实测 + 文档/第二实测互证);推断标「推断」。
> **版本**:v1.0(文件名按任务指定不带版本号,版本记于本行)。
> **来源索引**:见文末;仓库内引用一律给相对路径+行号。

---

## §0 摘要(一屏版)

- **业界没有「全 Rust 工具链 OS」先例;Redox 程度最深,但构建编排层不是 Rust**。Redox 的系统软件(内核/驱动/ion shell/redoxfs 及 `redoxfs-mkfs/-ar/-resize` 等宿主工具)全 Rust 且经 `cargo install` 分发,但主构建 = GNU Makefile + bootstrap/维护 shell 脚本 + TOML 配置 + Podman 容器,工具链本体 = rustc/GCC/LLVM 的补丁 fork(官方手册自述,§1.1)。
- **Fuchsia 官方口径直接支持「按消费面裁语言」**:其《Programming Language Policy》明文只约束「设备上运行的生产软件」,「开发者工具不在政策适用范围」;Python 3 被官方批准用于 Build / Host tools / 系统测试三个用途(§1.2)。工具面多语在 Fuchsia 是**立法过的常态**,不是欠账。
- **Linux 证明内核 Rust 化与工具面多语可长期共存**:内核 Rust 面 no_std+抽象层、叶子模块(驱动)不许直接碰 C bindings;而风格门禁 `checkpatch.pl` 至今是 Perl 脚本(§1.3)。
- **BerryOS 四层裁定建议(§2)**:a 内核+用户态(已达成,事实);**b 产线工具(OEM/SDK 交付面)=「彻底」的宣称边界**,核心论据 = OEM 出设备只需 rustup 一套工具链(今日需 rustc+python3+bash 三件),且签名核心 `berry-sign` 已是 Rust(先例在仓);c 测试门禁(内部面)双语不限、不计入宣称;d CI/一次性脚本永久可留。可宣称句式建议见 §2.4。
- **代价可控(推断)**:b 层现役 12 件 Python(约 8.9k 行)——且 **12/12 全部已有 `--selftest` 挂 `just test-host`**:现役 selftest 即 Python↔Rust 对拍钉板,叠加产线件「同输入同字节」的现役确定性纪律,字节级对拍迁移法成本低、可逐件回滚(§3.3)。估算 b 层 20–35 人日;全仓 50 件强迁 60–115 人日且测试面为负收益、不推荐(§3.4,推断)。
- **差异化资产判定(推断)**:按 b 层定义宣称「彻底 Rust」,宣称面**超过 Redox**(它未把工具面纳入宣称)、结构上与 Fuchsia 政策兼容(更严但不冲突)——独一档且可验证(`tools/` 产线件 `.py` 计数 = 0 即验收)。

---

## §1 业界对照

### 1.1 Redox OS:系统软件全 Rust,构建编排不是

Redox 自我定位即「用 Rust 写的通用操作系统」,2015 年立项目的之一是验证「首个用 Rust 写的先进操作系统」可行性(官方手册 Introduction/Why Rust)。分层核其语言实况:

| 层 | 语言实况 | 证据 |
|---|---|---|
| 内核/驱动/用户态程序 | Rust(系统软件层全 Rust) | 手册 Introduction:「general-purpose operating system written in Rust」 |
| 默认 shell | Rust(ion) | 手册 Shell 章:Ion 为默认终端 shell 与脚本胶水,手册内示例直接给 Rust 源码 |
| 文件系统宿主工具 | **Rust,且 `cargo install` 分发** | 手册 RedoxFS 章 Tooling 节:`cargo install redoxfs` 产 `redoxfs-mkfs / redoxfs-ar / redoxfs(FUSE) / redoxfs-clone / redoxfs-resize`(依赖 libfuse 3.x);镜像安装器 redox-installer 亦为 Rust 仓库 |
| 构建编排 | **GNU Make + shell + TOML + 容器,非 Rust** | 手册 Build System Reference:主 `Makefile` 载入 `mk/*.mk`;`podman_bootstrap.sh`/`native_bootstrap.sh` 两大入口脚本;`scripts/*.sh` 维护脚本十余件;`config/*.toml` 文件系统配置;配方旧格式 `recipe.sh` 已被 `recipe.toml` 取代(仅存兼容) |
| 工具链本体 | rustc/GCC/LLVM 补丁 fork | 手册 Cross-Compilation:「`make all` 下载 Redox 工具链构建全部配方(rustc、GCC 与 LLVM 的补丁 fork)」;`make prefix` 下载 Rust/GCC fork 并构建 relibc |

要点(对本系列的含义):

1. **「单件产线工具用 Rust 写、经 cargo 分发」是 Redox 验证过的成熟形态**——redoxfs 工具族与 BerryOS 的 mkfat/mkcpio/mkimage 同类(镜像/文件系统产线件)。
2. **Redox 没有把「工具链全 Rust」当目标**:构建层稳定停靠在 make+sh+TOML 十年量级;连 Rust 写的 bootloader 子项目也用 `make -C bootloader` 驱动(手册 RedoxFS 章示例)。全仓单语不是它的宣称。

### 1.2 Fuchsia:官方口径——语言政策只管设备面,工具面政策性多语

Fuchsia《Fuchsia Programming Language Policy》是本篇最重要的外部锚点,_scope_ 一节原文(意译):

> 本文件描述 Fuchsia 项目在「目标设备上运行的生产软件」所使用的语言……政策**不适用于**:(a) 终端开发者执行的开发者工具(无论跑在目标机还是宿主机);(b) 目标机上不在最终用户正常运行中执行的软件。

语言的官方裁决(同文件):

- **Rust**:平台源树全树批准,**例外 = 内核**(Zircon 内核限定用「有产业生产 OS 实绩的技术集」,即 C/C++);Rust 不对端侧外部开发者开放支持。
- **C/C++**:全树批准(C 限低层系统编程与 ABI 稳定接口)。
- **Go**:原则上不再批准,例外仅存量 netstack(长期应迁走)。
- **Dart**:按 RFC-0176 白名单制、新增需豁免。
- **Python 3**:官方批准三个用途——**Build / Host tools / 系统测试**(Python 2.7 废弃须迁)。配套 RFC-0129 把树内 Python 统一到语言版本 3.8 并用 vendored 解释器 shebang 钉死;RFC-0221 批准树外系统测试用 Python。

配套实据(源树直抓):旗舰开发者工具 `ffx`(src/developer/ffx)本体是 Rust(其 BUILD.gn:主目标 `src/main.rs`、`rustc_test`、`edition = "2024"`、依赖 `third_party/rust_crates:*`——2026-11-12 实抓 base64 原文),但同一目录里 `write_ffx_config.py` 是构建期 Python 动作(BUILD.gn 的 action script)——**单个 Rust 工具的构建胶水里 Python 共存**,是「工具面混合语言无碍交付」的活例。

对照含义:Fuchsia 把「语言纪律」全部押在**设备面**,工具面有意放活(还为 Python 的版本漂移立 RFC 打补丁)。BerryOS 的 a 层等价物(内核+用户态)已全 Rust——比 Fuchsia 设备面更纯;BerryOS 若再收 b 层(产线工具),是在 Fuchsia 明文不管的区域**自愿加码**,不与任何业界口径冲突。

### 1.3 Linux:内核 Rust 化的边界与常青的工具面

内核文档(rust/ 章,next-20261111 实抓)给出的边界事实:

- 内核 Rust **只能链 `core` 不能链 `std`**(crate 须 `#![no_std]`)——与 BerryOS 内核同款约束。
- 架构 = C 内核 + Rust 抽象层(bindings→abstractions→叶子模块),且「叶子模块(如驱动)不应直接使用 C bindings」——Rust 面是**增量挂载**,内核主体与构建系统(Kbuild/GNU make)不动。
- 工具面常青多语:风格门禁 `checkpatch.pl` 官方文档第一句即「是一个 perl 脚本」;Kbuild 生态横跨 C/shell/Perl/Python 数十年。

对照含义:Linux 证明了「内核面按运行域裁语言 + 工具面永不多语洁癖」可以支撑全球最大协作内核;它反方向也说明——**没有一个主流 OS 把「工具面单语」当工程目标**(推断)。「彻底 Rust」在 Linux 语境不存在,在 BerryOS 语境是**自设的高线**,因此边界必须自定义(§2)。

### 1.4 seL4 与工业级:形式化内核 + 脚本工具带是常态

- **seL4**:内核 C + Isabelle/HOL 证明栈(内部 04 号 :129、:161——`tools/bitfield_gen.py` 从 `.bf` 同源生成 C 访问器与 HOL 模型);仓库 `tools/` 目录直抓实据:Python 代码生成器一族(`bitfield_gen.py` 123KB、`invocation_header_gen.py`、`syscall_header_gen.py`、`lex.py` 40KB、`hardware_gen.py`、`condition.py`、`reciprocal.py`、`umm.py`)+ CMake(`helpers.cmake` 30KB)+ shell(`kernel_pylint.sh` 等)。组件框架 CAmkES 亦 Python(内部 04 号 :29)。**形式化最严的内核,工具带恰恰是脚本语言**——工具语言与系统可信度无耦合。
- **工业级三家(VxWorks/INTEGRITY/PikeOS)**:闭源,工具链语言不公开披露(ghs.com 产品页 2026-11-12 抓取 503;内部 07 号深研亦未录得工具实现语言)。唯一可下结论的是:工具语言**不构成商业 RTOS 的披露面/卖点**(推断)——没有客户因 MULTI/Workbench 的实现语言采购或拒绝。

### 1.5 先例裁决:「全 Rust 工具链 OS」有没有?

按「内核+用户态+产线+构建编排+测试门禁全部 Rust」的强定义:**无先例**(§1.1–1.4 四家核验;Redox 是最接近者)。分档排序(推断):

1. **Redox(最深)**:系统软件+文件系统宿主工具全 Rust,构建编排留 make/sh——「OS 本体 + 单件产线工具」双 Rust;
2. **BerryOS(现行,若只算 a 层)**:内核+用户态全 Rust(比 Fuchsia 设备面纯),工具带 py/sh 约 3.0 万行——宣称面窄于 Redox 实况;
3. **Fuchsia/Linux/seL4**:设备面/内核面有语言纪律,工具面政策性或事实性多语。

**若 BerryOS 按 §2 建议(宣称 = a+b 层)落地,宣称面超过 Redox**(Redox 从未宣称工具面单语,其构建层是 make+sh),成为可验证的独一档:「`tools/` 产线件 Python 计数 = 0」是机器可执法的验收线(现役 `docs_guard.py`/`shvarcheck.py` 同族的守卫即可执法)。

---

## §2 「彻底」的边界裁定建议(四层)

### 2.1 四层模型与 BerryOS 现状映射

裁定判据三问:**谁必须装它?装它要什么运行时?坏了挡住谁?**

| 层 | 定义 | 消费者 | BerryOS 现状(实测 2026-11-12) | 裁定建议 |
|---|---|---|---|---|
| **a 内核+用户态** | 镜像内一切代码 | 设备/最终用户 | 全 Rust(`AGENTS.md:9` 身份行;kernel/`#![no_std]` + user/ 33 crate 源树) | **已达成(事实)**,宣称基石 |
| **b 产线工具** | OEM 出设备 / 开发者出包必跑:组装·签名·打包·镜像 | OEM、SDK 外部开发者 | **12 件 Python 共 8,871 行** + 签名核心已 Rust(`user/berry-crypto/Cargo.toml:13,25` `berry-sign` host CLI):mkapp 2620 / berry_assemble 1738 / mkfat 878 / mkcpio 717 / mkimage 714 / elf2pe 463 / genfbfont 403 / genohicons 367 / genohstatus 359 / genhdfont 245 / genohwallpaper 208 / elf2flat 159 | **纳入宣称——「彻底」的目标层** |
| **c 测试门禁** | 开发/CI 执法与观测面 | 仓内开发者 | smoke/soak/bcts-run 等 sh 大件 + unsafe_audit/bench/sched_* 等内部 py,约 30 件 | **不计入宣称**;允许双语长期共存 |
| **d CI/一次性** | 工作流与维护脚本 | CI/维护者 | `ci.yml`(25 个 run 步,挂 docs_guard/writable_globals/unsafe_audit 等)、paths.sh/markers.sh/sync-public.sh 等 | **永久可留脚本** |

边界件归属说明(推断,开放问题 §4-1/§4-2):

- **`gen_abi.py`(1023 行,ABI 单源五面生成)**:产物(abi.toml/schema/syz)被产线与 SDK 消费——建议按「b 层伴生」纳入(或至少 SDK 外发前完成);暂列 b 层待裁件。
- **`berry-ld`(sh 链接器包装,「零 musl 字节」执法点,`tools/berry-ld:1-30` 头注)**:只在源码构建链(内核/用户态编译)出现;OEM 纯组装路径(`berry_assemble --check` 对 kernel 锚 SHA-256 现算对账,berry_assemble.py 头注)不需要它——建议归「构建内部面」,SDK 外发(若含源码构建)时升 b 层。
- **`paths.sh`(路径解析单源,`AGENTS.md:59`)**:全工具共底座,b 层工具 Rust 化后路径逻辑需内嵌或随件分发——迁移时的机械项,不改变分层。

### 2.2 b 层裁定的核心论据:OEM 交付面单语言价值

**论据一(环境矩阵收缩)**:OEM 按 ADR #82「组装不 fork」出设备,必跑 b 层链(berry_assemble 编排 → mkcpio/mkimage/mkfat 出镜像 → mkapp 出包 → berry-sign 签名;调用关系见 `tools/berry_assemble.py:56-74` 步⑥⑦、`tools/mkapp.py:17-28` 签名面、`tools/mkimage.py:111-126` 产线签名半边)。今日该链的环境需求 = **rustc(编 berry-sign)+ python3 + bash** 三件;全 Rust 化后 = **rustup 一件**(产物链内全部二进制同一工具链产出,版本随 `rust-toolchain.toml` 钉死)。少一层运行时 = 少一张版本兼容矩阵(推断)。

**论据二(版本钉死已有立法)**:`rust-toolchain.toml:6` 已钉 `channel = "1.97.0"`(SDK-02 拍板①,「同输入同字节」的编译器轴)——b 层 Rust 件天然继承该钉版;Python 侧无等价机制(仓内 Python 依赖 tomllib 须 ≥3.11,mkcpio.py:22-24 头注自己记录了版本敏感面的回避写法)。Fuchsia 为 Python 版本漂移付出 RFC-0129(vendored shebang)的立法成本,是此面的前车之鉴(§1.2)。

**论据三(反论与再论)**:反方最强论点 = 「Python 3 在 Linux 侧近乎预装」(Python 官方文档:「Python comes preinstalled on most Linux distributions」)。再论:①OEM 产线是**可灌注环境**而非消费笔记本,「预装广」的价值在 SDK 面(app 开发者)大于 OEM 面;②预装 ≠ 版本一致——预装 3.9 与 3.12 的语义差正是 Fuchsia 立法 vendored 的原因;③OEM 面单语言的**审计价值**独立于便利:产线工具与被签名产物同一工具链谱系,供应链叙事完整(推断)。

**论据四(先例在仓)**:签名核心 `berry-sign`(keygen/sign/verify/selftest)已是 Rust host CLI(`user/berry-crypto/Cargo.toml:13,25`),Python 侧 mkapp/mkimage 只是它的调用方(`tools/mkapp.py:25-27` 发现序、`tools/mkimage.py:163` `_find_berry_sign`)——b 层「最后一公里」是编排器与编码器,**不含任何新密码学面**,移植风险性质是「格式变换正确性」,恰好是可对拍的那类。

**论据五(设备壳先例)**:`tools/berry`(开发者设备壳,5,382 行 Rust,clap/winit/softbuffer/miniz_oxide 五依赖,`tools/berry/Cargo.toml:21-25`)证明仓内已有「宿主 Rust 工具 + 独立 workspace + 宿主依赖隔离(不进客机/ABI)」的完整先例(其 README:宿主依赖只在 tools/berry/Cargo.lock)——b 层 Rust 件的工程形态无需新设计。

### 2.3 c/d 层不计入的理由

1. **业界口径一致**(§1.2):Fuchsia 官方把 Build/Host tools/系统测试划为 Python 合法区;Linux 工具面 Perl/Python/sh 常青。测试门禁单语无先例背书。
2. **价值为零而成本真实**(推断):c 层消费者是仓内自己——环境里 python3 永远在(CI 已装);重写不改变任何外部面的体验。而 c 层恰是大件集中区(smoke.sh 1776 / bench.py 2024 / gui8-smoke.py 1939 / bcts-run.sh 1190),其中 QEMU 编排类(sh 家族)迁 Rust 是**负收益**:子进程编排/超时/标记扫描是 shell 主场,Rust 要重造进程管理(推断)。
3. **d 层是一次性逻辑**:CI YAML 本身就不是任何「OS 工具链语言」,维护脚本随用随弃,重写无回收价值。

### 2.4 可宣称定义建议(文字稿)

> **「BerryOS 彻底 Rust」= 内核、用户态服务与产线工具链(组装/签名/打包/镜像)全部 Rust。OEM 构建自己的设备,只需一套 Rust 工具链。测试门禁与 CI 维护脚本是内部工具,按工程效率选型,不列入语言宣称。**

配套建议(全部为提案):

- **宣称等式**:彻底 Rust = a 层 + b 层;**验收线可机器执法**:b 层件集(§2.1 清单)在 `tools/` 下 `.py`/`.sh` 计数 = 0(现有 docs_guard.py 同族守卫即可挂 CI)。
- **诚实条款(反宣称注记)**:对外话术主动注明「内部测试/CI 面保留 Python 与 shell」——先自曝优于被发现;这也与 §1 业界事实一致(无人因此扣分)。
- **身份行联动(提案)**:AGENTS.md 身份行「全 Rust(内核 Meihe no_std + 用户态服务)」可升级为「全 Rust(内核+用户态+产线工具链)」——单一动作随 ADR 立法,勿零散改口径。

---

## §3 bootstrap 与代价

### 3.1 依赖面:OEM 环境要求简化 vs Python3 预装广

| 维度 | 现状(Python 产线) | 目标(Rust 产线) |
|---|---|---|
| OEM 最小环境 | rustc(或 berry-sign prebuilt)+ python3 + bash | **rustup 单件**(或 prebuilt 工具集,§3.2) |
| 版本钉死 | rustc 钉 1.97.0;Python 无钉(须 ≥3.11 面) | 全链随 `rust-toolchain.toml:6` 单点钉死 |
| SDK 面(app 开发者) | mkapp/--new/--check 要 python3 | 随 SDK 分发形态走(§3.2),免装解释器 |
| 供应链审计 | 产线语言与产物工具链异源 | 同一工具链谱系(推断:审计叙事完整) |

依赖纪律建议(提案):b 层 Rust 件默认**零三方依赖或最小依赖**(对照 `tools/berry` 五依赖先例;`justfile:88` 起的 `RUSTFLAGS="-C linker=.../berry-ld"` 链接环境仅适用于客机目标件,宿主工具不受其约束);禁止网络访问;`paths.sh` 逻辑内嵌复制或抽薄为生成物(禁第二份语义源,对齐 berry_assemble「禁第二份加载器」纪律,berry_assemble.py:56)。

### 3.2 编译 vs 分发:三通道

| 通道 | 形态 | 先例 | 建议用途 |
|---|---|---|---|
| ① 仓内构建 | 随(独立)workspace `cargo build`,just 直调 target 产物 | `berry-sign` 现行发现序(mkapp.py:25-27:env BERRY_SIGN → target-cargo/user cargo-target 候选) | 开发/CI/OEM 主路(默认) |
| ② `cargo install` | 从 git tag 安装 | Redox redoxfs 工具族:`cargo install redoxfs`(§1.1) | SDK 面与轻量用户 |
| ③ prebuilt 二进制 | Releases 附平台 tar;`cargo-binstall` 元数据可装(「Binary installation for rust projects」) | rustup 本体即 prebuilt 分发范本;binstall 生态 | OEM 免编译通道(产线机不许编译器全量进驻时的选项) |

建议形态(提案):主路 = ①;b 层齐备后每 tag 附 ③(linux x86_64/aarch64 + macOS arm64 三矩阵,CI 出件);**不做** crates.io 公开发布(闭源期无必要——推断)。签名闭环机会:Releases 产物用 `berry-sign` 自签(狗粮循环),具体归 §4-3 开放问题。

### 3.3 迁移策略:逐件换 + 双轨对拍,现役 selftest 当钉板

**推荐:逐件迁移(非一次性)**。理由:①每件独立可验收可回滚(仓内「小步提交,能在 QEMU 独立验证的里程碑单独成提交」纪律的 host 版);②对拍 oracle 常驻期间,发现语义漂移可即回退;③一次性大爆炸迁移与现役 11 月推进(定制化/签名链/SDK/PM 各线)抢同一批人(推断)。

**钉板迁移法(五步,核心资产 = 现役 selftest)**:

1. **选件**:b 层优先序建议(由简到繁、由编码器到编排器):elf2flat → genohwallpaper/genhdfont/genohstatus/genohicons/genfbfont(资产五件)→ mkcpio → mkfat → elf2pe → mkimage → mkapp → berry_assemble(收口)。
2. **Rust 版落位**:CLI 参数面保持兼容(调用方 justfile/berry_assemble 零改动或最小改动);workspace 归属随 §4-2 裁定。
3. **Python 版降格为 oracle**:对拍脚本喂同一夹具,产物**字节级 diff**——可行性是现役立法过的:mkcpio「同输入同字节」(条目排序/mtime=0/TRAILER 确定性三件,mkcpio.py:10-13 头注)、mkimage「盘字节只随内容变」(manifest 定宽列/无时刻输入,厂商定制化深研 08 号 :324-334 采证);gen* 资产件输出 PNG/字体,生成器语义本就由 selftest 钉板(justfile:880-884 genfbfont 注)。
4. **双轨挂门禁**:Rust 件自带 `--selftest`(等价腿)挂 test-host;**Python 件的 selftest 腿保留为 oracle 腿**与之并挂(现役接线位:justfile:794-900,20 件 python3 `--selftest` 家族)。观察期 1–2 个版本。
5. **退役归档**:观察期满,Python 版移出 tools/(archive 或删除),b 层验收线执法生效。

**关键杠杆(事实)**:b 层 12 件 Python **全部**已挂 `--selftest`(justfile:794-900 逐件核验:mkapp:794 / berry_assemble:800 / elf2flat:818 / genohicons:836 / genohwallpaper:841 / genohstatus:848 / genhdfont:853 / elf2pe:860 / mkfat:861 / mkcpio:866 / mkimage:869 / genfbfont:882)——**对拍夹具零新建成本**;这是本仓「门禁自检」纪律(W1 接线判例:有 selftest 零自动执法 = 门禁坏静默劣化,justfile:796-799)直接兑付的迁移红利。

**免对拍难题项**:非确定性输入件(bench/时间采样)不属 b 层,天然规避;`.berryver` 根条目含 git-describe 动态值(mkcpio.py:24-27)——对拍时用同一 git 状态或固化 ident 降级值即可(细则归实施件)。

### 3.4 工作量估算(推断;待 01 号 S/M/L 权威表对齐回填)

档位定义建议(与 01 号统一后以其为准):**S** ≤0.5 人日(<200 行纯格式变换);**M** 1–3 人日(200–900 行编码器/生成器);**L** 3–8 人日(>900 行,或含协议/编排/联动复杂)。

**b 层逐件表(行数为 2026-11-12 `wc -l` 实测;人日为推断,含移植+对拍+接线)**:

| 件 | 行数 | 档(推断) | 人日(推断) | 备注 |
|---|---|---|---|---|
| mkapp.py | 2,620 | L | 5–8 | manifest 语义+签名调用+ABI 打戳+JSON 信封;SDK 面双栖 |
| berry_assemble.py | 1,738 | L | 4–6 | 九步管线编排、两段式校验;收口件 |
| mkfat.py | 878 | M+ | 2–3 | FAT 编码器+引导盘构造 |
| mkcpio.py | 717 | M | 1.5–2.5 | newc 单编码器+manifest 加载 |
| mkimage.py | 714 | M+ | 2–3 | 四件套+签名几何面(SIGN_LEN/TRUSTPK_LEN,:125-126) |
| elf2pe.py | 463 | M | 1–2 | PE 转换+8 拒面 |
| genfbfont.py | 403 | M | 1–2 | 结构契约+拒面 5(justfile:880-882) |
| genohicons.py | 367 | M | 1–2 | 资产件 |
| genohstatus.py | 359 | M | 1–2 | 资产件 |
| genhdfont.py | 245 | M | 1 | 资产件 |
| genohwallpaper.py | 208 | M | 1 | 资产件 |
| elf2flat.py | 159 | S | 0.5–1 | 纯变换 |
| **小计(12 件)** | **8,871** | — | **20–35** | 全件 selftest 已在(对拍红利) |
| gen_abi.py(待裁:伴生) | 1,023 | L | 3–5 | ABI 单源五面生成;纳入则 23–40 |

**非 b 层参考值(推断,不建议迁)**:c 层约 30 件(py 24 + sh 大件),强迁 40–80 人日且 QEMU 编排件负收益(§2.3);d 层不计。**全仓 50 件强迁总计 60–115 人日**(推断)。

敏感性(推断):①selftest 对拍夹具已在,较裸迁移省 20–30%;②最大不确定项 = mkapp 的 manifest/BCTS/SDK-2 联动(签名链四刀在飞,须与其刀序协调,勿并行改同一件);③b 层完成后若上 prebuilt 通道,再加 3–5 人日(CI 出件+binstall 元数据+三平台矩阵)。

---

## §4 开放问题(原样列出,不替拍板)

1. **berry-ld 归层**:sh 链接器包装(「零 musl 字节」执法点)按 §2.1 建议归构建内部面;SDK 若外发源码构建能力则升 b 层——随 SDK 边界 ADR 一并裁。
2. **b 层 Rust 件 workspace 归属**:并入根 workspace vs 独立 workspace(仓内两先例并存:berry-sign 在 user/berry-crypto、tools/berry 独立)——涉及根 `.cargo/config.toml` 的 target 锁交互(tools/berry README 记载的坑)。
3. **prebuilt 产物签名面**:Releases 工具集是否 `berry-sign` 自签+公布公钥锚(狗粮循环),随 SDK-2 信任簇条款。
4. **估算对齐**:01 号 S/M/L 权威表落地后,§3.4 逐件档位与人日回填;两表冲突以 01 号为准(其件级盘点更全)。
5. **宣称立法载体**:「彻底 Rust」宣称等式与验收线(§2.4)建议随一枚 ADR 立法并联动 AGENTS.md 身份行——一次动作,防口径漂移。
6. **迁移与在飞刀序的互斥**:mkapp/mkimage 正被签名链四刀(SDK-2)触碰,b 层迁移须排在其刀后或同刀合并,避免双改冲突(推断,排期归实施件)。

---

## 来源索引

### 仓内(相对路径+行号;行号为 2026-11-12 工作树实测)

- `AGENTS.md:9`(身份行「全 Rust(内核 Meihe no_std + 用户态服务)」)、`:50`(tools/ = 构建脚本)、`:59`(paths.sh 单源)
- `rust-toolchain.toml:6`(channel = "1.97.0",SDK-02 拍板①钉版)
- `justfile:562`(test-host 定义)、`:794-900`(20 件 python3 `--selftest` 接线位:b 层 12 件全含)、`:796-799`(selftest 零执法=静默劣化判例)、`:88` 起(berry-ld 链接环境)
- `tools/`:件数与行数实测(36 个 .py 共 23,571 行 + 14 个 .sh 共 6,483 行 = 50 件约 30.1k 行;两源:`ls|wc` 与逐件 `wc -l` 汇总,2026-11-12)
- `tools/mkapp.py:16-40`(签名面/berry-sign 调用与发现序/ABI 打戳)、`tools/berry_assemble.py:56-74`(V13 组装步⑥⑦调 mkcpio/mkimage、禁第二份加载器)、`tools/mkimage.py:111-126`(产线签名半边/SIGN_LEN/TRUSTPK_LEN)、`:163`(_find_berry_sign)、`tools/mkcpio.py:10-27`(同输入同字节确定性三件/.berryver 动态值/Python ≥3.11 依赖注)
- `user/berry-crypto/Cargo.toml:13,25`(host CLI berry-sign:keygen/sign/verify/selftest)
- `tools/berry/Cargo.toml:21-25`(clap/winit/softbuffer/miniz_oxide 依赖面)、`tools/berry/README.md`(独立 workspace/宿主依赖隔离/5,382 行 src 实测)
- `tools/berry-ld:1-30`(sh 链接器包装、「零 musl 字节」执法点头注)
- `.github/workflows/ci.yml`(25 个 run 步;`:55` docs_guard/`:64` writable_globals/`:374` unsafe_audit 挂 CI)
- `docs/路线图-v1.1.md`(Phase 8:OEM/真机前置三件套硬闸,ADR #82)
- `research/厂商定制化深研/08-槽位合同与组装工具链机制设计.md:324-334`(组装产线现役确定性采证:mkcpio 三件/mkimage 盘字节只随内容变)
- `research/04-seL4与L4家族.md:29`(CAmkES)、`:129`(Isabelle/HOL 证明栈)、`:161`(bitfield_gen 同源生成 C+HOL)
- `research/07-工业级三强-VxWorks-Integrity-PikeOS.md`(三家商业生态/工具链形态;未录得工具实现语言)

### 仓外(URL;均 2026-11-12 实抓)

- Redox 手册 Introduction(「written in Rust」/2015 起源):https://doc.redox-os.org/book/
- Redox 手册 Why Rust?:https://doc.redox-os.org/book/why-rust.html
- Redox 手册 Build System Reference(Makefile+mk/*.mk、bootstrap sh、scripts/*.sh、recipe.toml 取代 recipe.sh、rustc/GCC/LLVM 补丁 fork 工具链):https://doc.redox-os.org/book/build-system-reference.html
- Redox 手册 RedoxFS 章 Tooling(`cargo install redoxfs`→mkfs/ar/FUSE/resize 工具族;bootloader 用 make 构建):https://doc.redox-os.org/book/redoxfs.html
- Redox 手册 Shell 章(ion 为默认 shell,Rust 源码示例):https://doc.redox-os.org/book/shell.html
- Fuchsia Programming Language Policy(范围排除开发者工具;Python 3 批准用于 Build/Host tools/系统测试;Rust 全树批准除内核):https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages(原文镜像:https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/contribute/governance/policy/programming_languages.md)
- Fuchsia RFC-0129 Python in Fuchsia(3.8 统一+vendored shebang):https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0129_python_in_fuchsia
- Fuchsia RFC-0221 Python for out-of-tree system testing:https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0221_python_for_out-of-tree_system_testing
- Fuchsia ffx 源树(Rust 主程序+write_ffx_config.py 构建动作):https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/developer/ffx/ 与 …/frontends/ffx/BUILD.gn(src/main.rs/rustc_test/edition 2024 实抓)
- Linux 内核文档 Rust 章及 General Information(no_std/抽象层/叶子模块禁直接用 bindings):https://www.kernel.org/doc/html/next/rust/index.html 、https://www.kernel.org/doc/html/next/rust/general-information.html
- Linux 内核文档 Checkpatch(「is a perl script」):https://docs.kernel.org/dev-tools/checkpatch.html
- seL4 源树 tools/ 目录清单(bitfield_gen.py 等 Python 族+CMake+sh,GitHub API 实抓):https://github.com/seL4/seL4/tree/master/tools
- Python 官方文档 Using Python on Unix(「Python comes preinstalled on most Linux distributions」):https://docs.python.org/3/using/unix.html
- cargo-binstall(Binary installation for rust projects):https://github.com/cargo-bins/cargo-binstall
- Green Hills INTEGRITY 产品页(抓取时 503,仅存引):https://www.ghs.com/products/INTEGRITY-rtos.html
