# 命令行终端优化刀：键盘物理链四件 + berry-term v1 上桌

Status: implemented

一句话说明：owner 令「继续使用 workflow 模式实施命令行终端优化」的落地刀——兑现[命令行体验刀](../../plans/2026-11-15-命令行体验刀实施设计.md)契约 4 的件 1/5/6（键盘物理链四件、berry-term 本体、launcher 上桌三例外处置）并同批 APP_SLOTS 9→10 抬帽；规格轮三域侦察（wf_ffd8a8c2）+ 实施轮四域（wf_bea5a8b0）+ 主线集成复核三段式。

## 背景

命令行刀契约 4 六件挂账中，件 4 宿主 console 已由 console 刀先落（2026-11-16），件 2 inputd 焦点门控分发仍开放；本刀收口其余四件。规格轮（三域侦察 agent）先定谳三个方向性裁决，实施轮照规格落码：

- **动态喂入**：berry-term 持久子 bsh 走 `SpawnSpec.stdin_handle` 通道（boot console→bsh 同机制），弃行批发——每行 fresh bsh 丢全部 shell 状态（cd/env/历史环）、465KB ELF 每行一 spawn、行完结判据只能提示符嗅探。
- **出向零新原语**：键盘事件复用 PointerEvent 32B 定长帧，`device_id = DEV_KEYBOARD` 覆载约定（x=翻译后字节、phase=PHASE_DOWN、v1 只发按下）——guiproto 零新 opcode 零 ABI 面，注释两域分注（入向 OP_INPUT_INJECT=Linux input 码照 softime 词汇，出向=翻译后 ASCII+控制码）。
- **上桌 dock 换项**：DOCK_IDS 的 MMS 位换 TERMINAL（DOCK_SLOTS=4 与几何零改动，MMS 回网格经 is_dock_app 去重自动成立）——躲开 LAYOUT.JSN 持久化陷阱（dock 是代码常量直查注册表，与布局文件零耦合）。

## 交付

- **QEMU 键盘设备**：qemu.rs spawn_image 的 tablet 块内加 `-device virtio-keyboard-device`（与 tablet 同门——输入设备家族，headless 形零漂移；全名 mmio 形照 virtio-gpu-pci 别名陷阱注释）。四调用点全经 spawn_image 汇流，实际一处。
- **inputd 认领门多设备分类**（drivers/bins/gui-inputd/src/main.rs）：did=18 槽全收集，逐槽写 cfg `EV_BITS/evsel=EV_ABS` 读位图分类——ABS_X+ABS_Y 双位在场=指针类、缺席=键盘类；执法面指针类恰 1（park 原句保留）、键盘类至多 1（多出忽略）。键盘第二台独立建制（negotiate/setup_dma/setup_queue/post_events/DRIVER_OK），CLAIMED_REGS 两槽、trap() 复位两台，serve 回环与 serve_poll 各加第二收割。
- **KeyPump**（gui-inputd/src/lib.rs）：修正键态（shift/ctrl/alt）+ US 布局查表翻译 + 32B FIFO 出向队列 + post_char 直喂口；berry-virtio input.rs 冻结约 49 枚 Linux KEY_* 常量（含任务清单外补的 KEY_SPACE=57——实施 agent 初稿误写 32=KEY_D 被自家单测当场击杀，冻结钉板单列）。
- **出向混排与 INJECT 修正**：serve_poll 指针事件先填位、键帧补余位（批帽 INPUT_EVENTS_MAX=3 共享，溢出留队），键帧旁路 wm_annotate；INJECT 的 DEV_KEYBOARD 臂改喂 KeyPump（Linux 码过表）、DEV_TEXT 臂改 post_char 逐字节直喂——消灭原 `code: b as u16` 码域碰撞（ASCII 'a'=97 撞 KEY_RIGHTCTRL）。
- **消费面**：appshell pump.rs 在 PtrEvent::from_wire 前按 device_id 分流调 `HostHooks::on_key`（默认空实现——launcher 等既有实现者零改零警）；guiproto 设备类注释段两域分注立法（纯注释零代码）。
- **berry-term v1**（user/bins/berry-term/ 新 crate）：settings 骨架镜像（wm 门控用 wm_present_h——settings 的 wm_cli_h 在 fs+ui 礼物布局下失效）+ 终端四件：24 行×64B 定容行环逐行 Text 叠行（drawn 副本去重重排）、约 60 行行编辑器（0x08 退格/0x0A 提交/Esc/Tab 透传/箭头忽略）、持久子 bsh（stdin_handle 通道 + `--no-selftest` + fs 双礼物 + stack 16）、启动管道自检（静态喂 `echo berryterm\nuname` 四断言早退收集，过打 `berry-term: pipe ok`，不过 park；自检子 terminate+reap 收口）。子退出三态共用 mark_child_dead（EOF/链断/满队超时），v1 不重启。
- **槽位/标记/上桌**：APP_SLOTS 9→10 三镜像 + justfile build/spec 两循环（berry-manifest 常量/berry_assemble.py 镜像+样例 preset 第 10 条/svcmgr 标记写面自适应宽度 + const assert <100 双钉板/build-user crate 循环 berry-term + mkapp spec 循环 TERMINAL:berry-term——markers.sh 随动契约第六点，遗漏审查补记）；APP ok n=10、APPUI ok n=9 五处滚新（markers/smoke/smoke-smp/smoke-image/gui8-smoke，rootfs 标记条数 56 不变）；bcts@3.toml 冻结滚新（add-only 惯例，bcts@2 原样在档，selftest 35/35）；dock 位 3 换项走 DOCK_FACE_IDS 分段回退面（MMS 形几何复刻占位——origin 侧 owner 2026-11-16 位图/BYI1 裁定把 DOCK_BITMAP_NAMES 四位全清空（位图 id 过渲染面 BadMagic 致桌面死），本刀初稿的 md_hardware_monitor 位图占位随 rebase 收编弃用，OH 视觉资产正式替换挂账 ADR #63）；gui8 canonical 109B 零改动（布局只取前二注册项，app/TERMINAL.bap 名序排后）。

## 裁决与判例

- **键盘是增强面不是必需面**：键盘槽协商拒绝与 legacy 版本检查失败均降级为无键盘面（up 行 kb=none，指针服务继续），不连坐 tablet；指针类拓扑破坏仍 park 全服务（语义与原版一致）。park 诊断句「input slots = {}（须恰 1）」的计数值改打指针类台数（原打 did=18 总数），字面保留 smoke fail 文案引用。
- **post_events 入口复位 RINGSTAGE 暂存页 avail.idx**：两台设备共用建制暂存页，第二台首次 refill 不复位会把 idx 从 32 写回 17=自由运行计数倒退（设备面违约）——实施 agent 主动发现的潜伏雷，属本刀新增建制面必修件。
- **berry-term 自持帧循环弃 appshell 依赖**：稳态 wait_many 需第三路（帧回调+输入+子 stdout），appshell pump 等待面至多两路放不下；DEV_KEYBOARD 分流点照 pump.rs 同形内联，on_key 为 HostHooks 同签名自有方法。appshell 的 on_key 默认空实现仍保留（后续 appshell 系壳应用受益）。
- **码域双口径收口**：guiproto 注释原写「键盘 code = ASCII 可打印字节」只对出向成立，入向 INJECT 实为 Linux 码（softime 立法词汇）——两域分注澄清，词表各自单源。
- **QEMU VNC keysym→virtio-keyboard 路由未实测**（侦察遗留）：宿主真键盘直打链路以 QMP send-key/OP_INPUT_INJECT 为验证入口；起机探针见验证节。

## 验证

- 域内 host 套件（实施 agent 各域实跑）：berry-virtio 26 测、gui-inputd 35 测（KeyPump 10 新测 + PointerPump 零回归）、berry-guiproto 65 测、berry-appshell 27+6 测、tools/berry 160 测、berry_assemble selftest、bcts selftest 35/35 全绿；berry-term 单件 musl 构建绿（454840B，settings 链同形）+ mkapp 五道门禁绿（TERMINAL.bap 512000B）。
- 集成构建（主线）：just build（内核 + build-user + docs-guard + abi-check）绿；rootfs0.cpio 含 app/TERMINAL.bap。
- QEMU 起机探针（主线，键盘/无键盘对照两路）：**boot 链前段实证双绿**——键盘路 `gui-inputd: up slot=30 … kb=slot 29`（双设备分类认领 + 键盘建制 DRIVER_OK）+ `INPUT ok`；无键盘对照路 `kb=none` + `INPUT ok`（降级不连坐指针服务——增强面裁决的实机兑现）。G1.5 野指针 EXCEPTION 帧为内核自测预期输出（rogue dead/abuser alive 收口行在席）。链中段标记（HEXYL/BC/THR/FRZ/DPGP/DPGS/DPGN）两路一致到齐，无 panic/fatal。
- 探针后段（app 批/tomb/bsh 自验链）未在窗口内到达：宿主高争用（本轮 6-8 台并行 QEMU，load 1min 峰 35）下 TCG 竞速把已知 u3 族竞速（bsh 链与 app 批交错，命令行刀步骤 4 在册）推成 30 分钟零推进——`APP ok n=10`/`APPUI ok n=9`/deferred 记账/`LAUNCH ok`/`BSH ok` 与 QMP send-key 键入端到端四件留 smoke 周期覆盖（smoke.sh 四相 + smoke-smp 键盘设备行已同批挂入，低载窗实跑即验）。探针判读教训固化 berryos-dev skill：拓扑契约第四件（simdev=1 须配第二路 serial，否则 uart1_init D-abort）。
- smoke 拓扑滚新后 bash -n 语法绿；标记计数面零增减（裸 32/rootfs 56/RX 3 不变，仅字符串滚新）+ bcts@3 冻结件 tier1_floor=91 随持。

## Related

- [命令行体验刀实施设计](../../plans/2026-11-15-命令行体验刀实施设计.md)（契约 4 挂账源，本刀兑现其件 1/5/6）
- [shell设计-v1.0](../../../docs/shell设计-v1.0.md) §2.5 进展注（挂账台账面）
- [2026-11-15-命令行体验刀](2026-11-15-命令行体验刀.md)（管道契约钉板——berry-term 管道引擎的地基）
- [2026-11-15-波4-settings真应用首件](2026-11-15-波4-settings真应用首件.md)（settings crate 范式——berry-term 骨架母版）
- [2026-11-15-波3TabBar三页装配面](2026-11-15-波3TabBar三页装配面.md)（[ui] 门授能与 deferred 记账——TERMINAL 上桌前提）
