# 19 号：slice 推荐面与 damage 帧源复核——两波落地独立验证（只验不修）

> 复核日期：2026-11-15。服务对象：调度+性能优化轮后两波落地的独立复核——A 波 = slice 推荐面（[17 号](17-slice调参GUI实验.md) §4「desktop 档 `sched_slice_hz=250`」落两处：`berry_assemble --sample` 样例注释 + [产品配置层 §1.6-补](../../docs/产品配置层与槽位合同设计-v1.1.md) 追记；缺省改裁归 owner），B 波 = gfxdemo 部分包络模式（真 damage 帧源——`PARTIAL_ENVELOPE` 编译分支，回应 [18 号](18-定标与调参复核.md) §1.2「现役链无 r<1000‰ 触发」点名；实施笔记 [gfxdemo-partial-envelope](../../.agents/notes/implemented/2026-11-14-gfxdemo-partial-envelope.md)）。任务三面：①推荐面忠实性（默认未改/样例无活值/数值对 17 号回链）；②gfxdemo r<1000‰ QEMU 实证 + ramfb 零漂移 + smoke 绿；③门禁 + git 对拍。方法：**实跑复核**（与 18 号的原始件重算型不同——本轮关键断言全部重新采集）：自建干净 HEAD 内核 + 现 rootfs 组探针镜像重跑三靴 QEMU（vgpu/ramfb/陈核负对照，每靴超时帽+标记轮询）；gfxdemo 按构建配方独立复编做字节级对拍；berry_assemble selftest/--sample/drill 演练实跑；宿主测试两 crate 实跑；门禁实跑。确定度：**[验]** = 本复核独立复现（实跑/复编/源码双源对上）；**[偏]** = 复算与宣称有出入；**[断]** = 无法独立复核；推断一律显式标注。只验不修：发现挂账不动文件（本复核写入 = 本文 + README 索引行）。

---

## §0 复核总判（四条）

1. **A 波推荐面忠实，缺省未动 [验]**：三查全过——`--sample` 实跑输出核实 params 节全 `#` 注释行（样例不落活值，注释节在 TOML 语义 = 空节 → 不产 `etc/boot-params.toml`）；`250` 在 berry_assemble.py 全文唯一出现位 = 样例注释 :1089，词表 :172 (16,1024) 与 selftest 夹具 64（:1374/:1661-1687）原样；上游缺省 `arch/aarch64/src/timer.rs:29` `SLICE_HZ=10` 原样且 arch//kernel//sched//guifbd 保护路径对 HEAD 零 diff（§1.1）。样例注记与产品配置层追记（:220）的全部数字对 17 号 §4/§0-1 逐项回链命中，追记自措辞 ↓53%/↓16pp 算术复核成立；selftest 全过 + drill 演练仍出 `sched_slice_hz=64`（acme 件零 diff）。
2. **B 波 r<1000‰ 实证成立 [验]**：E 轮（vgpu 桌面档，自建探针镜像）独立复现 **`VGPU2 rect k=2 env=1080x960+0+960 r=500‰`**——包络恰下半屏（0+960 起 960 高）、K=2 与 16 号基线同形、slot=29 同槽、reap 0.132ms 落 16 号带（0.108-0.137ms）、16s 全标记到齐；F 轮（ramfb 同形去 vgpu 设备）链同形到齐且 vgpu/VGPU2 行计数 0 = 零漂移；smoke boot 相 45/45 exit 0；rootfs.cpio 内 bin/gfxdemo 与按 justfile 精确配方的独立复编**字节同一**（cddb216b…）；berry-gfx host 114 测 + guifbd lib 13 测全过（与笔记宣称同数）（§2）。
3. **陈核坑负对照独立坐实 [验]**：以 `~/.cargo-target` 陈核（10ff7ef0…，banner `v0.2.0-197-g429675903-dirty`——笔记点名串）组镜像按 vgpu 形启动——`FB ok vgpu` 缺席（ramfb 形）、vgpu/VGPU2 行 0、marker 链仍全绿（14s）——笔记「断 svcmgr→guifbd virtio 窗交接 → 缺省安全落 Ramfb 静默」的换核判别复现；「凡取现役 out/berryos.img 跑 vgpu 面前先验 boot banner」的告诫有据（§2.3）。
4. **门禁双绿 + git 对拍干净 [验]**：`docs_guard` PASS 0 警、`check-docs` 235 篇 0 警（本文落档后复跑）；两波自身写入恰四件（berry_assemble.py 单 hunk 注释面 / 产品配置层 doc +2 行 / gfxdemo main.rs / 未跟踪笔记）+ gitignored 构建副产物；launcher 两件 M（desktop.rs/frameloop.rs）mtime 11-13 04:34/04:38 早于两波窗（11-14 15:32/15:41）且 diff 内容 = 五键导航 recents_app 面（模拟器体验波）与 gfxdemo 无涉——owner 在途 M 同判（18 号 §3 先例）（§3）。

## §1 A 波复核：slice 推荐面（忠实性）

### 1.1 落地面与缺省未动

- berry_assemble.py 对 HEAD 唯一 hunk @1086-1094：`# sched_slice_hz = 64` → `# sched_slice_hz = 250` + 3 行注记（引 17 号三数 +「默认值改裁归 owner，样例只落推荐面。research/显示与加速体系设计/17 号 §4」）+ :1094 注释缩进整理（`#   etc/` 去 1 空格——C5 微面）。`--sample` 实跑输出逐行核：`[platform.boot.params]` 节全 `#` 注释行——样例不落活值；G5 语义 = 节非空才产 `etc/boot-params.toml` 条目（`--dry-run` 输出明文），注释节即空节，产品照抄样例也不会拿到 250。
- 缺省未动三源：①`arch/aarch64/src/timer.rs:29` `pub const SLICE_HZ: u64 = 10;` 原样（18 号 §2.1 同锚）；②`git diff -- arch/ kernel/ sched/ user/bins/guifbd/` 实跑空输出——保护路径零碰；③`kernel/src/bootparams.rs:20-29` 缺省零扰动合同在位未触。`250` 全文唯一出现位 = :1089 注释行（grep 实跑）；词表 :172 `"sched_slice_hz": ("u32", (16, 1024))` 原样。
- 数值回链：9.47→4.48s（17 号 §4 表 10/250 档链值）、吞吐 +26%（§0-1 配对口径 269.3/214.1）、QEMU CPU 100→84%（§4 表）逐项命中；追记自措辞「marker 链 ↓53%」= 1−4.48/9.47 = 52.7% 取整 ✓、「CPU ↓16pp」= 100−84 ✓；「10Hz 缺省 GUI 档错配」= 17 号推荐一原句义。
- 回归面：`--selftest` 全过（含「内置样例过检」行——改后样例仍过 v0 键集全谱检）；drill 演练 `tools/drill/acme/product.toml --dry-run` 仍出 `etc/boot-params.toml ← … sched_slice_hz=64`（acme 件零 diff——「词表/两键/治理半径零改动」的演练侧证据）。

### 1.2 产品配置层追记

- doc 对 HEAD 恰 +2 行（:220 追记段 + 空行）；§1.6 词表本体与全部其余节零 diff——追记自称「本句只落推荐面，词表/两键/治理半径零改动」属实。17 号相对链接目标存在；「`berry_assemble --sample` 样例注释行已同步该推荐值」双向兑现（§1.1）。

## §2 B 波复核：gfxdemo 部分包络（真 damage 帧源）

### 2.1 代码面与几何算术 [验]

- `const PARTIAL_ENVELOPE: bool = true;`（[gfxdemo main.rs](../../user/bins/gfxdemo/src/main.rs) :87，缺省 true）；绘制域 pass dirty = `Rect::new(0, half, w, h−h/2)`（:366-369）——1080×1920 档 = (0,960,1080,960) 下半屏；声明域 SUBMIT K=2 下半屏二分（:430-433：`q=lower/2` → {0,960,1080,480}+{0,1440,1080,480}），并集 = 绘制域——声明域 == 绘制域（不产声明外笔迹 = 真 damage）；包络率 = 1080×960/(1080×1920) = 恰 500‰。
- 机制面在码：[pass.rs](../../user/berry-gfx/src/pass.rs) :34-40 dirty 语义（写侧裁剪、区外结构性不可触碰）+ :45 全事实构造——「绘制侧 berry-gfx pass 本有 dirty 面而本程序此前未用」属实（旧码 `PassDescriptor::clear` = 无 dirty 整帧）；[guest.rs](../../user/bins/guifbd/src/guest.rs) :1391-1401 per-rect 首用一次性行（permille = env_area×1000/full_area）+ :682-697 px() 抽样分位点法（第 i 点取 rects[i%K] 内点，K=2 时恒落声明并集内）——「guifbd px() 抽样面零连带」在码。
- 编译分支选型注记核实：environ 空表桩/argv 恒空参的论断为笔记自况（未逐行复核 berry-libc/svcmgr spawn 面——标注：选型理由 [断]，行为面不依赖它）。

### 2.2 构建链与 rootfs 对拍 [验]

- rootfs.cpio（34975744B，11-14 23:05）内 bin/gfxdemo 以自写页对齐 newc walker（mkcpio 垫料 4096，[mkcpio.py](../../tools/mkcpio.py) :54-60——GNU cpio 标准工具不可列，18 号 B2 同注）抽出 = 349544B sha `cddb216b…`；按 [justfile](../../justfile) :222-224 精确配方（`--manifest-path bins/gfxdemo/Cargo.toml --target aarch64-unknown-linux-musl` + berry-ld RUSTFLAGS）独立 target dir 复编（2.90s）→ 与 cpio 件 **cmp 字节零差**——笔记「rootfs 内 gfxdemo 与新构建字节级同一」独立复现，且证 rootfs 内 = 含 PARTIAL_ENVELOPE=true 的现行源构建（源 mtime 15:41 早于构建 23:04）。

### 2.3 QEMU 三靴实跑 [验]

- 探针镜像：自建内核（HEAD 2904910a 内核源零 diff；`CARGO_TARGET_DIR=target-cargo cargo build` 增量 3.88s → 6720024B sha `68eb367e…`，banner `BerryOS boot 0.2.0 (2904910a-dirty rustc1.97.0 dbg)`——避开陈核坑）+ 现 rootfs.cpio + `tools/mkimage.py` 96MiB（镜像 sha `c82a3932…`）。QEMU 命令形逐段复刻 berry CLI [spawn_image](../../tools/berry/src/qemu.rs)（:179-285）：TCG/cortex-a72/256M/AAVMF 双 pflash（CODE readonly=on 经 -drive——见 C1 坑）/usb-storage snapshot=on/FAT12 数据盘（mkfat+BERRYOSVBLK! 印章）/tablet/force-legacy=false/vgpu 先于 ramfb（E 轮）/display fw_cfg `1080x1920@60`/rtc 注入；每靴轮询标记 + 超时帽收卷。
- **E 轮（vgpu）**：`FB ok vgpu slot=29 pmode=1280x800`（16 号同槽位）→ `VGPU2 reap ns=132000 k=2`（16 号带 0.108-0.137ms 内）→ **`VGPU2 rect k=2 env=1080x960+0+960 r=500‰`** → `GFXF ok n=7` → `GFX ok`，16s 全到齐；vgpu/VGPU2 行计数 3（= 笔记 E=3）。对照 16 号基线 `env=1080x1920+0+0 r=1000‰`：包络 = 恰下半屏、r 1000‰→500‰——**r<1000‰ 首实证独立复现，β-1 增量粒度参考锚在席**。
- **F 轮（ramfb，同形仅去 vgpu 设备）**：FBW ok/FBSERVE ok/WM ok/GFXF ok n=7/GFX ok 同形到齐（16s）；vgpu/VGPU2 行计数 0——**零漂移**（= 笔记 F=0，16 号 A/B 判读同款）。
- **陈核负对照**：`~/.cargo-target` 内核（`10ff7ef0…`，6717176B，11-14 02:01）组镜像（sha `eac21918…`）按 vgpu 形启动——banner `v0.2.0-197-g429675903-dirty`（笔记点名串）、`FB ok vgpu` 缺席（仅 `FB ok 1080x1920` ramfb 形）、vgpu/VGPU2 行计数 0、链仍全绿（14s 到 GFX ok）——「断 svcmgr→guifbd virtio 窗第二映射交接 → 缺省安全落 Ramfb（设计如此静默）」换核判别独立坐实；亦反证 E 轮 vgpu 行非偶然（同 rootfs/同设备面，唯一变量 = 内核）。

### 2.4 smoke 与宿主测试 [验]

- `timeout 150 env SMOKE_PHASES=boot tools/smoke-image.sh <探针镜像> <CODE 副本> <vars>` → **引导面标记 45/45 到齐，exit 0**（boot 相 = EFI 十段+接续 3+裸相 32+banner 形态+ident 非 nogit；生态 39/GFX 双/VER 哨兵不进 boot 相——与波轮口径同面）。CODE 固件经 `-pflash` 走 RW 打开被 DSH 文件 sandbox 拒（EPERM）——拷可写区副本（sha 同 `47765fe3…`）即过（C1）。
- 宿主测试：`cargo test -p berry-gfx`（host）**114 全过**；`cargo test --manifest-path user/bins/guifbd/Cargo.toml`（host lib）**13 全过**——与笔记宣称同数。
- smoke 断言零依赖核查：`tools/` 全量 grep `VGPU2 rect|r=1000|r=500` 零命中——rect 值变化不可能令 smoke 假红/假绿（笔记「smoke 断言面对 rect 值零依赖」成立）。

## §3 门禁与 git 对拍 [验]

- **门禁**：`python3 tools/docs_guard.py` → PASS（0 警）；`node .agents/tools/check-docs.mjs` → 235 篇 0 告警（本文 + README 索引行落档后复跑同绿）。
- **git 对拍（波轮写入 vs owner 在途，mtime + diff 内容双判）**：
  - 两波自身写入恰四件 + 副产物：`tools/berry_assemble.py`（M，单 hunk 纯注释面）、`docs/产品配置层与槽位合同设计-v1.1.md`（M，+2 行）、`user/bins/gfxdemo/src/main.rs`（M，11-14 15:41）、`.agents/notes/implemented/2026-11-14-gfxdemo-partial-envelope.md`（未跟踪，11-15 00:36）；构建副产物全 gitignored（rootfs.cpio/out//userbins/target-cargo/user cargo-target）。
  - 保护路径零碰：arch//kernel//sched//guifbd 对 HEAD 零 diff；launcher 两件 M mtime 11-13 04:34/04:38 早于波窗（berry_assemble/产品配置层 doc 15:32、gfxdemo 15:41），diff 内容 = `recents_app` 五键导航面（模拟器体验分刀波）——与 gfxdemo 部分包络无涉，owner 在途 M（18 号 §3 同判）。
  - 其余工作树 M（tools/berry/** VNC 波、justfile 11:48、docs 六件 12:44-12:53、berry-fsclient/hexyl/uu 11-13、.gitignore 01:02）mtime 全部早于波窗——非两波所写；17/18 号已随 `f07d86c2` 提交，research/显示与加速体系设计/ 工作树净。

## §4 发现汇总表（只验不修——归属 owner）

| # | 级别 | 位置 | 发现 | 影响 |
| --- | --- | --- | --- | --- |
| C1 | 环境坑（记录） | tools/smoke-image.sh :403-406 | `-pflash` 开 CODE 固件 = RW 打开——DSH 文件 sandbox 对 Homebrew Cellar 路径拒写 → QEMU `Operation not permitted` 即退；拷固件到可写区（字节同一）即过 | 后人复跑 smoke 的环境前置（mine 实测）；非波轮缺陷（波轮当时过门） |
| C2 | 挂账在册（注记） | 17 号 §6 | 18 号 B1（console-sender 注释锚 `sched/src/sched.rs:132` 错位，正锚 `arch/aarch64/src/timer.rs:132`）未修——17 号已随 f07d86c2 提交 | 引用失真延续；owner 域，本轮范围外 |
| C3 | 单靴值（微） | gfxdemo 笔记 §Verification | E 轮 reap ns=118000 为单靴值——本轮 132000，同在 16 号带（0.108-0.137ms）内，靴间/负载抖动 | 判读面（标记在席/包络值/行计数）不受扰；「同量级」结论两侧一致 |
| C4 | 镜像不可对拍（低危） | 同上 | 笔记探针镜像 `f52c71b2…` 的内核 = 已删干净 worktree 件（6720056B）不可考；本轮以 2904910a-dirty 自建核（6720024B）三靴全绿 | 结论对内核 dirty 标不敏感〔推断〕；镜像级逐字节对拍不可复现，如实挂账 |
| C5 | 顺手整理（微） | berry_assemble.py :1094 | 波轮 diff 含样例注释缩进整理（`#   etc/` 去 1 空格）——任务书面外微面 | 零行为影响（selftest 过）；忠实性无扰 |

## §5 复核方法限制如实

- **探针内核 = 2904910a-dirty 工作树自建**（非波轮干净 worktree 核）——banner/行为面三靴一致（E/F/负对照），但与波轮镜像不可逐字节对拍（C4）；用户态嵌入面（userbins 平像）取现工作树状态，与 HEAD 内核源的组合未经全量 `just build` 链（docs-guard/abi-check 由本复核分跑补偿）。
- **三靴各单跑**：判读面全为离散断言（标记在席/包络值/行计数/零漂移），单靴足以裁断；reap ns 连续量仅作带内佐证未双跑。
- **smoke 止 boot 相**（45 标记）——GFX 双不进该相，其实证由 E/F 轮直接承担（两形 GFXF ok n=7/GFX ok 双到齐）；VER 哨兵/kfile 字节对拍未过门（早退位于其后）。
- **TCG 单档 + 宿主并发场**（同 16-18 号限制）；LAUNCH ok 面未跑（TCG 饿死既有——12 号 §6/17 号 §5 同判，链口径止 GFX ok）。
- **零仓改**：本复核写入 = 本文 + README 索引行；全部探针/构建产物在 /tmp/b19 与 gitignored target 目录。

## §6 来源索引

- **本轮实测产物**（`/tmp/b19/`）：`probe.img`（`c82a3932…`）/`probe-stale.img`（`eac21918…`）、`{e,f,stale}-serial.log`（三靴串口全录）、`bootprobe.sh`（QEMU 复刻 harness——spawn_image 命令形 + 标记轮询 + 超时帽）、`extract_cpio.py`（4096 垫料 newc walker）、`gfxdemo-from-cpio`（`cddb216b…`）、`gt/`（gfxdemo 独立复编 target）与 `ht*/`（宿主测试 target）。
- **复核动作记录**：`CARGO_TARGET_DIR=target-cargo cargo build`（3.88s 增量）；gfxdemo 复编（justfile :222-224 配方 + 独立 target）；mkimage/mkfat+印章/vars 三件；QEMU 三靴（16/16/14s 到齐）；`timeout 150 SMOKE_PHASES=boot smoke-image.sh`（45/45）；cargo test 两 crate（114+13）；`berry_assemble --selftest/--sample/--dry-run`；docs_guard + check-docs；git diff/status + mtime 对拍。
- **仓内（路径:行号）**：`tools/berry_assemble.py` :172/:1086-1094/:1374/:1661-1687；`arch/aarch64/src/timer.rs` :29；`kernel/src/bootparams.rs` :20-29；`user/bins/gfxdemo/src/main.rs` :87/:366-369/:425-437；`user/berry-gfx/src/pass.rs` :34-45；`user/bins/guifbd/src/guest.rs` :682-697/:1391-1401；`tools/berry/src/qemu.rs` :133-145/:179-285；`tools/mkcpio.py` :54-60；`tools/smoke-image.sh` :88-106/:403-408/:416-458；`justfile` :222-224/:1319-1335；`docs/产品配置层与槽位合同设计-v1.1.md` :220。
- **被复核件**：两波工作树 diff（§1/§2 所引全部行号）；[gfxdemo 部分包络笔记](../../.agents/notes/implemented/2026-11-14-gfxdemo-partial-envelope.md)（B 波实施记录——其 Verification 各宣称逐项对照 §2）；[17 号](17-slice调参GUI实验.md) §4（推荐源）。
- **谱系**：[18 号](18-定标与调参复核.md)（复核体例先例 +「现役链无 r<1000‰ 触发」点名——B 波回应）；[16 号](16-v2定标与icount接入.md)（r=1000‰ 基线/reap 带/slot=29 锚）；[12 号](12-B1性能定标-双后端实测与v2模型.md)（LAUNCH 饿死与 HVF 缺口先例）；README.md（目录索引）。
