# Agent Note: 刀 5 应用 UI 承载链——wm 端授予与 svcmgr 帽债清偿

Status: implemented

## Problem

刀 4 定谳的缺口：APP_SPAWN 应用拿不到 wm 客户端端（gift 位只给 svcmgr 预拉面）——应用建不了窗，owner 投诉的「应用内无顶栏/无返回/无按压/无卡片」根源在此。

## Decision

- **wm 端授予链**：`app_launch`（svcmgr）在 manifest `[ui]` 在场（UiFace 非 Absent）时，从 `WMD_CLI_DUP` 常驻源现 dup 一枚 wm 客户端端（RIGHT_SEND|RECV|TRANSFER），随 spawn env extras 追加交付——launcher 装配同源，NET_A 先例。wmd 未起（src=0）静默不授，应用走 `wm absent` 降级。
- **gift 位**：app spawn extras 第三枚（hs[6]/GIFT_H[6]）——前缀紧致律 [4]=rootfs/[5]=fs/[6]=wm；与 svcmgr 预拉面的 input 端（hs[7]）同数不同位，两 spawn 面互不冲突。libc 新访问器 `wm_app_h()`。gift 组段按实长零 0 洞（T-1 判例）。
- **svcmgr 帽债清偿**：.bss 端点 0x1DFF0 距 pager_entry 钉位 0x1E000 仅 16B——wm dup 增量 223B 溢出。pager_entry 0x1E000→0x22000、pager_stack 顶 0x20000→0x24000（ld + 内核 spawn_user 同值双改），elf2flat --max 131072→143360。svcmgr 主体帽 120KB→136KB。

## Verification

build-user 全链绿（tlsscan 58 ELF 过）；noktest boot 复验：SVC ok + LAUNCH v=1 + launcher: clock + 无 desktop build err（svcmgr 提帽与授予链零回归）。appdemo musl 链绿并切 `wm_app_h()` 真链。appdemo 的 EFI GUI 形态全 UI 验证（点击 spawn → 建窗 → AppShell 渲染）需交互注入，挂下一批 smoke 面扩展。

## Consequences

- 应用建窗通道已开：appdemo 后续帧循环（present SUBMIT + FRAME_CALLBACK + 返回键退桌面）在 wm_app_h 真链上接。
- **状态栏 SYSTEM 层**（owner「应用内状态栏常驻」）的接线点已就绪：AVOID_SYS 避让在 wmd 表重算里现成，只差状态栏窗本身从 launcher 进程内迁 wmd 系统窗（GUI-8 裁决 e 翻案）。
- svcmgr 主体帽 136KB 不是终点：APP 域扩容（>4 活 app）或 appd 正式化会再顶——到时走 appd 独立进程分拆而非继续提帽。

## Related

- [2026-11-15-ux-knife4-appshell.md](2026-11-15-ux-knife4-appshell.md)
- [2026-11-15-控件并集矩阵.md](../../plans/2026-11-15-控件并集矩阵.md)
