# W3A-REVIEW —— 波3a 对抗审查报告

Date: 2026-11-15
Owner: 波3a 对抗审查员（只读审查；FAIL≤3 就地修）
对象: `wip/ui-wave3` @ HEAD bcb8f456d。基线勘定：主树 HEAD = wip/sched-1.1@01cd881b2，与本分支 merge-base = 509000eb6（分支创建点，reflog「Created from wip/sched-1.1」10:30:53）；基线..HEAD 共 27 笔，构成 = 前批波3 七笔（件①/⑤a/⑤b/⑤b补/④/⑥a/②）+ SB 三笔 + wave4 三笔 + SA 四笔 + 夹缝修 6d80a0fd2 + SC 两笔 + 文档债三笔（8c885820b/8820895a3/d541b5e4e）+ 验收批四笔（535096164/6b30aac79/786796321/bcb8f456d）。任务书「三笔」按波3a 三路交付（SA/SB/SC = W3A-VERIFY §① 表九笔）理解，审查面覆盖三路全部提交 + 验收批 + W3A-{IMPL-SA,IMPL-SB,IMPL-SC,VERIFY}.md 四件 + 主树 `.agents/plans/2026-11-15-波3桌面三栏与系统窗实施设计.md`（与工作树在册版 diff 逐字节同文，实证）。

## ① 设计符合度 —— PASS

- **SA appshell = §6 清单**：crate 公开面恰五模块 `rpc`/`pump`/`scene`/`icons`/`conf`（lib.rs 模块面注直引 §6），与 §6 表五行一一对应；第六面 `host`（Host 绑定表 + bind）在 SA 清单明示为「新增缝」（libc 注入点，非 §6 外私货）。§6「新增面 v1」（wm 生命周期封装/Nav 页栈/back 消费/ime_wanted/Dialog/Menu/Toast/APPDEMO）在 SA「范围与边界」明文不在本刀、设计 §8 对应条目未勾——部分交付如实挂账，非缺失冒领。保真抽验：RPC_CAP_NS=300s 与 b1646469d 原 main.rs:73 逐字同值且测试钉板（注意 §6 行文「10s 帽重臂」与实码不符——10s 实为 pump IDLE 帽 IDLE_CAP_NS，见建议 a）；status_icons/wallpaper 两文件 git mv R100 保历史。
- **SB wmd = §3 状态机**：R2 全要素核符——OP_WM_BACK 按波2 冻结线面（guiproto WM_BACK_REQ_LEN=8B 无参帧，3825 行钉板 `[0x67,0,…]`）由 wmd 查表路由焦点窗（table.rs `back()`：无焦点 ST_NOENT；无事件通道旧式窗直接 DESTROY = §3 配套立法末条原文；有通道非阻塞投递 WM_EVT_BACK + 武装 500ms 单槽）；解除条件双臂 = BACK_ACK（wid 失配忽略）+ 任何表变更（`bump()` 清 `pending_back`，table.rs:162）——§3「无 BACK_ACK 且无表变更」字面全落；兜底 `on_back_tick` DESTROY + `refocus_after_removal` 表序最末可见窗（§3 R2 回桌面句）。配套立法五件全在：属主通道（CREATE gift 随行/DESTROY 关闭）、帧型 0x90/0x91/0x92（带 0x90..=0x9F 落全部 op 带外、错投恒 ST_BAD）、BACK_ACK 1B、500ms 单槽进 serve 循环零新线程（main.rs wait 集拼装）、TABLE_WAIT 冻结（§9）+ 钉板测试 `table_wait_frozen_single_slot_semantics`。R1 launcher 侧已落（frameloop.rs:481-519 应用在焦先收→sys 页回桌面 tab→桌面翻页，一 back 一义）；R3 appshell 侧属新增面挂账（如实）；R4 设计明示本波不做。命名 WM_EVT_* 随 crate INPUT_EVT_* 家族（§3 散文写 WM_EVENT_*，非冻结名违约）。
- **SC TabBar = §1 条**：BottomTabs big 档直用 pattern 冻结名 + 紧凑 @1x 镜像（pattern 无密度参数，如实注记）；桌面页 = SwipePager 四页+dock 原样内嵌（只换父位）；应用列表页 = List 容器 + Card 白底行（LISTITEM_H 144/48 双档 = 48vp 两口径）+ Input 件 + 固定过滤词表 ALL/A-F/G-M/N-Z（syspages.rs:63，纯函数 filter_matches 带完备互斥测试）——Search 输入链（softimed/TextEvent/收链）明标 3b 暂缓、v1 降级如实；设置页 Boot Toggle 真落 BOOT.JSN + Display Radio 互斥/Seg + Sound Slider permille 真存 SETTINGS.JSN（两键均走 appshell::conf 单簇原子写，settings_io.rs:35-42）；press_map 翻页重登记 = 侦察 :31 挂账同批清偿（帽 28，rehang 三拍单源）。**in-content 过渡位诚实标注在案**：desktop.rs:51「随 launcher 内容层整层被盖——02 号 §2.3 边界，SYSTEM 窗随 I1」+ 裁决四-e（状态栏/壁纸 = 内容层非 wmd 系统窗）+ 设计 §8「launcher 双窗化建树」条未勾 + W3A-VERIFY §①b「SYSTEM 层视觉提交零实现」——过渡态三处互证，未把 in-content 冒充双窗化完成。

## ② 3b 暂缓面零偷跑 —— PASS（独立复核，非转抄）

- softimed：user/bins/ 无 softimed 目录、tools/rootfs-manifest.toml 零条目；全仓 `softimed|softime` 仅注释两处（syspages.rs:13 降级注记 + guiproto lib.rs:114 层注释）——零实现。
- 沉浸/IME：`IMMERSIVE|ime_wanted` 全仓大小写不敏感零命中；WM_CREATE_F 旗面仅 AVOID_SYS（1<<0），未知位拒面测试在册（guiproto:3458 断言 0x02 拒）——连旗位都未偷占。
- 带合成/订阅客户端：guifbd guest.rs 零 OP_WM_SUBSCRIBE 调用、零带缓存、零重放；3a+验收全 17 笔提交文件面逐一核验零触碰 guifbd（`git show --stat` 循环全 0）。launcher 仍单窗提交 surface_id=0xFFFF（frameloop.rs:185）——双窗化零偷跑。OP_WM_SUBSCRIBE 协议+服务侧落地属 SB 件③ 立法面（§3 表变更广播/§4 通知源），客户端消费挂账移交，W3A-VERIFY §①b 如实记账。

## ③ 零行为回归承诺 —— PASS（独立复跑）

- 断言面不变：b1646469d（抽取前净基点）与 HEAD 四文件（frameloop/main/desktop/layout_io）标记字符串集合 diff 逐一 SAME（LAUNCH/PAGE/CLICK/LAYOUT/LAYOUTW/click ready 族）；RPC 帽原值保真；pump 调用序与原 frameloop 同构（berry_*/gp::/rpc 序列 diff 仅回调移出四项——clock/TABLE_GET/DESTROY/power_off 归 LoopHost 回调，与抽取设计一致）。
- 独立复跑（隔离 CARGO_TARGET_DIR=/tmp/w3a-review-target，排除共享投毒形态）：berry-appshell **27/0**、berry-guiproto **52/0**、gui-wmd lib **14/0**、gui-inputd **25/0**、berry-ui 全量 **243/0**（29 个测试目标逐一定和）——与 W3A-VERIFY §② 数字逐一相符；`tools/gui8-smoke.py --self-test` SELFTEST PASS。
- gui8-smoke 真机红：接受为基线既有红——87432B 堆 abort 签名与 SA 验收补记（125a61ede）逐字同形、净基点 b1646469d 独立 worktree 同红实验在案、根因归 wave4 图标堆悬崖（PX_CAP≈879KB vs launcher 1MiB 静态堆），修复归属 launcher/wave4 lane 有据；3a 在机 LAYOUTW/LAYOUT ok 过链为正证据。本次审查不重跑真机 smoke（兄弟 lane TCG 饱和 + 堆悬崖未修，重跑必红且无新信息）。
- 验收批 535096164（berry-ui Select 一行修）：`git diff b1646469d..验收基点 -- user/berry-ui/` 为空（三路零碰 berry-ui）+ b9788e248 出生点独立实证同红 + 修后 243 绿（本次复跑证实）——存量出生红非 3a 回归，修法一行区带参数对齐钉板，证据链完整。

## ④ 纪律 —— PASS（两处边界扩张均披露在案，非偷跑）

- 提交面精确：3a+验收 17 笔 `git show --name-status` 逐一核验，无搭车文件；三路声明禁碰面逐笔核符（SA 零碰 guifbd/gui-wmd、SB 零碰 launcher（恰 guiproto+gui-wmd+justfile）、SC 恰 5 文件全在 berry-launcher）；SA∩SB justfile 交叠双行并存互不覆写（appshell 行 829 / gui-wmd 行 840，均在树）。
- 主树脏文件零触碰：主树 10 改 + 11 未跟踪（波4/终端刀/bsh 域）与 3a 提交路径零交集；3a 全部提交经本工作树 reflog 在案（工作树物理隔离，无主树侧提交）。并行工作树零碰：ui-wave4 等他树脏面（user/、W4-*.md）与 3a 提交路径无交集；本工作树未跟踪物（.target-kernel/.target-launcher/tools/.berry-verify）审查全程未触碰（审查自建 /tmp 隔离 target）。
- 边界扩张两笔（均先例+披露，判不违纪）：535096164 越三路声明面修 berry-ui 存量红——任务书「红则修」条款 + 出生红三重证据在案；8c885820b/8820895a3/d541b5e4e 修 vnc wave9/10 与刀6 他 lane 文档断链——CI --strict 挡门基线债、「只修链接不改内容」入 commit message，且 W3A-VERIFY §⑥ 以「并行文档三笔」如实记账。

## ⑤ check-docs —— PASS（含分刀节勘定）

- `node .agents/tools/check-docs.mjs`：260 篇 0 告警；`--strict` 同绿 exit 0（含本审查就地修后复跑）。W3A-*.md 在仓库根（check-docs 默认面外），其内唯一相对链接（W3A-IMPL-SC → 装配面笔记）目标存在，实证 OK。
- 分刀节勘定（如实记录）：设计稿全文 144 行无名为「分刀」的节——分刀事实的树内落点 = §1 七件套分解 + §8 未完成事项（双窗化/softimed/新增面等未勾条目即 3b 暂缓面的清单面对应物）+ 各 W3A 稿「范围与边界」节 + W3A-VERIFY §①b（3a/3b 边界的操作定义）；主树与工作树两版设计稿同文，无第三版。此为任务书措辞与树内现实的偏差，非交付缺陷（见建议 c）。

## 发现与处置

- **FAIL-1（已就地修）**：`.agents/notes/implemented/2026-11-15-波3TabBar三页装配面.md` 持久键钉板节残留初稿笔误 16B/17B——6b30aac79 修了 settings_io.rs 头注/两断言与 W3A-IMPL-SC.md 但漏改同批笔记，与单源真值 `BootMode::canonical`（splash 17B/verbose 18B，python 实测）矛盾。处置：本审查同批改 17B/18B 并注「6b30aac79 定谳、本行随改」，与本报告一并显式路径提交。
- 建议级（不阻门，供波3 收口翻 active 时采纳）：a) 设计稿 §6 rpc 行「10s 帽重臂」正字为 300s（RPC_CAP_NS 实值；10s 是 pump IDLE 帽）；b) §3 R2 括注「wid=焦点窗」补注「无参帧、wmd 查表路由」（波2 冻结线面如此，防后续读者按括注找 wid 域）；c) 3a/3b 分刀边界现只落工作树根 W3A-*.md（沿 ui-knife3 的 KNIFE3-*.md 根位先例），建议波3 收口时以一节或一笔 notes 把分刀及暂缓面固化进 .agents，免散件失传。

## 结论

**PASS**。五项逐查全过：§6/§3/§1 三面对得上且部分交付处均有诚实挂账、3b 暂缓面独立复核零实现、断言面与五套 host 测独立复跑全绿（27+52+14+25+243）+selftest、纪律面 17 笔零越权（两处扩张均有授权依据与披露）、check-docs 260 篇 0 告警。1 FAIL（笔记 canonical 长度残留）已就地修并随本报告入库；建议级三条留档。审查操作纪律：全程只读 + /tmp 隔离 target，工作树唯一改动 = FAIL-1 修复与本报告，主树/他工作树/未跟踪物零触碰。
