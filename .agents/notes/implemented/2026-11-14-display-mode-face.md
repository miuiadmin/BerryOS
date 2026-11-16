# Agent Note: fw_cfg display 模式请求解析纯面（berry-dmface 落件）

Status: implemented

## Problem
kernel/src/main.rs `display_mode_req`（GUI P2 显示模式请求源）的判定链五门——fw_cfg `opt/berry/display` 项 size 域 (0,32] / UTF-8 / trim NUL+ASCII 空白 / `@` 后缀截断 / WxH u32 十进制域——此前内联于 kernel crate（host 测不可达），而它是显示面 `None` 降级（QEMU 默认模式兜底）的语义单源：错一门 = 要么放过畸形串、要么误拒合法模式致显示面静默降级。W3 lane face-crate 纪律（safeck/chanface/aokface 先例族）要求此类纯判定律 host 钉板。

## Decision
新建 berry-dmface（零依赖 face crate）单源 `parse_display_mode(bytes) -> Option<(u32,u32)>` + `MAX_ITEM_BYTES=32` 常量；kernel main.rs `display_mode_req` 薄壳消费（fw_cfg `find_file`/`read_to` MMIO 物理臂留壳，`e.size` 门换 `MAX_ITEM_BYTES` 单源）。host 6 测钉板：全链 happy path（含 @Hz 截断/NUL 填充 trim/空白族全收）/ size 域恰界 32 过 33 拒（NUL 填充几何构造）/ UTF-8 畸形拒 / @ 截断后无 x·双 x·多 @ 形 / u32 域（恰界 u32::MAX、零合法、前导零、hex 前缀拒、`+` 合法 `-` 拒、大写 X 拒）/ trim 后空串拒。注意迁移保形：kernel 原实现 `e.size > 32` 在 read 之前（读臂短接），dmface 侧 bytes.len() 门语义等价（kernel 仍先 size 短接再 read 再 parse）。

## Alternatives considered
扩 berry-fwface（W3-Q52 fw_cfg 目录行走 crate；否：fwface 面是 fw_cfg 协议几何（select/BE/DMA），display 串解析是消费侧语义，混面扩 crate 职责边界）；kernel 内表驱动测试（否：kernel host 测不可达，W3 lane 纪律即为外迁钉板）；把 `display_mode_req` 整函数外迁（否：函数含 fw_cfg 物理臂，物理臂必须留壳）。

## Related
[timer slack face](./2026-11-13-timer-slack-face.md)（同 face-crate 纪律先例）；消费面 docs/ GUI P2 系（`display_mode_req` 调用点 main.rs 显示初始化段）。
