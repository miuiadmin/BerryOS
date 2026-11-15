# Agent Note: 刀 5 第二批——appdemo 全 UI 与状态栏系统窗（含两项定谳）

Status: implemented

## Problem

刀 5 首段（wm 端授予）后两件事：① appdemo 只打到 `wm absent` 降级线，全 UI 链（建窗→AppShell→渲→SUBMIT→DESTROY）无活证；② owner「应用内状态栏常驻」需要 wmd 表内有 SYSTEM 层窗才成立。

## Decision

- **appdemo 全 UI**：wm 四步（GETINFO→CREATE[AVOID_SYS]→SET[GEOM+VIS]→DESTROY）+ present 面（GETINFO→VMO 三连→AppShell+双卡片 soft 渲→SUBMIT[FULL]+alias 随行→FRAME_CALLBACK）。RPC 帮手 BUSY 自旋**加 10s 帽**（原无 deadline——send 永 BUSY 即静默死循环，launcher 版有 RPC_CAP_NS 而此版漏抄；recv 同补 AGAIN 帽）。入口 `hw=` 诊断行（定位用，实证后可留作冒烟面）。
- **状态栏系统窗**：launcher 段③前注册双窗——SYSTEM 层 (0,0,w,39px) + APP 层桌面窗（y=39 起，AVOID_SYS）。GUI-8 裁决 e 有限翻案：像素面 v1 无合成器，以「避让几何 + 互不越界」为约——launcher 照旧全屏渲（状态栏像素在 fb 顶部），应用窗有效几何被 SYSTEM 窗裁顶 = 永不覆盖状态栏。

## 两项定谳（验证方法论）

1. **launcher ack 解析切片坑**：`parse_wm_create_ack(&rep)` 传整 256B 缓冲被 `buf.len() == WM_CREATE_ACK_LEN` 协议门恒拒 → `syswin ack bad st=0`（rep[0]=ST_OK=0 的误导表象）。修：`&rep[..n]`（rpc_recv 实收长）。**教训：所有 parse_* 必须按实长切片**。
2. **孤儿 QEMU 堆积**：`berry shot` 超时退出（exit 4）时 CLI 死但 QEMU 活（`--keep` 语义），每超时泄漏一个 100% TCG 实例，多轮后 load 10+，后续 run 被饿死出「假冻死」（serial 40 分钟零推进）。**教训：连续 shot 前先 `ps` 清点 QEMU；超时 run 的 serial 只作下限证据不作挂死结论**。另：pkill qemu 会误杀 owner 的 berry-live 常驻（`--restart-on-crash` 会自愈重启，但别再犯）。
3. **boot 批时序**：app 批在 bsh 批后 ~57k switches（TCG ~25 分钟）；干净单实例下提前到 ~300 行（switches 快进）。`APP ok n=2` + `APPUI ok n=1` = appdemo 拉起+UI manifest 验证+真退 0；但「APPDEMO ui」行全程缺席（含成功/降级/所有错误路径的输出）——判定被高负载窗口阻断，待低负载复跑 hw= 诊断轮定谳。

## Verification

- 全链编译绿（appdemo musl + launcher musl + build-user 58 ELF tlsscan 过）。
- EFI GUI run：`APP ok n=2`（拉起+reap 0）+ `APPUI ok n=1`（[ui] 元数据面）+ launcher 双窗无 `syswin ack bad` + clock 正常——**除 appdemo UI 行缺席待定谳外全绿**。
- appdemo UI 行缺席的终判挂低负载复跑（诊断镜像已提交 7a33c8268）。

## 终判补录（2026-11-15 晚，五轮二分）

「appdemo UI 行缺席」非 appdemo 因素——二分定谳：shot16（launcher **坏版** fail-exit，无帧循环）批全绿含 `app APPDEMO up/fs ok`；shot17-21（launcher **好版** 96389e1ad，帧循环常驻）稳定挂死在 bsh 批 HEXYL 后 fs_selftest 段（HVF 同挂，非 TCG 慢速）。结论：**双窗修复把 launcher 从 fail-exit 救活，暴露了帧循环×bsh 批的结构竞速**（既有债非本刀回归），转 issue #343（内核调度域）。诊断加固已回滚（d357a2160 复原 aac09a31a 全 UI 形态）。方法论教训：**boot 批死点定位先看 launcher 好/坏形态**——坏 launcher 的「全绿」是帧循环缺席的假绿（GUI 面没跑），好 launcher 的挂死才是真信号。


## Related

- [2026-11-15-ux-knife5-wm-grant.md](2026-11-15-ux-knife5-wm-grant.md)（首段）
- [2026-11-15-控件并集矩阵.md](../../plans/2026-11-15-控件并集矩阵.md)
