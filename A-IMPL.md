# A-IMPL —— 刀 A 实施落账（应用槽合流 / 镜像接线 / 入口收敛 / 计数锁数 / 节流复核）

基线：worktree `wip/land-a` 自主树 HEAD 3af18dda1（Merge wip/sched-1.1 into wip/ui-spring）。输入 = [A-RECON.md](A-RECON.md)（2026-11-15 对账裁决）；实施全程 `CARGO_TARGET_DIR` 隔离 = 本 worktree `.target-a/`（直接 cargo 面）；镜像装配走 justfile 自有 `user/cargo-target`（worktree 内天然隔离，不触主树）。

## 〇、结论速览

1. **八槽合流落位**：FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER 六槽取 wave4 真 crate（cherry-pick），SETTINGS 体取主树 settings crate（本刀接线），appdemo 取 wave4 控件画廊——七槽 + 画廊全数真 crate 入镜像，appstub ELF 退役。wave3 六 `-app` crate + settings-app 全数弃置，无代码并入（对账结论：无互补面）；SETTINGS 槽唯一「弃侧并入」= wave3 的 justfile 接线段形态知识（非代码）。
2. **主树镜像接线**：GUI-8 循环对七槽全数走 APPTOMB 食谱（musl + berry-ld RUSTFLAGS → staging → mkapp 五道门禁）；appstub ELF 构建行退役，apps/<ID>/ 资产目录保留为 contacts/settings 两槽的 icons/views 兜底源 + genohicons.py 输出位。
3. **计数锁数**：smoke-image.sh ECO_MARKERS 与 markers.sh ROOTFS_MARKERS 的 `APP ok n=`/`APPUI ok n=` 前缀通配改锁实数 `n=9`/`n=8`，附「加/减应用槽随动契约」注释（六处同批改齐清单）；互非子串机查复跑零新增混叠。
4. **五入口收敛**：摘取 ux-unify 6e676da3e 的 berry 面四 hunk（Run accel / Shot profile+accel / Start profile+accel / Host profile+accel → desktop+auto）；Shell 结构体与 dispatch 无参回退主树已收敛（821d3f152），未重复摘取。shot 确定性由 commands.rs:197 `deterministic=true` 保恒 TCG。
5. **节流复核**：APP_SPAWN_LIVE_MAX=4 语义在七应用（实为八 [ui] 槽）串行 spawn 下完好——裁决 B 后 boot 批零占槽，帽预算全归 launcher SPAWN 形；预检在 spawn 前、满表拒新不拒旧 loud、退出槽位稳态环必回收、wait_many 集尺寸 2+2×4 与帽同源。零代码改动。
6. **额外发现（前置阻塞修复）**：HEAD 自带 kernel 构建断——2900c356c（虚拟化三刀）声明 `pub mod oneshot` 并消费 `crate::oneshot::tighten_deadline`，但 `sched/src/oneshot.rs` 漏提交（作者工作树未 add；仓内唯一实例 = 终结者支线 merge 2e775c12b 的冲突解产物）。本刀自该 blob 恢复 `sched/src/oneshot.rs` + `sched/host/oneshot_law_test.rs` 两漏件（签名与 HEAD 消费点精确匹配），律测 7/7 绿。

## 一、提交与改动清单

cherry-pick（`-x` 留源注记）：

| 落地 commit | 源 | 内容 |
|---|---|---|
| 4d01add63 | wip/ui-wave4 1ec2afa41 | 六应用真 UI crate（files/photos/camera/contacts/messages/browser：源码 + Cargo + crate 级 manifest + icons/views）+ W4-IMPL-*.md 六册 + W4-RECON.md |
| 993aee2b4 | wip/ui-wave4 f3a89e893 | appdemo 控件画廊翻新（gallery.rs 1028 行 + main.rs 755 行）+ justfile 六 crate 接线 + svcmgr 裁决 B（boot 批 [ui] 入册不拉起 + `app <id> gui deferred` 记账行 + app_launch defer_gui 形参）+ smoke/smoke-smp 逐槽 deferred 断言 + gui8-smoke 两处重裁 + berry-manifest fixture 重钉 + W4-VERIFY.md |

两件 cherry-pick 均干净落地（基座文件 svcmgr/smoke 三脚本/berry-manifest 在 base a2f9ff888..HEAD 间零漂移；berry-ui spring 面 +56 行纯增量，六 crate 消费 API 无删改）。

刀 A 手改（未含 cherry-pick）：

| 文件 | 改动 |
|---|---|
| justfile | ① SETTINGS 槽换真 settings crate：`settings` 入七 crate musl 构建循环 + `SETTINGS:settings` 入装配 spec 循环（manifest 走 crate 自有 22 行全形态，icons/views 走 appstub 兜底——与 contacts 同形）；② 删 SETTINGS stub 段五条 cp；③ appstub ELF 构建行退役（注释改记：零 bin/ 消费者，apps/ 资产目录保留兜底职能） |
| tools/berry/src/main.rs | 四结构体 default_value 翻转（§五明细） |
| tools/smoke-image.sh | ECO_MARKERS `APP ok n=`→`APP ok n=9`、`APPUI ok n=`→`APPUI ok n=8` + 随动契约注释块 |
| tools/markers.sh | ROOTFS_MARKERS 同对锁定 + 随动契约注释块（消费方 smoke.sh/smoke-smp.sh source 单源自动吸收） |
| sched/src/oneshot.rs（新，commit 34dbfc4f3） | 自 2e775c12b blob 4dc570971 恢复——2900c356c 漏件，37 行纯决策律 |
| sched/host/oneshot_law_test.rs（新，同上） | 自 2e775c12b blob 64e4f98d 恢复——同漏件，96 行 host 钉板（justfile test-oneshot 配方在册引用） |
| .agents/notes/implemented/2026-11-15-刀A应用槽合流与计数锁数.md（新，commit 784de947a） | 同批决策记录（Curia 规则 3）+ 写前检索分类结论；oneshot 三件套笔记互链双向 |
| .gitignore（commit 9ebca27f4） | 收 `/.target-a/`（任务指定隔离 target，同 user/cargo-target 先例） |
| justfile test-oneshot 配方（commit 6d317fca1） | `mkdir -p target-cargo` 前置——配方首跑即败坑（rustc -o 落位目录未必在），rm -rf 后复跑 7/7 绿 |

## 二、每槽裁决表

共同底账见 A-RECON §一：wave3 谱系不含 5452649af（svcmgr GUI 通道授权），其 app 在任何自动化路径下收不到 wm 端，「真 UI」仅构建期验证；wave4 = pattern 真消费者 + 七路验收（musl check 零警告 / smoke 全绿 / 降解清单六册）。

| 槽 | 胜者（源） | 弃者（源） | 弃侧并入点 | 裁决依据 |
|---|---|---|---|---|
| SETTINGS | 主树 settings crate（9035af061，941 行真交互：INPUT_POLL 批拉 → pump_input → take_events 驱动 Toggle/Slider/Check on_change；wm 缺席 loud park） | wave3 settings-app（1bc9c6f00，345 行 berry_gfx 手绘静态页，首帧后常驻 park 无输入消费） | wave3 的 justfile 接线段形态知识（app-stage 换真 bin 的先例）——本刀接线即其知识落地；**代码零并入**（静态手绘页无可并入面） | A-RECON §二：主树版唯一带交互与事件消费，manifest/[ui]/svcmgr 授权链全配；wave3 接的是更大秒退壳（其 svcmgr 无 GUI 授权） |
| FILES | wave4 files（1749 行：fsclient 真 READDIR FAT12 + STAT 定型 + statfs 空闲真值、chip 过滤、Scroll 跟手；4 测） | wave3 files-app（57fcbc464，331 行 bin/etc/user 三行硬编码静态卡，源码注释自认「点击事件未来口」） | 无（0 测 / 0 pattern / 无 manifest / 验收句仅存源码注释） | A-RECON §一行 1 |
| CONTACTS | wave4 contacts（1434 行：NavHost 详情页 push/pop + 拨号瞬态 + Star 收藏过滤；25 pattern 位） | wave3 contacts-app（57fcbc464，333 行四联系人行静态） | 无 | A-RECON §一行 2 |
| PHOTOS | wave4 photos（1512 行：2×3 网格 + Stack 叠层看片页 + PHOTOS.JSN 持久化） | wave3 photos-app（b1646469d，285 行六色缩略图静态网格） | 无 | A-RECON §一行 3 |
| CAMERA | wave4 camera（1490 行：HUD + 快门白闪特效 + Saved 计数 + CAMERA.JSN） | wave3 camera-app（b1646469d，276 行暗底亮框快门钮静态） | 无 | A-RECON §一行 4 |
| MMS/messages | wave4 messages（1922 行：四会话 + chip 快捷短语 + 草稿 + 本地节拍模拟回复；5 测） | wave3 mms-app（b1646469d，305 行四会话行静态） | 无 | A-RECON §一行 5 |
| BROWSER | wave4 browser（1911 行：书签网格 + Input/Go-Stop + Progress + NavHost + BOOKS.JSN；7 测） | wave3 browser-app（57fcbc464，353 行地址栏卡 + 四书签卡静态） | 无 | A-RECON §一行 6 |
| APPDEMO（画廊） | wave4 appdemo 翻新（f3a89e893：gallery.rs 1028 + main.rs 755，pattern 26 格 + kind 22 格常驻画廊；manifest stack_pages 8→64/label Gallery；fixture 重钉；smoke 双断言） | 无对家（wave3/主树均 42 行秒退原形） | ——（独有翻新，无碰撞） | A-RECON §三 |

裁决理由汇总（A-RECON §一尾）：六槽不合成、无互补面——wave3 六 crate 唯一相对优势「已进镜像」被 wave4 裁决 B + 全链 smoke 证据覆盖（进镜像的 park 壳在无 GUI 授权 svcmgr 下永不可达）。wave3 全部分支资产（六 `-app` crate + settings-app + 其 justfile 段）建议整支弃置；其「build-user 绿」口径与「真 UI」措辞的落差已记对账底册（A-RECON §七.4）。

## 三、主树镜像接线细节

- **构建**（justfile build-user 七 crate 循环）：`CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" cargo build --release --manifest-path bins/$c/Cargo.toml --target aarch64-unknown-linux-musl`，`c ∈ {settings files photos camera contacts messages browser}`——APPTOMB 食谱（:315-327 先例）逐字同形。
- **staging → mkapp**：`userbins/app-stage/<ID>/{manifest.toml, bin/<name>, icons/, views/}` → `python3 ../tools/mkapp.py userbins/app/<ID>.bap`（五道门禁：manifest 唯一/bin 唯一/ELF 魔数等）。manifest 单源 = crate 自有（七槽全带 crate 级 manifest.toml）；icons/views 单源 = crate 内目录，contacts/settings 两槽无资产目录走 `bins/appstub/apps/<ID>/` 兜底（if 两形，W4-IMPL-CONTACTS §1.1 口径）。
- **rootfs 收编**：`tools/mkcpio.py ../rootfs.cpio --manifest ../tools/rootfs-manifest.toml --split 32` 自动收 `app/*.bap` + `.bap.sig`。实测（本刀构建）：十件包 4.64MiB（八 GUI 槽 495,616-557,056B/件，SETTINGS.bap 495,616B 内含真 settings musl ELF 439,664B static stripped——旧 stub 壳 61,440B 对比）；rootfs 总量 39,014,400B≈37.2MiB，分卷 rootfs0.cpio 恰 33,554,432B（=32MiB 单卷帽整）+ rootfs1.cpio 5,459,968B——A-RECON §4.3 口径（单卷 32MiB + Σ≤128MiB）内合法，总量余 ≈90.8MiB。
- **appstub 退役形态**：ELF 构建行删除（零 bin/ 消费者）；`user/bins/appstub/src` 壳本体在册保留（后续新增槽未落 crate 前的应急占位——重建接线 = 恢复构建行 + cp staging bin/）；`apps/<ID>/` 资产目录保留（contacts/settings 兜底源 + genohicons.py/genohstatus.py 再生成输出位）。

## 四、smoke 计数锁数

改动两处（消费链：smoke.sh/smoke-smp.sh source markers.sh 单源；smoke-image.sh 独立硬拷贝同对）：

- `tools/smoke-image.sh` ECO_MARKERS：`"APP ok n="` → `"APP ok n=9"`、`"APPUI ok n="` → `"APPUI ok n=8"`；
- `tools/markers.sh` ROOTFS_MARKERS：同对锁定。

诚实计数口径（与生产侧 svcmgr 谓词零漂移）：9 = 八枚 [ui] 槽（APPDEMO/SETTINGS/FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER——裁决 B 后 boot 批入册不拉起、gui deferred 记账计入 ok）+ APPQUIET（console=false 批拉起形）；8 = 前述 [ui] 八枚资产面（APPUI 谓词 ui_decl>0 ∧ ui_ok==ui_decl）。真值漂移（如缺槽/验签败）时 svcmgr 打 `APP ok n=8` → 锁串不中 → FAIL 指认到行。

随动契约（注释已嵌两文件，六处同批改齐、漏一即 FAIL）：

1. smoke-image.sh ECO_MARKERS 两计数；
2. markers.sh ROOTFS_MARKERS 两计数；
3. tools/gui8-smoke.py `MARK_APP_N9` 锚；
4. tools/smoke.sh + tools/smoke-smp.sh 逐槽 `app <id> gui deferred` 名单；
5. justfile 七槽 spec 清单；
6. 槽数破 10 先扩 berry-manifest `APP_SLOTS` + svcmgr `APP ok n=` 单数字位编译断言（`APP_SLOTS < 10`）带参宽度。

保持前缀式的计数族（注释同页记因）：`PS ok n=`（boot 批进程组合随批次语义浮动，非槽位单源）；`CLI/FS/GFXF/PAGE ok n=`（步数单源在产侧 bsh/fserv/gfxdemo/launcher，宿主断言侧不双写）。互非子串机查复跑：两数组零新增混叠（boot 数组 `DEMO ok ⊂ AIDEMO ok` 为历史已知在册混叠，非本刀引入，未动）。

## 五、五入口收敛（ux-unify 6e676da3e 摘取面）

摘 Run/Shot/Start/Host 四结构体 8 行 default_value（单文件自包含，手工取 hunk——整枝 cherry-pick 会与主树 spring/launcher 演进冲突，且 Shell/dispatch 两 hunk 主树已收敛，A-RECON §5.3 判定）：

| 入口 | 改动前 | 改动后 |
|---|---|---|
| RunCmd.accel | `tcg` | `auto` |
| ShotCmd.profile / .accel | `default` / `tcg` | `desktop` / `auto` |
| StartCmd.profile / .accel | `phone` / `tcg` | `desktop` / `auto` |
| HostCmd.profile / .accel | `phone` / `tcg` | `desktop` / `auto` |
| ShellCmd.profile / .accel | 已收敛 `desktop`/`auto`（821d3f152，2026-11-13 owner 裁定） | 不动 |
| dispatch() 无参回退 | 已收敛 `desktop`/`auto`（同上） | 不动 |

前置核验（A-RECON §5.3 全在主树）：profile "desktop" 在册（profile.rs:34 PROFILES 三件）；`choose_accel(pref, deterministic)` 缝在席（qemu.rs:39——`deterministic=true` 恒 TCG，commands.rs:197 shot 路传 true，截图确定性不动）；smoke 三脚本直调 qemu-system-aarch64 自带旗，不受 CLI 默认翻转影响。未携整分支（launcher scrim/时钟/间距面与主树冲突大，A-RECON §5.3 判「刀 2 走 wave3 dd9452a7d 独立移植先例」）。

## 六、APP_SPAWN_LIVE_MAX=4 语义复核（结论：不破，零改动）

1. **帽预算独归 SPAWN 形**：裁决 B 后 boot 批对 [ui] 槽入册不拉起（`defer_gui=true` 路径 spawn 面让渡，`app_gui_deferred` 记账即返）——零 APP_CHILDREN 占位；非 [ui] 槽（APPQUIET）批内 spawn + `app_reap_tail` 同步收口（tail 直接消费不入表）。APP_SPAWN_LIVE_MAX=4 全部预算留给 launcher 点击（SPAWN RPC，`defer_gui=false`）。
2. **预检在 spawn 前**：`app_serve_spawn` 先扫 `APP_CHILDREN[0..4]` 锁空槽序号（单线程独占，serve 期无并发插槽），满表先拒（不拉起即无收口尾，无占位失败面）。
3. **满表拒新不拒旧 loud**：`eco: app <id> app slots` + ACK err=1——七应用串行点击全活场景：第 1-4 枚占槽常驻（GUI 帧循环活期），第 5-7 枚逐一点击逐一声 eco 拒绝；已有四枚监督/drain 照常（`app_supervise` 每稳态环全表探活），无死锁无静默。
4. **退出槽位必回收**：活子退出 → TERMINATED 信号入 `wait_many` 唤醒集（hs/sigs 尺寸 `2 + 2×APP_SPAWN_LIVE_MAX = 10` 与帽同源编译期钉死）→ 下一环 `app_supervise` take 槽位 → `app_reap_tail`（EOF 60s 帽 + reap 30s 帽）→ 槽归 None。串行 spawn 且前序退出形：槽回收 ≤ 一稳态环，后续 spawn 不触帽；退出与下一点击竞速的瞬窗最坏一次 loud 拒（下击即成），非破绽。
5. **帽值语义**（:812-817 头注原文）：「桌面 4 页 × 每页 1 点击并发面的宽余量（v1 demo 拓扑实活 ≤2）」——4 = 结构性上限非 UX 承诺；八 [ui] 槽注册 > 4 帽 = 故意的并发上限（全活桌面非 v1 目标态），拒面 loud 可归因。

## 七、验证账

| 验证 | 结果 |
|---|---|
| musl check（`.target-a` 隔离，`--release --tests`）八件：settings/files/photos/camera/contacts/messages/browser/appdemo | 全过零 error 零 warning |
| berry-manifest host 测试（fixture 重钉随 appdemo 翻新） | 46/46 绿 |
| berry-ui host 测试（spring 面 + wave4 pattern 消费回归） | 61 pattern 测 + 全套件绿 |
| berry CLI host check（四 hunk 后） | 编译零错 |
| `just build-user`（全装配 + mkapp 五道门禁 + mkcpio + TLS lint） | exit 0；十件 .bap；rootfs 分卷合法（§三） |
| `just build`（kernel，恢复 oneshot 漏件后） | exit 0（4 条预存 warning，非本刀面） |
| `just test-oneshot`（恢复件律测） | 7/7 绿 |
| `just smoke`（全相 QEMU：裸相 32 + rootfs 相 56 + RX 交互相 + FAT 子目录 + FAT 持久化 + B4 掉电相） | **PASS** exit 0——rootfs 相 56/56 到齐，锁串 `APP ok n=9`/`APPUI ok n=8` 实机过验 |
| 逐槽 `app <id> gui deferred` 八行 + `APP ok n=9`/`APPUI ok n=8` 锁串 | smoke rootfs 相逐槽断言全过；GUI 形（gui8 boot A 串口）同证据三重：`APP ok n=9`/`APPUI ok n=8`/8× deferred（含 SETTINGS 真 crate 槽）逐字在席 |
| `just gui8-smoke`（AAVMF EFI + tablet + QMP 点击链） | boot A 于桌面链楔死（`launcher: icon decode BadMagic` :397 → LAUNCH ok 缺席 → svcmgr G1.5 data abort + WTDIE）——与 W4-VERIFY §六.2 A/B 判别实验定谳的**预存环境面**逐字同签名（e58c94ccf「BadMagic 首帧零字节另刀挂账」；pre-波4 主树镜像同环境同楔死，图标载荷 sha256 对拍 SAME），非刀 A 回归；app 批证据在楔死前已全绿（上行）。补跑归合并前环境窗口（W4-VERIFY §六.3 同款挂账） |
| `just smoke-image`（EFI ECO_MARKERS 全表） | FAIL at 预存环境面——360s 未到齐，缺席清单自 `B1 ok n=4` 起（交互链 + 演示族 + TERM/CLI + LAUNCH/PAGE）；**`APP ok n=9`/`APPUI ok n=8`/`WM ok` 不在缺席清单 = 锁串 EFI 路到齐过验**。与 gui8 boot A 停段逐字同族（B1/U4/INT/POLL/MMP/CLK/SIG/CLI/TERM 同缺席 + 同组 eco 降级）——W4-VERIFY §六.2 已以 pre-波4 主树镜像 A/B 对照定谳该 EFI 形停段/桌面楔死为宿主环境面非载荷面 |

## 八、风险与移交

1. **sched oneshot 漏件已就地恢复但上游分支仍缺**：wip/sched-1.1 尖（03d6dc9b5）与 wip/ui-spring 谱系树内均无 `sched/src/oneshot.rs`——本刀恢复件（源 2e775c12b blob）应回灌 sched 归属线；`just test-oneshot` 配方的 target-cargo mkdir 坑已顺手修（commit 6d317fca1）。
2. **kernel 4 条预存 warning**（berry-kernel bin）：非本刀触面，未动。
3. **boot 数组历史混叠**（`DEMO ok ⊂ AIDEMO ok`/`M3DEMO ok n=5`）：在册已知，非本刀引入；若日后 boot 日志同现两形会互相顶替，归 markers 治理线。
4. **wave3 分支资产处置**：建议整支弃置不留 cherry-pick 欠账（A-RECON §七.4）；其六 crate 从未合入本树，无需清理动作。
5. **gui8-smoke / smoke-image 的环境面**：本宿主当前形态下 EFI 形 boot 在交互链中段（B1 族起）停滞 + 桌面链 icon decode 楔死（§七表内两行 + gui8/smoke-image 停段逐字同族对照）；W4-VERIFY §六.2 的 A/B 判别（pre-波4 主树镜像同环境同楔死 + 图标 sha256 对拍 SAME + 三重离面对拍）已定谳为 e58c94ccf 挂账的宿主环境面，非刀 A 回归。刀 A 所需的判据面（APP/APPUI 锁串 + 八 deferred）在两形 boot 日志中全绿。两脚本补跑归合并前环境窗口（W4-VERIFY §六.3 先例）。**smoke-smp 未跑**（时间窗；其断言面 = smoke.sh 同源 deferred 名单 + markers.sh 单源锁串，UP 形已全绿，SMP 形风险面 = 调度时序非本刀触面）。
6. **`eco: app APPQUIET it 04` 非降级**：M4 intent 透传观测行（90da90ba3 既有：拉起后 getattr 读回 sched intent 档位 hex 打印——APPQUIET manifest intent=background=0x04 恰回读一致 = 链路自证）。「family 诊断族非标记——不 gate smoke」，裁决 B 后 APPQUIET 为唯一批拉起 app 故恰一行；与 W4-VERIFY §4.1「eco: app 零行」口径的差异 = 该句指降级形（exit code/no EOF/app slots 族），观测行不在此列。
