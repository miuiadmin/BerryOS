# 01-非Rust面全景盘点（彻底 Rust 化研究 · 一号）

> **采集日期**：2026-11-12（HEAD = `124c3007`，厂商定制化轮十四 G 清偿 P0 全闭时点）
> **性质**：**计划研究底稿**——只查不修，不构成任何已实施变更；供本系列后续篇（Rust 化方案与刀序、路线图 v1.2 素材）消费。
> **消费面**：`research/彻底Rust化研究/` 系列后续篇 + 「完善计划 + 彻底 Rust 化计划」的立法/路线图回写候选。不进 build/CI，不改动任何产线件。
> **采集方法**：`tools/` 全目录逐件（`ls` 清点 + 每件头注速读：角色/selftest/消费方）；消费面对读 = `justfile`（1427 行全读）+ `.github/workflows/{ci,ci-light,sdk-publish}.yml` + `docs/命令速览.md` + 工具间交叉引用 grep。
> **口径**：「语言」按件本体判定（.py/.sh/无扩展 sh/Rust/数据件）；角色四档（产线|测试|辅助|一次性）按头注自述 + 实际消费链双源定档；代价 S/M/L 为**推断**估值（S≈天级、M≈数天、L≈周级，含 selftest 全谱迁移）。

---

## §0 结论速览

1. **规模**：`tools/` 根层可执行件 **51 件**（36 .py + 14 .sh + 1 无扩展 sh `berry-ld`），约 **30,094 行**（wc 实测：.py 23,571 行 + .sh 6,483 行 + berry-ld 40 行）；另有 `tools/berry/`（**已是 Rust** host 设备壳 workspace）及其 `packaging/` 下 11 件分发打包 .sh。任务书「约 50 件」与本盘点逐件清点互证（两源：目录 `ls` + `git ls-files`）。
2. **已在位的 Rust 件只有两处**：`tools/berry`（设备壳，ADR #62）与 `berry-sign`（`user/berry-crypto/src/main.rs` bin，宿主 target 现场构建）。任务书括注「berry_assemble/mkapp/mkimage/mkcpio/gensourceindex/berry-sign 已 Rust」经实测**只有 berry-sign 属实**——其余五件均为 .py 在册（`tools/berry_assemble.py:1`、`tools/mkapp.py:1`、`tools/mkimage.py:1`、`tools/mkcpio.py:1`、`tools/gensourceindex.py:1`）。本底稿按实测口径修正。
3. **核心产线 12 件还 Python/sh**（§三①），其中 **OEM 交付链 6 件**（berry_assemble/mkcpio/mkfat/mkimage/mkapp/gensourceindex）+ 认证面 2 件（bcts-run.sh/markers.sh）是「彻底 Rust」宣称的**硬缺口**。
4. **最小充分集 = 新 Rust 化 11 件**（§四）：OEM 六件 + bcts-run + elf2flat + elf2pe + gen_abi + berry-ld（tlsscan/size 两道产线门禁列为强烈建议的第 12/13 件）。完成后可宣称「**产线 + 认证面 100% Rust**」；内部开发面（smoke/soak/bench 等约 30 件）建议显式豁免并在口径句中声明。
5. **最大迁移约束 = 单编码器 import 图**（§六）：mkcpio 被 mkapp/berry_assemble/tlsscan/smoke-dpgsoak 复用，mkfat 被 mkimage/pwrcls 复用——迁移必须按底座先行、整簇同刀，否则「单编码器纪律」破约。
6. **附带红利**：tlsscan 与 berry-tls 现存 python↔rust **双份算术义务**（`tools/tlsscan.py:27-32`），Rust 化即消灭双实现；CPIO_PAD「四点同步」义务（`tools/mkcpio.py:50-54`）同理收敛。

---

## §一 采集面总账

### 1.1 可执行件语言分布（tools/ 根层）

| 语言 | 件数 | 行数（wc） | 备注 |
|---|---|---|---|
| Python | 36 | 23,571 | 全部 stdlib-only 除四件 PIL + 一件 nm（§1.3） |
| Bash | 14 | 6,483 | smoke/soak/bcts/paths/markers/ver-check/sync-public |
| POSIX sh | 1（`berry-ld`） | 40 | musl 链接器包装，产线件 |
| **小计** | **51** | **30,094** | ≈任务书「约 50 件」 |
| Rust | tools/berry 1 workspace | src 18 模块 + Cargo | 已 Rust（`tools/berry/Cargo.toml`） |
| （berry/packaging） | 11 .sh | — | Rust 件的分发打包壳（AppImage/AppDir/dmg/cross-docker），辅助档 |

两源互证：`ls tools/` 逐件清点 = 51；`git ls-files tools/` 命中 .py/.sh 61 = 51 根层 + 11 berry/packaging −1 berry-ld（无扩展单独计）。

### 1.2 数据/资产件（非可执行，不构成语言面但随迁评估）

| 件 | 角色 | 消费方 |
|---|---|---|
| `abi.toml` | ABI 常量层 IDL 单源 | gen_abi.py / mkimage.py（platform level）/ mkcpio.py（.berryver）/ berry_assemble.py（反回滚门）/ bcts-run.sh（版本声称位）/ smoke-image.sh 内嵌段 |
| `abi.schema.json` | abi.toml schema | gen_abi.py（装载校验，`tools/gen_abi.py:22-26` selftest 面） |
| `abi.syz` | gen_abi 产物（syzkaller 骨架，SEC-01 件 25） | checked-in，外部 syz 工具 prospective |
| `rootfs-manifest.toml` | rootfs 条目映射（V13 数据化） | mkcpio --manifest / berry_assemble / justfile build-user / sign-user |
| `linker-aarch64.ld` | 内核链接脚本 | `kernel/build.rs`（消费证：build.rs 绝对路径注入） |
| `bench_baseline.txt` + `bench_baseline_icount.txt` + `..._traceoff.txt` + notes.md | bench 基线（活数据） | bench.py |
| `sched_baseline.json` | 调度基线（活数据，git track） | sched_ci.py/sched_dashboard.py |
| `writable_globals.txt` | 可写全局清单 | writable_globals.py --check |
| `wycheproof_ed25519_test.json` | Wycheproof fixture（0.9rc5） | genwycheproof.py |
| `testkeys/`（README 入册；dev.sk/dev.pk 本地 gitignored） | dev 签发纪律件 | just sign-user / berry-sign keygen（`tools/testkeys/README.md:3-5`） |
| `bcts/frozen/`（bcts@0.toml / bcts-t2@0.toml / t2 md） | BCTS 冻结件（Tier1 88 标记） | bcts-run.sh 回放态 |
| `drill/acme/product.toml` | OEM 演练参照物（38/39 号） | berry_assemble --check 演练 |
| `evidence-g3-*.md` ×9 | G3 系列收官证据快照 | 只读历史（档④） |
| `__pycache__/` | python import 副产物 | 未入册（git ls-files 0 命中），无迁移义务 |

### 1.3 外部依赖面（python stdlib 之外）

- **PIL（Pillow）**：`genhdfont.py`/`genohicons.py`/`genohstatus.py`/`genohwallpaper.py` 四件 build 路径硬依赖、selftest 结构面零依赖降级（各头注「PIL 缺席打 skip」）；`genfbfont.py` 纯 art 无 PIL。
- **nm（llvm-nm）**：`shim_audit.py:20`（「依赖 llvm-nm」）。
- **git**：`ver-check.sh`、`sync-public.sh`、`mkcpio.py`（.berryver git-describe）、`gen_abi.py --check`（G-ver-1 HEAD 对拍）。
- **tomllib（Python ≥3.11）**：`gen_abi.py`、`mkcpio.py --manifest` 路、`berry_assemble.py`、`bcts-run.sh` 内嵌段（:78）、`smoke-image.sh:117-127` 内嵌段、justfile sign-user 内嵌段（:376）。macOS CLT 自带 3.9 不够（`tools/gen_abi.py:32-33`）——版本面是现役摩擦点，Rust 化顺带消灭。
- **QEMU/cargo（编排对象非依赖）**：smoke/soak/bench/sched_ci 族。

---

## §二 全景盘点表（tools/ 每件一行）

角色档：**产**=产线｜**测**=测试与门禁｜**辅**=辅助｜**一**=一次性/历史。
消费方：**J**=just 配方｜**CI**=GitHub Actions｜**手**=手工/直调（含 test-host selftest 单列标注 s）。
代价：S/M/L（推断）；收益：**A**=OEM 交付单语言｜**B**=产线一致性｜**C**=零（内部开发面，仅叙事价值）。
依赖：stdlib=python 标准库；余者注名。

### 2.1 Python 件（36）

| 件 | 角色 | 消费方 | 代价 | 收益 | 依赖 |
|---|---|---|---|---|---|
| `berry_assemble.py`（1738 行） | 产（V13 组装器，ADR #82） | J:test-host s(:800)；手:--check/--dry-run、drill/acme | L | A（核心） | stdlib+tomllib |
| `mkapp.py`（2620 行） | 产（.bap 打包+五道门禁+签名集成） | J:build-user ×11(:282-307)、s(:794) | L | A（核心） | stdlib+tomllib |
| `mkimage.py`（714 行） | 产（MBR+FAT16 整盘镜像+manifest v1） | J:image(:1324)、s(:869)；CI 经 just smoke-image | M | A（核心） | stdlib+tomllib；import mkfat |
| `mkcpio.py`（717 行） | 产（cpio newc 编码器底座） | J:build-user(:320)、s(:866)；被 mkapp/berry_assemble/tlsscan/smoke-dpgsoak import | M | A（核心） | stdlib+tomllib(--manifest 路)+git |
| `mkfat.py`（878 行） | 产（FAT12/16 确定性造盘器） | J:run-rootfs(:490)/desktop(:1374)、s(:734-737,861)；smoke.sh ×4、soak 族 ×3、smoke-smp/smoke-lockdep | M | A（核心） | stdlib；被 mkimage/pwrcls import |
| `gensourceindex.py`（1181 行） | 产（官方源索引两件套，SDK-2 §五） | J:sign-user(:381)、s(:1000) | M | A（核心） | stdlib；调 berry-sign |
| `elf2flat.py`（159 行） | 产（ELF→平像，build-user 6 处） | J:build-user ×6(:59-82)、s(:818) | S | B | stdlib |
| `elf2pe.py`（463 行） | 产（ELF→PE32+ EFI 应用） | J:build-boot(:1302)、s(:860) | S | B | stdlib |
| `gen_abi.py`（1023 行） | 产+门禁（ABI 单源五面产物） | J:abi-gen/abi-check(:1124-1128)、build 前置(:388)、s(:924)；smoke.sh:171；CI 经 just build/bcts | M | B | stdlib+tomllib+git |
| `tlsscan.py`（444 行） | 门禁（TLS lint FAIL 门）+census 审计 | J:build-user 尾挂(:334)、s+audit(:876-877) | M | B（顺带消灭 py↔rust 双算术） | stdlib；import mkapp/mkcpio |
| `size.py`（429 行） | 测+产线哨兵（体积基线+ADRP 断言） | J:size/size-release/build-release(:397,1115)、s(:872)；smoke.sh:165；CI 经 just | M | B（--assert-flip 挂产线） | stdlib |
| `bench.py`（2024 行） | 测（六通道基准编排+基线对拍） | J:bench/bench-rt/bench-icount；手:--lat/--trace-dual/--dpg（命令速览:68-71）、s(:958) | L | C | stdlib+qemu |
| `gui8-smoke.py`（1939 行） | 测（GUI-8 交互相双 boot rig） | J:gui8-smoke/gui8-smoke-normal(:1237,1254)、s(:1017) | L | C | stdlib+qemu |
| `p3sim.py`（613 行） | 测（P3-0 admit 九场景 E2E 编排） | J:test-host s(:981)；手:E2E | M | C | stdlib；调 berry-sign/mkapp |
| `fastlane_sim.py`（1401 行） | 辅/研究（快道状态机推演验证，28 号） | J:test-host s(:1008)；手 | M | C（研究已收官，归档价值） | stdlib |
| `advisor.py`（428 行） | 辅（启发式策略生成器 baseline） | 手+s(:893) | S | C | stdlib |
| `policy_ab_eval.py`（307 行） | 辅/测（shadow A/B 判定） | 手+s(:888)；sched_dashboard import judge | S | C | stdlib |
| `sched_ci.py`（427 行） | 测（调度 CI 编排） | 手+s(:932)；**未挂 CI**（workflow 无此步） | M | C | stdlib+qemu |
| `sched_dashboard.py`（586 行） | 测（调度仪表盘/基线回归） | sched_ci subprocess+手+s(:908) | M | C | stdlib |
| `sched_perf_analyze.py`（202 行） | 测（PERF 行验收判官） | 手+s(:900) | S | C | stdlib |
| `starvation_check.py`（328 行） | 辅（饥饿界证书） | 手+s(:858) | S | C | stdlib |
| `shim_audit.py`（251 行） | 辅/半历史（G-eco-d libc shim 裁决证据源） | 手+s(:915) | S | C | **nm/llvm-nm** |
| `docs_guard.py`（304 行） | 门禁（入口规则执法） | J:build/check/build-release/build-x86/berry-build 前置(:388-401)；CI:ci.yml:55、ci-light:43 | M | C（内部纪律面） | stdlib |
| `writable_globals.py`（342 行） | 门禁（static mut 清单） | J:check 前置(:405)；CI:ci.yml:64、ci-light:49；s(:948) | S | C | stdlib；import unsafe_audit |
| `unsafe_audit.py`（833 行） | 门禁（unsafe 台账双向对拍） | CI:ci.yml:374、ci-light:59；手重产；s(:940) | M | C | stdlib |
| `shvarcheck.py`（108 行） | 门禁（bash3.2 吞名守卫） | J:test-host 前置(:567)；CI:ci-light:62；s(:1033) | S | C（若 sh 件退役则随之消失） | stdlib |
| `echod.py`（154 行） | 测（宿主 TCP 回显对端） | J:run-rootfs(:496)；smoke/smoke-smp/soak×3/smoke-lockdep/smoke-dpgsoak；s(:966) | S | C | stdlib |
| `evsclient.py`（237 行） | 测（EVS/SKT 宿主对拍 client） | J:run-rootfs(:501)；smoke.sh ×2(:391,426)、smoke-smp、soak×3、smoke-lockdep；s(:967) | S | C | stdlib |
| `siminj.py`（253 行） | 测（sim 电池帧注入+ACK 对拍） | smoke.sh ×2(:1173,1751)；手 run-rootfs；s(:830) | S | C | stdlib |
| `pwrcls.py`（315 行） | 测（B4 掉电盘面分类/毒环注入） | smoke.sh ×6(:1651-1767)；s(:824) | S–M | C | stdlib；import mkfat（selftest） |
| `genfbfont.py`（403 行） | 辅（8×16 art 三档字体表生成） | 手重产+--check 挂 berry-gfx host 测试（tests/genfbfont_guard.rs）；s(:882) | S | C | stdlib（零 PIL） |
| `genhdfont.py`（245 行） | 辅（32×48 HD 点阵，Noto OFL 源） | 手+s(:853) | S（Rust 侧需 ab_glyph/fontdue 类 crate，**推断**） | C | **PIL**（build 硬依赖） |
| `genohicons.py`（367 行） | 辅（OH 资产 108×108 图标） | 手+s(:836) | M（PNG 编码+LANCZOS→image crate） | C | **PIL** |
| `genohstatus.py`（359 行） | 辅（状态图标/dock 内建面） | 手+s(:848) | M | C | **PIL**；importlib 载 mkapp |
| `genohwallpaper.py`（208 行） | 辅（launcher 壁纸 135×240） | 手+s(:841) | S–M | C | **PIL** |
| `genwycheproof.py`（571 行） | 辅（ed25519 向量表→berry-crypto） | 手重产+--check+s(:990) | S | B（签名链质量面，建议随链迁） | stdlib |

### 2.2 Shell 件（15）

| 件 | 角色 | 消费方 | 代价 | 收益 | 依赖 |
|---|---|---|---|---|---|
| `bcts-run.sh`（1190 行） | 测（**BCTS 认证回放器**，ADR #84） | J:bcts(:1147)；CI:bcts job（ci.yml:409）；s 挂 test-host(:1026) | L | **A**（认证面=OEM 拿走） | bash+python ≥3.11（内嵌 5 段 heredoc :202,344,575,694,715） |
| `smoke.sh`（1776 行） | 测（主冒烟八跑） | J:smoke(:1139)；CI smoke job（ci.yml:306） | L | C | bash+python（内嵌 QMP heredoc ×2 :1118,1192） |
| `smoke-image.sh`（561 行） | 测（EFI 镜像冒烟） | J:smoke-image(:1207)；CI（ci.yml:354） | M | C | bash+python（tomllib heredoc :117） |
| `smoke-smp.sh`（767 行） | 测（SMP 八相） | J:smoke-smp(:1198)；不进 CI | M | C | bash |
| `smoke-x86.sh`（99 行） | 测（x86 七标记） | J:smoke-x86(:1185)；CI 直调（ci.yml:331） | S | C | bash |
| `smoke-lockdep.sh`（320 行） | 测（lockdep 双相） | 手直调（命令速览:39） | M | C | bash |
| `smoke-dpgsoak.sh`（328 行） | 测（dpg 满配 soak） | 手直调（命令速览:75） | M | C | bash+python（mkcpio.entry heredoc :91-104） |
| `soak.sh`（341 行） | 测（单核长跑） | J:soak(:1092) | M | C | bash |
| `soak-smp.sh`（376 行） | 测（双核满载长跑） | J:soak-smp(:1111) | M | C | bash |
| `soak-g3.sh`（456 行） | 测（G3-5 激进 soak） | 手直调（命令速览:74） | M | C | bash |
| `markers.sh`（50 行） | 测（标记单源=数据件） | source 于 smoke.sh/smoke-smp.sh；bcts-run 活树清单；smoke-image KERNEL_MARKERS 硬拷贝（:8-9 三镜注） | S | A（BCTS 清单随认证面走） | bash 数组 |
| `paths.sh`（60 行） | 辅（产物路径单源） | justfile 属性背引号(:10,17,284)；smoke-x86/lockdep/dpgsoak/soak-g3；命令速览直调 | S | C | bash+cargo metadata |
| `ver-check.sh`（58 行） | 门禁（版本一致性） | J:ver-check(:1134)；CI:ci.yml:92 | S | C | bash+git |
| `sync-public.sh`（101 行） | 辅（双仓镜像导出） | 手 | S | C | bash+git+rsync |
| `berry-ld`（40 行） | **产**（musl 链接器包装：剥 crt/−lc） | J:build-user 全 musl 链（:88-123 约 20 处 -C linker） | S | B（或由 rustc 配置原生替代，**推断**） | POSIX sh+rust-lld |

### 2.3 Rust 件（已在位，无需迁移）

| 件 | 角色 | 消费方 |
|---|---|---|
| `tools/berry/`（host workspace：clap/winit/softbuffer/serde_json/miniz_oxide） | 辅→产品面（设备壳 CLI，ADR #62） | J:berry-build/berry/shell/desktop/berry-smoke（justfile:1334-1389） |
| `berry-sign`（= `user/berry-crypto/src/main.rs` bin，宿主 target） | 产（签名链 CLI：keygen/sign/verify/sign-raw/admit/selftest） | mkapp --sign-key、gensourceindex、p3sim、just sign-user；发现序 BERRY_SIGN→target-cargo（justfile:350-365） |

### 2.4 数据件

见 §1.2 表。语言面无关；其中 `markers.sh`（数据形态的 sh）与 `bcts/frozen/*.toml` 因属认证清单而入档②④讨论。

---

## §3 四档结论（每档给优先级与顺序建议）

### ① 核心产线（组装/签名/打包/索引/编译链）

**还 Python/sh 的 12 件**：`berry_assemble.py`、`mkapp.py`、`mkcpio.py`、`mkfat.py`、`mkimage.py`、`gensourceindex.py`、`elf2flat.py`、`elf2pe.py`、`gen_abi.py`、`tlsscan.py`、`size.py`、`berry-ld`（sh）。**已 Rust**：`berry-sign`。`tools/berry` 是设备壳非产线。

判定依据（两源：头注角色自述 + justfile 消费位）：
- OEM 交付链（ADR #82「组装不 fork」三包四层的工具面）＝ berry_assemble（product.toml 校验/装配计划，`tools/berry_assemble.py:2-6`）→ mkcpio（rootfs 编码）→ mkfat（FAT 单编码器）→ mkimage（整盘+源索引挂接 `--source-instance`，`tools/mkimage.py:22-29`）→ mkapp（.bap 五道门禁+签名集成，`tools/mkapp.py:15-29`）→ gensourceindex（官方源两件套）→ berry-sign（✓ 已 Rust）。OEM 拿走的清单里六件 Python + 一件 Rust = **交付单语言断裂点**。
- 编译链一致性件：elf2flat（build-user 6 处平像）、elf2pe（EFI 链）、gen_abi（ABI 五面产物，挂 build/smoke 前置）、tlsscan（FAIL 门挂 build-user :334）、size --assert-flip（挂 build-release/smoke）、berry-ld（musl 全链 linker 包装）。

**优先级 P0（OEM 交付链六件，顺序按 import 底座先行）**：
1. `mkcpio` → 2. `mkfat` → 3. `mkimage`（import mkfat）→ 4. `mkapp`（import mkcpio）→ 5. `gensourceindex`（独立，早于 berry_assemble 因其为源索引真签面）→ 6. `berry_assemble`（import mkcpio.load_manifest，最后收口）。
   同刀义务：`smoke-dpgsoak.sh` 的 mkcpio.entry heredoc（:94）、bcts-run/smoke-image 内嵌 tomllib 段随迁；selftest 全谱随件平移（test-host 30+ 处接线同步）。
**优先级 P1（编译链一致性六件）**：`elf2flat`（S，最快首胜）→ `elf2pe` → `berry-ld`（评估 rustc 原生 `link-arg`/`-Zlinker-features` 或小 Rust driver 替代——**推断**，需实验刀）→ `gen_abi` → `tlsscan` → `size --assert-flip` 面。
**顺序理由**：mkcpio 是被 4 处 import 的单编码器底座（`tools/mkcpio.py:50-54`「四点同步」自证），先迁它=后续件全部在 Rust 侧复用同一 crate；mkfat 同理（mkimage/pwrcls 依赖）；gensourceindex 与 berry-sign 合同面最近（INDEX_NAME/KEYS_NAME 单源 + sign-raw），迁完即「签名+索引」双 Rust；berry_assemble 最后因其消费面最宽（rootfs-manifest/abi.toml/资产对账）且依赖 mkcpio 接口。

### ② 测试与门禁

**须 Rust（认证面=OEM 拿走的必须单语言）2 件**：
- `bcts-run.sh`——BCTS v0 回放器（ADR #84）：活树态/冻结态/镜像腿（--image = 定制镜像认证入口，`tools/bcts-run.sh:14-29`）+ Tier2 静态对账 + 版本声称位对账；内嵌 5 段 Python（toml/工件对账）。OEM 兼容性认证交付物若含 bash+python 解释器需求即破「单语言交付」。
- `markers.sh`——标记清单单源（bcts-run 活树清单取此处；smoke-image KERNEL_MARKERS 硬拷贝三镜 `tools/markers.sh:8-9`）。迁 Rust 后应转为数据件（如 toml/include! 常量）+ 双消费面同步义务一次收口。
- 连带：`bcts/frozen/*.toml` 冻结件格式保持（数据件）；`abi.schema.json` 随 gen_abi。
**可留 host 脚本（内部开发面，第二波或不迁）约 28 件**：smoke.sh / smoke-image.sh / smoke-smp.sh / smoke-x86.sh / smoke-lockdep.sh / smoke-dpgsoak.sh / soak×3 / bench.py / gui8-smoke.py / p3sim.py / sched_ci / sched_dashboard / sched_perf_analyze / echod / evsclient / siminj / pwrcls / advisor / policy_ab_eval / starvation_check / docs_guard / writable_globals / unsafe_audit / shvarcheck / shim_audit / ver-check / paths.sh / fastlane_sim。
理由：消费方全部是开发者本机/自托管 CI（ci.yml/ci-light.yml），不进 OEM 交付包；bash 对 QEMU 编排+串口轮询+trap 生命周期是成熟形态，Rust 化收益≈零（代价却普遍 M/L——smoke.sh 1776 行、bench.py 2024 行）。**注意**：p3sim/echod/evsclient 等与签名链 E2E 相邻，若 P3 真机窗口把「admit 编排器」列入交付面，则 p3sim 升档②（留观）。
**优先级**：bcts-run+markers = P0 尾（与 ①⑥ berry_assemble 同刀或紧随——bcts-run 头注 :4-5「berry-assemble 成型后迁组装门禁」本就预埋了合并窗口）；其余 P2/豁免。

### ③ 辅助（字体烘焙/图标/壁纸/生成器/设备壳）

- 已 Rust：`tools/berry`（设备壳）——无需动作；其 `packaging/*.sh` 11 件是分发打包壳（AppImage/dmg/cross-docker），属**平台发布工程面**非 OS 交付面，建议留 shell（各平台打包工具链本就以 shell 为胶水，**推断**：改写无收益）。
- Python 烘焙五件：`genfbfont`（S，纯 art 零 PIL，最适合先迁出示范）→ `genwycheproof`（S，随签名链 P0 顺带）→ `genhdfont`/`genohwallpaper`/`genohicons`/`genohstatus`（M，PIL→image/ab_glyph 有光栅化对拍成本，黄金哈希钉板可保逐字节共识——selftest 已把「像素级」钉死，Rust 重产对拍有现成基准）。
- `paths.sh`/`sync-public.sh`/`ver-check.sh`：C 档收益，留。
- **优先级 P2**：仅 genwycheproof 随 P0 顺带；其余按需（换字源/资产时顺手 Rust 化，无专项刀价值）。

### ④ 一次性/历史

- `evidence-g3-*.md` ×9：G3-1d/3/4a/4b/5/5c/5d/5recon/6 收官证据快照——**只读历史，永不迁移**（含计数勘误注记，如 evidence-g3-1d.md:8-11「现役计数以 markers.sh 单源为准」）。
- `fastlane_sim.py`：28 号快道重研的可执行验证件，研究已收官；selftest 仍挂 test-host（:1008）——建议**维持现状**（研究复现工具），若 test-host 瘦身可降级为手动。
- `shim_audit.py`：G-eco-d 裁决证据数据源（d-3c hexyl 终验），半历史；维持。
- `p3sim.py`：P3-0 已收官（DoD-10）但 admit 面随 P3 真机窗还会演进——**留观档②注**，不迁。
- `bench_baseline*.txt`/`sched_baseline.json`：活数据非历史。
- **优先级：无动作**（历史件的「Rust 化」是伪需求）。

---

## §4 「彻底」的最小充分集定义

**宣称句（建议口径）**：「BerryOS 工具链产线与认证面 100% Rust——组装、打包、镜像、索引、签名、ABI 生成、编译转换、兼容性认证回放全部 Rust 单语言；内核与用户态本就全 Rust。内部开发面（冒烟/长跑/基准编排、文档门禁）保留 host bash/python，不进 OEM 交付包。」

**最小充分集 = 11 件新 Rust 化（+2 件已在位）**：

| # | 件 | 档 | 前置 |
|---|---|---|---|
| 1 | mkcpio | ① P0 | 底座先行；含 --split/--manifest/.berryver/selftest-split |
| 2 | mkfat | ① P0 | 双 fixture 共识基准平移（berry-fatfs host 测试对拍） |
| 3 | mkimage | ① P0 | 复用 #2；manifest v1 逐字节冻结不破 |
| 4 | mkapp | ① P0 | 复用 #1；berry-manifest 镜像校验/BYI1/签名集成 |
| 5 | gensourceindex | ① P0 | 与 berry-sign（✓Rust）sign-raw/verify-raw 合同对拍 |
| 6 | berry_assemble | ① P0 | 复用 #1 load_manifest；drill/acme 演练回归 |
| 7 | bcts-run（+markers 单源化） | ② P0 | 五段内嵌 python 随迁；冻结件格式不动 |
| 8 | elf2flat | ① P1 | 最小件，首胜示范 |
| 9 | elf2pe | ① P1 | build-boot 链 |
| 10 | gen_abi | ① P1 | 五产物逐字节 oracle（--selftest 在册） |
| 11 | berry-ld | ① P1 | 或 rustc 原生机制替代（实验刀裁决） |
| ✓ | berry-sign | 已 Rust | — |
| ✓ | tools/berry | 已 Rust（非产线） | — |
| +12（建议） | tlsscan | ① P1 | 消灭与 berry-tls 的双份算术义务 |
| +13（建议） | size --assert-flip 面 | ① P1 | ADRP 解码器已有向量自检 |

**为什么这 11 件是「最小充分」**：拿走这组后，OEM 定制产线（product.toml→.bap→rootfs→整盘镜像→源索引→签名）与认证回放（BCTS）全链无一处 Python/bash 依赖；剩余 28 件全部只被 `just`/CI/开发者手工消费（§2 消费方列可机械复核），宣称口径句已显式豁免。反之，缺任何一件（尤其 mkcpio/mkapp/bcts-run）宣称即破——它们是交付包里 OEM 必然执行的入口。

**迁移红线（单编码器纪律，§六展开）**：#1-#4 必须整簇一刀或保序连刀，禁止「mkapp 用 Rust、mkcpio 留 Python」的跨语言 import 混形。

---

## §5 遗漏检查（任务书「检查遗漏」项——文件扩展名盘点会漏的面）

1. **sh 内嵌 Python heredoc 10 段**：bcts-run.sh ×5（:202,344,575,694,715）、smoke.sh ×2（:1118,1192 QMP client）、smoke-image.sh ×1（:117 manifest PR 对拍，tomllib）+ :388（vars.fd 0xFF 生成 python3 -c）、smoke-dpgsoak.sh ×1（:91 import mkcpio.entry）。→ Rust 化产线件时必须连这些内嵌段一起收，否则「件迁了、链没断」。
2. **justfile 内嵌 Python 2 处**：sign-user 的 tomllib 条目循环（justfile:376）、run-efi 的 vars.fd 生成（justfile:1422）。
3. **tools/ 之外的同族面**：`.agents/tools/check-docs.mjs`（Node ≥18，CI 重轻两道都跑——ci.yml:61/ci-light:46）。若「全仓工具单语言」口径成立它是第三语言；按本底稿口径（产线+认证面）它属内部文档门禁，豁免。
4. **CI 内嵌 bash 门禁**：bare-conflict-markers git grep 段、全类别原语 grep 守卫段、sdk-publish-guard 三档 grep/package 段（ci.yml:47-52,72-77,180-260）——YAML 内嵌逻辑，无件可迁；豁免或未来抽成 Rust CLI（**推断**：低价值）。
5. **gitignored 本地件**：`tools/testkeys/dev/dev.sk|pk`（本地存在、未入册，`tools/testkeys/README.md:3-5`）、`tools/__pycache__/`、`tools/berry/cargo-target/`——不构成交付面但本地清点时会出现。
6. **双仓镜像面**：`sync-public.sh` 白名单含 `tools`（:25）——**公开镜像仓同样带全套 Python 工具**；「彻底 Rust」宣称若面向公开仓受众，口径句需两仓一致。
7. **Python 版本面**：≥3.11 依赖 6 处（§1.3）是现役环境摩擦（macOS CLT 3.9 直拒）——迁移后此面整体消失，属隐性收益。
8. **test-host 未挂 CI**：30+ 处 --selftest 接线只在本地 `just test-host`（ci.yml 全文无 test-host 步，仅注释提及 :144-145）——迁移平移 selftest 时若想保持执法强度，需评估是否顺手挂 CI（**推断**：与 Rust 化正交，但同刀窗口便宜）。

---

## §6 迁移约束与风险

1. **单编码器 import 图（硬约束）**：mkcpio ← mkapp/tlsscan/berry_assemble/smoke-dpgsoak；mkfat ← mkimage/pwrcls；mkapp ← tlsscan/genohstatus(importlib)；unsafe_audit ← writable_globals；sched_dashboard ← sched_ci。跨语言期最长不得超过单刀窗口，否则「禁第二份加载器/编码器」纪律（`tools/berry_assemble.py:55-56`、`tools/mkimage.py:16-17`）破约。
2. **python↔rust 双镜像义务清单**：tlsscan F1 ↔ berry-tls tls_place（`tools/tlsscan.py:27-32`）；CPIO_PAD 四点 ↔ berry-loader/mkapp（`tools/mkcpio.py:50-54`）；mkfat fixture ↔ berry-fatfs host 测试（justfile:734-737）；pwrcls FAT12 编解码 ↔ mkfat（`tools/pwrcls.py:53,61`）。Rust 化后全部收敛为单源——**这是比「单语言叙事」更硬的工程收益**。
3. **逐字节冻结面**：mkimage manifest v1、BSRC TLV v1、BSIG 信封、.bap 包字节、rootfs 同输入同字节、fixture 双基准——迁移验收=现有 --check/--selftest oracle 逐字节对拍全绿后再切换（各件 selftest 已备好基准，这是本仓「checked-in 产物即真相」风格的直接红利，`tools/gen_abi.py:28-30`）。
4. **行数体量**：P0 六件合计约 7,848 行（mkapp 2620+berry_assemble 1738+gensourceindex 1181+mkcpio 717+mkimage 714+mkfat 878）+ bcts-run 1190 行——估 4-6 个专项刀（**推断**）。
5. **宿主 target 交付形态**：berry-sign 现场构建先例（justfile:361-366 target-cargo 惯用位）可复制；OEM 交付建议预编译二进制 + 源码双形态（**推断**，随 SDK 分发链定）。
6. **口径立法动作**：宣称前须回写 AGENTS.md 身份行/路线图 + `docs/命令速览.md`（维护合同：新 just 配方/工具语言边界变更）；本系列 02 号篇承接。

---

## 来源索引

**仓内一手（路径:行号，HEAD=124c3007）**：
- 目录清点：`tools/`（ls 与 git ls-files 双源，2026-11-12 采集）；`tools/berry/Cargo.toml`；`tools/berry/src/`（18 模块）；`tools/berry/packaging/`（11 .sh）。
- justfile：`justfile:10,17,284`（paths.sh 消费）；`:59-82`（elf2flat ×6）；`:88-123`（berry-ld 全链）；`:282-334`（mkapp/mkcpio/tlsscan 产链）；`:344-386`（sign-user/berry-sign 发现序/内嵌 tomllib）；`:388-405`（docs-guard/writable-globals 前置）；`:496-501`（echod/evsclient）；`:567`（shvarcheck --check）；`:734-737`（mkfat fixture）；`:794-1033`（test-host selftest 接线带）；`:1124-1128`（abi-gen/check）；`:1139-1207`（smoke/smoke-bcts/smoke-smp/smoke-image）；`:1302`（elf2pe）；`:1324`（mkimage）；`:1422`（run-efi 内嵌 python）。
- CI：`.github/workflows/ci.yml:9,55,64,92,306,331,354,374,409`；`.github/workflows/ci-light.yml:15,43,49,59,62`；`.github/workflows/sdk-publish.yml`（全 87 行，纯 cargo 无 python）。
- 工具头注：`tools/berry_assemble.py:1-60`；`tools/mkapp.py:1-70`；`tools/mkimage.py:1-60`；`tools/mkcpio.py:1-60`（含 :50-54 四点同步）；`tools/gensourceindex.py:1-55`；`tools/mkfat.py:1-55`；`tools/elf2flat.py:1-18`；`tools/elf2pe.py:1-33`；`tools/gen_abi.py:1-50`；`tools/tlsscan.py:1-45`（:27-32 双算术义务）；`tools/size.py:1-39`；`tools/bench.py:1-60`；`tools/bcts-run.sh:1-55`；`tools/smoke.sh:1-60,165,171,391,570,1118-1192,1173,1751`；`tools/smoke-image.sh:110-132,388`；`tools/smoke-dpgsoak.sh:91-104`；`tools/markers.sh:1-50`；`tools/paths.sh:1-60`；`tools/berry-ld:1-40`；`tools/ver-check.sh:1-45`；`tools/sync-public.sh:1-101`；`tools/testkeys/README.md:1-12`；`user/berry-crypto/src/main.rs:1-40`（berry-sign）；`tools/drill/acme/product.toml:1-8`；`tools/bcts/frozen/bcts@0.toml:1-6`。
- 文档：`docs/命令速览.md:16-76`（全表）；`AGENTS.md`（身份/维护合同）；`docs/路线图-v1.1.md`（背景，现役站位）。
- 数值两源：件数/行数=ls+wc 与 git ls-files 互证（§1.1）；berry-sign 已 Rust=user/Cargo.toml bin 声明+main.rs 头注+justfile:361-366 构建位互证；test-host 未挂 CI=ci.yml/ci-light.yml 全文无该步+ci.yml:144 注释互证。

**外部**：无（本篇纯仓内盘点；Wycheproof 上游出处见 `tools/genwycheproof.py:9-13` 仓内注记）。
