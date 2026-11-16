# 波一刀 B：状态栏可信化（真时钟注入 + scrim + 模拟态标注 + 残帧清屏）

Status: implemented

## Problem

桌面 UX 审计（`research/桌面UX审计/02-系统导航与状态栏.md` §1–§2、§6.1 M1；03 号 §2.2）钉了四个状态栏可信度缺口：①时钟恒 12:00 编译期常量（`desktop.rs` `STATUS_CLOCK`），一眼假；②状态栏纯容器无衬底，壁纸青蓝渐变直接透进顶带（「顶栏是蓝的」主因之一），白图标对壁纸亮段的非文本对比不达 §3.2 的 3:1；③信号/电池为恒满格模拟值但无任何「这是模拟值」标注，不诚实；④gfxdemo 开机自测帧（深蓝灰底+色带+GFX 白字）退出后残留 fb，launcher 首帧前用户把测试图案当桌面看（02 号 §2.1 解释 A 的直接来源）。

根子：QEMU 拓扑无 RTC 设备，全仓无 rtc 驱动，`berry_clock_read` v1 只有 CLOCK_MONOTONIC（壁钟注记「留 v2」）；berry-ui Rect 无 alpha 通道；QEMU 无信号/电池真值源；guifbd blit-at-SUBMIT 无合成器，最后提交者覆盖一切。

## Decision

四件最小刀，全走「现有通道扩展」，零新 syscall、零协议改动：

1. **真时钟 = fw_cfg 注入 + 既有 0x61 syscall 的 id 域扩展**。宿主 `tools/berry/src/qemu.rs` `spawn_image` 把当下 epoch 秒写 `-fw_cfg name=opt/berry/rtc,string=<secs>`（display 注入先例同形，UTC 口径）；新内核模块 `kernel/src/rtc.rs` boot 期单次消费（epoch + 同刻 MONOTONIC 锚双 AtomicU64）；`sys_clock_read` 加 `CLOCK_REALTIME=1` 分支 = `epoch×1e9 + (now−锚)`。abi 单源 `tools/abi.toml` 加 `[[clock]] REALTIME=1` 再生成五面（「壁钟留 v2」注记随之落地——id 0 行为逐位不变，add-only）。launcher `main.rs` 单取换算 "HH:MM" 传入 `desktop::build` 首摆；`frameloop.rs` 每轮醒（稳态 10s 帽）续摆，分钟字变才 `set_text`（等长换文 = 纯 PAINT）+ a11y in-place 覆写 + `wake`。注入缺席（-kernel 冒烟/自建拓扑）→ -ENOTSUP → 回退 12:00 静态占位，零标记零漂移。
2. **scrim = 预混色 Rect**（berry-ui v1 无 alpha——预混是唯一通道）。50% 黑混壁纸顶部均色：genohwallpaper 源前 15 行实算均色 #06BCC6 → 预混 #035E63；白图标/白小字对它 6.6:1，同过非文本 ≥3:1 与小字 ≥4.5:1（应用规范 §3.2）。插入序 = z 序：壁纸 → scrim → 内容列，整宽 × status_h。
3. **模拟态标注**：左组 signal/wifi 后加 S16x32/S8x16 小字「SIM」；a11y 三 label 改 `signal simulated full` / `wifi simulated full` / `battery simulated 100`（SIM/UTC 视觉件不占 a11y 槽——32/32 帽满，诚实标注折进既有 label）。时钟旁「UTC」角标仅 REALTIME 在场显字（缺席空文本零宽，占位钟态不误导）。
4. **残帧清屏 = gfxdemo 退场黑帧**（02 号 §3 B1 建议形态）：`GFX ok` 前再提交一帧纯黑（`fb.fill(0)` + FULL 形 SUBMIT + FRAME_CALLBACK + RELEASE，与首帧同 credits 周期）——gfxdemo 退场屏幕归黑，launcher 首帧前的间隔显示黑而非测试图案。COMMIT 纯记账不 blit（guifbd `handle_hdi` 核实），该黑帧即最终上屏态。

配套：`ktest.rs` 时钟负样本 id 1→2（id=1 已合法化，2 注入态无关恒非法）；`desktop.rs` 头注陈旧「三 Rect 带状壁纸」段随刀更新为现行形态（02 号 §1.3 挂账项）。

## Alternatives considered

- **新 syscall 取壁钟**：任务书明文越界；且 0x61 的 id 域扩展在 ABI §8.3 `berry_clock_id_t` 既有框架内，零号新增，弃。
- **launcher 经 handoff/svcmgr 环境拿 epoch**：svcmgr 自身拿不到（fw_cfg 唯一消费面是内核），转一手反而多一层，弃。
- **宿主画时间串进镜像/字库**：伪造而非通道，弃。
- **guifbd/svcmgr 触发清屏**：guifbd 协议无 clear op（加 = 协议改动面）；svcmgr 不持 fb；launcher 首帧本就整屏覆盖，窗口期只剩 gfxdemo 退场前——gfxdemo 自清最小，弃他选。
- **scrim 用 CLEAR_BG #101828 中性深色**：对比 15.6:1 更高，但与壁纸青绿域断裂感强；预混 #035E63 视觉延续壁纸色系且双指标达标，取预混（设计裁量，数值两案均合规）。
- **宿主本地时区换算**：std 无跨平台本地偏移 API（无 chrono 依赖纪律）；UTC + 「UTC」角标如实注，本地化挂账波二。

## Consequences

- 真钟链五面：qemu.rs 注入 → rtc.rs 消费（串口证据行 `RTC ok epoch=`）→ CLOCK_REALTIME → launcher `launcher: clock HH:MM utc (realtime)` 证据行 → 每分钟换字。精度 = TCG boot 时延级（秒级），分钟摆字语境可忽略。
- gui8 冒烟拓扑（自建 QEMU argv，无 rtc）行为逐位不变：launcher 回退 12:00、无 UTC 角标、无 clock 证据行——冒烟标记集零扰动（已实证 Boot A：WM/GFX/GFXF 全过）。
- `berry run/shot` 等宿主 berry 起的镜像 boot 串口多一行 `RTC ok epoch=`（marker 形态同 DISP ok，非断言面）；ktest 负样本改 id=2 后两态（有/无注入）皆绿。
- a11y 32/32 帽保持（角标不占槽）；桌面树 354→357 节点（<512；刀 D 并行再 +4 = 361）。
- **boot 实证踩坑（z 序）**：scrim 初版插在 content 之后（Root 第 3 子）——「先插者在底」使其成为最顶层，把状态栏内容整条盖住（截图实证：顶带纯 scrim 零白件）。修正 = scrim 必须在 content 插入之前（壁纸 → scrim → 内容列）。教训：Root 直接子的 z 序 = 插入序，尾插不是「垫底」而是「压顶」。
- **顺带发现的存量缺口（非本刀产物，挂账）**：冻结相（无数据盘，`app_h==0`）`icons::init` 早退不设 `FACES`，`IconSource::dims/pixels` 对非壁纸 id 一律空服务——状态/dock/导航图标在冻结相桌面全部不渲染（壁纸走特判通道故在）。全相（APP_LIST 跑通）不受影响。修法 = init(0) 分支补 `FACES.set(status_faces())`（一行），归图标服务面随刀补。
- 波二挂账：本地时区（宿主注入偏移或 guest TZ 换算）；simbattery 通道接真值（图标随注入变）；CLOCK_REALTIME 的 ktest 正例（随注入态门控，暂以串口证据行代）；冻结相 FACES 空服务缺口。

## Related

- 研究底稿：`research/桌面UX审计/02-系统导航与状态栏.md`（§1–§2 现状、§6.1 M1 修法、§3 B1）、`03-顶栏字体触摸-根因与修法.md`（§2.2 蓝顶带归因）。
- ABI 单源：`tools/abi.toml`（clock 段）、`tools/gen_abi.py`（五面再生成）。
- 并行波：同批刀 A（window.rs）、刀 D（三键导航/PSCI power）与本刀共享 launcher 四文件与 kernel/src/{main,syscall,ktest}.rs——改动区域不相交，合并构建绿。
