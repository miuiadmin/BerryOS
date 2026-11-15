# A-VERIFY —— 刀 A 验收（镜像帽对账 / boot 证据 / 计数锁数 / 入口收敛 / 零手拼）

基线：worktree `wip/land-a` HEAD 6d317fca1（刀 A 全部八笔在席：两件 wave4 cherry-pick + SETTINGS 接线 + 计数锁数 + 入口收敛 + oneshot 恢复三笔 + 笔记）。验收输入 = [A-RECON.md](A-RECON.md)（对账裁决）+ [A-IMPL.md](A-IMPL.md)（实施落账）。环境备注：验收窗口内兄弟 lane 三至四台 QEMU 常驻满载（main 树 / /tmp/berry-adv / gui-1080p / cbox 各一，CPU 各 ~100%），TCG 时序劣化与 W4-VERIFY §六.1 记载的碰撞形态同类——判读与取证按 e58c94ccf 环境碰撞先例如实标注（§四）。

## 〇、验收结论速览

1. **①镜像构建 + rootfs 帽对账：全绿未超帽**。`just build-user` 复跑 exit 0（十件 .bap `mkapp --check` digests 全 OK）；rootfs.cpio 39,014,400B（37.21MiB）→ 分卷 rootfs0 恰 33,554,432B（= 32MiB 单卷帽整，≤ 帽合法）+ rootfs1 5,459,968B；Σ = 39,014,400B ≤ 128MiB 总量帽（VMO_MAX_SIZE 口径），余 90.8MiB。A-RECON §4.3 口径（单卷帽 ≠ 整体帽）复核成立——**未超帽，裁决（裁 appdemo 画廊页或瘦身）不触发**，零改动。
2. **②boot 验证：刀 A 判据面两形全绿；桌面链被预存环境面阻断（先例代位如实标注）**。GUI 形（berry device start，desktop profile 1080×1920 + FAT12 数据盘 + tablet）：`LAUNCH v=1 wmax=15 1080x1920 input=65545`、`WM ok`、`ECO ok`、`APP ok n=9`、`APPUI ok n=8` 全绿后，桌面链于 icons 解码期打 `launcher: icon decode BadMagic` 恰一行、`LAUNCH ok` 300s+ 缺席——与 W4-VERIFY §六.2 A/B 判别定谳的预存环境面（e58c94ccf「首帧零字节」挂账）逐字同签名；本验收加测 **ramfb 截图像素分析 = 1080×1920 共 2,073,600 像素全零、唯一色数 1**（「首帧零字节」的字面复核）。逐应用点开 + berry shot 截图七连因此不可得（应用 spawn 唯一入口 = 桌面点击；berryctl 仅 ver/ps/mem、bsh 无 app 内建、gui8-smoke 点击链同楔）——按任务预案「场被占则环境证据先例代位」，代位证据 = rootfs 形逐槽标记采集（§三.2，八槽 deferred 逐槽在席）+ W4-VERIFY §六.2 三重离面对拍（mkapp --check / APPUI ok n=8 同载荷验签 / 图标 sha256 新旧 SAME）+ gui8-smoke `--self-test` 点击链断言全过（§三.4）。
3. **③smoke-image：新锁数门过；整链红在预存环境面（两度同缺席清单复现 + 产侧零改动 A/B 定谳）**。两跑均 FAIL——360s 标记未到齐，缺席清单**逐字相同**：自 `U3 ok` 起（bsh 交互族 + 演示族 + CLI/TERM + LAUNCH/PAGE）；**`APP ok n=9`/`APPUI ok n=8`/`WM ok` 不在缺席清单**（两跑均于串口日志清理前直读复核三者在席 = 锁串 EFI 路过验）。内核全程存活（switches 金丝雀持续至强杀）。**A/B 定谳（本验收新增，代码面）**：`git diff --name-only 3af18dda1..HEAD` 对停段族产侧（bsh/bench1/stdhello/hexyl/fsmgr/mkfat/kernel）**零触碰**——停段不可能由刀 A 代码引入；sched 仅增 oneshot 恢复件（base 2900c356c 已声明并消费、缺文件即构建断，恢复 = 还原作者本意，非新行为）。
4. **④入口收敛断言（读代码）**：六面全一致 `desktop`/`auto`——RunCmd.accel :96、ShotCmd :116/:118、StartCmd :284/:286、HostCmd :324/:326、ShellCmd :362/:366、dispatch() 无参回退 :889/:891。前置复核：profile "desktop" 在册（profile.rs:34，1080×1920@60）；`choose_accel(pref, deterministic)` deterministic=true 恒 TCG（qemu.rs:39-43），shot 路传 true——截图确定性不动。
5. **⑤零手拼 grep 复查保持：七面全中**（W4-VERIFY §1.2 口径逐格复跑，§二.2 表）；按压字面量门代码面零命中（photos/camera 共 7 处全为注释引述）。
6. **⑥红则修：代码面零红**。独立复跑全绿：八 crate musl check（`--release --tests`，`.target-a` 隔离）零 error 零 warning；berry-manifest 46/46 + berry-ui 61 pattern 全套件（5/11/7/15 邻套件同绿）+ oneshot 律测 7/7；gui8-smoke `--self-test` PASS（含裁决 B 破坏负样本）；berry CLI berry-build exit 0；smoke.sh/smoke-smp.sh 逐槽 deferred 断言 + markers.sh/smoke-image.sh 锁串在席复核 + 两标记数组互非子串机查零碰撞。全量 `just smoke` 两跑均红在宿主面非代码面（run1 = cbox lane echod 独占 18080，boot 相 32/32 绿后死于互斥端口；run2 = rootfs 相 600s 停段于 FAT 族，停段后标记仍在缓慢到席 + 串口 bench 字母与标记行字节交错——时序劣化面特征，§四.3）——A-IMPL §七同代码全量 PASS（20:5x 窗口）在册，补跑归环境窗口。
7. **提交**：刀 A 代码三题分笔已在席（应用合流 4d01add63+993aee2b4 / 镜像接线与计数 e7bae2f5a+89fb060ba / 入口收敛 28b34c919，均显式路径）；本验收批补落过程文档三件（A-RECON/A-IMPL/A-VERIFY，§七）。

## 一、步骤 ①：镜像构建与 rootfs 帽对账

### 1.1 构建复跑（验收侧独立）

- `just build-user`（worktree 内 `user/cargo-target` 隔离）——exit 0；产链全绿：十件 .bap → mkcpio `../rootfs.cpio 39014400B x62 条目` → 分卷 `rootfs0 33,554,432B + rootfs1 5,459,968B（重装 39014400B）` → tlsscan x60 ELF 全过（F1/F2/F3）。与 A-IMPL §三实测逐字节同值（可重现构建）。
- 十件包 `mkapp --check`：全部 OK、digests=ok（APPDEMO 557,056B / BROWSER 552,960B / MMS 532,480B / CONTACTS 516,096B / PHOTOS 512,000B / CAMERA 503,808B / FILES 499,712B / SETTINGS 495,616B / APPQUIET 348,160B / APPTOMB 352,256B）——八 GUI 槽全 5 条目（manifest+bin+SHA256SUMS+icons+views），SETTINGS.bap 495,616B 内含真 settings musl ELF（对 A-RECON §二「镜像内 61,440B 秒退壳」的置换完成）。

### 1.2 帽对账（A-RECON §4.3 口径）

| 项 | 实测 | 帽 | 判 |
|---|---|---|---|
| rootfs.cpio 总量 | 39,014,400B = 37.21MiB | Σ ≤ 128MiB（VMO_MAX_SIZE，A1-2 帽分家） | ✓ 余 90.8MiB |
| 单卷 rootfs0.cpio | 33,554,432B = 32.000MiB | 单卷 ≤ 32MiB（fw_cfg 卷帽 = kernel `ROOTFS_MAX`，A1-3b） | ✓ 恰帽顶不分（≤ 含等号） |
| 单卷 rootfs1.cpio | 5,459,968B = 5.21MiB | 同上 | ✓ |
| 分卷重装和 | 39,014,400B == 总量 | 零字节漂移 | ✓ |

- **超帽裁决不触发**：总量余量 90.8MiB、单卷链 `--split 32` 自动消化（消费端 `opt/berry/rootfs0..9` 连续前缀发现序 + VMO 逐页拼装零改动）——无需裁 appdemo 画廊页、无需瘦身。波 4 → 刀 A 增量（rootfs 38,555,648B → 39,014,400B，+448,752B ≈ SETTINGS 真壳置换 stub 的净增）仍在帽内宽裕。
- 口径事实单源引用：帽定义原句在 A-RECON §4.3（justfile:352-354/:540-542、kernel/src/main.rs:94/:158）；本表只作验收侧实测对拍，不双写口径。

## 二、步骤 ④⑤：入口收敛断言与零手拼复查

### 2.1 四入口收敛（读代码断言，2026-11-15 现场）

| 入口 | 位置 | profile | accel |
|---|---|---|---|
| Run | main.rs:96 | ——（kernel-boot 面，无 profile） | auto |
| Shot | main.rs:116/:118 | desktop | auto |
| Start | main.rs:284/:286 | desktop | auto |
| Host | main.rs:324/:326 | desktop | auto |
| Shell（既收敛对照） | main.rs:362/:366 | desktop | auto |
| dispatch 无参回退（既收敛对照） | main.rs:889/:891 | desktop | auto |

四翻转面与既收敛两面（821d3f152）全一致——ux-unify 6e676da3e 摘取完成、无残留分裂。前置：profile.rs:34 `id: "desktop"`（1080×1920@60，256MiB）在册；qemu.rs:39 `choose_accel(pref, deterministic)`——`deterministic=true` 首行直返 TCG；commands.rs `shot()` 调用点传 `true` = shot 确定性恒 TCG 不受默认翻转影响。

### 2.2 零手拼 grep 复查（验收侧独立复跑，七面全中）

| crate | ViewKind::Card 直载 | ViewKind::Rect | 按压字面量（代码面） | Color 字面量构造 | W4-VERIFY §1.2 口径 |
|---|---|---|---|---|---|
| files | 0 | 1 | 0 | 1（state_color 解包） | = |
| contacts | 0 | 1 | 0 | 2（ON_ACCENT + state_color） | = |
| photos | 1（网格格直载） | 2 | 0（3 处全注释） | 0 | = |
| camera | 0 | 2 | 0（4 处全注释） | 1（快门白闪） | = |
| messages | 0 | 2 | 0 | 2（同 contacts 形） | = |
| browser | 0 | 3 | 0 | 1（state_color） | = |
| appdemo | 0 | 1 | 0 | 1（ON_ACCENT 镜像） | = |

计数法：`Color { r:` 字面量形（排除 `-> Color {` 签名行）；按压门 grep `PRESSED_BG|0xCCE3|0x404040|scrim` 后逐条核对注释域。settings crate（主树件，非 wave4 门面）同法加测 = 2（TEXT_FG/VALUE_FG 字色常量，同惯用族）——不在 W4 门内，如实附记。

## 三、步骤 ②③：boot 验证与 smoke 计数门

### 3.1 GUI 形桌面链（berry device start，1080×1920 desktop profile）

串口证据（run 1794748417-833183000-24053-host，TCG）：

- 到席：`LAUNCH v=1 wmax=15 1080x1920 input=65545`、`WM ok`、`ECO ok`、`APP ok n=9`、`APPUI ok n=8`——app 批（含八槽 deferred 记账）在 GUI 形全绿。
- 楔死：`launcher: icon decode BadMagic` 恰一行后 `LAUNCH ok` 300s+ 缺席；内核存活（bench/switches 金丝雀持续）。
- **ramfb 加测（本验收新增取证）**：`berry screenshot` 楔死态截图像素分析 = 1080×1920 共 2,073,600 像素**全 `(0,0,0)`、唯一色数 = 1**——e58c94ccf「首帧零字节」挂账的字面复核（显示面从未收到任何帧）。
- 判读：与 W4-VERIFY §六.2 A/B 判别（pre-波4 主树镜像同环境同楔死 + 图标 sha256 新旧逐包 SAME + 三重离面对拍）定谳一致 = 预存宿主环境面非刀 A 面。

### 3.2 rootfs 形聚焦 boot（逐槽标记采集——② 的代位证据主件）

复刻 smoke.sh 阶段 3 拓扑（kernel + fw_cfg rootfs0/1 双卷 + virtio-blk + tablet，无 echod/net 面），串口断言全过（50s 到齐）：

| 断言 | 实测 | 判 |
|---|---|---|
| `APP ok n=9` | 1 | ✓ |
| `APPUI ok n=8` | 1 | ✓ |
| 8× `app <ID> gui deferred`（APPDEMO/SETTINGS/FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER） | 每槽恰 1 | ✓ 逐槽在席 |
| `app APPDEMO up`（boot 批拉起破坏面） | 0 | ✓ 裁决 B 语义 |
| `PS ok n=12` | 1 | ✓ |
| `ECO ok` / `PIPE ok n=6419` | 1 / 1 | ✓ |
| `eco: app` 降级族 | 恰 1 行 = `eco: app APPQUIET it 04`（intent 透传观测行，A-IMPL §八.6 口径——非降级） | ✓ |
| 其余 eco 行 | `fb absent`（-nographic 无 ramfb，拓扑性）/ `dns rpc`+`tcps reply`（宿主无 echod，拓扑性）/ `berry` ×2（power/thermal up 观测行） | ✓ 全部拓扑可归因 |

### 3.3 smoke-image 两跑（新锁数门过，整链红在预存环境面）

| 跑 | 场态 | 缺席清单 | APP/APPUI/WM 锁串 |
|---|---|---|---|
| run1（21:05） | 四 lane 满载 | `U3 B1 U4 TAB INT POLL DPGS DPGN MMP CLK SIG CLI TERM LAUNCH PAGE` | 三者到席（直读复核） |
| run2（21:39） | 三 lane、load ~5.3 | **与 run1 逐字相同** | 三者到席（直读复核） |

- 两跑停段清单逐字相同 + 内核金丝雀全程存活 + A-IMPL 跑缺席自 B1 起（停段位点随负载漂移、清单族恒定）= 时序劣化面特征。
- 产侧 A/B（代码面定谳，本验收新增）：base 3af18dda1..HEAD 对 bsh/bench1/stdhello/hexyl/fsmgr/mkfat/kernel 零 diff——停段族产侧字节不变，红不可能由刀 A 引入；与 W4-VERIFY §六.2 pre-波4 镜像 A/B（同停段族）互证。
- **新锁数门（任务 ③ 的判据面）两跑全过**；整链补跑归合并前环境窗口（W4-VERIFY §六.3 先例）。

### 3.4 计数锁数断言面复核（零 QEMU）

- `tools/smoke-image.sh` ECO_MARKERS（41 条）与 `tools/markers.sh` ROOTFS_MARKERS（56 条）锁串 `APP ok n=9`/`APPUI ok n=8` 在席；两数组互非子串机查零碰撞（验收侧独立脚本复跑）。
- `tools/smoke.sh:1024` + `tools/smoke-smp.sh:441` 逐槽 `app <id> gui deferred` 断言在席。
- `python3 tools/gui8-smoke.py --self-test` PASS——Boot A 精确断言（1/1/0 + deferral×1 + APPDEMO up 恰 1；重复/错序/缺 ready/boot 批拉起/APPQUIET 负样本全 FAIL 面）/ Boot B（LAYOUT×1、写路径一次性门）/ 常态隔离 / QMP 注入几何，零 QEMU 全过。

### 3.5 全量 smoke 两跑（红在宿主面，代码面证据在册）

- run1：boot 相 32/32 到齐后死于 echod 启动——`127.0.0.1:18080` 被 cbox lane `tools/echod.py` 独占（smoke.sh:337 起自托管互斥面；kill 兄弟进程违并行会话纪律，等待让位）。
- run2（端口让位后）：rootfs 相 600s 停段于 FAT 族（缺 FAT/FATW/DIR/CD/B1/DF/U4/TAB/INT/POLL/EVS/SKT/EPL/NETS/MMP/CLK/SIG/M3DEMO/DPGS/DPGN/CLI/TERM）——但停段后标记仍在缓慢到席（deadline 后日志尾见 DPGS/DPGN/FS ok 接连到达）且串口 bench 字母与标记行**字节交错**（`app … gui deferred` 行被 bench 任务并发写拆字 = grep 不可命中形）；**app 批面在该 boot 同样全绿**（APP ok n=9/APPUI n=8/VBLK/BLKD 在席）。时序劣化面特征与 §3.3 同族。
- A-IMPL §七同代码全量 PASS 在册（20:5x 窗口，boot 相 32 + rootfs 相 56 + RX + FAT 双相 + B4 全链）；本验收独立复核的分相判据面（§3.2 rootfs 形 app 批 + §3.4 锁数/断言/自检）全绿。

## 四、环境碰撞分账（e58c94ccf 先例沿用）

1. **场态**：验收窗口兄弟 lane 三至四台 QEMU 常驻（main 树 18:07 起跑 / berry-adv / gui-1080p（gui8-smoke --vgpu --only-a --boot-a-timeout 900，21:2x 自灭）/ cbox），各 ~100% CPU；本验收自跑 QEMU 期间 load 峰 17。W4-VERIFY §六.1 同型碰撞的余波。
2. **停段族归因链**：①产侧零 diff（§3.3 A/B——bsh/bench1/fserv 族字节不变）；②W4-VERIFY §六.2 pre-波4 镜像同环境 A/B 同停段族；③停段位点逐跑漂移（B1↔U1↔U3↔FAT）而族恒定 + 内核金丝雀存活 + deadline 后标记仍到席 + 串口字节交错——四征齐指时序劣化，非载荷缺陷。
3. **全量 smoke run1 的端口互斥**：cbox lane echod 独占 18080（自托管互斥设计内行为），非代码红；让位后 run2 即过 echod 门（停段归 §四.2 面）。
4. **判读纪律**：BadMagic/首帧零字节/停段族若复现于干净机，按 e58c94ccf 挂账归 launcher icons 通道/bsh 交互时序另刀，非刀 A 面；smoke-image/gui8-smoke/全量 smoke 补跑归合并前环境窗口（W4-VERIFY §六.3 先例）。

## 五、改动清单（验收批本体）

- 零代码改动（红则修条款：代码面零红，全部独立复跑绿；两脚本红面均定谳预存环境面 §四）。
- 本报告 `A-VERIFY.md`；同批补落 [A-RECON.md](A-RECON.md) / [A-IMPL.md](A-IMPL.md)（实施期过程文档，先前未入库——W4 批 RECON/IMPL/VERIFY 随代码入库的同款收尾）。

## 六、截图七连索引（如实代位）

| # | 应用（槽） | 计划取证 | 实得 | 代位证据 |
|---|---|---|---|---|
| 1 | SETTINGS | berry shot 桌面点开一帧 | 不可得（桌面链楔死在前，§三.1） | rootfs 形 `app SETTINGS gui deferred` 恰 1（§3.2）+ SETTINGS.bap 495,616B 真壳入镜像（§1.1）+ mkapp digests OK |
| 2 | FILES | 同上 | 不可得 | `app FILES gui deferred` 恰 1 + 同上 |
| 3 | PHOTOS | 同上 | 不可得 | `app PHOTOS gui deferred` 恰 1 + 同上 |
| 4 | CAMERA | 同上 | 不可得 | `app CAMERA gui deferred` 恰 1 + 同上 |
| 5 | CONTACTS | 同上 | 不可得 | `app CONTACTS gui deferred` 恰 1 + 同上 |
| 6 | MMS | 同上 | 不可得 | `app MMS gui deferred` 恰 1 + 同上 |
| 7 | BROWSER | 同上 | 不可得 | `app BROWSER gui deferred` 恰 1 + 同上 |
| 附 | 桌面（楔死态诊断帧） | —— | 全零单色帧（像素分析入 §3.1；诊断中间件不入库） | §3.1 |

七连取证被阻断的机制链：应用 spawn 唯一入口 = launcher 桌面点击（APP_SPAWN RPC）→ 桌面首帧前楔死（icon decode BadMagic + ramfb 零字节）→ 无可点面；berryctl 无 spawn 子命令、bsh 无 app 内建、gui8-smoke 点击链同楔。补跑承载 = `just gui8-smoke`（环境窗口开后即验，W4-VERIFY §六.3 同款挂账）；槽裁决表引用 = [A-IMPL.md](A-IMPL.md) §二（八槽胜者/弃者/弃侧并入点/依据全表）。

## 七、提交

- 既有（实施批，显式路径分笔）：`4d01add63`+`993aee2b4`（应用合流——六 crate + appdemo 画廊 + svcmgr 裁决 B + smoke 随动）；`e7bae2f5a`（镜像接线——SETTINGS 真壳 + appstub ELF 退役）+ `89fb060ba`（计数锁数）；`28b34c919`（入口收敛）；`34dbfc4f3`/`784de947a`/`9ebca27f4`/`6d317fca1`（oneshot 恢复 + 笔记 + gitignore + 配方修复）。
- 本批（验收，显式路径）：A-RECON.md、A-IMPL.md、A-VERIFY.md。
