# launcher 键盘消费面：on_key 覆写 + keynav 序列重组

Status: implemented

一句话说明：补上键盘事件链的最后一环——[2026-11-17 键盘物理链刀](2026-11-17-命令行终端优化-键盘物理链与berry-term.md)已把键帧送进 appshell pump 的 `HostHooks::on_key` 分流点，但 launcher 一直是默认空实现（键盘帧到桌面壳即被静默吞掉）；本刀让 launcher 真实消费键盘——箭头 UP/DOWN ESC 序列重组后翻桌面 pager。

## Problem

键盘链三段核查（gui-inputd / berry-launcher / berry-appshell）结论：前两段与第三段的分流面全部在席（均为 HEAD 已提交内容），唯一断点在 launcher 消费端。

- gui-inputd：`KeyPump`（Linux EV_KEY 码 → US 布局翻译字节，修正键感知）+ `serve_poll` 键出向混排（字节经 `key_frame` 构 `device_id=DEV_KEYBOARD`、`x=字节` 覆载帧，与指针事件同批进 `encode_input_events_rep`）——键盘事件已在 INPUT_POLL 回复中透传。箭头出向词汇 = UP/DOWN 的 ESC 序列三帧单字节连发（xterm 同形）；LEFT/RIGHT/HOME/END 表外不出向（gui-inputd 负测 `keypump_key_left_ignored` 钉板）。
- berry-appshell：事件类型无 Keyboard 变体是刻意立法而非缺口——键盘走 PointerEvent 的 `device_id` 覆载约定 + `on_key` hook（guiproto 注释两域分注），避免新 opcode/ABI 面。
- berry-launcher：`LoopHost` 未覆写 `on_key`，键盘帧落入 HostHooks 默认空实现——桌面态（无 app 覆盖时）键盘零消费。berry-term 虽有真实键盘消费（哑终端透传子 stdin），但它是独立 bin 自持帧循环，不救桌面壳。

## Decision

最小面三件，全部落在 berry-launcher：

- **keynav.rs 新模块**（纯逻辑）：ESC 序列重组单步状态机 `seq_step(&mut kseq, code) -> Option<KeyNav>`——三态（基态/已收 ESC/已收 ESC `[`），final `A`/`B` 产 `PagePrev`/`PageNext`；失序、表外 final、基态非 ESC 字节一律吞（bsh editor「非 A/B finals 一律吞」同纪律）；连发 ESC 折算为「前帧裸吞 + 本帧新序列头」。host 单测 12 例 = 本刀 6 例（完整序列/连按/基态吞/裸 ESC 复原/双 ESC/表外 final）+ 同日测试补强对拍六景（UP/DOWN 完整序列逐帧态/DOWN×2/失序 ESC→X→A/基态 A/ESC 随机字节后 `[`→A）。
- **frameloop `on_key` 覆写**：调 `keynav::seq_step` 重组，动作 = 桌面 pager 翻页（`snap_to` + `rehang_view`，on_msg 翻页/nav home 键同款收尾；wake 归 pump 键帧既有语义）。三重门：系统页在树（`tabs.cur != 0`——桌面被 sys 页不透明底盖住）吞；覆盖态（wmd 表可见非自 APP 窗在场）吞——v1 无合成器，翻页重画会把桌面画到 app 像素上面；边界页（page 0 按 UP/末页按 DOWN）无操作。
- **`covered_by_app` 判据**（LoopHost 固有方法）：`on_covered_tap` 的表查询面同款裁剪（TABLE_GET + `covering_app_win`），只判在场不收盖；查表失败 fail-open 同款。

垂直箭头（UP/DOWN）控水平 pager 是 inputd 出向词汇约束下的映射，代码注释如实记「v2 扩词汇后可换 C/D finals」。

## Alternatives considered

- **扩 gui-inputd `translate_seq` 加 LEFT/RIGHT 出向**（翻页语义更自然）：推翻 2026-11-17 刀的负测钉板立法（「bsh editor 非 A/B finals 一律吞——LEFT/RIGHT 无出向消费语义」），动已提交稳定面——弃，v2 若桌面键盘词汇扩需求再议。
- **裸 Esc 语义**（回桌面/back 键）：单帧 ESC 与序列头无法即时区分，需超时裁决而 `on_key` 无 timer 钩子——延迟判定会漏尾帧裸 Esc，弃。
- **数字键快捷启动 dock 槽**：spawn 面耦合覆盖门/spawn 守卫/router（并行刀在途参数），刀面膨胀，记 v2 候选。
- **syspages Apps Search Input 件键盘文本喂入**：过滤联动是组件级改造（现过滤为点击 chip 驱动），超最小刀边界，记 v2 面。

## Consequences

- 桌面态箭头 UP/DOWN 翻页可用（覆盖态与系统页态保守吞——覆盖态键盘转发给焦点 feeder 属输入焦点路由架构面 B 的延伸面，归该刀自理，本刀不动 `Router` 结构避免同文件在途混装）。
- berry-launcher bin 的 host 单测被 main.rs guest 内联汇编结构性阻断（`mod keynav` 的 `#[cfg(test)]` 在仓内编译进 test cfg 但 `cargo test` 两种 target 均无法出二进制；并行会话的 inputfeed 单测同款处境）——keynav 单测以临时独立 crate 形态实跑验证（6/6 绿）后删除，仓内测试代码留存待 bin 测试基建解封。
- 与 2026-11-20 并行刀（输入焦点路由架构面 B，同日改 frameloop.rs/main.rs）物理同文件但逻辑零交叠：本刀方法挂 render_hold 锚后、不引用 `Router` 与 `inputfeed` 任何符号，两刀 hunks 可辨。
- 验证：musl target `cargo check` + `cargo build`（berry-ld 链）绿；QEMU 端到端（QMP send-key 箭头 → 翻页标记）未跑——宿主高载窗 TCG boot 3-10 分钟且需键盘设备拓扑，留低载窗补验。

## Related

- [2026-11-17 命令行终端优化：键盘物理链四件 + berry-term](2026-11-17-命令行终端优化-键盘物理链与berry-term.md)——键帧覆载约定与 `on_key` hook 的立法刀（「launcher 等既有实现者零改零警」的空窗由本刀收口）。
