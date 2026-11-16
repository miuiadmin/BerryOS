# W4-VERIFY —— 波 4 先行批验收（七路 + 裁决 B 接线）

基线：worktree `wip/ui-wave4` HEAD 769515a9a。验收输入 = 七路实施报告（[W4-IMPL-FILES.md](W4-IMPL-FILES.md) / [W4-IMPL-CONTACTS.md](W4-IMPL-CONTACTS.md) / [W4-IMPL-PHOTOS.md](W4-IMPL-PHOTOS.md) / [W4-IMPL-CAMERA.md](W4-IMPL-CAMERA.md) / [W4-IMPL-MMS.md](W4-IMPL-MMS.md) / [W4-IMPL-BROWSER.md](W4-IMPL-BROWSER.md) / [W4-IMPL-APPDEMO.md](W4-IMPL-APPDEMO.md)）+ 共同侦察 [W4-RECON.md](W4-RECON.md)。环境备注：验收全程兄弟 lane 六至八台 QEMU 满载（CPU 全核 100%、free 页 ≈8.7k×16KiB），TCG 时序与宿主内存压力显著劣化——判读与取证均按 e58c94ccf 环境碰撞先例如实标注（§六）。

## 〇、验收结论速览

1. **七 crate 实施面全收**：足迹恰七 crate（六新 + appdemo 翻新）、零手拼 grep 门逐项过、musl check（含 --tests）七件零 error 零 warning、文案全 ASCII；宿主 user 工作区测试全绿（一处 berry-manifest fixture 钉板随 appdemo 翻新重钉——波 4 批唯一真红，见 §二.4）。
2. **裁决 B 落地**（boot 批 [ui] 入册不拉起 + `app <id> gui deferred` 记账行）：svcmgr `app_launch` 增 `defer_gui` 形参（boot 批 true / SPAWN RPC false），置位于全部装载守卫与 [ui] 资产验证之后、spawn 面之前——W4-RECON §1.7 铁律（常驻 park 形由 boot 批拉起 = app_reap_tail 永等）的接线序前置件。
3. **镜像装配落地**：justfile build-user 六 crate 接线（APPTOMB 食谱，资产 crate 内单源/contacts 留 appstub 兜底），SETTINGS 唯一残留 stub 槽（crate 在册不在七路报告面，接线归后续批）。
4. **boot 验证**：`tools/smoke.sh` **全量 PASS**（boot + rootfs 相 54/54 + RX 交互 + FAT 子目录 + FAT 持久化 + B4 掉电 + A8 毒环——含本批 8× gui deferred 逐槽断言与 PS 新值两处随动面）；rootfs 形聚焦 boot 逐字取证；GUI 形 app 批两 boot 同绿；逐应用点击 spawn 取证被预存 launcher 抖动阻断，A/B 判别实验（旧镜像同环境同楔死）定谳非波 4 面（§六如实分账）。
5. **包体预算**：八改造包 4,161,824 B = 3.97 MiB（稿估 6.70 MiB 表和之下、单包最大 0.53 MiB < 0.9 查因门、BAP_MAX 4 MiB 远未触）；rootfs 38,555,648 B = 36.8 MiB（rootfs0 恰 32.0 MiB 帽 + rootfs1 4.8 MiB 分卷消化；总量帽 128 MiB 余 ≈91 MiB）。
6. smoke 断言诚实随动（旧值→新值全记录，§五）+ gui8-smoke 两处重裁（RECON §1.7 预告面）同批落。

## 一、步骤 ①：diff 复核（七 crate 互不相交、基座零触碰、零手拼 grep）

### 1.1 足迹（git status 全量）

- 追踪文件修改仅四件 = `user/bins/appdemo/{Cargo.toml,Cargo.lock,manifest.toml,src/main.rs}`（appdemo 翻新批自报面）+ `src/gallery.rs` 新增；六 crate（files/contacts/photos/camera/messages/browser）全为 untracked 新目录。
- **launcher/guifbd/gui-wmd/svcmgr/justfile/berry-* 基座：七路提交时零触碰**（各报告 `git status` 佐证一致）；验收批随后动的面 = svcmgr 裁决 B + justfile 接线 + smoke 三脚本 + berry-manifest fixture 重钉（§二、§五——均为验收批职权）。
- appstub/apps/ 旧资产未动（contacts 批资产供给仍走该源）。

### 1.2 零手拼 grep 门（验收侧独立复跑，全过）

| crate | ViewKind::Card 直载 | ViewKind::Rect | 按压字面量门（scrim/PRESSED_BG/0xCCE3/0x404040） | Color 构造 | 报告口径 |
|---|---|---|---|---|---|
| files | 0（全走 pattern 配方） | 1（Stack 面层叠层习语） | 代码面零命中 | 1（state_color 解包单源） | §五符 |
| contacts | 0 | 1（详情页底） | 零命中 | 2（ON_ACCENT 镜像注记 + state_color） | 恰 1/恰 2 ✓ |
| photos | 1（网格格直载，corner_radius=LISTITEM_RADIUS 常量单源） | 2 | 零命中（照片批含注释引述 1 处非消费） | 0 直构造 | 恰 1 ✓ |
| camera | 0（零卡片零列表） | 2（快门外扩框/标题底） | 零命中（0.95/950 仅存注释） | 1（白闪 Rect——设计稿 §2.5 明载特效） | ✓ |
| messages | 0（list_item/card 配方） | 2（双页底叠层） | 零命中 | 2（同 contacts 形） | 恰 2 ✓ |
| browser | 0（card 配方） | 3（双页底 + 卡按压面层） | 零命中（=950 仅头注引述） | 1（state_color） | 恰 3/恰 1 ✓ |
| appdemo | 0（画廊全配方） | 1（Rect kind 展格本体） | 零命中 | 1（ON_ACCENT 镜像） | ✓ |

文案 ASCII：七 crate 全部 `b"..."` 字面量经 LC_ALL=C 扫描——唯一非可印字节 = 路径 `\0` 尾与 `\n`（C 串/换行合法形态），非 ASCII 文本零命中。appdemo 源含中文注释（注释域合法，字面量域全 ASCII——报告口径即此）。

## 二、步骤 ②：编译与测试门（全绿，一处真红已修）

1. **七 crate musl check**（`--release --tests --target aarch64-unknown-linux-musl`，berry-ld RUSTFLAGS，fresh target dir 复核）——七件全部 Finished **零 error 零 warning**。
2. **宿主 user 工作区测试**（`cargo test --manifest-path user/Cargo.toml --target aarch64-apple-darwin -p berry-ui -p berry-manifest -p berry-appproto -p berry-layout -p berry-exec -p berry-fsproto`）——全部 ok（含 berry-ui pattern 61 测 + gallery2 口径件）。
3. svcmgr 裁决 B 后 check：既有 3 警（target-feature 弃用族 + dpg 旧 unused_assignments）**零新增**。
4. **真红一枚（红则修兑现）**：`berry-manifest` `fixture_appdemo` 钉板红——appdemo manifest `[runtime] stack_pages = 64` 与 `label = "Gallery"` 两键翻新后 fixture 共识钉未随更（断言 64 vs 8 / Gallery vs APPDEMO）。按该测试自身重钉先例（ba288f7「按现 fixture 真值重钉」同款）随批重钉 + 注记；重跑 46 测全绿。
5. gui8-smoke plumbing 自检（`--self-test`，零 QEMU）：精确哨兵/canonical 109B/计数与顺序（含新负样本：boot 批拉起 APPDEMO = 裁决 B 破坏面）/常态隔离/QMP——PASS。

## 三、步骤 ③⑥：裁决 B + 镜像装配

### 3.1 svcmgr 裁决 B（boot 批 ui deferred）

- `app_launch` 增形参 `defer_gui: bool`（boot 批调用点 true / `app_serve_spawn` SPAWN RPC 调用点 false）；裁决点置 `ui_face` 之后——[ui] 资产验证（APPUI 谓词数据源）、type/id/entry/compat/net 守卫、调用侧 P3 验签**全部照走**，仅 spawn 面让渡 launcher。
- 新诊断行 `app <id> gui deferred`（`app_gui_deferred`——小写 `app ` 前缀与 appdemo 自证行同族；与 `app APPDEMO up` 互非子串、无 " ok" 形不与标记面相撞）。
- `APP ok n=` 谓词两支化：非 [ui] 槽 = 拉起 ∧ 退出真码 0（原形）；[ui] 槽 = 入册 + deferred 记账（计入 ok——n=9 仍打，语义 = 拉起子集 ∧ deferred 子集全过，W4-RECON §1.7「仍打 9」）。

### 3.2 justfile build-user 接线

- GUI-8 七应用 appstub 循环退役 → 六 crate 段（`for c in files photos camera contacts messages browser` musl + berry-ld 绝对路径构建；staging = crate manifest + ELF + icons/views，crate 缺资产时兜底 appstub 源——contacts 批口径）。
- SETTINGS 残留 stub 段（crate 9035af061 在册不在七路面，接线归后续批——裁决 B 已清其入镜像前置）。
- `tools/rootfs-manifest.toml` 零改（条目名不变）；未走 sign-user（.sig 缺席零漂移 = 既有默认链）。

### 3.3 产物实测（mkapp 五道门禁全过）

```
FILES 499,712B  PHOTOS 507,904B  CAMERA 499,712B  CONTACTS 516,096B
MMS 528,384B    BROWSER 552,960B APPDEMO 557,056B SETTINGS 61,440B(stub)
APPQUIET 348,160B  APPTOMB 352,256B（两不动件）
rootfs.cpio 38,555,648B x62 条目 → rootfs0 33,554,432B（恰 32.0 MiB 帽）
                                  + rootfs1 5,001,216B（4.8 MiB）
tlsscan x60 ELF 全过
```

九包 `mkapp --check` 全 OK（digests=ok，.byi 全量重验——GUI 形 BadMagic 判读的离面对拍面，§六）。

## 四、步骤 ④：boot 验证

### 4.1 rootfs 形聚焦 boot（QEMU tcg，kernel+fw_cfg 双卷+virtio 盘）——全绿

串口断言（/tmp/w4v/rootfs-boot.log）：

- `APP ok n=9` ✓（谓词两支化后 8 deferred + APPQUIET 拉起退 0）
- `APPUI ok n=8` ✓
- 8× `app <id> gui deferred`（APPDEMO/SETTINGS/FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER 逐槽在席）✓
- `eco: app` 零行 ✓（零降级——六 crate 包在 GUI 授权链前不炸不降）
- `app APPDEMO up` 零行 ✓（boot 批不再拉起 console app——裁决 B 语义面）
- `PS ok n=12` ✓（新值——见 §五.3 旧值新值账）

### 4.2 全量 smoke（`tools/smoke.sh`）——PASS

boot 相 + rootfs 相 54/54（含 `APP ok n=`/`APPUI ok n=`/`PS ok n=` 与本批 8× gui deferred 逐槽断言——任一缺席即 FAIL）+ RX 交互相 + FAT 子目录相 + FAT 持久化相 + B4 掉电相 + A8 电源毒环——**全链 PASS**（smoke 输出「smoke: PASS（金丝雀 402 条诊断 + rootfs 相全链 + RX 交互相 + …）」）。

### 4.3 GUI 形（AAVMF + out/berryos.img + FAT12 数据盘 + tablet + QMP）

- **app 批串口证据与 rootfs 形逐字一致**（三 boot 取样：APP ok n=9 / APPUI ok n=8 / 8× gui deferred）——GUI 形裁决 B 面绿。
- 桌面链三 boot 均于 icons 解码期打出 `launcher: icon decode BadMagic` 后未达 `LAUNCH ok`——**A/B 判别实验定谳为预存环境面非波 4 面**（§六.2：主树 pre-波4 镜像同环境同楔死 + 图标字节新旧逐包相同 + 三重离面对拍）。
- 逐应用点击 spawn（APPDEMO 串口三行）+ 截图取证因此未获桌面窗口——按 e58c94ccf「rootfs 相为代码面证据 + 环境碰撞如实分账」先例记录，补跑归合并前环境窗口（§六.3）。

## 五、smoke 断言诚实随动（旧值 → 新值全账）

| 断言面 | 旧 | 新 | 所在 |
|---|---|---|---|
| `APP ok n=` | 9（全槽拉起+reap 0） | 9（8 deferred + APPQUIET——语义两支化，值不动） | markers.sh 前缀断言零改 |
| `APPUI ok n=` | 8 | 8（deferral 在 ui_face 后，谓词零漂） | 同上 |
| rootfs 相 appdemo 证据行 | `app APPDEMO up`（boot 批拉起自证） | 8× `app <id> gui deferred` 逐槽断言（console=true 串口能力面证据随迁 GUI 形点击 spawn——gui8-smoke 承载） | tools/smoke.sh、tools/smoke-smp.sh |
| `PS ok n=` | 13（ps 渲染窗行数，含 APPDEMO exited(0) 账本行） | 12（boot 批不拉起 console app → 账本无行；家名单 12 断言面零扰动） | 前缀断言零改；值随动如实记录 |
| gui8-smoke Boot A boot 锚 | 基础 5 + `app APPDEMO up`（boot 批证据吃完再点） | 基础 5 + `app APPDEMO gui deferred`（桌面就绪锚 = LAUNCH/PAGE 本体——RECON §1.7 重裁一） | tools/gui8-smoke.py |
| gui8-smoke 点击计数 | 新增 `app APPDEMO up` == 1（boot 批 1 行为基线） | 全日志恰 1 行 + 点击前零行强断言 + deferral 行恰 1（重裁二） | 同上（Boot B/常态隔离同步增 deferral 恰 1 / up 零行） |
| bcts 冻结卷 | 前缀形 `APP ok n=`/`APPUI ok n=`/`PS ok n=` | 零漂（前缀断言不钉值） | tools/bcts/frozen/* 不动 |

## 六、环境碰撞分账（e58c94ccf 先例）与 GUI 形残余

1. **环境现场**：验收窗口内兄弟 lane 六至八台 QEMU 常驻（ns-journey 双台 + berry-live + berry-adv + ux-berry shot（--timeout 2400）+ hermes CI 监视），CPU 全核 100%、宿主 free 页一度降至 ≈8.7k×16KiB（18 GiB 机）——TCG 时序劣化一个量级，期间一台验收 QEMU 被宿主压力外杀（rc 非 0、零 stderr）。后段 lane 退潮（8→4 台）后全量 smoke.sh 即全链 PASS。
2. **GUI 桌面链 BadMagic 的 A/B 定谳**：验收三 boot + **主树 pre-波4 镜像（10:28 产物）对照 boot** 四样本同环境同楔死（`launcher: icon decode BadMagic` 后 `LAUNCH ok` 缺席；旧镜像的报错词被并发串口字节覆写但 `launcher: icon decode` 前缀在场，app 批面旧镜像照常 `APP ok n=9` + `app APPDEMO up`——对照组行为完全符合其旧形态，排除「环境普遍杀 boot」解释）。三重离面对拍排除包缺陷：①九包 `mkapp --check` .byi 全量重验 OK；②boot 期 `APPUI ok n=8` = svcmgr 侧对同九包 icon 面 BYI1 头校验逐包通过；③新旧包 `icons/*.byi` 字节逐包相同（sha256 对拍八件全 SAME——波 4 图标载荷零变化）。归因 = e58c94ccf 在册未收口的「首帧零字节」抖动面（该刀取证：app 通道字节完美、读侧同偏移一真一零，另刀挂账）在当前宿主压力形态下确定性复现——非波 4 面。
3. **残余与补跑**：逐应用点击 spawn 标记（APPDEMO 三行串口）+ berry shot 截图未获（桌面链楔死在前）。代码面证据 = 裁决 B 两形 boot 绿 + 全量 smoke 绿 + SPAWN RPC 路径行为零改动（`defer_gui=false` 原路）+ gui8-smoke plumbing 自检（点击计数/顺序/裁决 B 破坏负样本全过）；补跑归合并前环境窗口（`just gui8-smoke` 即承载）。
4. **判读纪律**：如 BadMagic 复现于干净机，按 e58c94ccf 挂账归 launcher icons 通道另刀，非波 4 面。

## 七、步骤 ⑤：包体预算表（实测 vs 帽）

| 包 | 旧 B | 新 B | Δ B | 稿估 .bap | 判 |
|---|---|---|---|---|---|
| FILES | 49,152 | 499,712 | +450,560 | 0.85 MiB | 实 0.48 MiB ✓ |
| CONTACTS | 61,440 | 516,096 | +454,656 | 0.80 MiB | 实 0.49 MiB ✓ |
| PHOTOS | 57,344 | 507,904 | +450,560 | 0.80 MiB | 实 0.48 MiB ✓ |
| CAMERA | 57,344 | 499,712 | +442,368 | 0.75 MiB | 实 0.48 MiB ✓ |
| MMS | 77,824 | 528,384 | +450,560 | 0.85 MiB | 实 0.50 MiB ✓ |
| BROWSER | 94,208 | 552,960 | +458,752 | 0.90 MiB | 实 0.53 MiB ✓ |
| APPDEMO | 385,024 | 557,056 | +172,032 | 0.90 MiB | 实 0.53 MiB ✓ |
| SETTINGS（stub 残留） | 61,440 | 61,440 | 0 | — | 不在本批 |
| **八改造包和** | 843,776 | **4,161,824（3.97 MiB）** | +3,318,048 | 6.70 MiB（表和） | 实测低于稿估 2.7 MiB ✓ |
| rootfs.cpio | 35,676,160（34.0 MiB） | 38,555,648（36.8 MiB） | +2,879,488 | ≈40 MiB | ✓ |
| 32 MiB 单卷帽 | rootfs0 32.0 + rootfs1 2.0 | rootfs0 33,554,432（恰 32.0）+ rootfs1 5,001,216（4.8） | — | 分卷消化 | ✓ |
| 128 MiB 总量帽 | 用 34.0 | 用 36.8 → 余 ≈91 MiB | — | — | 宽裕 ✓ |

单包查因门（>0.9 MiB）零触发；BAP_MAX 4 MiB 单包帽最接近者 BROWSER 0.53 MiB。

## 八、改动清单（验收批本体）与提交

- `user/bins/svcmgr/src/main.rs`——裁决 B（app_gui_deferred + defer_gui 形参 + boot 批谓词两支化注记）。
- `justfile`——六 crate 接线段 + SETTINGS 残留 stub 段 + appstub 构建注记刷新。
- `tools/smoke.sh` / `tools/smoke-smp.sh`——appdemo 证据行随动（8× deferred 逐槽断言）。
- `tools/gui8-smoke.py`——两处重裁（boot 锚 + 恰 1 行）+ deferral 恰 1 断言（Boot A/B/常态隔离三面）+ 自检夹具随裁。
- `user/berry-manifest/src/lib.rs`——fixture_appdemo 两钉重钉（stack 64 / label Gallery）。
- 本报告 `W4-VERIFY.md`。

提交两笔（显式路径）：①`feat(apps/波4-先行): 六应用真 UI——pattern 零手拼+v1 降解清单`（六 crate + 六报告 + RECON）；②`feat(apps/波4-先行): appdemo 控件画廊+镜像装配与 smoke 计数随动`（appdemo 五件 + 其报告 + svcmgr/justfile/smoke 三脚本/berry-manifest 重钉 + 本报告）。
