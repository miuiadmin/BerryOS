# GUI-8 实施设计草案（berry-launcher 桌面）——2026-11

> 版本：**1.0**（v1.0）｜2026-11-06

> 依据：`docs/GUI-8实施侦察-2026-11.md`（裁决编号承袭其 §4）+ 五路 scout 复勘
> （2026-11-03，berry-ui 能力面/launcher+契约/fs+spawn/记账+预算/smoke 验收面，
> 全程只读锚点制）。基线：**gui6 @ac55ecb**（= main 5f447ce 全集 + GUI-6
> c3f1044，0c 同日并线）。裁者 = 本草案立稿会话（berryos-6c）；0c 复审四裁决
> 全同意 + 诸钉已并入正文（2026-11-03）。

## §0 钦定与范围盒

**做什么**：launcher 从 park 骨架（e41ae92 接线刀后形态）长成真桌面——壁纸 +
页×网格 + 状态栏 + 点击 spawn + 布局 FAT 持久化。M-GUI 主验收四件（规划 §4:94）：
模拟器打开 GUI 桌面 / 左右滑动翻页 / 点击启动应用 / 跨 boot 保布局。

**冻结既定**（零重议）：

- 裁决 C：launcher = 单 GUI 进程内嵌 berry-ui，经薄服务 IPC，内核零新增面。
- 裁决 D：RGB32、一切几何经 GETINFO 协商禁硬编；TCG 滑动不达 → 降步进翻页。
- 裁决 E：launcher 崩溃 → 监督重启（本草案最小件，见 §3-裁决四-e）。
- 范围盒砍单（规划附录 B）：拖拽/手势导航/Dock/通知全砍——**v1 布局编辑面随之砍**：
  布局持久化验收走 selftest 变体写（§1-V4），真实编辑交互 = v2。
- GUI-9 前提 day-one：稳定 NodeId + 语义角色（a11y 骨架挂位，§3-裁决八）。

**开工闸门（已裁，0c）**：GUI-8 基于 **gui6@ac55ecb** 开工；gui6→main 回流窗
由 0c 在 dd S-4 验证链毕后统一开，本线勿自行推 main。本草案亦落 gui6
（经临时 worktree 提交，主干分支 0c 持有）。

## §1 验收面与 smoke 相拓扑

### 1.1 四验收面 → 标记映射

| 验收面 | 标记（命名候选，canon 两两互非子串复验 = 落码义务） | 相 |
|---|---|---|
| V1 桌面到达 | `LAUNCH ok`（首帧呈现 + 布局装载毕；现役非标记行 `LAUNCH v=` 保留） | EFI 冻结十段 |
| V2 合成交互 | `PAGE ok n=2`（合成 swipe 翻页两格；合成 click 未采用——见下注） | PAGE 冻结十段 |
| V3 真链注入 | `CLICK ok n=1`（virtio-tablet→inputd→gift→launcher 真 click → **真** APP_SPAWN RPC 全程；复用 `PAGE ok n=2`） | EFI GUI 交互相（新） |
| V4 跨 boot | `LAYOUTW ok`（变体布局落盘 + 回读自洽）/ `LAYOUT ok n=`（Boot B 读回变体格数） | EFI GUI 交互相双 boot |

合成 selftest 语义：合成点只在 berry-ui 输入泵入口（`push_pointer`）——referee→
SwipePager→Click 全逻辑真、唯 pointer wire 假；合成 selftest 只产 `PAGE ok n=2`。
真链（V3）的 CLICK spawn RPC 与 reap 真跑；真链（V3）与跨 boot（V4）共挂新相。

> **段④ 落定注记（integration）**：`CLICK ok n=1` 判据 = **首个真实
> `Msg::Click` → APP_SPAWN ok 回执**（真链 V3），非合成 click；冻结相
> `app_cli_h()==0` 结构性静默，故 CLICK 只在交互相出现。探针实测：QMP
> 单击必须落在格容器命中面内——格 Col 原 `h: None` 只占 Grid 轨首 68px
> （内在 icon 48+gap 4+label 16），点轨心会落到 Grid 容器（`Msg::Click`
> 无槽可映射，CLICK 永缺）；desktop 格改定高 = 轨高后 CLICK 链通
> （integration 修，透明容器零视觉差）。

> **哨兵/常态隔离注记（acceptance fix B）**：`GUI8.SMK` 文件名本身用户可
> 创建，故哨兵判据 = payload **逐字节恰** `BERRYOS-GUI8-SMOKE-V1\n`（缺/
> 错 payload 一律 fail closed；harness 建盘写该串）。`CLICK ok` 与
> `launcher: click ready` 同挂此哨兵门——常态 boot 点击仍真 spawn，但不
> 打印任何新标记。harness `--normal-boot` = 该隔离门：同镜像/tablet/数据
> 盘拓扑但无哨兵，点击 APPDEMO 后只认新增 `app APPDEMO up`×1，断言
> `CLICK`/`LAYOUTW`/`LAYOUT`/`click ready` 全零。dual 相 `click ready` 为
> **强制状态谓词**（缺席 FAIL，不 sleep 回退）；Boot A 精确
> `CLICK ok n=1`×1 / `LAYOUTW ok`×1 / `LAYOUT ok`×0 + 新增 APPDEMO up×1 /
> APPQUIET×0，Boot B 精确 `LAYOUT ok n=2`×1 / `LAYOUTW`×0 / `CLICK`×0；
> 宿主 `LAYOUT.JSN` 验收 = 恰 **109 canonical bytes** +
> `APPDEMO(0,0,0)`/`APPQUIET(page=3,row=0,col=1)`（不再只数格数）。

### 1.2 smoke 相拓扑（结构裁决展开见 §3-裁决一）

- **EFI 冻结十段零字节扰动**：新标记 LAUNCH ok/PAGE ok 由 launcher 自身
  打印（拓扑同 WM ok 先例，挂 ECO_MARKERS；CLICK 随段④ 交互相——修约 1）；
  LAYOUT 族标记结构性缺席（无数据盘）。
- **新相「EFI GUI 交互相」**（smoke-image 增段，独立于冻结十段）：挂
  `-device virtio-tablet-device` + `-qmp unix:` + 独立 mkfat FAT12 数据盘
  （LBA0 印章 `BERRYOSVBLK!`，fsmgr 挂载门零改即收——`fat.rs` 锚 `for &cand in
  &[0u64, 16u64]`/`FatBits::Fat12` 当场验真；根 `GUI8.SMK` payload 逐字节
  恰 `BERRYOS-GUI8-SMOKE-V1\n`）。双 boot：A 注入 swipe/click + 变体落盘
  → SIGTERM；B 同盘复挂（B4 `disk.pwr.img` 三 boot 同 `-drive` 先例）→ 读回
  断言 `LAYOUT ok n=2` + 宿主 canonical 109B。`--normal-boot` 常态隔离
  （无哨兵）另起一 boot，spawn 仍发生且四标记全零。
- **rootfs 47 冻结面零扰动**：结构性保（fb 门链层层短路——`WMD_CLI_DUP` 门 +
  fb 探测门不变，scout S5-F7 验证），无需加防。
- 涉 smoke.sh 家族改动归主线串行（规划 §4:97）；smoke-smp.sh ROOTFS_MARKERS
  复制数组 + awk 守卫随动（若 rootfs 相标记零增则免）。

## §2 架构形态

### 2.1 进程拓扑（EFI GUI 形）

```
svcmgr（稳态 echo 往返 + app 域 serve 常驻 ← 新增第二通道）
 ├─ guifbd（present/hdi 服务端，现状）
 ├─ gui-wmd（wm 服务端；单 wm 客户端不变量 D3 保持——状态栏/壁纸=launcher
 │           窗口内容层，非 wmd 系统窗 ⇒ gfxdemo WM ok 断言零连带改）
 ├─ gui-inputd（H_MMIO 门 = tablet 在位；仅交互相活）
 ├─ fsmgr（数据盘在位 ⇒ FAT12 挂载；冻结相 RAM-only）
 ├─ berry-launcher（wm 客户端 + present 客户端 + input 客户端 + fs 客户端
 │                  + app 域客户端——首个长活 GUI 进程）
 └─ 9 app boot 批（APPDEMO/APPQUIET + 7 个 appstub 视觉占位；点击 spawn 走 app 域 RPC）
```

### 2.2 extras 装配表（env 8/8 恰满，bsh 满配先例，零内核抬帽）

| 槽 | gift | 来源 | 冻结十段相 |
|---|---|---|---|
| base 0-3 | console/stdin/self_proc/self_vmar | 同现状 | 在 |
| extras[0] | wm 客户端 dup（`wm_cli_h()`） | wmd arm Ok 路（现状） | 在 |
| extras[1] | fs 客户端端（`fs_h()`） | fsmgr arm 后 **分相错开**让位（§3-裁决四-b） | 缺——修约 1/R1 裁定（v2 修约 2）：让位判别 = BLK_OK≠0（数据盘在位 = vblk 自验过），冻结相 fs 留 bsh（三元皮零漂） |
| extras[2] | app 域客户端端（新 `app_cli_h()`） | launcher arm 门内 channel_pair | 缺——随 fs 让位同门截断（netd 条款镜像：前缀紧致禁中段洞） |
| extras[3] | input 客户端端 dup（`input_h()`） | inputd arm Ok 路 | 缺（无 tablet——INPUT_A≠0 不再判交互相（修约 2），此槽随 fs_gift=0 前缀截断缺席） |

直行复刻 spawn 需 rootfs VMO = 第 5 枚 → 内核 CHAN_HND_MAX 8→9 四处联动
（ADR #61 论据②同款地形）——被裁决二排除，gift 清单恰满即设计约束达成。

### 2.3 svcmgr arm 序重排（含让位序）

现序：… gfxdemo(+阻塞 reap) → **launcher** → fsmgr → vnet → inputd → … → bsh。
新序：… gfxdemo(+reap) → fsmgr → vnet → inputd → **launcher** → stdhello → … → bsh。

- 追认 D3 不变量：launcher 仍在 gfxdemo reap 之后（D3 本意单 wm 客户端序，
  fsmgr/vnet/inputd 皆不触 wm 域，重排安全——scout S3-F3 论证）。
- launcher 移到 fsmgr 后 = fs_client 已存在可赠；移到 inputd 后 = INPUT_A 可 dup。
- bsh arm（:1413 `gifts[1] = fs_client`）改条件路由（修约 1 后判别 =
  `BLK_OK≠0`，修约 2）：交互相让位 launcher（bsh 退 [rootfs] 单皮——
  fs_selftest 无 fs_h() 门会 park，park 契约由段④ 立法）；冻结相/rootfs
  形 bsh 三元皮零漂。
- 监督最小件：launcher arm 后台非阻塞 wait + 崩溃 → 一次重启 + eco 行 loud。

## §3 裁决书（编号承袭侦察 §4；裁决三已兑现归档）

### 裁决一（验收 recipe）：a+b 合体——EFI 数据盘双 boot 新相 + 冻结相合成 selftest

**裁定**：侦察倾向 a（headless）被 scout 新事实升级——EFI 链挂独立 mkfat FAT12
数据盘 = **fsmgr 零改刀即通**（挂载门本认 FAT12@LBA{0,16}；berryos.img 本盘
FAT16@LBA2048 双重不可达才显难）。故跨 boot 走 EFI 新相（真桌面写真盘真读回），
合成 selftest 仍挂冻结十段作回归快门。
**论据**：a 单走只证「LAYOUT.JSN 在 FAT 往返」，不证「桌面的布局跨 boot 存活」
（M-GUI 字面）；b 的旧成本估计（动 fsmgr）被数据盘方案消解。
**Kill**：AAVMF/virt 与 virtio-tablet 形制冲突实证不可行 → 降合成 selftest 为主
验收 + 跨 boot 降 rootfs 族 layoutcheck 自验件。

### 裁决二（spawn 路径）：svcmgr 常驻 app 域 RPC（新 mini 协议 crate）

**裁定**：新 `berry-appproto` 双 op（§5）；svcmgr 稳态 loop 挂第二通道常驻 serve。
launcher 零 rootfs VMO、零守卫复刻。
**论据**：①守卫单源（app_launch 全链 bap_scan→manifest→compat→ui→fs 授能→
spawn→reap 留 svcmgr，无第二份维护面）；②extras 恰满（§2.2）；③reap 30s 绝对帽
归 svcmgr 域内消化（GUI app 常跑分钟级——RPC 形回执即刻、退出收尾异步）；
④微内核原则「先问能不能放用户态」——RPC 对端就是用户态服务。
**连带**：boot 批照旧全量（APP ok n= 语义零漂）；.bap 增删事件 = **boot 枚举 +
op 码留位**（APP_EVENT 占位不实施——v1 rootfs 静态 cpio 无运行期增删物理通道，
规划 :116 顺延注口径）；点击 spawn 复跑已跑过的 app = 幂等进程创建，无冲突。
**钉（0c 复审）**：①APP_LIST 分片帽一次到位——分片序号 + 乱序/重放拒面随
v1 冻结（勿重蹈 DESC 帽两刀补课），wire 细节见 §5；②svcmgr 第二通道 serve
形态沿 guifbd `wait_many` 多路先例。

### 裁决三（布局 schema）：已兑现归档

berry-layout v1 落 main（493a879）——parse/emit/check_canonical 全 pub、
schema 字节级冻结。**页数三源冲突定谳**：`PAGE_MAX=4` 为权威（crate 头注自书
「桌面几何 48/4」= 4 页 × 12 格恰满 CELL_MAX；GUI-6 §9「5 页×24 格」是容量
推算口径非需求）；SWIPE_PAGES_MAX=8 是 pager 机械上限不约束 schema。

### 裁决四（拉起与拓扑，五分件）

- **a 拉起**：已兑现（e41ae92 svcmgr 预拉 + arm + 门），零改。
- **b fs 让位序**：Q-义务② 选项 **(b) 分相错开**——fb 门（`WMD_CLI_DUP≠0`）为
  相判别器：GUI 形 fs 端让位 launcher（bsh 退三元皮），rootfs 形现状零漂。
  不取 (a) fsmgr 多端点 serve（fsproto 严格串行无请求号，多读者 = 应答互截
  ——单读者协议是硬约束）；(c) 不落盘被主验收否决。
  GUI app 的 fs 授能互斥：**桌面态 = 无 app 活**（布局写在桌面态、全屏 app 活
  时结构性无布局编辑——砍单既定），互斥自然满足；授出端 dup 子自持 + 父收口
  后再用（现役解法）为 v2 多 app 并存预留注记。
- **c 输入拓扑**：**launcher 直连 input 域**（input 端 gift extras[3]，GUI-4
  草案 §5.3「消费者 = gui-wm」表述随本刀更正）+ berry-ui 进程内 hit test
  （GUI-3 既定设计：node_at + referee）。v1 桌面 = 唯一 GUI surface，跨窗路由
  （wmd hit test 前送）零需求——wmd「永不主动 send」串行 RPC 纪律不破。
  多窗口事件前送 = 多 GUI 客户端刀（#61 已预留，本刀不触发）。
- **d 监督重启**（裁决 E 兑现件）：最小 = svcmgr 后台非阻塞 wait + 一次重启帽 +
  eco 行 loud；不可行则 kill 挂账 GUI-9（侦察 §3.5 既定 kill 位）。
- **e 状态栏/壁纸归属**：**launcher 进程内**（规划 §3 倾向兑现）——状态栏 =
  launcher 窗口顶部内容层、壁纸 = 窗口底衬，皆非 wmd 系统窗。三鸟：D3 单 wm
  客户端不变量保持 / gfxdemo `WM ok` 断言零连带改（`gfxdemo main.rs:28` 注记
  「GUI-8 加系统窗须随改」不触发）/ WM_WINDOWS_MAX=15 复核免（恒 1 窗）。
  注记（0c）：状态栏/壁纸出现交互面（点击/下拉）时须回 wmd 立法——自绘是
  v1 显示形态非永久豁免。

### 裁决五（psproto 17→18 + launcher 入账）：同刀必做

修改面（scout S4-F4 钉死）：①`berry-psproto lib.rs` 锚 `MAX_ENTRIES: usize = 17`
→ 18（REPLY_MAX 254≤256 自动过编译断言；**19 = 268 绝壁**——256 帧帽最后一站
注记入册）；②launcher arm Ok 路 `ps_reg(b"launcher", ALIVE, 0)` + 不 reap
（D1 三出口清零注释随改）；③svcmgr `≤ PS_MAX=16` 陈注释随改；④ADR 追记沿
#61 义务① 16→17 先例。**时序论证**：不抬帽先入账 = 表 17 恰满 → berryctl
自身行被丢 → bc 自验结构性 FAIL——两件必须同刀。
EFI 相 PS 值 16→17 零漂对拍面随注（smoke-image 注释同步，15→16 先例同位）；
家名单是否添 launcher 行落码期裁（gui-wmd 不在册先例 vs 常驻拓扑真值）。

### 裁决六（壁纸形态）：程序化双色带 v1，位图通道 v2

**裁定**：树内 Rect 带状壁纸（Root 直接子满幅叠放——`layout.rs` place_children
尾臂 Root/默认臂满幅语义，scout S1-F2）：状态栏底黑带 + 上 2/3 深蓝灰底 +
下 1/3 石板带，三 Rect 零资产零缩放约束。
**论据**：位图壁纸被三面合围——berry-gfx 无缩放、Image blob 须恰屏幕尺寸、
屏幕尺寸经 GETINFO 动态（禁硬编 800×600）⇒ 静态资产结构性装不下；BYI1
65536px 帽独立排除全屏编码。位图通道（mkcpio assets/ 条目 + cpio_find 直取
先例在）挂 v2 等缩放原语。

### 裁决七（交互注入通道）：合成 selftest 为基线 + 真链新相为增件

合成事件经 berry-ui 输入泵入口注入（逻辑全真、wire 假）——免新注入面、冻结相
常开；真链注入挂 §1.2 新相（tablet+QMP——qmp_inject down→move→up 能力已在，
smoke.sh 锚 `qmp_inject`；EFI 路参数化复用为实施细节）。串口鼠标兜底协议
（规划裁决 A 第三层）本刀不实施。

### 裁决八（容量帽策略与 a11y）

- press_map（16 帽）：**v1 不做按压反馈**（视觉反馈 = Click 后 spawn 回执标记行；
  press visual 须 Rect/Text 承载 = 每格 +1 节点不值）。砍单注记。
- a11y（32 帽）：挂稳定骨架件——状态栏 2 件（ROLE_TEXT）+ 4 页 Grid 容器
  （ROLE_LIST）+ 当前页 cell 容器 12 件（ROLE_BUTTON + app 名 label）= 18/32；
  翻页时 take/set 重挂当前页。GUI-9 dump 寻址键前提达成（骨架稳定 + 语义角色在）。
- 树规模：4 页 × 12 格 × ~3 节点（cell Col[Icon,Text] + 容器）+ 骨架 ≈ 160-200
  节点 < 512；**实树数入档**（GUI-6 §9 预埋义务兑现，实测数进落地注记）。

## §4 实施刀程（四段，各段独立可验可提交）

**段① 协议与折算（纯函数面，零 guest 扰动）** ✅ 已落 2026-11-03（三笔 8ce7d3b/782fb3b/7196b56 落 gui8-draft；test-host 984/0 零漂 + abi-check PASS + berry-ui 双面 164/167；对抗审查 1 MAJOR[parse_spawn_request 越帧读→帧长界修复+脏尾回归钉]/1 MINOR 全收编；guest smoke 47/47 对拍顺延 dd 端口空窗）
- `berry-appproto` crate：APP_LIST/APP_SPAWN 编解码 + host 测（#54/#50 纪律：
  host 测编解码先行）；APP_EVENT op 码留位。
- intent 折算表落 `berry-guiproto`：SCENE_* 直通退役（**停发非删值**——常量
  保留 + deprecated 标注，add-only 纪律，删 wire 值 = 破跨版本流）、VOTE scene
  字节改携 berry_intent 六档值（常量经 berry-sys 单源镜像，禁第三份手抄）；
  berry-ui `ballot_for` 票源改发 intent 值；gfxdemo VOTE 用点同刀改；SDK 透传
  三口 API 面预埋（berry-libc pthread_attr intent 位 + berry-exec SpawnSpec
  intent 字段；CHAN_OPT_NO_INTENT 已在）。
- 验收：just test-host 全绿零漂（berry-ui **双面复跑**：基线 163/166 + intent
  面新测）；abi-check PASS；冻结面不破（Node 144B / TLV add-only / ChangeMask
  位义 GEOM=0b1100_0111、PAINT=56——GUI-6 草案 §16 实施合折）。
- 工程注：dd S-4 验证链在途占共享 target dir——host 测走隔离
  `CARGO_TARGET_DIR=$HOME/.cargo-tmp-gui8` 避让。

**段② svcmgr 装配（共享面——动前 ping 0c 错峰，§10）** ✅ 已落 2026-11-03（d057144；沿革二十续——对抗审查 4 MAJOR 收编 + R1 判别器二次修约 INPUT_A→BLK_OK 仪器定谳；验收四门 = test-host 984/0 零漂 + 手跑 EFI 复现 + smoke-image 71/71 + rootfs 相 47/47）
- arm 重排（§2.3）+ fs 分相错让位 + input dup 赠礼 + app 域常驻 serve
  （稳态 loop wait_many n=2）+ psproto 18 + ps_reg 入账 + 监督最小件。
  钉（0c）：gfxdemo「单活 wm 客户端」头注随动更新——launcher 真接 wm 后
  双客户端互斥语义写清（谁在表、reap 挂死卫生段是否仍成立）；smoke 相
  LAUNCH 标记行序对拍。
- 验收：rootfs 相 47/47 零漂（bsh 三元皮回归）+ EFI 冻结十段零字节扰动
  （除新增 LAUNCH 族标记）+ PS 值 17 定向对拍。

**段③ launcher 主体** ✅ 已落 2026-11-03（0a023af；沿革二十一续——settle 竞态 TCG 载荷定谳 + 栈面 R5 stack_pages 8→64 + 对抗审查 R2 收编；验收三门 = rig 780s 满窗 LAUNCH ok 1/PAGE ok n=2 1 恰一次 + smoke-image 73/73 + test-host 全绿；ECO_MARKERS 31→33）
- 布局桩（壁纸三带 + 状态栏 Col + SwipePager track + page_node 填 Grid
  4×3）+ IconSource 供件桥接（boot 期 APP_LIST 拉取 → 静态注册表 → BYI1
  decode → LRU 装载编排——消费侧义务兑现）+ 长活帧循环 embedder（等输入/
  present 事件或 next_wakeup 期限 → push_pointer → wake → pump_input →
  Msg 分发 + pager.on_msg/tick → render_frame → 光栅进 VMO → SUBMIT →
  FRAME_CALLBACK → note_presented → RELEASE → vote；per-SUBMIT fresh VMO
  dup 别名池 + credits≤2 背压 + IDLE sticky 停帧票）+ 布局读写（berry-layout
  parse/emit + libc fs 面；缺席回落默认 + 读失败回默认重写自愈 + 写失败非致命
  三约束）+ 合成 selftest（PAGE 标记 + 变体布局写；CLICK 归段④ 真链）+ LAUNCH ok 首帧标记。
- 依赖接线：berry-ui/berry-gfx/berry-layout 进 launcher 独立 workspace
  （guifbd 同形先例：no_std crate 入 musl std 程序）。
- 验收：EFI 冻结十段新两标记（LAUNCH ok/PAGE ok）到齐 + UTM 手验桌面起；
  CLICK ok 随段④ 交互相验（修约 1）。

**段④ smoke 新相 + 文书收官** ✅ 已落 2026-11-05/06（段④ 刀链 76700fb/bbdd1ba/e05b8e7/20a7844 → 068ce78 验收面入 main；`just gui8-smoke` 独立关卡 = Boot A `CLICK ok n=1`×1 + `LAYOUTW ok`×1、Boot B `LAYOUT ok n=2`×1 + 宿主 canonical 109B 读回 + `gui8-smoke-normal` 常态隔离四标记全零；规则/07 GUI8 行 + justfile 双配方随刀入册）
- EFI GUI 交互相（tablet+QMP+数据盘双 boot；哨兵 payload 精确 magic；
  click ready 强制；精确计数 + 新增 APPDEMO up + canonical 109B 读回）+
  `gui8-smoke-normal` 常态隔离（无哨兵，spawn 仍发生而四标记全零）+
  LAYOUT 族标记 + 标记族 canon 全量复验 + smoke 三脚本/规则 07 相构成表/
  沿革五件套 + justfile :215-218 stale 注释回写 + smoke.sh 六处 stale `73`
  顺手清 + ADR 修订（#58 收官注记/#61 义务②兑现/psproto 18 追记）+
  索引登记 + S7 终判数据采集（双打点进遥测档，判 s7 转 CI or kill）+
  m-gui-wip 分支处置注记。
- **文书收官兑现注记（2026-11-05/06 随刀 + 2026-11-10 W2 对账补账）**：
  五件套已落 = 规则/07 GUI8 行 + justfile gui8-smoke/-normal 双配方 +
  ADR #58 兑现注记（2026-11-03 段①）+ 索引 GUI-8 行 + 沿革；**余项两件
  跨刀待收**——① S7 终判双打点采集（判定面 = launcher embedder，berry-ui
  s7 feature 仍 feature-gate 未接；规划 :125 D20-① 判定文案在册）②
  m-gui-wip 分支处置注记（分支已删零注记——沿革 :1456 处置裁定「保留至
  GUI-8 收官，此后可删」为现状事实源）。规划 §4 末「GUI-8 落地注记」
  补齐归本笔同刀（docs lane W2）。

## §5 协议面：berry-appproto v1

- 通道：launcher arm 门内 `channel_pair()`，服务端 svcmgr 常驻 serve，
  客户端端 extras[2] 赠 launcher（libc 访问器 `app_cli_h()` 新增，同槽惯例）。
- `APP_LIST_REQ`（空）→ 分片流应答：N × `APP_ENTRY{id[24], label[64],
  icon_w, icon_h, blob_len}` + blob 按 192B 分片跟帧（BYI1 原样字节，launcher
  侧 decode——两端零解码逻辑重复，图标 ≤65536px 帽由 BYI1 域执法）。
  终帧 `APP_LIST_END{n}`。**分片序号 v1 冻结**（0c 钉）：每帧携 idx 单调
  递增，乱序/重放 = ST_BAD 拒，一次到位免 DESC 帽式补课。数据源 = svcmgr
  bap_scan 记账表现查（manifest [ui] label/icon + icons/*.byi——GUI-7 面
  复用零重扫）。
- `APP_SPAWN_REQ{id[24]}` → `APP_SPAWN_ACK{ok, err}`（svcmgr 走 app_launch
  全守卫；reap 异步收尾不阻塞回执）。
- `APP_EVENT` op 码留位零实施（.bap 增删推送——v2 /apps 树前提）。
- RPC 纪律：fsclient 严格串行同形（一请求一应答、无请求序号、单读者）；
  收发帽沿 fs_rpc 10s 形。

## §6 记账与帽（全族对账）

| 帽 | 值 | 状态 |
|---|---|---|
| psproto MAX_ENTRIES | 17→18 | 段② 抬（256 帧帽最后一站注记） |
| env extras | 4（8/8 恰满） | 零内核改（bsh 满配先例） |
| berry-layout PAGE/CELL | 4/48（4 页 ×12 格） | 权威定谳（裁决三） |
| MAX_NODES/CAP/DESC_MAX | 512/1024/8192 | 实树 ~200 入档（段④） |
| a11y/press_map | 18/32 用、不用 | 裁决八 |
| LAYOUT_BYTES_MAX | 2048B 单簇近原子 | berry-layout 既定，写路径遵循 |
| 落盘 DEADLINE | 新相另设（TCG 布局落盘 ~10s 级成本模型） | 帽只兜结构性挂死 |

## §7 ADR 与文书清单

- **ADR #58 收官三件**（硬义务非可选）：SCENE 直通退役 + 折算映射落码 +
  SDK 透传三口 API 预埋——段① 一体交付；ADR 正文注记「GUI-8 桌面收官兑现」。
- **ADR #61 义务②**：布局走 fsclient 兑现注记（分相错开形态入册）；
  psproto 18 抬帽追记沿义务①先例。
- **#54（纪律原锚 #50）**：app 域 wire host 测编解码先行（段①）。
- **#60 ④**：launcher 树规模实测立项同步（段④ 入档）。
- 文书五件套：smoke.sh/smoke-smp.sh/smoke-image.sh + 规则/07 相构成表 +
  阶段沿革；规划 §4 末补「GUI-8 落地注记」（已落/顺延/排障三段式）。
- AGENTS 命令速览：无新 just 配方则零回写（合成 selftest 走 smoke-image
  既有入口）。

## §8 kill criteria（裁决 D 线 + 本草案新增）

1. TCG 滑动 <10fps 且降载仍不达 → 步进翻页无动画（裁决 D 既定）。
2. AAVMF/virt + virtio-tablet 形制冲突实证 → 裁决一 Kill 降级（§3-裁决一）。
3. 图标装载超 32MiB 帽 → 降分辨率/减密度（侦察既定）。
4. 布局写盘超交互容忍 → 降定长记录二进制（规划 §1 双选既定）。
5. 监督重启往返不可行 → boot 拉起不监督挂账 GUI-9。
6. svcmgr 稳态 serve 双通道插入证不可行（单线程架构阻力）→ 裁决二降
   launcher 直行 + 内核抬帽另刀（最后手段，预告 0c）。

## §9 风险与首验点（段内首验，验不过即回草案改）

- **R1**（段② 首验）：bsh 在 EFI 形失去 fs gift 后 park 行为漂移（现
  「park 在 fat 段」语义）——十段字节对拍门兜底；漂移则让位判别改
  inputd/数据盘在位信号。**已触发收编（对抗审查 MAJOR [0]+[1]）**：
  WMD_CLI_DUP 判别在 EFI 冻结相恒开 = 恒让位 → fs_selftest 无门 park
  杀 11 标记；冻结相 launcher 恰 [wm]（e41ae92 接线刀同构零漂）。
  **二次收编（仪器实测，修约 2）**：fallback `INPUT_A≠0` 实证废——
  inputd 预拉只门 H_MMIO（virt 恒在）且 spawn Ok 即 store（tablet 探测
  晚于 store、park 不回清）→ EFI 冻结相恒真误让位，bsh fs_h()==0 →
  PUT64 ENOENT park → fs 族 11 标记连坐（仪器：bsh 自打 `PUT64 not
  found h=0`；基线 stash 对照 71/71 全绿）。**判别器 v2 = BLK_OK≠0**
  （vblk 预拉自验写读 LBA5 过 = 真 virtio-blk 数据盘在位；EFI 冻结相
  usb-storage 非 vblk 域自验即拒恒 0；rootfs 形=1 但 arm 门恒跳不消费）。
  语义恰合：launcher 需 fs ⟺ LAYOUT.JSN 所在布局卷在位（fsmgr 挂同一
  盘，两信号同源同真假）。监督重启装配同门（冻结相重启仍 [wm]——
  防 段③ 布局读起 bsh↔launcher 双读者混流面，C-1 刀 3 危害类）。
- **R2**（段② 首验）：svcmgr 稳态 loop 挂第二通道后 echo 往返时延零可感
  （boot 自验门既有断言兜底）。
- **R3**（段③ 首验）：长活 present 客户端 credits 背压 + blit-at-SUBMIT
  交互面——首帧后连续翻页 100 次压力手验（UTM）。
- **R4**（段④ 首验）：AAVMF 下 virtio-tablet 事件可达 inputd（H_MMIO gift
  链在 EFI 引导路的完整性——内核 virtio 枚举经 DTB，EFI 路 DTB 传递是
  隐含前提，段④ spike 先证）。
- **R5**：launcher 栈页定值（berry-ui 树 + MeasCtx 2KB + old_abs 16KB +
  JSON 递归——#60 栈面审计义务）：musl 程序栈 8MiB 兜底，首跑后实测入档。

## §10 跨会话门与预埋

- **门 1（对 0c，已裁毕）**：四裁决全同意 + 诸钉并入；草案经临时 worktree
  落 gui6（主干分支 0c 持有）；gui6→main 回流窗 = dd S-4 验证链毕后 0c 统一
  开。svcmgr 仍 = 共享面，段② 动前再 ping（add-only 错峰纪律）。
- **门 2（对 dd）**：段②/④ 涉 smoke-image.sh/smoke.sh 主线串行窗——与
  dd S-4 smoke-smp 窗错峰（dd 验证链在途：~/.cargo-target + QEMU 18080-18083，
  预计 1-2h）；段① host 测已裁走隔离 target dir。
- **预埋 GUI-9**：位图壁纸通道（mkcpio assets 先例）/ .bap 事件推送实施 /
  截图像素对拍（本刀不借 QMP screendump——全推 GUI-9 既定）/ 多窗口事件
  前送（#61 预留）/ dump 注入器寻址键（a11y 骨架已 day-one）。
- **挂账**：布局真实编辑交互 v2；程序化壁纸升级位图 v2；串口鼠标兜底协议
  （裁决 A 第三层）不实施；launcher 稳态 icount 成本 B-2 后量。

## §11 草案方法注记

五路 scout 只读锚点制（berry-ui 能力面读 worktree gui3 = gui6@c3f1044，
0c 会话 worktree 零写入）；承重四事当场复核：fsmgr FAT12 挂载门 / launcher-
fsmgr-bsh arm 序 / berry-layout PAGE_MAX=4 / psproto MAX_ENTRIES=17。
残差：EFI 路 DTB→virtio 枚举完整性（R4 首验）；launcher 栈深（R5 实测）；
ECO_MARKERS 31 全量清单未逐项列（落码期 canon 复验覆盖）。
