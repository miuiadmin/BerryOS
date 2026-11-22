# B 线：显示 + 桌面遗漏审计——承诺 vs 实现逐项对拍（刀 1-9 / 桌面 UX 六痛点 / β-1 壳半 / 调度集成指南）

Type: research
Status: current
Updated: 2026-11-15

> 审计对象：显示与加速体系设计（docs/显示与加速体系设计-v1.0.md 刀 1-9 与十裁）+ research/显示与加速体系设计/01-19 号 + research/桌面UX审计/01-04 号的全部承诺、裁决、建议，逐项对拍 2026-11-15 19:50 工作树实现态。判定四档：已实现 / 骨架在 / 未实现 / 超预期（实现了文档没写的）。方法：文档承诺清单化 → 逐项 grep/读码核对 → 双源交叉（研究件实测记录 + 现树代码行号）；外部事实不重取，引研究件的取证；推断显式标注。本件只写自己，零仓内其他改动。

## 0. 一页结论

1. **九刀落位：2 刀兑现（刀 3 S1 spike、刀 5 B1-a guest 半——均有实测复核件与现树代码双证）、v2 与 icount 两件超预期旁支兑现、其余 6 刀主体未动**。刀 1（1080P 初始闭环）四子项全未按设计落——方向对账的 owner 实裁是**竖屏**（与 docs v1.0 裁六「横屏」相反，文档未回填）、窗口 1:1 未落、launcher 断点 vp 化未落、README 档位表未对账；且现树 1080P 桌面带两颗活病（a11y 34>32 恒黑 + ramfb 1080 非 32 对齐扫出断裂，修均在未回合分支）——**刀 1 验收线在现树必不过**。刀 6/7/8（壳半 wgpu、动态分辨率、β-1）零落地，berry CLI 连 virtio-gpu 设备都未挂。
2. **桌面 UX 六痛点：五痛有实质交付、一痛（⑤按压反馈）库层备好消费面空置**。①窗口逻辑点口径、②导航（三键承诺→五键落地，超预期）、③顶栏清屏+scrim、④真时钟/scrim/SIM 全链、⑥比例 advance 三面均已实现在树；⑤按压反馈抬帽至 28 已备但 `set_press_map` 全 launcher 零调用；⑥的 kerning/抗锯齿/CJK 与 ④的冻结相 FACES 存量 bug、SYSTEM 窗（I1）仍挂账。
3. **β-1 壳半：研究全 GO（11 号六环节实证 + 15 号 wgpu PoC GO + 13 号终裁立为 Mac 终态通道），壳集成零开工**。差的是 owner 面五件：D-Bus p2p 客户端、wgpu 呈现骨架、Frame damage 载荷、virtio-gpu 设备接线与 console 绑定序、softbuffer/轮询退役——且 2026-11-14 模拟器方案规划把产品窗口档转向 Apple VZ，QEMU 线的 β-1 壳半事实上降位为「日常验证主力线的待办」。
4. **显示管线调度集成指南 v1.0：工具面承诺全部属实（itally/BENCHRWAKE/bench --lat/policy 工具链/bench-icount 全在树），但指南的核心号召——显示管线任务自标 intent 0x02——零采纳**（svcmgr 对 guifbd spawn `intent: 0`，svcmgr/main.rs:1222；intent 透传首件只接了 app manifest 面）。指南 128 建议 vs 17 号 250 推荐的漂移未回填。
5. **最大结构性遗漏 = 设计承诺的「性能=能用」主链停在半程**：传输/呈现卸载研究链（10-13 号）把下一刀定死为「B1-b 壳半→β-1 通道」，现树零兑现；实际投入走向了 VNC 十一波产品化（web 桌面/TLS/LAN/打包）——超预期但方向不同，且 12 号已证 VNC 出口 33fps 结构硬顶，LAN 演示「卡」的三成架构账正落在未兑现的这半链上（模拟器性能分层诊断.md）。

## 1. 刀 1-9 逐刀对拍（docs/显示与加速体系设计-v1.0.md:87-97 刀序表）

| 刀 | 承诺（验收线） | 判定 | 依据（两源） |
|---|---|---|---|
| 1 | 1080P 初始闭环：①方向对账 ②窗口 1:1 ③launcher 断点 vp 化 ④README 对账 | **未实现（含裁决翻转与两活病）** | 下文 §1.1 |
| 2 | HVF 补缝三件 + 可选 hvf 冒烟档 | **主体未实现（两缝被别线部分消化）** | §1.2 |
| 3 | S1 spike 三验合一，产 B1 排刀终案 + 4K 解锁判据回填 | **已实现（go），产出留一缺口** | §1.3 |
| 4 | Profile 增 density + desktop2k/4k 词表 + fw_cfg 扩 density 段 | **未实现** | §1.4 |
| 5 | B1-a guest 半：berry-virtio-gpu crate + guifbd 后端 + `VGPU ok` | **已实现** | §1.5 |
| 6 | B1-b 壳半：winit+wgpu 骨架 + Frame damage 载荷 + softbuffer 退役 | **未实现（零开工）** | §1.6 |
| 7 | B1-c 动态分辨率 + density 事件流（切档不重启） | **未实现** | §1.7 |
| 8 | β-1 终态 + icount/墙钟双轨 + CI 矩阵（macOS 主）+ 4K 解锁评估 | **icount 一件已实现，余未实现** | §1.8 |
| 9 | product.toml `[platform.display]` 立键 + 组装投放 + lint | **未实现** | §1.9 |

### 1.1 刀 1：1080P 初始闭环——四子项全未按设计落，验收线现树必不过

- **①方向对账（裁六冲突，owner 一句定谳）：已定谳但定成了竖屏，与 docs v1.0 裁六相反，文档未回填**。现树 `tools/berry/src/profile.rs:30-40` desktop 档 = 1080×1920 竖屏，owner 注原文「竖屏 1080P 手机主形态（owner 2026-11-13 裁定：初始分辨率即 1080P、竖屏 + 底部导航键，不做低清/横屏妥协）」；`justfile:1427` `just desktop` 优先取 `out/berryos-portrait.img`（竖屏镜像）。而 docs v1.0:73 裁六定「desktop 档 = 1920×1080 横屏」，其 §5 重议触发器 8（方向翻案：档位值翻转、体系不变）首用无记录——设计文档至今未翻状态。开放问题①指出的两注冲突也原样留在树内：`tools/berry/src/main.rs:888-890` 双击/无参入口注释仍写「横屏 1080P + 硬件加速」（与 profile.rs 竖屏注字面相反）。判定：裁决已落（竖屏），文档面欠一次回填 + 一行注释清理。
- **②窗口初始 1:1（0.9/0.55 因子与 0.9 屏幕钳制退役）：未实现**。`tools/berry/src/window.rs:202-213` `base_window_factor` 仍是 `panel_h > 1200 ? 0.55 : 0.9`，`:224-243` `default_window_size` 仍带 0.9 屏幕钳制与 0.2/320×240 下限——全部保留（只是波一 A 刀把口径从物理像素换成了逻辑点，见 §2-①）。单测面同步未翻新：`window.rs:1206-1216` `hidpi_desktop_window_is_logical_not_physical` 仍断言 desktop 档 486×864 逻辑点（0.9 基准），且测试入参还是旧 desktop 面板 540×960（`window.rs:1211`），与现 profile 1080×1920 脱节。docs v1.0:89 刀 1② 的「因子/钳制退役 + 单测翻新」整件未动。
- **③launcher 断点 vp 化过渡（desktop.rs:317 判据改按 vp 宽）：未实现**。`user/bins/berry-launcher/src/desktop.rs:317` 仍 `let big = vp_w >= 600 && vp_h >= 1800`（竖屏 1080×1920 专属像素阈，03 号:50 点名的病灶原样）；`desktop.rs:98` `OH_DENSITY: u32 = 3` 硬编原样；berry-ui 无 `vp()` 单点换算函数（全仓 grep 零命中，推断：未开工）。注意：owner 裁竖屏后此判据在 1080×1920 档恰好命中 big 分支（「病灶」暂时无害化），但 03 号裁八要求的体系化（断点 vp 化/density 单源）对 2K/4K/OEM 档仍是前置欠账。
- **④README 档位表对账：未落**。`tools/berry/README.md:186` desktop 行仍是旧态「540×960 | 1080×1920」（面板 540×960 早已不存在于 profile.rs），03 号:31 点名的唯一未同步件至今未同步。
- **验收线现状：不过，且带两颗活病**。docs v1.0:89 验收「`just desktop` 开窗 1080P 逻辑尺寸 1:1；1080P 桌面走 md 档布局」——窗口层不是 1:1（见②）；客机层现树 1080P big 布局 a11y 账 34 > `A11Y_MAX=32`（`user/berry-ui/src/lib.rs:211` 现值 32；desktop.rs 五键 :755-759 + 4 状态 + 4 dock + 1 页容器 + 20 格 = 34，desktop.rs:782-786 的「3 键恰满 32」注释已过时）→ `NodeFull` → launcher 建树失败 park → 恒黑（`.agents/research/2026-11-15-1080p恒黑根因会审.md` 断点链，修复 `01bb3e37a`（A11Y_MAX 32→40）在 wip/boot-events-knife6 未回合）；其上还有第三层断点——ramfb 宽 1080 非 32 对齐使 QEMU surface 按 1088 建而扫出断裂（同文「终审补记」，对齐补丁只在 /tmp 未入库）。两病叠加意味着**现树按刀 1 验收线跑 `just desktop` 必黑屏**，1080P「初始不妥协」目标在客机侧当前是负资产状态。

### 1.2 刀 2：HVF 地基补缝——三缝两缝未补，被后续线部分消化

- **①spawn_kernel 补注 `opt/berry/accel` 键（1 行级）：未实现**。`tools/berry/src/qemu.rs:161-176` `spawn_kernel` 全函数无任何 fw_cfg 注键；注键只在 `spawn_image`（`qemu.rs:273-276`）。01 号:59 实证的后果（`berry run --accel hvf` 走到 FPING 即 FPSR panic）现树应仍复现（推断，未重跑——判例机制 ktest 门控只认键，键不注则断言不跳）。
- **②DPG panic（原 dpg.rs:719）立账调查：原样未结，但被北极星线部分消化**。断言本体仍在树（`kernel/src/dpg.rs:774`「第 {} 页非 pager 供给值」）；后续演化：§A149 判例（HVF EL1 写 EL0 用户页）+ 2026-11-15 北极星审计三刀把同类 HVF fatal 的根因修到 `current_asid` 读点迁 CONTEXTIDR（`arch/aarch64/src/mmu.rs:277-301` 现树在席，`.agents/notes/implemented/2026-11-15-北极星审计demo-off-fatal虚化三刀.md`）。判：刀 2② 的「立账调查」以更大范围的加速器语义差调查被消化，但 dpg_press 在 HVF 原生速度下仍可 assert（北极星笔记「HVF 残余两件时序标定 panic」段如实立账）——未关账。
- **③soak 反转机制定谳（HVF ≈5× 慢于 TCG）：未定谳**。全仓 grep「soak 反转」后续零命中（01 号:115 开放问题②原样）；16 号只收口了 idle 满核归因（boot 演示常驻面，引 06 号调度），与 soak 反转是两件事。
- **④可选 hvf 冒烟档：无**。justfile 无 hvf-smoke 类配方（grep 零命中）；`berry-smoke`（justfile:1440-1448）仍 TCG 确定性口径。补偿面（别的线落的，非刀 2 本体）：`qemu.rs:282` GUI 家族默认注入 `opt/berry/noktest`（2026-11-15 owner 裁决）+ kernel demo-face 门控（`kernel/src/main.rs:749-816`）——两者把 07 号/09 号的 boot 期 ktest/演示常驻面饥饿大幅削掉，但 HVF 桌面主路径至今仍被 §A149 类 fatal 挡住（`.agents/research/2026-11-15-模拟器性能分层诊断.md` 第 0 层表：三构建全死，HVF 无一颗健康内核可测）。

### 1.3 刀 3：S1 spike 三验合一——已实现（go），产出留一缺口

- 三验全过且有独立复核：设备验（TCG 串口实收 `vprobe: slot 28 did=16` 非静默枚举）、传输验（缩口径 `-display none + -vnc` 存活）、HVF 验（UEFI 载体双 boot `ktest done`×2）——05 号实录 + 06 号独立复跑双源（research/显示与加速体系设计/05-S1spike-三验实录.md §0、06-刀三与字体复核.md §1）。
- 残缺口：刀 3 产出栏的「4K 解锁判据回填」（docs v1.0:91）至今无显式落盘——06 号:66 点名后，全仓除 03 号设计承诺原文外零新增（本审计 grep 核实）；B1 排刀终案以 05 号 §6「五条输入」实质存在但未按名落档。
- 附产物三硬前提（vprobe 白名单/force-legacy=false/设备全名）已被刀 5 吸收为现树代码（vprobe 白名单、`qemu.rs:253-255` force-legacy 注入），实证闭环。

### 1.4 刀 4：档位体系补全——未实现

Profile 结构无 `density` 字段（`tools/berry/src/profile.rs:8-16` 六字段：panel/render/refresh/memory）；档位表无 desktop2k/desktop4k（`profile.rs:26-77` 六档原样）；fw_cfg display 串无 density 段（`qemu.rs:266-268` 只 `{mode}`）。03 号 §2.1-2.3 的档位表定稿、值域 lint 全未动。连带：竖屏翻案后 desktop4k 的 3840×2160 横屏形态是否仍成立也成悬案（推断——03 号档位表以横屏定调，方向翻案未回填波及此表）。

### 1.5 刀 5：B1-a guest 半——已实现（本线兑现质量最高的一刀）

- berry-gpuproto 协议 crate：08 号六验全过（uapi 七组对照 + round-trip 29 测），现树 `user/berry-gpuproto/` 在席。
- guifbd 双后端：09 号判过（ramfb 红线 974 行逐字节保序 + vgpu 主链双拓扑三槽位 + HDI 契约零改动）；现树 `user/bins/guifbd/src/guest.rs` vgpu 模块、svcmgr 交接（`user/bins/svcmgr/src/main.rs:1205-1232` SpawnSpec 带 stdin_handle/mmio_vmo）在席。
- 超出刀 5 承诺的加成（详见 §5）：v2 per-rect flush + 异步化提交（853ac85c，16 号定标 reap 0.111-0.137ms + 5.8k 指令）、vprobe 白名单、`VGPU2 rect` 观测行、gfxdemo 部分包络真 damage 帧源（19 号独立复现 r=500‰）。
- 但注意：**berry CLI 宿主侧从不挂 virtio-gpu 设备**（`tools/berry/src/qemu.rs:264` spawn_image 仅 `-device ramfb`；全 tools/berry grep virtio-gpu 零命中）——vgpu 后端在默认 berry 链路上永不激活（`FB ok vgpu` 只在 BERRY_QEMU 包装或手动命令下出现，12/16 号实测即此形态）。即：guest 半就位、宿主接线未接，双后端的「主」路当前只活在研究探针里。

### 1.6 刀 6：B1-b 壳半 β-0（wgpu 呈现）——未实现，零开工

- `tools/berry/Cargo.toml` 依赖面：winit 0.30 + softbuffer 0.4 + rustls + qrcodegen——**无 wgpu**；`window.rs:1/9/990` softbuffer 仍是唯一上屏路径。
- Frame damage 载荷：`tools/berry/src/display/frame.rs:29-36` Frame 结构仍只有 seq/width/height/pixels——02 号:66 预留的 `damage: DamageSet` 字段位（壳草案 §5.3）未落，`Caps.damage` 仍是无载荷的 bool（frame.rs:23）。
- 13 号:32 重排后的第一刀（B1-b 壳半 = β-0 收尾 + 60Hz pread 轮询退役 + wgpu 骨架）三件全无。12 号:97 证实的「双通道并存付双份」现状在现树原样（壳轮询照跑）。
- 替代性进展（超预期、不同方向）：VNC 十一波产品化——`berry vnc web`（noVNC 壳页）/TLS/VeNCrypt/ws/LAN/QR 配对/Linux AppDir/AppImage/tarball/Windows 交叉打包（justfile:1450-1500 berry-vnc-smoke/berry-vnc-tls-smoke、tools/berry/README.md:225、.agents/plans/2026-11-13-vnc-desktop-suite.md 等）。这把 β-0 的「VNC 过渡档」反方向做成了产品面；12 号:12 已证 VNC 出口 30ms 刷新定时 = 33fps 结构硬顶，60fps 红线在该通道上不可达——产品化投入与「性能=能用」目标之间存在结构张力（模拟器性能分层诊断.md 第 3 层表把「ramfb 全帧通路」列为架构上限之一）。

### 1.7 刀 7：B1-c 动态分辨率 + density 事件流——未实现

guifbd 无 events_read/GET_EDID/CONFIG_GEN 消费（guest.rs 全文 grep 零命中——02 号:75-77 的事件面只用了 INIT_SEQ 静态初始化）；info VMO 无 density 第 7 域（guifbd/内核 grep density 零命中）；壳无 ScanoutMap/Resize 接窗（见 §1.6）。「运行中切档 480×1000↔1080×1920 不重启」验收线零兑现；01 号钉的「换分辨率需重启」现状未变（RES 菜单仍是 QEMU 全重启语义，01 号:23）。v2 落的 per-rect 是传输面（13 号排刀第 3 刀的素材），不构成刀 7 本体。

### 1.8 刀 8：β-1 终态 + 双轨遥测 + CI 矩阵 + 4K 评估——icount 一件兑现，余未实现

- **icount/墙钟双轨性能遥测：已实现（超预期旁支）**——`tools/bench.py --icount` 通道 + `tools/bench_baseline_icount.txt` 基线 + justfile:1125 `bench-icount` 配方在席；16 号首接入显示/调度面（VGPU2 reap 5,808 指令定值、switches@marker 指纹）。注意其形态是 bench/串口观测面，非 docs v1.0:96 设想的「性能标记进遥测」常驻面（推断——刀 8 原文的遥测面无对应实现）。
- β-1 D-Bus p2p 客户端：未实现（tools/berry 无 dbus/jeepney/GLib 依赖，grep 零命中）。
- VNC loopback 输入退役：未实现（VNC 反而是投入重镇）。
- CI 矩阵（macOS 主）：未实现——`.github/workflows/ci.yml:39/392` 仍全 `[self-hosted, linux-x64, brew]`。
- desktop4k 解锁评估：未做（连带刀 4 未立词表，无评估对象）。

### 1.9 刀 9：product.toml `[platform.display]` 立键——未实现

`tools/berry_assemble.py` 词表 v1 仍只有 `sched_slice_hz` / `intent_default` 两键（`berry_assemble.py:172-173`），grep「platform.display」全仓零命中；03 号 §2.2-2.3 的 override 子表、etc/display.toml 内核回退源、sidecar、组装期 lint 全未动。#83 升版挂接无从谈起。

## 2. 桌面 UX 六痛点终态（research/桌面UX审计/01-04 号承诺 → 现树）

| 痛点 | 承诺（修法） | 终态判定 | 依据 |
|---|---|---|---|
| ①窗口（逻辑点口径/1:1/竖屏裁定） | 01 号修法 a（逻辑点）+ docs 刀 1②（1:1） | **逻辑点口径已实现；1:1 与波二清单未落；竖屏裁定已落但文档未回填；现树 1080P 带恒黑两病** | §2.1 |
| ②导航（02 号 M2 三键/M3 关机） | 三键 44px + berryctl power + PSCI | **已实现且超预期（五键）；EFI 相关机=诚实降级（I3 未落）** | §2.2 |
| ③顶栏（03 号 B1 清屏 + scrim 归 02 号） | gfxdemo 退出清屏 + 状态栏衬底 | **已实现** | §2.3 |
| ④状态栏（02 号 M1 真时钟/scrim/SIM） | 宿主注入→RTC→CLOCK_REALTIME→每分钟摆字 | **已实现全链；FACES 冻结相存量 bug 未修（更正 2026-11-17：已修，见 §2.4 注）；SYSTEM 窗（I1）未落** | §2.4 |
| ⑤触摸（03 号 C 刀按压反馈+toast） | press_map 接通 + 抬帽 + spawn toast | **未实现（抬帽至 28 已备、消费面零调用）** | §2.5 |
| ⑥字体（03 号 A 刀比例 advance） | 烘焙/图集/排布三面 + kerning 挂账 | **三面已实现；kerning/抗锯齿/CJK 挂账维持；状态栏手算宽残留** | §2.6 |

### 2.1 痛点①窗口

已实现：逻辑点口径（`window.rs:195-246` 纯函数 + LogicalSize，04 号:23-28 对拍 21 单测绿）；owner 竖屏裁定落 profile（§1.1-①）。未实现：窗口 1:1（docs 刀 1②，§1.1-②）；波二清单——TOOLBAR_H 48 物理像素逻辑化（`window.rs:26` 原样）、窗口居中、ScaleFactorChanged 重排（`window.rs` 该分支仍只重绘）、RES 菜单语义文案（01 号 §3c-②）——04 号:106 列的波二面现树全部未动（逐项 grep 核实，推断：无人认领）。「显示不到下面」01 号已双端证不复现，结案。

### 2.2 痛点②导航

已实现且超预期：02 号 M2 承诺三键（back/home/power），现树为**五键** back/recents/home/rotate/power（`desktop.rs:755-759`）——recents 有真语义（多任务键记忆最近 spawn 成功 app，`frameloop.rs:269/792-796`），rotate 为转屏键（推断语义归「模拟器体验六波」owner 线，19 号:58 记 launcher 在途 diff 即此面）；命中区 big 160px（owner「大点」裁定，`desktop.rs:143-146`——超 44px 硬指标）；berryctl `power` 子命令 + `sys_power_off`（0x88）+ PSCI SYSTEM_OFF 调用面全落（04 号:89）。未落：EFI 桌面主路径 conduit 恒死（`arch/aarch64/src/psci.rs:94-105` efi_boot 分支原样）→ 用户桌面关机仍是诚实降级提示（02 号 I3 关机流程正规化未做）；应用内返回（I2 系统键语义）、SYSTEM 窗（I1）未做（`desktop.rs:49` 注「SYSTEM 窗随 I1」）；三键可视 E2E 验证顺延项（04 号:154）未见补验记录（grep 零命中）。

### 2.3 痛点③顶栏

已实现：gfxdemo 退出清屏（`user/bins/gfxdemo/src/main.rs:566` `fb.fill(0)` + FULL SUBMIT + FRAME_CALLBACK + RELEASE——03 号 B1 最小选「gfxdemo 自清」支）；壁纸蓝透状态栏主因由 scrim 消（§2.4）。未动（设计裁量/挂账）：桌面图标蓝系配色（03 号 §2.4-4，genohicons.py 色表原样）；label ASCII 立法待段④ CJK 解禁（appstub manifest label 仍英文，挂账维持）。

### 2.4 痛点④状态栏

已实现全链（五段）：宿主 epoch 注入（`qemu.rs:286-289`）→ 内核 `rtc.rs` 单次锚定 → `CLOCK_REALTIME` id 域（0x61 add-only，零新号红线守住）→ launcher 首摆 + 每分钟摆字（`frameloop.rs:303-312`）→ scrim `STATUS_SCRIM #035E63`（`desktop.rs:134`）+ SIM/UTC 角标（`desktop.rs:137-139`）+ a11y「simulated」如实标注（`desktop.rs:462-483`）——04 号 §2 判过 + 现树代码双证。未落：**冻结相 FACES 存量 bug 未修**——04 号:75 点名一行修（init(0) 分支补 `FACES.set(status_faces())`），现树 `user/bins/berry-launcher/src/icons.rs:158-160` `if app_h == 0 { return; }` 仍早退不设 FACES（FACES.set 只在 pull 失败分支 :163）——无数据盘拓扑下状态栏仍无图标（更正 2026-11-17：已修——6e1697170 首修 init(0) 分支补 set_faces、b9e7d4b64 波3 迁移携行，现行位 user/berry-appshell/src/icons.rs:273/:281，终判在 [M-最终判定](M-最终判定.md) §2.1 #17；本段 :158-160/:163 行锚属迁移前旧文件）；simbattery 真值接入（I4）未做；SYSTEM 窗升格（I1）未做；本地时区（UTC 挂账波二）未做。

### 2.5 痛点⑤触摸

未实现：03 号 C 刀两件（按压反馈接通 + spawn toast）均未落——`set_press_map` 在 launcher 全树零调用（grep 实证），`desktop.rs:22` 注释「press_map v1 不做（视觉反馈归 W2 spawn 回执标记行）」原样，spawn 回执标记行也无对应实现（frameloop grep toast/回执零命中，推断：未开工）。已备：`PRESS_MAP_MAX` 从 16 抬到 **28**（`user/berry-ui/src/lib.rs:295`，超出 03 号建议的 24——库层容量就绪）；链路本身 03 号已证通（`CLICK ok n=1` 在册）。判定：库层骨架在、消费面空置——六痛点评级里唯一停在「研究完成、实施零开工」的一条。

### 2.6 痛点⑥字体

已实现：比例 advance 三面全落且过复核（06 号 §2）——烘焙面 `ADVANCE_HD` v2 追加段 + 双黄金哈希（`tools/genhdfont.py:14/100`）、图集面 trait 缺省 + HD 覆写（berry-gfx font.rs）、排布面 `ProportionalTextMetrics` 换装（`user/bins/berry-launcher/src/main.rs:257`）+ R2-8 判据侧兜底。挂账维持（06 号 §2.5 原样）：kerning、灰度抗锯齿、CJK 显示名（段④）；`desktop.rs:404-408` 状态栏宽 `len × cell_w()` 手算等宽残留（06 号:111 点名，owner lane，现树原样——grep 核实该算式仍在）。

## 3. β-1 壳半差距清单（研究 GO、壳集成未做——owner 面差什么）

研究侧三件全 GO：11 号 D-Bus p2p 六环节 macOS 全链实证（QMP getfd/add_client → RegisterListener → ScanoutMap 共享图 mmap 实收 → UpdateMap → Mouse.SetAbsPosition；memfd→mac = mkstemp 匿名文件 + MAP_SHARED 定谳，02 号降档线退役）；15 号 wgpu 30.0.2 PoC GO（Metal 后端、直配 write_texture 三几何 readback 逐字节 PASS、damage K=8 帧 25-30µs、帧预算账过线）；13 号终裁 β-1 立为 Mac 呈现终态通道（VNC 降诊断通道）。**现树壳集成 = 零**。差的是 owner 面五件（按 13 号 §2 排刀第 1-2 刀 + 11 号 §4-2 八步拆解）：

1. **virtio-gpu 设备接线与 console 绑定序**（β-0 宿主侧第一刀，09 号注记二）：`qemu.rs:264` 仍 ramfb 单设备且 ramfb 先声明——gpu 不挂、VNC 面恒黑的问题链根本未进 berry CLI。这是全部壳半工作的前提。
2. **Frame damage 载荷 + wgpu 呈现骨架**：frame.rs 无 damage 字段；无 wgpu 依赖；softbuffer 未退役；60Hz pread 轮询未退役（12 号「双通道并存」现状延续）。
3. **D-Bus p2p 客户端**（手写线协议子集或 GDBus，02 号:147 估 0.4-0.8k 行）：零开工；11 号 §5-5 的 UnknownObject 竞态仍是无答案的开放风险（若选手写路线）。
4. **输入反走 + VNC loopback 退役**：零开工。
5. **终验面**：「QEMU 实连 + ScanoutMap 实收」验收断言、60fps 帧源压顶（12 号 §6 未测面）——依赖前三件。

背景性风险（推断，需 owner 表态）：2026-11-14 `.agents/plans/2026-11-14-模拟器方案规划.md` 把模拟器分三层——QEMU+TCG 降 CI、QEMU+HVF 日常主力、**Apple VZ 产品窗口（GPU 加速显示 + 自有窗口）**，P2 明言「VZVirtioGraphicsDeviceConfiguration + guest 半 B1 系对接」。即产品级 GPU 呈现的战略承载从「QEMU β-1 壳半」部分转向 VZ；QEMU 线的 β-1 是否仍按 13 号排刀兑现、还是降位为 VZ 的过渡投资，docs v1.0 未更新、无裁决记录——这是刀 6/7/8 长期悬空的结构原因，建议随 v1.0 升版一并定谳。

## 4. 显示管线调度集成指南 v1.0 对拍（docs/显示管线调度集成指南-v1.0.md）

| 指南承诺 | 现状判定 | 依据 |
|---|---|---|
| §1.2 声明通道当前可用（svcmgr/launcher spawn 链 setattr 0x02） | **机制属实；显示管线零采纳**——svcmgr 对 guifbd spawn `intent: 0`（svcmgr/main.rs:1222）；guifbd/gui-wmd/launcher/gfxdemo 无一家声明 INTERACTIVE | berry-exec :711 SCHED_FLAG_INTENT 通道在席；采纳面只有 app manifest（intent M4 透传首件，.agents/notes/implemented/2026-11-15-intent-m4-svcmgr透传首件.md——APPDEMO it 03/APPQUIET it 04） |
| §1.3 intent_default = interactive 产品级缺省 | 词表在席（berry_assemble.py:173）；**缺省值仍是 utility 而非 interactive**——样例注释未改，采纳为零（推断：无人配置） | berry_assemble.py:173 注「缺省 utility」 |
| §1.4 intent v1 无 pick 序消费点、band 带键可用、消费点随 v2 | 属实且与现树一致（M4 band 骨架纯面笔记在册；北极星线外无新消费点） | 指南 :60-63 vs .agents/notes/implemented/2026-11-15-intent-m4-band-骨架纯面.md |
| §2 policy 注入（weight +32 / EPP Perf）当前可用 | **工具链属实（policydemo/advisor/policy_ab_eval 全在树）；显示服务条目零注入** | tools/policydemo、tools/advisor.py、tools/policy_ab_eval.py 在席；无任何显示服务 policy 表落地记录（grep 零命中） |
| §4.2 sched_slice_hz = 128 建议 | **建议已被 17 号实测数据超越未回填**——17 号推荐 250 均衡默认/1000 响应优先（10Hz 缺省 GUI 档错配实锤）；19 号已把 250 落进 berry_assemble 样例注释与产品配置层追记；指南仍写 128 | 指南 :129 vs 17 号 §4、19 号 §1；指南 2026-11-14 落盘后未更新 |
| §5.1 tele: intent_tally | 属实——itally crate + `intent_tally` 行装配在席 | itally/src/lib.rs:18-27 |
| §5.2 BENCHRWAKE / bench.py --lat | 属实——BENCHRWAKE 在 kernel（main.rs:4396）；--lat 通道在 bench.py:25-26 | 两源 |
| §5.3 just bench-icount | 属实——justfile:1125 | — |
| §4.2「帧循环负载画像已入册（CP-12 验收线），显示团队实测数据直接喂这条线」 | **半兑现**——16/17 号已产出显示负载的调度数据（switches 指纹/slice 调参），但 CP-12 消费侧（M4 EEVDF 消费点）未开窗 | 16 号 §4.4、17 号全文 vs .agents/notes M4 系列 |

指南对拍总判：**「当前可用」清单全部属实（无虚诺），但指南的靶心用户（guifbd/wmd/launcher/gfxdemo）一条都没接**——指南本身更像「能力说明书」而非「已接线记录」；另有一处 128→250 的数据漂移待回填。这与 §1 的刀 6/7/8 悬空同构：显示线的调度接入卡在「指南已写、消费未动」。

## 5. 超预期清单（实现了设计文档/审计件没写或超出承诺的）

1. **v2 per-rect flush + 异步化提交**（853ac85c）：docs v1.0 刀序里无此刀（它是 12 号 §4.3 发现爆点后的新增刀），13 号 §2 排为第 3 刀——**实先于刀 6/7/8 落地**，16 号三面定标 + icount 定值（reap 0.111-0.137ms / 5,808 指令）验收。
2. **gfxdemo 部分包络真 damage 帧源**（`PARTIAL_ENVELOPE=true`，gfxdemo main.rs:87；19 号独立复现 `VGPU2 rect k=2 env=1080x960+0+960 r=500‰`）——回应 18 号点名，r<1000‰ 首实证。
3. **VNC 十一波产品化**：web 桌面（noVNC 自有壳页）/TLS/VeNCrypt/ws/LAN/QR 配对/三平台打包（Linux AppDir/AppImage/tarball + Windows 交叉）——远超 docs v1.0 对 VNC 的「过渡档/诊断通道」定位（方向张力见 §1.6）。
4. **noktest fw_cfg 注入 + demo-face 门控**（qemu.rs:282；kernel main.rs:749-816）：削掉 boot 期 ktest/演示常驻面饥饿——07 号/09 号当时按「既有缺口挂账」记录的问题被体验六波线顺手解决。
5. **intent M4 透传 + band 骨架**（app manifest → SpawnSpec → sched_setattr → Task 双字段；带定义/实例化面）：指南成文同期落的地基，超出指南「声明通道可用」的最低承诺。
6. **五键导航**（超 02 号 M2 三键承诺，含 recents 多任务语义与 rotate 转屏键）。
7. **icount 定值通道接入显示/调度面**（16 号，bench.py --icount + VGPU2/BENCHWAKE/switches 三观测面）——docs v1.0 刀 8 遥测面的先行兑现（形态为 bench 面非常驻遥测，见 §1.8）。
8. **北极星三刀的 HVF 语义修**（current_asid 迁 CONTEXTIDR 等）：超出刀 2② 的原始账面，把 HVF fatal 族往「HVF 档语义 = TCG」推进（仍有残余两件时序 panic，未全通）。
9. **berry-ui 组件 P1/P2 数据层多批**（2026-11-15 系列笔记）与波 2 控件底座、波 4 settings 真应用——桌面 UX 审计 01-04 号范围之外的 UI 面扩展。

## 6. 汇总与遗漏排序（按对「性能=能用 / 1080P 初始不妥协」两目标的伤害度）

1. **P0：现树 1080P 桌面双病未回合**（a11y 34>32 恒黑 + ramfb 32 对齐断裂）——owner 已有修复在 wip/boot-events-knife6，回合即愈；不回合则「初始 1080P」目标整体倒挂（.agents/research/2026-11-15-1080p恒黑根因会审.md）。
2. **P0：壳半零开工（刀 6/8）**——60fps 差距榜首（VNC 33fps 硬顶 + 段 H 双通道旧税）自 12 号定谳至今无一刀落地；VZ 转向若成立需正式裁决并回填 docs v1.0（§3）。
3. **P1：berry CLI 不挂 virtio-gpu 设备**——guest 半投资（刀 5+v2，全树质量最高的一块）在默认路径上不可达，研究-产品断点。
4. **P1：刀 1 剩余三件**（1:1 窗口/断点 vp 化/README 对账）+ 裁六方向翻转的文档回填 + 两处 owner 注释冲突清理。
5. **P2：刀 2①③（spawn_kernel 注键 1 行级 + soak 定谳）**——长龄小项；②已被北极星线部分消化。
6. **P2：桌面五痛收尾**——按压反馈消费面（⑤，库层已备）、FACES 冻结相一行修（④）、kerning/CJK 挂账（⑥，随段④）。
7. **P2：刀 4/9（density 字段/2K-4K 档/product.display 键）**——依赖刀 6/8 解锁 4K 值域，设计位保留。
8. **P3：调度指南消费**（显示服务 intent 0x02 + policy 表 + 128→250 回填）——接线成本一行级，收益在 v2 EEVDF 开窗后兑现。

## 7. 来源索引

设计件：docs/显示与加速体系设计-v1.0.md（:73 裁六/:87-97 刀序/:91 刀 3/:96 刀 8/:103-121 重议触发器与开放问题/:123-134 性能预算）；docs/显示管线调度集成指南-v1.0.md（:9-13 速查/:30 档位/:124-129 帧率保障/:149-162 实测建议）。研究件：research/显示与加速体系设计/{01,02,03,05,06,07,08,09,10,11,12,13,15,16,17,18,19}-*.md（各节号随文引）；research/桌面UX审计/{01,02,03,04}-*.md 与 README.md。仓内代码（2026-11-15 19:50 工作树直核）：tools/berry/src/profile.rs:8-16/26-77；tools/berry/src/window.rs:1/9/26/195-246/990/1206-1216；tools/berry/src/qemu.rs:161-176/253-255/264/266-268/273-276/282/286-289；tools/berry/src/main.rs:888-890；tools/berry/src/display/frame.rs:23/29-36；tools/berry/Cargo.toml；tools/berry/README.md:50/186/225；justfile:1125/1423-1448/1450-1500；user/bins/berry-launcher/src/desktop.rs:22/49/98/134-139/143-150/317/404-408/462-485/545-551/755-759/782-800；user/bins/berry-launcher/src/frameloop.rs:269/303-312/792-796；user/bins/berry-launcher/src/icons.rs:150-175；user/bins/berry-launcher/src/main.rs:257；user/berry-ui/src/lib.rs:211/295；user/bins/guifbd/src/guest.rs:1274/1304/1406；user/bins/svcmgr/src/main.rs:1205-1232；user/bins/gfxdemo/src/main.rs:87/365/566；user/berry-ui/src/metrics.rs:70-72；tools/genhdfont.py:14/100；kernel/src/main.rs:749-816/4396；kernel/src/dpg.rs:774；arch/aarch64/src/psci.rs:94-105；arch/aarch64/src/mmu.rs:277-301；tools/berry_assemble.py:172-173；tools/bench.py:25-26；itally/src/lib.rs:18-27；.github/workflows/ci.yml:39/392。过程件：.agents/research/2026-11-15-1080p恒黑根因会审.md；.agents/research/2026-11-15-模拟器性能分层诊断.md；.agents/plans/2026-11-14-模拟器方案规划.md；.agents/plans/2026-11-15-体验大改造六波实施方案.md；.agents/notes/implemented/2026-11-15-intent-m4-svcmgr透传首件.md；.agents/notes/implemented/2026-11-15-intent-m4-band-骨架纯面.md；.agents/notes/implemented/2026-11-15-北极星审计demo-off-fatal虚化三刀.md。git 提交：853ac85c（v2 per-rect+异步）、01bb3e37a/e58c94ccf（knife6 修复，未回合）、7fd1709c6/5a6c798a9（五键引入，病窗口）。推断项清单：§1.1-①竖屏实裁的验收语义、§1.2-①panic 复现、§1.4 档位表横屏悬案、§1.8 遥测面形态、§2.1 波二清单无人认领、§2.2 rotate 键语义归属、§2.5 spawn 回执标记行、§3 VZ 战略转向、§4 intent_default 零配置、§5-9 组件扩展归类——均已在正文标注。
