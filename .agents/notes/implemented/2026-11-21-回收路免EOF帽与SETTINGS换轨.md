# 回收路免 EOF 帽与 SETTINGS 访问器换轨——真机 CONTACTS 回收冻结案主修

Status: implemented

## Problem

2026-11-21 晚真机场实抓（槽刀+presarb 上线后）：用户连点 SETTINGS 四轮 recycle 秒收桌面存活；转点 CONTACTS 后 it 03 开出→再点→recycle 后桌面冻死（wmd q=32 爆满，点击全到无人消费）。对抗审查 11 案 confirmed 定谳主凶：外部 terminate(9) 杀活体 console=false app 后，回收路 `app_reap_tail` 的 `collect_eof` 确定性烧满 60s `APP_EOF_CAP`——EOF 是 berry-libc 用户态约定（exit/_exit/abort 才发零长帧），内核击杀不跑目标用户态；v1 通道明裁「对端关闭检测不做」（kernel/src/channel.rs:18），READABLE 派生位=本端队列非空 → 永不置位 → 等满 60s 超帽。svcmgr 单线程 serve 环（echo/launcher/wmd/app 监督同环）全冻 60s = 桌面「卡死」体感；连环点击 = N×60s 串行冻结。SETTINGS 四轮秒收之谜同刀定谳：settings 声明 fs 词但 wm 门取 `wm_cli_h()`=GIFT_H[0]=rootfs VMO（wants_fs 族真通道在 extras[2]=GIFT_H[4]）→ spawn 即「wm absent」park，park 先发 stdout EOF → 回收走快通道从未烧帽——即 SETTINGS 自 2026-11-15 K1 布局起真机从未真正开窗。

## Decision

七项清偿（审查 11 裁决全处置），主修锚 user/bins/svcmgr/src/main.rs：

- **免 EOF 帽（P1 主修）**：`app_reap_tail` 增 `killed` 参——外部击杀形（`app_spawn_recycle` 传 true）旁路 `collect_eof`（对端已死于构造，等待是结构性死代码）并关两端；boot 批同步形与 `app_supervise` 自然死亡面恒传 false 零漂。修后任意活体 app 回收归毫秒级，60s 烧帽类整类消失。
- **收口门重构（P2 句柄泄漏）**：close(out_s)/close(stdout_c) 从 `!eof_seen` 门拆出——collect 门保留 `!eof_seen && !killed`，close 门改 `out_s != 0` 两端恒关（治 parked 形回收每击净漏 2 句柄槽；kernel HandleTable 帽 65536 有界缓漏）。
- **eco 面分诊（P1-noise）**：killed 形 reap Ok(c) 打短行 `killed` 替代 `exit code`，`no EOF` 行随帽旁路结构性消失——预期态击杀与真异常退场分形，串口排障不再被设计路径噪音稀释；ps 账本仍记真码（kernel 落的退出码即事实）。
- **kill-before-validate 反序修正（P2 两案）**：app_launch 守卫前缀单源抽 `app_read_guards`（bap_scan→manifest→type/id/entry→compat/net→ui_face，纯读零句柄），`app_serve_spawn` 在回收/驱逐决策前先验——守卫败 = 拒新不杀旧；剩余不可前验面（wm attach 即唯一可行性探针；回收 DETACH 恰腾 wmd 槽供新 attach）注释文档化，失败净损失面以 `killed for nothing` 行可见化。
- **cbox 死亡面补齐（P2）**：`app_spawn_recycle` 在 reap 后补 sole 计算 + `cbox_death_flush`（与 app_supervise 死亡面同式）——回收独占的静桌面账本落盘窗不再跳过。
- **头注契约改写（P3 两案）**：M-C 段「collect_eof 60s + reap 30s 异步兑现」改为「回收/驱逐路免 EOF 帽恒毫秒级」如实记载；「复数同 id 由 ② 逐拍收敛」改为「结构性不可达（spawn 恒先过 ①）；异 id 僵尸收敛 = 表满点新 id 触发 ②」。
- **SETTINGS 换轨（本案新发现潜在 P1）**：settings/src/main.rs `wm_cli_h()`→`wm_present_h()`（=GIFT_H[4]=wants_fs 族真通道；files/photos/camera/browser/berry-term 同款），头注与降级门注释同批改写；settings_io 走 `fs_h()` 不受影响。修后 SETTINGS 复活为活体泵 app，且由主修保护不再被烧帽。messages/contacts 是 !wants_fs，`wm_cli_h()` 恰正确勿连带改。
- **测试面补缺**：tools/gui8-smoke.py 增 `--recycle-gate`（justfile `gui8-recycle-gate`）——gui8 全臂 APPDEMO 载荷 console=true 恒盲本类缺陷；门 = 单 boot 活体 BROWSER（console=false；CONTACTS 等 dock app 紧凑档无 dock 渲染不可点）图标首击 spawn + 多任务键二击同 id 回收（recents 重拉对窗态确定，避开覆盖态点击门的窗起时序不可观测），判据 = it→recycle→killed→it 行序 + 零 no EOF/exit code + 二击时延 ≤ 首击 + 40s（烧帽回归 +60s 必爆帽；killed 行缺席 = 主修回退即 FAIL）。

## Alternatives considered

- **本表持 stdout_c 原值自发 0 长 EOF 帧即时唤醒 collect**：可达同效但多一路伪协议面（自发帧与子真帧不可分）——跳过形更简，弃。
- **回收路专用短帽（≤1s）collect**：仍留秒级同步等待进 serve 线程，违硬约束，弃。
- **ps 账本 TERM-by-svcmgr 独立状态位**：需 berry-psproto 协议面扩容，超出本刀「只动 svcmgr main.rs」界——挂账，eco 面分诊先行。
- **门载荷用 CONTACTS**：CONTACTS/PHOTOS/SETTINGS/TERMINAL 是 desktop.rs DOCK_IDS dock app，800x600 紧凑档无 dock 渲染不可点——改 BROWSER。
- **二击再点图标**：app 窗在盖时覆盖态点击门先收盖消费点击，窗起时序（console=false 私有 stdout 无串口观测面）不确定——改多任务键重拉（导航键覆盖态恒最先）。

## Consequences

- 回收/驱逐路径 ACK 延迟上界 = 守卫解析 + 一次 spawn 链，无秒级同步等待面；「单发桌面冻结+陈旧点击自续链」60s 类整型消失。
- SETTINGS 真机行为变更：从 spawn 即 park（静态页）变为真开窗活泵 app——四槽生态多一个常驻活体，与 files/browser 同族。
- gui8 回归门依赖 `killed` 短行在位（主修锚）：修回退即门红；门自检零 QEMU（--self-test 覆盖正负样本）。
- 回收门随[总攻批3 生命周期终态](./2026-11-22-总攻批3生命周期终态.md)升 v2 三段制（it→alive→recycle→killed→it——同图标二击走 already-alive 聚焦路、back 销窗后三击走回收路）；`killed` 短行主修锚与零 noEOF 判据维持。
- 遗留挂账：ps 独立状态位（psp 扩容）；photos:24/browser:1245 头注里「settings（无 fs 词）」旧述失准（settings 现带 fs 词且已换轨）——两处域外注释未随批改（本刀界外），下批滚。
- 实施期并发注记：同仓兄弟会话同窗在 svcmgr/src/main.rs 落 wm push/subscribe 刀（wm_subscribe_deliver 族），两刀 hunks 无交叠、cargo check 全绿共存；gui8-smoke.py 曾出现一次外部整文件回写致部分 hunks 失踪，已内容锚定重敷并自检复绿——主会话提交前宜对两文件做 hunk 归属清点。
