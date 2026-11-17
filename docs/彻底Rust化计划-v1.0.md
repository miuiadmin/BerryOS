# 彻底 Rust 化计划 v1.0

> **性质**：计划文档（用户令「补充彻底 Rust 计划」定稿）· 预 ADR 候选——升册号段以 [架构决策](架构决策.md) 在册下一可用号为起点、与其它预 ADR 候线错开后再领号（本稿原记 #86 与 11-13 立稿的显示线〔`显示与加速体系设计-v1.1.md` 头注「号段自 #86 起」〕双占，2026-11-15 二轮审计 G 号登记「未按让先落者律错开」；调度线 v1.2 已先让号。**归属方向两读**：按让先落者律本稿 11-12 首占应保号，按既成指认链〔索引 :135 + 调度 v1.2 让号锚〕显示线占 #86——裁决归 owner，裁前本稿不领号，双占以本声明消解；沿革五百四十九）
> **采集日期**：2026-11-12 · **证据基座**：`research/彻底Rust化研究/01-03 号`（README 为入口）
> **状态**：实施中——独立件先行：elf2flat 已五步全闭换轨（b598e5bbf Rust 件 + 8d8a2f3ed 产线换轨，[补档笔记](../.agents/notes/implemented/2026-11-17-elf2flat产链Rust换轨补档.md)）、elf2pe 已换轨现役（2a342d813 Rust 件 + 3a07d5a42 产线换轨二刀、f04563276 Python 降格 oracle——本计划漏记换轨已闭，车队窗遗漏审查 2026-11-17 补账）、mkcpio 已全闭换轨（55afb366 Rust 件首刀〔刀序首位〕+ b4718d50 产线换轨〔build-user mkcpio 步 python3→Rust bin〕，2026-11-17）+ mkfat Rust 件首刀已落（d5d3d328，刀序第二位——产线现役件仍 Python，二刀换轨归 justfile 配方）；主刀序 §4 mkcpio/mkfat 两档已启，余五档未动，边界件开放问题 §7 仍开

---

## §1 裁定：四层边界与宣称口径

| 层 | 面 | 现状 | 计入「彻底」 |
|---|---|---|---|
| a | 内核 + 用户态服务 | ✅ 全 Rust（day-one） | ✅（已达成） |
| **b** | **产线工具（OEM/SDK 交付面）** | **七件中四件纯 Python**（立稿 2026-11-12 时点口径；elf2flat/elf2pe/mkcpio 其后换轨，mkfat 首刀已落产线待二刀，余量见 §2 表） | ✅ **宣称边界** |
| c | 测试门禁（smoke/docs_guard/bench 等约 28 件） | Python/bash | ❌ 显式豁免（Fuchsia《Language Policy》同口径：政策只管设备上生产软件，开发工具明文豁免） |
| d | CI/一次性/历史件 | bash/py 混 | ❌ 永久豁免 |

**宣称句（b 层落成后生效）**：BerryOS——内核、用户态、产线工具 100% Rust;业界无先例（Redox 构建层留 make+sh,Fuchsia 工具面立法多语——证据 03 号 §1）,落成即超 Redox 工具链纯度。

**核心论据**：OEM 环境从 rustc+python3+bash 三件收缩为 **rustup 单件**（rust-toolchain.toml 已钉 1.97.0）;berry-sign 已 Rust 在仓（先例形态）。

## §2 迁移清单（b 层 11 件 + 已在位 1 件）

| # | 件 | 现语言 | 规模 | 代价 | 备注 |
|---|---|---|---|---|---|
| 0 | berry-sign | ✅Rust | — | — | 已在位（berry-crypto bin） |
| 1 | mkcpio | ✅Rust(2026-11-17) | — | — | 已换轨:Rust 件 55afb366(刀序首位首刀,lib+CLI 独立小工作区,垫页几何/重装恒等/拒面 13 型/伴生 .sig 收编四性质+CLI 对拍字节级恒等)+接线 9166fd01(test-host 2518→2531)+产线换轨 b4718d50(build-user mkcpio 步 python3→Rust bin,刀序首位收口);Python `mkcpio.py` 不退役——六处模块 import 图根(mkapp/tlsscan/bench/smoke-dpgsoak 的 entry/build_archive、berry_assemble/gensourceindex 的 load_manifest,单编码器纪律)保模块面+CLI 对拍 oracle |
| 2 | mkfat | Rust件已落·产线未换(2026-11-17) | — | — | Rust 件 d5d3d328(2026-11-17,刀序第二位首刀:lib+CLI 独立小工作区,build_volume FAT12/build_volume_puts EFI 引导卷 FAT12/16 双宽+SPC 自适应+fat_path_comps 8.3 拒面全谱+7 测含 CLI 对 Python 现役件字节级对拍三面 sha256 恒等);产线现役件仍 Python(mkimage import build_volume_puts 单编码器纪律不动),二刀换轨再换 justfile 配方 |
| 3 | mkimage | py | ~1100行 | M+ | 伴生件依赖前两件 |
| 4 | mkapp | py | ~2620行 | **L** | BYI1 编码+签名集成全谱（可能 L+） |
| 5 | gensourceindex | py | ~900行 | M | 惰性 import mkapp/mkcpio |
| 6 | berry_assemble | py | ~1400行 | L− | 消费全族 |
| 7 | bcts-run | sh+5段py | ~1300行 | M | 认证面——OEM 拿走单语言件 |
| 8 | elf2flat | ✅Rust(2026-11-16) | — | — | 已换轨:Rust 件 b598e5bbf(#393,五步 2/3/4)+产线换轨 8d8a2f3ed(第 5/5 步,build-user 六处,py 降格 oracle 改名 `elf2flat_oracle.py`) |
| 9 | elf2pe | ✅Rust(2026-11-17) | — | — | 已换轨:Rust 件 2a342d813(字节级对拍口径)+产线换轨 3a07d5a42(justfile 现役件,立法注释在换轨处)+f04563276 py 降格 oracle 改名 `elf2pe_oracle.py`——本计划漏记换轨已闭,车队窗遗漏审查 2026-11-17 补账;BerryBoot 产线 |
| 10 | gen_abi | py | ~500行 | M | 边界件（abi 产线伴生,归属 §7 裁） |
| 11 | berry-ld | ld脚本+py | — | S 或路线裁 | rustc 原生 linker 配置实验刀先裁（§7） |

**总量**：20–35 人日（推断,01 号 S/M/L 权威表对齐）。

## §3 豁免清单（c/d 层,显式在册防误伤）

c 层约 28 件（smoke/soak/bench/sched/echod/evsclient/文档门禁族/shvarcheck/unsafe_audit/字体烘焙族/berry 设备壳内脚本面/fastlane_sim/p3sim 留观件等——全清单=01 号 §分档表）;d 层一次性/历史件。豁免依据:Fuchsia 政策原文（03 号 §1-2）+ 内部开发面消费方为零 OEM 触达。

## §4 刀序（单编码器 import 图保序——01 号硬约束）

```
mkcpio → mkfat → mkimage → mkapp → gensourceindex → berry_assemble → bcts-run
                              ↘ elf2flat/elf2pe（独立,随时）· gen_abi/berry-ld（边界裁后）
```

**排期约束**:mkapp/mkimage 正被签名链后续刀触碰——Rust 化排在其后或同刀合并(03 号 §4-6)。

## §5 迁移法:钉板五步(03 号 §3)

1. Python 版降格为 oracle(改名 `*_oracle.py` 入豁免区)
2. Rust 新件落 `tools/`(或 §7 裁定的 workspace)——同名 CLI 形态
3. **字节级对拍**:同输入 → 产物 sha256 恒等(现役「同输入同字节」确定性纪律是杠杆)
4. selftest 全绿(12/12 件已有 selftest 挂 just test-host——零新建夹具成本)
5. just/CI 配方换轨 → oracle 退役入 d 层

## §6 验收线(机器可执法)

`tools/` b 层清单内 `.py` 计数 = 0(docs_guard 扩一 G 规则或 CI grep——随首刀落)。进度(2026-11-17):elf2flat/elf2pe 已出列(oracle 改名入豁免区),mkcpio CLI 产线已换轨(b4718d50)但 `mkcpio.py` 保模块面不改名(六处 import 图根)计数不动,mkfat Rust 件首刀已落(d5d3d328)产线仍 py——现役 b 层 `.py` 计 9(车队窗遗漏审查 2026-11-17 补账口径延续),验收线随末件生效。

## §7 开放问题(随首刀裁)

1. b 层 Rust 件 workspace 归属:并根 workspace vs `tools/` 独立( berry-ld 实验结果影响)
2. gen_abi 归属(b 层伴生 vs c 层)
3. berry-ld 路线:rustc 原生 linker 配置(优先实验)vs 小 Rust driver
4. 分发通道:仓内 cargo 构建(默认)/cargo install/prebuilt+binstall——三通道文档化随 SDK 分发刀
5. sh 内嵌 Python 10 段+justfile 内嵌 2 段的随刀收编(01 号遗漏面清单)

## §8 计划完善联动(02 号缺口矩阵收口)

- 路线图 v1.1→**v1.2**(本计划同窗):八域收录+三新轨(工程债/五道闸倒计时/**彻底 Rust 化轨道**占位)
- 厂商定制化 G 系:P0×3 已闭(124c3007),P1×6+P2×4 随 G 后续刀
- AGENTS 身份行联动:b 层落成后「全 Rust(内核+用户态+产线工具)」升格

## 来源索引

- `research/彻底Rust化研究/01-非Rust面全景盘点.md`(51 件逐件/四档/刀序约束)
- `research/彻底Rust化研究/02-整体计划查漏-缺口矩阵.md`(八域矩阵/v1.2 收录清单)
- `research/彻底Rust化研究/03-彻底Rust化策略-业界对照与边界.md`(Redox/Fuchsia/Linux/seL4 对照/四层裁定/钉板五步/工作量)
- 外部 URL 全录 03 号文末
