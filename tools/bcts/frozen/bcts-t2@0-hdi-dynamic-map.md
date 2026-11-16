# BCTS Tier 2 hdi 9 用例动态回放侦察映射表（38 号附录 B 节·旁注件）

> **件性**：`tools/bcts/frozen/bcts-t2@0.toml` 的动态回放侦察旁注（**非冻结件本体**——
> 冻结件只读不改；本件 = 槽位开放（Q8 四件套全绿）前「guest 可观测面盘点 + harness
> 需求清单」，即冻结件头注 :11-14 所指「动态 wire 回放随槽位开放」的前置侦察）。
> 归属：38 号演练 B 产出（Tier 2 动态回放侦察+可跑子集·显示后端槽位）；A 报告
> 收编时本节整体可迁（38 号附录 B 节）。
> 侦察基线：commit 06c4aa5 工作树（2026-11-12）；实跑证据见 §4。

## 0. 冻结语义边界（如实记）

- Tier 2 v0 消费面 = 清单回显（`--dry`）+ 静态对账（实现侧协议 crate op 全集 ⊇ 合同
  ②件 op 表）——`bcts-t2@0.toml:11-14`；语义合同③件时序/失败注入面 = deferred
  （status 标记），动态回放落地时转 frozen「行为零改动迁入」（:13-14）。
- **本件不改任何判定语义**：只回答「若明天开动态回放，hdi 9 用例里哪些腿今天已有
  guest 证据、哪些腿必须先立 harness」。覆盖判定按**腿**（leg）粒度记——正例用例
  的 expect 常含多腿（wire 应答腿 + 语义不变量腿），现役标记只覆盖前者时如实标半。

## 1. 现役可观测面盘点（hdi 位唯一实现链）

| 面 | 形态 | 相 | 代码锚 |
|---|---|---|---|
| 服务自验 | `FBW ok w=… h=… cells=…`（guifbd 自验三步全过自打） | smoke-image EFI 相（ECO 标记 `tools/smoke-image.sh:312`） | `user/bins/guifbd/src/main.rs:905` |
| console 上屏 | `FBSERVE ok`（serve 循环首条消息） | 同上 | 同 `:391` |
| 客户端全链 | `GFX ok`（含 **hdi mini 序**，§2 逐腿拆） | 同上 | `user/bins/gfxdemo/src/main.rs:519` |
| 服务端真值 | `GFXF ok n=7`（首呈现 ∧ px() 抽样 7 点全过 ∧ console_full_n≥1） | 同上 | `user/bins/guifbd/src/main.rs:648-651`（SAMPLE_N=7 `:95`） |
| 中继面 | `WM ok`（wm 四步；present/hdi 经 wmd 透传不改编码） | 同上 | `user/bins/gfxdemo/src/main.rs:289` |

**相位事实（结构性）**：guifbd 恒只在 EFI 链存活（`guifbd/src/main.rs:11-12`——
`-kernel` 主路无 GOP，svcmgr 探测失败不 spawn；`tools/smoke-image.sh:236-238` 同注）。
故 hdi 动态可观测面**只在 smoke-image EFI 相**；smoke.sh 主路八跑 hdi 标记结构性缺席
（rootfs 相 `WM ok` 恰 0 次的负向对账归主会话，`tools/smoke-image.sh:294-296` 注）。
svcmgr 装配门：guifbd = H_FB 在席 ∧ cpio `bin/guifbd`（`user/bins/svcmgr/src/main.rs:1050-1081`）
→ gui-wmd = present_cli≠0 ∧ `bin/gui-wmd`（:1141-1185）→ gfxdemo（:1247 起）→
launcher。端点拓扑（GUI-5 起）：hdi 客户端端经 **gui-wmd 中继**到 guifbd
（`gui-wmd/src/main.rs:140-148`：`0x40..=0x43 → hdi_cli 透传`；随行数执法
`gui-wmd/src/relay.rs:41-58`）；hdi 服务端真身 = `guifbd/src/main.rs:660-755`。

服务端时基：ramfb 软 vsync `SOFT_VSYNC_NS=16_666_666≈60Hz`（`guifbd:77-78`——
vsync_cadence=declared 的首实现声明值）。

## 2. 映射表：hdi 9 用例逐条（用例 id → 冻结 expect → 现役证据 → 判定）

冻结件行号：用例 `bcts-t2@0.toml:385-455`；语义条款 `:460-515`；合同全文
`docs/产品配置层与槽位合同设计-v1.1.md` §2.5（:275-319）。

### 2.1 可跑子集（现役标记直接覆盖或半覆盖——smoke-image EFI 相实跑绿，§4）

| # | 用例（toml 行） | expect 腿拆解 | 现役证据（标记+代码） | 判定 |
|---|---|---|---|---|
| 1 | `hdi-reg-vblank-pos`（:385-392） | ①使能形恰 4B+随行恰 1 Event → 1B ST_OK；②Event 每 vsync 沿 signal 恰一次 | ①gfxdemo `:468-473` `encode_reg_vblank_req(1)+[ev]` → `expect_ok` 断 1B ST_OK（服务端 `guifbd:662-677`：恰 1 event 执法+ST_OK）；关闭形同链 `:510-515`（REG_VBLANK(0) ST_OK——#5 前置面顺带活）。全部过链才 `GFX ok` | **①绿 ②零观测**——event 随 send 迁服务端后，**无任何客户端 wait 该 event**（沿信号代码在位 `guifbd:308-312` armed 每点脉冲，但零消费面）。= 半覆盖（wire 正例腿绿/沿信号腿不可观测） |
| 2 | `hdi-vblank-wait-pos`（:401-408） | ①恰 4B 阻塞 → 16B `[st=0][r][seq u32][ts_ns u64]`；②seq 严格单调增、每 vsync 恰 +1 | ①gfxdemo `:475-483` 单次 wait → `parse_vblank_rep`（`berry-guiproto/src/lib.rs:1283-1291`）执法**恰 16B+st=OK**，断 `seq≥1 ∧ ts_ns>0`；②仅单次采样——单调性（≥2 采样差分）不可验。服务端 `guifbd:301-307/:313` 每点 `seq+1` 应答 parked | **①绿 ②不可验**（单样本）。= 半覆盖（帧形+单点活性绿/单调性零差分面） |
| 3 | `hdi-prepare-pos`（:417-424） | 恰 20B+随行恰 1 VMO；format=XRGB8888 → 4B ST_OK+slot<2 | gfxdemo `:485-505`：`PrepareReq.format=FMT_XRGB8888`（`guiproto:232`）随行恰 1 dup 别名 → `parse_prepare_rep`（`guiproto:1357-1366`）断**恰 4B+ST_OK+slot<2**；应答随行恰 1 fence 断 `nh==1` 收下即关。服务端 `guifbd:759-816` | **全绿**（帧形/格式基线/槽域/随行 VMO+fence 四点全断言）。= 全覆盖 |
| 4 | `hdi-commit-pos`（:441-448） | ①恰 12B `[slot<2][target=0]`、已 PREPARE 槽 → 1B ST_OK；②tear-free：原子换帧（真后端）或**整帧拷贝（软后端）** | ①gfxdemo `:507-508` COMMIT(slot,0) → 1B ST_OK（服务端 `guifbd:723-742`：v1 **纯记账**，target 不排队）。②软后端 tear-free 兑现位点在 present 域 SUBMIT（damage 裁剪整帧 CPU blit `guifbd:576-603` + px() 抽样 7 点对拍 `:604-638`）→ 观测面 = `GFXF ok n=7` | ①绿；②**经软后端退化形在 present 域兑现**（hdi COMMIT 本身无呈现动作——兑现位点偏移如实记；合同 §2.5:310 允许软后端整帧拷贝形）。= 腿①全覆盖+腿②间接覆盖 |

可跑子集小结：**4 例接上现役证据**（1 全覆盖 + 1 应答腿全/呈现腿间接 + 2 半覆盖），
全部由单标记 `GFX ok`（+`GFXF ok n=7`）承载——现役证据即「hdi mini 序全断言过链
才打标」的状态谓词（gfxdemo `:462-467` 头注；`tools/smoke-image.sh:277-288` 标记注）。

### 2.2 不可跑子集（需测试 harness/合同面注入——最小 harness 逐条标注）

| # | 用例（toml 行） | 注入内容 + 期望 | 服务端现状（代码在位性） | 最小 harness（随槽位开放刀） |
|---|---|---|---|---|
| 5 | `hdi-reg-vblank-neg`（:393-400） | 注册后无 vsync 窗 → 零 signal（零唤醒不变量，虚假 signal=违例） | ramfb 软 vsync 恒 16.67ms 到点——「无 vsync 窗」在首实现**不可构造**；可执法合同面 = REG_VBLANK(0) 关闭后零 signal（`guifbd:687-691` 撤 armed；`:319-321` armed∧parked 双空撤点=零唤醒回位） | guest 测试客户端：自建 event → REG_VBLANK(0) → `wait_one(event, >2×SOFT_VSYNC_NS)` 断 TIMEDOUT ∧ observed bits 不变（脉冲形 clear+set 同位 `guifbd:308-312`——bits 复位可观测） |
| 6 | `hdi-vblank-wait-neg`（:409-416） | 连续应答序列验 ts_ns：滞后追赶跳过已过点、不补偿积压、不伪造未来 | **实现面在位零观测**：`guifbd:300`（point=名义点作 ts）+ `:314-317`（`while next<=t skip`——滞后追赶跳过已过点） | 循环客户端 ≥3 次 VBLANK_WAIT 记 `(seq,ts)` 差分：`ts[i+1]-ts[i] ≡ k×16_666_666 (k≥1 整数)` ∧ 应答时 `ts ≤ now`（伪造未来判负）。同步覆盖 vsync_cadence「按声明值验」（:460-466） |
| 7 | `hdi-prepare-neg-format`（:425-432） | format 值不在协商集（值非位域）→ ST_UNSUPPORTED | 在位：`guifbd:768-772` `format != FMT_XRGB8888 → ST_UNSUPPORTED`（拒面 close 随行）。wire 任意 u32 合法帧（`guiproto:1293-1295` 注、`parse_prepare_req:1327-1342` 不拦）→ **可经正规编码器注入**（`encode_prepare_req` 无 format 值域 assert） | 测试客户端一发 `format=2`（如误用位域形 FMT_ARGB8888 值）断 1B ST_UNSUPPORTED |
| 8 | `hdi-prepare-neg-slotcap`（:433-440） | 两槽占用且几何异 → 第 3 PREPARE → ST_FULL（随行 VMO 退还） | 在位：`guifbd:775-798` 槽策略（空槽→几何同覆盖重用→否则 ST_FULL+close 随行）；gfxdemo 仅占 1 槽（`:485-505`） | 测试客户端 3 个已 commit VMO（几何 A,A,B）三连 PREPARE：断 slot=0,1 后第三次 ST_FULL |
| 9 | `hdi-commit-neg`（:449-455） | slot ≥ HDI_SLOTS_MAX=2 越帽野值帧 → 1B ST_BAD | 服务端拒面在位：`parse_commit_req:1382-1391`（`:1387` slot≥2 → None）→ `guifbd:744-747` close_all+ST_BAD。**正规编码器不可达**：`encode_commit_req:1370-1371` assert slot<2 | 测试客户端**手组裸 12B 帧** `[0x43][slot=2][r16][target u64]` 绕过协议 crate 编码器（wmd 中继随行 0 合法透传 `relay.rs:49-56`，不拦） |

注入通道注意（#7-#9 共用）：走 wm 客户端端点（现役拓扑）则先过 wmd 中继随行数执法
（`relay.rs:41-58`）；直连 hdi 客户端端（svcmgr 装配面）亦可——harness 拓扑随
`_api$N` harness 包面（`bcts-t2@0.toml:2-3`）立项定形。

### 2.3 边界外如实记（非 hdi 9 例、勿并入）

- 语义条款 `failure_isolation`（deferred，`:509-515`）——后端崩→DEAD 降级失败注入，
  与 svc-fs 同族 deferred，不在本表 9 例内。
- `LAUNCH ok`/`PAGE ok n=2`（launcher，`frameloop.rs:17/:47`）= present 域长活帧循环
  自测——非 hdi 面，不接 hdi 用例。
- `FBW ok`/`FBSERVE ok` = guifbd 服务自验/console 上屏面（hdi 域外的同服务标记），
  仅作服务在席证据（§1 表），不接 hdi 用例。

## 3. harness 需求汇总（最小面）

一个 guest 测试 bin（暂名 `hdi-selftest`，cpio 入册、EFI 相 svcmgr 预拉或 bsh 手跑）
+ per-case 断言单标记（`HDI ok n=` 族，状态谓词）即可覆盖 #5-#9 及 #1②/#2② 两腿；
唯 #9 需裸帧注入通道（编码器 assert 是合法帧门不是注入门——harness 面天然绕过）。
即：**7 腿缺口、1 个 harness 件、0 协议变更**（`bcts-t2@0.toml:13-14`「行为零改动
迁入」的可行性实证）。

## 4. 实跑记录（2026-11-12，本轮）

- 复跑入口：`just smoke-image` 同源三件现产——image = mkimage 117440512B
  （sha256 6efdfc16…，manifest platform_revision=1）；KERNEL = 工作树现产 debug ELF
  6402760B；rootfs.cpio = 34930688B 现产；`SMOKE_KEEP_LOG=1 tools/smoke-image.sh`
  （AAVMF 固件因宿主沙箱以同内容 staged 副本代入——/opt/homebrew 原件 QEMU pflash
  开写模式被沙箱拒，读通道无限制，副本与原件字节同源）。
- 结果：**smoke-image PASS**（84 标记：EFI 十段恰一次 + 接续 src=handoff + 裸相 32 +
  生态 39 + VER 哨兵 ok 在席/mix 零席 + 字节对拍 kfile=6402760B/init=34930688B +
  金丝雀严格递增 + RX 注入后活性复验 + sim 面零计数）。
- **hdi 链证据行**（串口日志 926 行内 grep 命中；标记嵌稳态矩阵行中，下列行号为
  保留日志行号；时序 = 服务自验 → 中继起 → wm 四步 → 服务端首呈现真值 → 客户端
  hdi mini 序收口——GFXF 先于 GFX = 双标记分立的故障定位面设计实证）：

```
:348  …FBW ok w=800 h=600 cells=100x37      ← guifbd 自验三步（服务在席）
:349  FBSERVE ok                             ← console fan-out 首条
:351  …eco: VER ok v0.2.0-142-g06c4aa5-dirty lvl 1
:353  …gui-wmd: up                           ← hdi 中继在席（启动诊断行）
:358  …WM ok                                 ← wm 四步（中继路径）
:360  …GFXF ok n=7                           ← 服务端首呈现真值（7 点抽样全过）
:364  GFX ok                                 ← 客户端全链（含 hdi mini 序 §2.1 #1-#4）
:395  …LAUNCH ok  /  :398 …PAGE ok n=2       ← launcher（present 域，边界外 §2.3）
```

- Tier 2 现役消费面同窗复核：`tools/bcts-run.sh --tier 2 --dry` 绿（31 例回显含
  hdi 9 例逐条 in/exp、语义条款 status 逐条；静态对账面不变）。

## 5. 引证索引

- 冻结件：`tools/bcts/frozen/bcts-t2@0.toml`（hdi 契约 :348-364；op 表 :367-383；
  用例 :385-455；语义 :460-515；harness 包面注 :2-3）
- 合同：`docs/产品配置层与槽位合同设计-v1.1.md` §2.5（:275-319；「钉诚实不钉频率」
  :319）
- 服务端：`user/bins/guifbd/src/main.rs`（SOFT_VSYNC :77-78；tick_vsync :292-322；
  handle_hdi :660-755；prepare :759-816；标记 :391/:650/:905）
- 客户端：`user/bins/gfxdemo/src/main.rs`（hdi mini 序 :462-515；GFX ok :519）
- 中继：`user/bins/gui-wmd/src/main.rs:140-148`；`src/relay.rs:41-58`
- 协议：`user/berry-guiproto/src/lib.rs`（op :80-86；ST 域 :133-139；FMT :232；
  HDI_SLOTS_MAX :272；VER_HDI :323；hdi 编解码 :1229-1391）
- 装配：`user/bins/svcmgr/src/main.rs:1050-1081/:1141-1185/:1247`
- 相位：`tools/smoke-image.sh:312`（ECO_MARKERS 含 GUI 族七标记）；`tools/smoke.sh`
  （主路 -kernel 无 GOP——guifbd/hdi 面结构性缺席）
