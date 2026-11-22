# 演示面 fw_cfg 门控: idle 修窗首刀(kernel 面)

Status: implemented

## Problem
默认镜像 boot 演示常驻面永不退出,guest 结构性无 idle——「idle 满核税」测量窗实为满载窗(06 号主根因)。task_a/task_b 无条件 spawn 的内核忙循环(main.rs :795-808 旧位) + boot EL0 演示舰队(good 'u'/abuser 'S'/waiter 'W'/pinger 'P'/ponger 'p'/spawner-pong 'C''s' + rogue_ptr + watchdog)全无门控。

## Decision
fw_cfg 具名键 `opt/berry/demo-face`(noktest 先例同形,极性同向):absent/"on" = 演示面在(现状零漂移,EFI 无键路径天然缺席);"off"/"clean" = 门关。读取照 display/rtc 先例(find_file → read_to → trim),值面 fail-open(畸形一律按 on)。门关面:task_a/b 忙环、舰队 build/表/spawn、waiter+waker2(4-1 对)、pinger/ponger、watchdog(舰队活性断言者,随舰同门——否则 2000 轮忙等后 panic)。自退出验收对(sleeper/waker、pi、intent、fa、svc_tag、ai)与 ktest/bench/金丝雀不动。宿主注入口:justfile `DEMO` 变量直跑便门(`just DEMO=off run`);真通道 = berry CLI fw_cfg 面(owner 域)。

## Verification
双态实测(Apple M3 Pro/QEMU 11.0.3 TCG,采样口径 = QMP query-cpus-fast thread-id 的 proc_pid_rusage,时间字段实测定标 24MHz timebase——非旧文档 us):desktop 形(berry CLI 档手动复刻:EFI 镜像+数据盘+ramfb+noktest+display+rtc) ON=98.9% 满核(对齐 06 号 p1 94.7%/12 号 99.5%);bare 形 absent 与显式 on 双跑标记集逐项相同(零漂移)。OFF 态:DEMO gate off 行恰一次;A/B 零字母、舰队标记(G1.5/WAIT/CHAIN/MANY)零行;ktest/BLOCK/PI/INTENT/FAST/SVC/AIDEMO 自退出标记全在。

## Residual discovered
**门关后 desktop OFF 仍 99.9%——残余 = svcmgr 域演示链(talker 'd' 洪流 + echo 'v' 环)**:svcmgr `_start` 无条件拉起 echo/talker/echo2(user/bins/svcmgr/src/main.rs :365-384),talker 稳态 = 无帽节流忙环(user/bins/talker/src/main.rs :52-58);desktop OFF 稳态串口纯 'd' 流、HMP PC 采样 8 样本 7 落 EL0 用户文本(0x1214/0x1220)零落内核忙环——残余全在 EL0 svcmgr 链。svcmgr 不读任何 fw_cfg/bootparams,无现成门可注(本刀纪律 svcmgr 不动)。06 号 §3.1「服务 wait 面全体阻塞形」对 svcmgr 稳态环不成立(app 形 park 的只是 wait_many 尾,环体 echo 往返无条件每圈跑)。**idle→~0% 的完整闭合需要 svcmgr 侧演示门(owner 域后续刀)**;bare 形 OFF 实测 49.2%(v+d 双环残余,同源)。
→ **后半刀已闭合(2026-11-15 同日)**:[svcmgr idle 后半刀](2026-11-15-svcmgr-demo-face-idle-闭合.md)——x1 env 通道递门+bare OFF 实测 0.22% user;另唤醒两个同族结构缺口(单核 idle 停泊缺/yield 账本 IDLE 灌水)一并收口。

## Alternatives considered
门极性反转(在位才出演示——否:smoke/run 全相零漂移被破坏,06 号 §4-1 明裁产品/桌面档默认缺席);watchdog 不随门(否:舰队缺席即超时 panic,OFF 态 boot 崩);waker2 保留(否:waiter 信号无对端,4-1 验收司机随对同门才语义完整)。

## Related
06 号 §4-1 修窗件 1(v1/v4 0.0% 两锚);[CP-08 PM 接线](2026-11-15-cp08-pm-wiring.md);[svcmgr D3 帽扩容](2026-11-15-cp08-d3-cap-pm-prepull.md)(同窗 ① 号刀——本刀与其 main.rs/justfile 改动区不相交)。
