# svcmgr WTDIE 自陷取证（boot 基线 shot 崩溃链定谳）

Status: implemented
Type: note

Updated: 2026-11-15

## 现象

berry shot（`--boot-mode splash`、无 display key 的混合形态）串口：`SPLASH 25 services-steady` 后 svcmgr 死于 `EXCEPTION sync D-abort(lower) PC=0xc578 FAR=0x40000000`，紧随 `G1.5: kill user task 'user_svcmgr'` 与 `WTDIE h=0x00050035 sig=0x0002 e=0xff92`；桌面从未完成（无 `LAUNCH ok`），内核矩阵任务照跑（switches 计数继续爬升）。

## 取证方法（stripped ELF 无符号定位法）

svcmgr 产物 stripped（user/cargo-target/aarch64-unknown-none/release/svcmgr），但 `.text` 基址 0x1000 恰等于 `USER_CODE_VA`（user.rs:50）——运行时 VA 即 ELF VA，objdump 直接反汇编。三步定谳：

- PC 0xc578 落在公共自陷块：`c574: mov w8,#0x40000000; c578: ldr xzr,[x8]`——故意读未映射 0x4000_0000 触发 D-abort，家族自陷惯例（svcmgr `trap()` 源码同款，main.rs:7636；guifbd trap() main.rs:1297 亦同值）。
- x30=0xaf24 回溯真实调用点：af08-af20 = `mov w0,#0x52（channel_send）；w3=#0x27（39 字节）；svc #0；bl 自陷块`——svc 后无条件自陷 = 源码「先单发诊断再 trap」形态。
- 39 字节 + 寄存器 x14-x18 的 ASCII 片段（`"=0x00000\n"` 等 hex 格式化产物）+ x19=2(sig)、x21=0x50035(h)、x20=0xffffffffffffff92(e=-110) 三元组，精确对上源码 `wait()` 的临终诊断行 `WTDIE h=0x%08x sig=0x%04x e=0x%04x\n`（39B，main.rs:5010-5024）。**WTDIE 行是 svcmgr 自打的，不是内核打的**——此前按内核行为排查的方向全错。

## 定谳

svcmgr `wait()` 带 10s 绝对帽（「对端断供即链断」设计）；被等对象按位分解：`h=0x50035` = app-list 流通道（`app_serve_list` 的 `send_retry` 面，非 console 通道——早期「console 通道」读法已废）；等 `SIGNAL_WRITABLE`（sig=2）超时（e=-110 = ERR_TIMEDOUT，abi_gen.rs:173）→ 按设计自陷。非内存腐化、非内核 bug、非 guifbd 三路轮询饿死（该理论被帧级探针否证——见下）：**launcher 停止消费 app-list 应答帧 ≥10s** 才是根因方向。

## 楔死机制（帧级探针定谳，2026-11-15 gui8 实证）

帧级探针（icons.rs pull() 每帧打 `pf <e|c|z> n=<entries>`）+ main 分阶段探针（ph1-ph5）在 gui8-smoke（/tmp/gui8-smoke.hx3frhwr，900s 形）逐位钉死楔点：

- 流序健康推进：`pf c` ×数十帧（entry 0 图标分片全量到账）→ `pf e n=1`（entry 1 的 ENTRY 帧）→ **`launcher: icon decode BadMagic` 打到半行冻结**（与无探针形 v3wznprt 字节位一致——同截断位复现两次）。
- 楔点指令级定位：eprintln 格式化是**分片写**——「launcher: icon decode 」与「BadMagic」两片已送达串口，格式化 `IconError::BadMagic { got: [u8;4] }` 的数组 Debug 实现所在**冷代码页**缺页 → demand fault → `user_svcmgr_pager` 供页永不到达 → launcher 在指令中冻结（内核 fault-wait 无帽、无打印面、无 panic——零 'panicked'/零 abort 签名、switches 速率不变均与此相合）。
- 下游链：launcher 停收 → app-list 通道（8 深）填满 → svcmgr `send_retry` 阻塞 WRITABLE → `wait()` 10s 帽到期 → WTDIE 单发诊断 + 自陷 → G1.5 kill `user_svcmgr` → 桌面永不出（guifbd HB cell=25 armed=false 停摆）。
- BadMagic 本体（entry 0 = APPDEMO 图标 blob 首字节零）与楔死独立：坏件只触发诊断行，**冻结在诊断行的冷页缺页**。预触页后诊断行打全（2026-11-15 pf1 车辆 /tmp/gui8-smoke.rr0zospv）：`BadMagic { got: [0, 0, 0, 0] }`——堆页已全量预触仍读零 = **非客户端缺页坏供**。pf1 车辆后离线核账：镜像内 APPDEMO icon（rootfs 偏移 0x3000+0x56000=0x59000，11386B，60 分片）字节完好 BYI1 w=108 h=108（盘面核验）；服务端 serve 期 10B 头验读同偏移通过（否则 blob_len=0 零分片——60 分片实发证明验读通过且 w/h 合法）；appproto 编解码两侧代码审读 + host 测在册无恙；**其余 7 app 图标 blob 全量字节完美解码**（含 46KB BROWSER——通道与 vmo_read 长读路径整体健康）。矛盾收敛于：同一 VMO 同一偏移、微秒级间隔，10B 验读得 BYI1 而首个 192B fill 读出零——且两 launcher 实例（同 boot 同通道）同形，单 boot 单次腐化亦不能解（验读与 fill 几乎同时）。嫌疑面收窄至首个 CHUNK 帧的内核通道拷贝/首帧特异路径，另刀挂账（探针方案已备：服务端 fill 后打 frame[5..13] hex + 验读 hex 对拍）。串口取证注：canary 与用户打印逐段交错，`pf` 行约四成被撕——grep 计数只作下界（0.6 系数逐条目均匀，核账时按此折算）。
- pager 停摆机制未判明（候选：PAGER_HOLD 门 / 池空泊键丢醒——stall_kick 自身 0x23 池空亦泊的已知签名 / 供表槽跳过）：X6 遥测（pg=1232 sw=1197）止于楔点前且 pager 无 console，亚表面归内核域挂账（E 臂/d-3b 同族）。

## 防御刀（同日落）

launcher main() 头部整幅预触页（`prefault_image()`——走自身 ELF 头 PT_LOAD 幅每 4KiB 读 1 字节，main.rs）：全部缺页前置到 pager 实证健康窗，桌面关键路径此后零 demand fault。避雷非拆雷——pager 停摆根修归内核域。

## 第二级故障：堆 OOM abort（预触页把流送通后暴露，同日 pf1 车辆定谳）

预触页生效后 app-list 流两次全量走通（旧楔点两度过——防御刀实证有效），桌面仍不出：launcher 死于 **`memory allocation of 87280 bytes failed` → abort**（串口三行铁证 + `eco: launcher died` + 监督复跑同炸——同 boot 两实例同点 deterministic）。87280B = `Box::new(UiMain::new)` 的 Ui<512> arena 整块分配。

根因 = **分配序 + 帽算术双错**：icons 拉取先吃堆（8 图标 × 108×108 = 93312px ≈ 365KB 像素 Vec + 本底状态/dock/导航面 ≈ 130KB + blob 分片增长颠簸），1MiB 静态堆（berry-libc `HEAP_SIZE`）first-fit 最大连续洞 < 87280 → 整块分配炸堆。旧 `PX_CAP=225_000` 的算术把像素当字节（225K px × 4B = 900KB，加 arena 与本底必超 1MiB），且漏记导航六面本底（26136px）。此路径此前从未到达——旧楔死永远先拦在流中段，预触页拆楔后才首见天日。

修复（同批落）：**arena 先行**（UiMain Box::leak 提到 icons::init 之前——最大单块从童子堆整块 carve，碎片化再也伤不到它；set_icon_source 经 FACES OnceLock 惰性取用，先行无害）+ `PX_CAP` 225K→195K（对 1MiB 堆全量重算：780KB + arena 87280B + 颠簸/头/碎片余量 < 1MiB；app 域可用 ≈ 121K px，当前 8 图标 93312px 帽内全绿零降级）+ px_used 本底补记导航六面 + blob 预留帽 64KB（wire blob_len 512KB 是拒面不是预留尺寸——`with_capacity(blob_len)` 会被 wire 谎报大值整块预留顶穿堆）。

## 复现升级（2026-11-15 同日补，gui8 真 GUI 形）

`just gui8-smoke` boot A（display key 注入 = 真 GUI 形，guifbd gui_mode6=1、13/14/17 自事件在席）复现同签名：SPLASH 19 格全序（本车辆 = vblk+display 并集车，含 11 vblk-up）但 `25 desktop-tree` 缺席——launcher 完成 WM 握手（`LAUNCH v=1`）后建树无输出楔死 ~34000 switches → `user_svcmgr` 死于 PC=0xaf24（wait() WTDIE 诊断块）+ `WTDIE h=0x00050035 sig=0x0002 e=0xff92` 同值。与混合形态合流：**楔死面不挑 gui 旗**，真 GUI 形照样饿死。当日反例：手跑 noktest GUI 形（display+bootmode=splash+noktest 三键齐）同镜像 `LAUNCH ok` 90s 绿——noktest 键改服务 spawn 配速（probe 等待松化），竞速窗不开。判别器修正：**复现与否由 spawn 配速/负载决定，不由 gui 旗决定**；noktest 三键形 = 目前唯一稳定绿形。

## 处置

- 防御刀已落：launcher `prefault_image()` 整幅预触页（见上）——pf1 车辆实证旧楔点两度过。pf1 车辆 gui8 硬门 **FAIL**（第二级 OOM abort，见上节——非防御刀失效）；OOM 修复（arena 先行 + 帽重算）已落，同日 oomfix1 复跑 gui8-smoke **PASS**（/tmp/gui8-smoke.cegr_ec7，双 boot 全绿：A 形五标记 + CLICK×1 + LAYOUTW×1 + APPDEMO up×1；B 形五标记 + LAYOUT×2 + 零重写 + LAYOUT.JSN canonical 109B 跨 boot 存活）——防御刀 + 分配序纪律双修全链闭环。
- 内核域挂账：`user_svcmgr_pager` 停摆家族（池空泊键丢醒/stall_kick 自泊/PAGER_HOLD 面）——亚表面在 svcmgr pager_main 与内核 dpg 面，与 E 臂停摆（[[earm-stall-svcmgr-suicide-verdict]] 记忆所指判例）/d-3b 族同册。
- 数据面挂账（另刀）：entry 0 = APPDEMO 首个 CHUNK 帧零字节之谜（见楔死机制节——验读与 fill 同偏移微秒间隔一真一零，7 app 通道全健康，嫌疑收窄至首帧特异路径；探针方案已备）。
- 早期「guifbd 三路轮询饿死 console」理论作废：楔点在 launcher 进程内，guifbd 无辜（HB 持续、cell=25 armed=false 停在等桌面）。
- 判别器（2026-11-15 修正版）：手跑真 GUI 形绿 ≠ 竞速不存在；gui8 形（无 noktest 键紧配速）= 稳定复现形——pager 停摆窗在紧配速下才压得住 demand 流。

## 可复用取证配方

stripped 用户态产物定位三件套：`.text` 基址即 `USER_CODE_VA` 直接反汇编；自陷块地址族（0x4000_0000 故意读）识别 panic 尾；x30 回溯 panic 真实调用点 + 寄存器 ASCII 片段对格式化串。ERR 码查 abi_gen（-110 TIMEDOUT / -9 BAD_HANDLE / -22 INVALID_ARG / -37 BAD_STATE）。
