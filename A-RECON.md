# A-RECON —— 刀 A 对账（波 4 双刀碰撞 / 镜像接线 / 入口收敛 / smoke 计数）

基线：worktree `wip/land-a` 自主树 HEAD 3af18dda1（Merge wip/sched-1.1 into wip/ui-spring）。对账对象：wip/ui-wave4 尖 f3a89e893（六 crate 提交 1ec2afa41 + appdemo/装配 f3a89e893）vs wip/ui-wave3 尖 b1646469d（批 2 57fcbc464 + 批 3 b1646469d，SETTINGS 首件 1bc9c6f00）；wip/ux-unify 尖 6e676da3e。全部行号与字节数为 2026-11-15 各树现场直取。

## 〇、结论速览

1. **六应用槽全线取 wave4**：wave4 六 crate（files/contacts/photos/camera/messages/browser）= pattern 零手拼 + 真交互 + 持久化 + 全 manifest + v1 降解清单 + 七路验收（musl check 零警告 / smoke 全绿）；wave3 六 `-app` crate = berry_gfx 手绘静态页、零测试零 pattern、无 crate 级 manifest、验收标记只存在于自家源码注释里（全树 grep 零 harness 挂线）、svcmgr 无 GUI 授权（5452649af 不在其谱系）——app 在任何 smoke 路径下都拿不到 wm 礼物，wm_cli_h()==0 静默 exit 0，「真 UI」仅构建期验证。逐槽详比对见 §一。
2. **SETTINGS 槽**：保主树 settings crate（941 行，真交互帧循环 + 全 manifest），弃 wave3 settings-app（345 行静态页）；接线知识取 wave3 的 justfile 段形态，但接线本体必须走 wave4 已落的裁决 B（defer_gui）——wave3 无裁决 B，其接法若真拉起 park 形 app 会让 app_reap_tail 永等（W4-RECON §1.7 铁律）；wave4 自己也把 SETTINGS 接线挂后续批（stub 残留）。详比见 §二。
3. **rootfs 32MiB 疑云解散**：32MiB 是**单卷帽**（fw_cfg 单卷，kernel `ROOTFS_MAX = 32<<20`），不是 rootfs 整体帽；整体硬界 = Σ ≤ 128MiB（VMO_MAX_SIZE，A1-2 帽分家）。wave3 的 38023168B≈36.3MiB、wave4 实测 38555648B≈36.8MiB，均由 `--split 32` 自动分卷消化（rootfs0 恰 32MiB + rootfs1 余量），合法，余量 ≈91MiB。原句引文见 §四。
4. **五入口收敛可摘**：ux-unify 6e676da3e 的 berry 面 = 单文件 11 行翻转（Run/Shot/Start/Host 四结构体 default_value）；Shell 结构体默认与双击回退主树已收敛（821d3f152 vnc 批，2026-11-13 owner 裁定）。摘取前置全在主树：profile "desktop" 在册（profile.rs:34）、choose_accel(pref, deterministic) 缝在席（qemu.rs:39）、shot 已传 deterministic=true（commands.rs:197）。不需要整分支（其 launcher/berry-ui 面与主树 spring 重做冲突）。见 §五。
5. **smoke 计数**：smoke-image.sh:326 / markers.sh:24 的 `APP ok n=` 是前缀通配（grep -qF 子串），计数不锁在脚本；真锁 = tools/gui8-smoke.py:82 `MARK_APP_N9 = "APP ok n=9"` 全串（锚列表 :1096/:1421/:1444/:1459）+ svcmgr 侧 `APP_SLOTS<10` 编译期单数字断言。改法（wave4 已示范）：svcmgr 谓词语义先行两支化 + smoke.sh 逐槽断言行 + gui8-smoke 重裁 + PS n 随动。见 §六。

## 一、波 4 双刀六应用槽逐槽对账

共同底账：wave4 基线 769515a9a 含 9035af061（settings 垂直切片）+ 5452649af（svcmgr GUI 通道授权——wants_gui 判别 + wm/input 礼物槽位）；wave3 谱系**不含 5452649af**（`git merge-base --is-ancestor` 实证 NOT in wave3 / IN main），其 svcmgr 全文件无 wants_gui/wm 礼物面——这是六槽共同的结构性分水岭：wave3 app 永远收不到 wm 端，代码里的建窗路径在任何自动化路径下不可达。

质量五维（测数 = crate 内 `#[test]`；pattern = `pattern::` 引用处数；降解 = v1 vs 终态差异清单；manifest = crate 级 manifest.toml）：

| 槽 | wave4（无后缀 crate） | 测/行/pattern | wave3（-app crate） | 测/行/pattern | 裁决 |
|---|---|---|---|---|---|
| FILES | files：fsclient 真 READDIR FAT12 + STAT 定型 + statfs 空闲真值；chip 过滤（目录首字母派生词表）；Scroll 跟手拖滚；apps 树缺席如实空态 | 4 / 1749 / list_item+chip+top_bar | files-app：bin/etc/user 三行**硬编码**静态卡；注释自认「点击事件未来口——v1 静态展示面」 | 0 / 331 / 0 | **取 wave4** |
| CONTACTS | contacts：NavHost 详情页 push/pop + 拨号瞬态行 + Star 收藏过滤（Seg） | 0 / 1434 / 25 | contacts-app：四联系人行静态 | 0 / 333 / 0 | **取 wave4** |
| PHOTOS | photos：2×3 网格 + Stack 叠层看片页 + PHOTOS.JSN 持久化 | 0 / 1512 / 23 | photos-app：六色缩略图静态网格 | 0 / 285 / 0 | **取 wave4** |
| CAMERA | camera：HUD + 快门白闪（设计稿明载特效）+ Saved 计数瞬态行 + CAMERA.JSN | 0 / 1490 / 26 | camera-app：暗底亮框快门钮静态 | 0 / 276 / 0 | **取 wave4** |
| MMS/messages | messages：四会话 + chip 快捷短语发送 + 草稿展示 + 本地节拍模拟回复 | 5 / 1922 / 41 | mms-app：四会话行静态（头像色块+名字+摘要） | 0 / 305 / 0 | **取 wave4** |
| BROWSER | browser：书签网格 + 地址行 Input/Go-Stop + Progress 模拟加载 + NavHost 页页 + BOOKS.JSN 持久化 | 7 / 1911 / 31 | browser-app：地址栏卡 + 四书签卡静态 | 0 / 353 / 0 | **取 wave4** |

补充维度：

- **manifest 完整度**：wave4 六 crate 各带 22-23 行全形态 manifest.toml（[app]/[capabilities]/[runtime stack_pages=64]/[security 三键]/[ui icon+label+views]）；contacts 无 crate 级 icons（装配兜底 appstub 源，W4-VERIFY §3.2 明载）。wave3 六 crate **全部无 manifest.toml**，装配一律 cp `bins/appstub/apps/<ID>/manifest.toml`。
- **降解诚实度**：wave4 每应用一册 W4-IMPL-*.md，§三「v1 形态 vs 终态差异（3b 依赖降解清单）」逐项 5-10 条，每条给迁移面（appshell 收编/裁决 A 换轨/IMMERSIVE/theme 样式族/conf 键族），并诚实标注缺失（如 photos/camera 的「0.95 缩放档无等价面（缺失）」）。wave3 无降解文档，commit message 即全部说明；其「FILES ok」验收句只存在于 files-app/photos-app 源码注释（`git grep` 全树仅两处源码命中，smoke.sh/markers.sh/svcmgr 零挂线）。
- **pattern 消费**：wave4 = pattern 首批真实消费者（W4-RECON §1.4：此前 pattern.rs 27 公开面位唯一消费面 = 自带单测），且零手拼 grep 门验收侧独立复跑（Card 直载/Rect/按压字面量逐 crate 钉数）。wave3 零 pattern（Cargo.toml 无 berry-ui 依赖，纯 berry_gfx CommandEncoder 手绘）。
- **验收证据**：wave4 = W4-VERIFY 七路：musl check（含 --tests）七件零 error 零 warning；宿主 user 工作区测试全绿（berry-ui pattern 61 测；一处真红 berry-manifest fixture 随 appdemo 翻新重钉，红则修兑现）；`tools/smoke.sh` 全量 PASS（含 8× `app <id> gui deferred` 逐槽断言）；gui8-smoke 两处重裁；包体 8 件 3.97MiB、单包最大 0.53MiB。wave3 = commit message 自称「构建零错+build-user 绿」（注意措辞是 build-user 绿，非 smoke 绿）；rootfs 36007936B（SETTINGS 件）/37015552B（批 2）/38023168B（批 3）为构建产物尺寸。
- **装配形态**：wave4 justfile = `for c in files photos camera contacts messages browser` 统一 musl+berry-ld 循环 + crate 自有 manifest/icons/views 入 staging（缺资产兜底 appstub）；SETTINGS 残留 stub 段。wave3 justfile = 逐批显式段（批 2「独立构建段+循环剥离三 id」、批 3「六 app 循环统一」七应用 7/7），资产恒取 appstub。

裁决理由汇总：六槽不合成、无互补面——wave3 六 crate 的唯一相对优势是「已进镜像」（.bap 里是真 ELF），但其真 UI 在无 GUI 授权的 svcmgr 下不可达，进镜像的只是更大的秒退壳；wave4 以裁决 B + 全链 smoke 证据覆盖了同一目标。wave3 六 crate 全数弃；wave4 六 crate + appdemo 翻新全数取。

## 二、SETTINGS 槽三版本详比与裁决

| 维 | 主树 settings（9035af061，在册） | wave3 settings-app（1bc9c6f00） | wave4 处置 |
|---|---|---|---|
| 行数/链 | 941 行；std musl 链 + berry-ui 五 deps | 345 行；四 deps 无 berry-ui | 未动（crate 在册不在七路报告面） |
| UI 形态 | berry-ui desc Props 建树：Toggle/Slider/Check 真交互件 + on_change EV_* 事件 | berry_gfx 手绘静态页：TopBar+三组标题+白底卡行 | — |
| 帧循环 | INPUT_POLL 批拉→push_pointer→pump_input→take_events→damage 驱动 render（空集零 SUBMIT）→vote 变化才发→IDLE 帽 | 首帧 SUBMIT 后常驻 park（无输入消费） | — |
| 降级门 | wm 缺席 = `settings: wm absent (no gui gift)` + park（窗口义务语义） | wm 缺席 = 静默 exit 0（netd 双门先例） | — |
| manifest | crate 级 22 行全形态（含 [ui] 三键 + stack_pages=64） | 无（用 appstub SETTINGS 模板） | — |
| 测试 | crate 内 0（W4-RECON §1.4 口径；pattern 消费归后续批） | 0 | — |
| 接线 | **未接线**：主树 justfile :328-337 循环对 SETTINGS 仍 cp appstub（W4-RECON 结论 1：镜像内 SETTINGS.bap 61,440B 是秒退壳） | **已接线**：justfile app-stage SETTINGS 换真 bin，rootfs 36007936B | **未接线**：SETTINGS 残留 stub 段，接线归后续批（裁决 B 已清前置） |

裁决：**体取主树 settings crate，接线知识取 wave3 段形态，弃 wave3 settings-app crate**。理由：①主树版是唯一带交互与事件消费的版本，且 manifest/[ui]/svcmgr 授权链（5452649af）全配；②wave3 版已接线但接的是静态手绘页，且其 svcmgr 无 GUI 授权——拉起后 wm_cli_h()==0 静默退，接线等于换了个更大的秒退壳；③wave4 的裁决 B（boot 批 [ui] 入册不拉起 + `app <id> gui deferred`）已清「park 形入镜像 = app_reap_tail 永等」的前置铁律，settings 接线应搭该车落，而非复制 wave3 的裸接线。

## 三、appdemo 槽（wave4 独有翻新，无碰撞对家）

wave4 f3a89e893：appdemo 由 42 行秒退自验件翻新为控件画廊（gallery.rs 1028 行 + main.rs 761 行）——pattern 组件画廊 + 帧循环常驻；manifest 翻新（stack_pages 8→64、label Gallery）；berry-manifest fixture 钉板随更（真红一枚，重钉修复）；smoke 随动（boot 批不再拉起，`app APPDEMO gui deferred` 恰 1 行 + 点击 spawn `app APPDEMO up` 恰 1 行双断言）。wave3/主树 appdemo 均为 42 行原形。**取 wave4**。

## 四、主树镜像接线现状与 rootfs 尺寸账

### 4.1 GUI-8 循环 cp appstub 位置

主树 justfile :328（注释「GUI-8 视觉补全：7 个占位 app（图标/入口填充；内容未实现）」）起：

```
for spec in SETTINGS:settings FILES:files PHOTOS:photos CAMERA:camera CONTACTS:contacts MMS:messages BROWSER:browser; do
    id=${spec%%:*}; bin=${spec##*:}
    mkdir -p "userbins/app-stage/$id/bin"
    cp "bins/appstub/apps/$id/manifest.toml" "userbins/app-stage/$id/"
    cp cargo-target/aarch64-unknown-none/release/appstub "userbins/app-stage/$id/bin/$bin"   # :333
    cp -R "bins/appstub/apps/$id/icons" ...; cp -R "bins/appstub/apps/$id/views" ...
    python3 ../tools/mkapp.py "userbins/app/$id.bap" "userbins/app-stage/$id"
done
```

即七槽全为 appstub 静默壳（none target ELF）；settings crate（941 行）未进链。appstub 本体构建段 :168-171。真 crate 入包先例 = APPTOMB 段 :315-327（musl target + berry-ld RUSTFLAGS 食谱）。

### 4.2 mkapp/userbins 装配形态

staging = `userbins/app-stage/<ID>/{manifest.toml, bin/<name>, icons/, views/}` → `tools/mkapp.py userbins/app/<ID>.bap`（五道门禁：manifest 唯一/bin 唯一/ELF 魔数等）→ rootfs 条目 `app/<ID>.bap` 由 `tools/mkcpio.py ../rootfs.cpio --manifest ../tools/rootfs-manifest.toml --split 32` 收编（justfile :348-351）；`.bap.sig` 伴生自动收编（缺席零漂）。主树 userbins/app/ 现十件：APPDEMO/APPQUIET/APPTOMB + 七 stub 槽。

### 4.3 rootfs 尺寸账与 32MiB 帽口径（结论：单卷帽，非整体帽）

32MiB 定义原句：

- justfile:352-354：「A1-3b（余量刀）：mkcpio --split 32 = 主路恒走多卷产链——≈28MiB rootfs = 1 卷 rootfs0.cpio（**单卷 ≤32MiB 与内核 fwcfg 单卷帽同口径**，容量工程前置-A1 §3.1）；越帽自动分卷、消费端（内核连续前缀发现序 opt/berry/rootfs0..9 + VMO 逐页拼装）零改动。」
- justfile:540-542：「rootfs fw_cfg 卷动态递送……（内核连续前缀发现序 opt/berry/rootfs0..9 优先，**单卷帽 32MiB**）。」
- kernel/src/main.rs:94：`const ROOTFS_MAX: usize = 32 << 20;`；:158：「两帽执法（§3.1 帽分家）：单卷 ≤ ROOTFS_MAX（产链契约）、Σ ≤ …」；:97：「VMO_MAX_SIZE（128MiB，syscall.rs）同口径；Σ 超帽 = 产链违约，拒收。」

实测口径：帽 = fw_cfg **单卷** 32MiB + 总量 Σ ≤ 128MiB（VMO_MAX_SIZE，A1-2 帽分家后各帽不再联动）。主树现场产物：rootfs.cpio 单档 35,680,256B（34MiB）+ 分卷链 rootfs0.cpio 恰 33,554,432B（=32MiB 整）+ rootfs1.cpio 2,125,824B——主树今天已超 32MiB 且合法分卷中。wave3 自称 7/7 时 38,023,168B≈36.3MiB、wave4 实测 38,555,648B≈36.8MiB（W4-VERIFY §〇.5：rootfs0 恰 32.0MiB 帽 + rootfs1 4.8MiB 分卷消化；总量帽 128MiB 余 ≈91MiB）。「超 32MiB 帽」疑云 = 把单卷帽误当整体帽；历史成因可溯 docs/阶段沿革.md:203（55c6f3e 容量一体时代 VMO/ROOTFS/BerryBoot init/盘 四帽同口径 32MiB，A1-3b 帽分家后单卷/总量两分）。

## 五、五入口收敛（ux-unify 摘取面）

### 5.1 6e676da3e diff 形态

四文件 +72/−21：tools/berry/src/main.rs（22 行、11 处翻转——**自包含可摘**）；user/bins/berry-launcher/src/desktop.rs +55（scrim 动态预混/时钟宽/图标间距——刀 2）；icons.rs +9（wallpaper_top_rows）；user/berry-ui/src/lib.rs +7（text_width 缝）。 berry 面翻转明细：RunCmd.accel tcg→auto；ShotCmd.profile default→desktop + accel tcg→auto；StartCmd/HostCmd.profile phone→desktop + accel tcg→auto；ShellCmd 同对翻转；dispatch() 无参回退同对。

### 5.2 主树四入口默认分裂点（tools/berry/src/main.rs 现行值）

- :96 RunCmd accel="tcg"；:116 ShotCmd profile="default" + :118 accel="tcg"；:284 StartCmd profile="phone" + :286 accel="tcg"；:324 HostCmd profile="phone" + :326 accel="tcg"——四入口分裂。
- :362/:366 ShellCmd profile="desktop"/accel="auto" + dispatch :889/:891 无参双击回退 desktop/auto——**已收敛**，来源 821d3f152（vnc 批，注记「2026-11-13 owner 裁定：双击/无参入口 = 横屏 1080P + 硬件加速」），非 ux-unify 移植。

### 5.3 摘取建议

摘 Run/Shot/Start/Host 四结构体 8 行 default_value 翻转（单文件、无跨文件依赖）：前置全在主树——profile "desktop" 在册（profile.rs:34 PROFILES 三件 default/desktop/phone）；choose_accel(pref, deterministic) 缝在席（qemu.rs:39）；shot 确定性已保（commands.rs:197 传 true）。注意 ShellCmd 与 dispatch 两 hunk 主树已吸收，cherry-pick 会空转/轻冲突，宜手工取四 hunk 或 `git cherry-pick -n` 后弃多余。**不携整分支**：其 launcher 四文件 + PRESS_MAP_MAX 16→28 + splash 重放（d3c3e4033）与主树 spring 重做/launcher 演进冲突面大；刀 2 状态栏面若要，走 wave3 dd9452a7d 的独立移植先例（其注记明示「tools/berry 入口面不携」）。

## 六、smoke 计数锁与改法

### 6.1 通配形态

- tools/smoke-image.sh:326 ECO_MARKERS 数组含 `"APP ok n="` 与 `"APPUI ok n="`——前缀通配（消费 = grep -qF 子串匹配），任意计数过；tools/markers.sh:24 同形；tools/smoke.sh:50 注释同口径。
- 计数真锁两处：①tools/gui8-smoke.py:82 `MARK_APP_N9 = "APP ok n=9"` 全串，boot 相等待锚列表 :1096/:1421/:1444/:1459 四处在用；②生产侧 svcmgr main.rs：`APP ok n=` 仅 ok==app_n 全过才打，n ≤ APP_SLOTS=9 由编译期 `APP_SLOTS<10` 断言钉单数字位（:2198 域），APP_SLOTS 自 berry-manifest 族 import（:60）。APPUI 同理单数字位（APP_SLOTS<10 const assert，smoke.sh:943-944 注）。

### 6.2 锁数改法（wave4 已示范的完整链条）

1. svcmgr 谓词语义先行：wave4 将 APP ok 两支化（非 [ui] 槽 = 拉起 ∧ 真码 0；[ui] 槽 = 入册 ∧ deferred 记账计入 ok）——n=9 仍打、语义重定义，避免动锁面。
2. smoke.sh 增逐槽断言（可归因）：wave4 `for id in APPDEMO SETTINGS FILES PHOTOS CAMERA CONTACTS MMS BROWSER; grep -q "app $id gui deferred"` 八行。
3. gui8-smoke.py 重裁：MARK_APP_DEMO_DEFERRED 全串 + `_require_exact_count` 1/0 双向（无点击 boot 恰 1、点击后 up 恰 1）；MARK_APP_N9 锚随槽增减同笔改。
4. 连带计数：PS ok n= 随 appdemo 不再 boot 拉起而 −1（前缀面自动吸收，精确值断言处需随动）；APPUI n= 随 [ui] 槽数变（wave4 后 8 槽不变）。
5. 若槽位数本身增减（超过 9 或减）：改 berry-manifest APP_SLOTS + svcmgr 编译期单数字断言 + gui8-smoke MARK_APP_N* + 主树 userbins 七槽 spec 清单；markers.sh/smoke-image.sh 前缀形态零改。

## 七、落地风险与随动清单（供刀 A 合批参考）

1. wave4 基线 769515a9a 落后主树 HEAD 3af18dda1（sched-1.1 合并 + spring 重做等）；其触面的基座文件（svcmgr main.rs +53、justfile、smoke 三脚本、berry-manifest fixture、appdemo 四件）与主树演进的对撞需合批时逐文件三勘。
2. wave4 装配段自带 smoke 随动（gui deferred 断言 + gui8 重裁 + PS −1）——取六 crate 必须同批取这四件随动，否则 boot 批拉起 park 形 = APP ok 永等全线 FAIL（§1.7 铁律）。
3. SETTINGS 接线（主树 settings crate 入包）是 wave4 留白，落它需：justfile APPTOMB 食谱段 + 裁决 B 已在（wave4 批带到）+ APPUI n 随动（9 槽 [ui] 计数不变则零改）。
4. wave3 全部分支资产（六 -app crate + settings-app + 其 justfile 段）建议整支弃置，不留 cherry-pick 欠账；其「build-user 绿」口径与「真 UI」措辞的落差记入对账底册。
