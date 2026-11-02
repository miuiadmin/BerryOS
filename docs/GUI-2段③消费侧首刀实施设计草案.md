# GUI-2 段③ 消费侧首刀实施设计（B 形态接线——guifbd present/hdi 域服务端 + gfxdemo 首帧上屏）

> 状态：**已收官**（2026-11-02——设计 workflow 六工拍板 + 实施 workflow 9 工落地 + 主会话串行
> 收割：test-host 817/0（+1 新测）+ smoke-image 两连 PASS **69/69**（GFX ok/GFXF ok n=7 恰一次、
> 既有 67 零漂移）+ smoke 八跑全绿（rootfs 相 46 零变化 + 负向 GFX==0 + `eco: fb absent` 恰一行）
> + check/docs_guard 绿；PS 台账实测 EFI 相 **15** / rootfs 相族 **13**——底稿两相对调已勘（§5/§7-⑥））。
> 前置：GUI-2 段①② 已落 main（`5317e09`/`098d01b`——berry-gfx pass 形态 API + soft 光栅核 host 面全备）；GUI-4 段①② 已收官（guiproto 三域 wire 全定形 + input 域接线，`1023072`）；GUI-1 显示链在 CI（FBW/FBSERVE）。
> 定形锚点：`docs/GUI里程碑规划-2026-10.md` :102——「GUI-4（输入事件往返）或 GUI-2 消费侧首刀，先到先定」——**GUI-4 已到，本刀即锚点兑现刀**。

## §0 范围（本刀做 / 不做）

**本刀 = B 形态三件事的第一个真实消费者闭环**：berry-gfx 渲一帧 → VMO → `SUBMIT` → guifbd present 域服务端 → ramfb 上屏 → `FRAME_CALLBACK` 回执 → smoke 断言。协议 wire 层段① 已全定形（本刀零协议改动——ADR #54 纪律「协议先定形、无消费者不接线」的接线半边）。

**做**：
1. guifbd serve 循环升格：单路 console recv → **wait_many 三路**（console + present + hdi 三 channel——R2 核实 console pipe 实为 channel pair，三路 wait 语义同构）；
2. present 域服务端真身六 op（GETINFO / SUBMIT / FRAME_CALLBACK / RELEASE / VOTE / POWER_MODE）最小实现；
3. hdi 域 ramfb 第一实现（REG_VBLANK / VBLANK_WAIT / PREPARE / COMMIT——软 vsync，`§9-⑨` 挂账兑现）；
4. console 路恒 FULL damage 语义验收（规划 :102「第一个消费者一并验收」义务——console put_str 语义 = submit(FULL) 走同一提交路径，「无 damage 不提交」不变量 day-one 执法）；
5. credits/latch 语义（v1 钉死 **blit-at-SUBMIT** 形：SUBMIT 处理内同步 CPU blit + 应答 token，
   credits = 在途未 RELEASE 别名数 ≤2 执法；squash/DISCARDED/UNSQUASHABLE 分支 = v1 死路径
   留异步后端刀——§7-③ 钉板与 R1 应修-1 一并消解）；
6. gfxdemo 消费端（berry-gfx pass API 渲帧 → SUBMIT 往返）+ 分发 = **svcmgr EFI 相预拉**
   （b-2 案端点直递——§7-①；rootfs 相随 guifbd 门静默跳过零新行；bsh 手跑走
   `present_h()==0` 降级行——netd 双门先例同款）；
7. smoke-image EFI 相 `GFX ok` + `GFXF ok n=` 双标记（67→69；rootfs 相零扰动）。

**不做**（挂账 §7）：gui-wm 分发跳（GUI-5，段① §9-⑧）；virtio-gpu 后端（S1 spike 另刀）；PM 脉冲真落地（GUI-6+）；vote 仲裁策略（本刀只记账不仲裁——票面收下即回执）；稳态零唤醒 smoke 断言全形态（本刀先落「无未决请求 = 无主动 send」结构面，能耗代理断言挂 GUI-6）；多 surface/多客户端（C 形态退化 B 的单客户端面）。

## §1 立项依据（钦定源）

- ADR #50：B 形态三件事 = submit(buffer, damage K≤8+FULL) / frame_callback(未来时刻表+credits≤2) / release；HDI 三口 ramfb 第一实现；**「无 damage 不提交」不变量 + latch/squash（GUI-10 补充）+ per-interface version（段① GetinfoRep 已落 ✓）**。
- ADR #51：soft 后端永久 CI 基线——本刀 gfxdemo 用 berry-gfx soft 渲帧即该基线的首个设备面消费。
- ADR #53：三态停帧是验收项非优化项——本刀 IDLE 面（脏区空∧无未决 = 不发任何消息）结构先行。
- 规划 :87 GUI-1 行：serve 循环按 B 形态三件事定形（原案 GUI-1 顺延件——本刀兑现）。
- 规划 :88 GUI-2 行：消费侧 = host 离屏像素断言（已落）→ 设备上屏（本刀）。
- 段① 草案 §9-⑨（HDI 域消费者接线挂账）+ §9-⑧（gui-wm 分发 = GUI-5 不在本刀）。

## §2 拓扑与通道（粗案）

```
gfxdemo（svcmgr EFI 相预拉）           guifbd（EFI 链常驻）
  │ berry-gfx pass 渲帧 → VMO 写像素     │
  │ present 域 channel ──────────────→ │ wait_many 三路（全 channel 同构）：
  │   SUBMIT(vmo 别名, damage K≤8/FULL) │   ① console（gift3——既有文本路，恒同步
  │   FRAME_CALLBACK（阻塞应答）←────── │      直写 + FULL 记账升格）
  │   RELEASE(vmo_h) ←────────────────  │   ② present channel（服务端真身六 op）
  │ hdi 域 channel ─────────────────→   │   ③ hdi channel（软 vsync——VBLANK_WAIT
  │   REG_VBLANK(event)/VBLANK_WAIT/    │      parked 到点应答，deadline =
  │   PREPARE/COMMIT                    │      min(下次软 vsync, now+10s) 进调用）
  └─────────────────────────────────────┴ ramfb（fbcon 之外整窗可作帧目标）
端点分发（b-2）：svcmgr 在 guifbd 预拉段 channel_pair()×2（探测门 else 分支内）——
  服务端端随 guifbd extras（2→4 枚，恰 8/8）；客户端端随 gfxdemo 预拉 extras 直递（6/8）。
```

关键地形（scout + 设计 workflow 三路调研已核；锚见 §8/§9）：
- **guifbd 恒只在 EFI 链存活**（boot EFI-GOP 命中才授 fb 窗）→ 首帧上屏验证面 = smoke-image
  （EFI 相）；rootfs 相 gfxdemo arm 随 guifbd 门静默跳过（零新行、零标记扰动）。
- **wait_many 在册**（NR 0x05，n≤16、deadline 绝对 MONOTONIC ns、poll-first 无丢唤醒——
  R2 轴 1 全核）：三路 serve 地基零内核改动；console pipe 实为 channel pair，三路同构。
- **boot 授能链不动**：fb 窗 + info VMO 既有；present/hdi 端点 = svcmgr 预拉段建对分发
  （§7-① b-2）——零内核改动、bsh 面零触碰。
- ramfb 无翻页 → **v1 = blit-at-SUBMIT**（SUBMIT 处理内 damage 裁剪 CPU blit → px() 抽样
  7 点对拍 vmo_read → present_seq++ → 8B token 应答）；FRAME_CALLBACK = token 校验即答
  （PRESENTED + credits + presented_ns=clock）。「未来时刻表」走 hdi 域 VBLANK_WAIT
  （软 vsync 时刻 = clock 合成进 ts_ns）——wire 零改动（FrameDoneRep 16B 无空位，§7-②）。
- **credits ≤2** = 在途别名数执法（满 → ST_FULL 1B）；**RELEASE** 归还别名（≤4）；
  **latch** = v1 退化「应答即 latch」；squash 死路径（§0-5）。
- **「无 damage 不提交」立法**：服务端两条 ST_BAD（K>0∧FULL 同设 / K=0∧无 FULL——
  guiproto enum doc :494-497 明示服务端拒面）+ console 面恒 FULL 入同一记账路径
  （纯计数，**恒同步直写不入队**——R2 A2-a 立法）。

## §3 逐文件落点表（设计 workflow 综合 agent 产物 + 批判修订吸收；改动面 = 7 文件 + 1 新 crate）

**kernel/\*\*（含 boot 授能块 kernel/src/main.rs:1116-1236）与 user/berry-gfx/\*\* 零改动；
tools/smoke.sh 零改动（rootfs 相零扰动本体）。**

| # | 文件 | 动作 | 改什么（精确段） |
|---|---|---|---|
| 1 | `user/berry-guiproto/src/lib.rs` | 小增（**零 wire 改动**） | 新增 `pub fn parse_vblank_wait_req`——4B 恰长 + op 位校验，对齐 `parse_reg_vblank_req`（:1087-1096），补 :1098-1099 自注缺位；cfg(test) 加 roundtrip/拒面测试。不动任何常量/编码/帽（ADR #54 协议面零漂移） |
| 2 | `user/berry-libc/src/io.rs` | 小增 | 四个同槽别名访问器（gift2_h/gift3_h 先例 :190-224）：`present_srv_h()`/`hdi_srv_h()` 读 GIFT_H[4]/[5]（guifbd——hs[6]/hs[7] 落种位 :102-107 已在）；`present_h()`/`hdi_h()` 读 GIFT_H[0]/[1]（gfxdemo）。同步 :29-34 槽位总注释 + **netd_h/input_h 两个 accessor 的「独占槽」doc 一并改**（R1 备注①） |
| 3 | `user/bins/svcmgr/src/main.rs` | 中增（两 arm） | ① guifbd 预拉段（:403-446）：**fb 探测门 else 分支内、spawn 前** `channel_pair()`×2（A5-a——建在门外 = rootfs 相白建 4 对象 + eco_skip 路恒泄漏）；:430 extras 扩 `[H_FBINFO, H_FBPIPE, present_srv, hdi_srv]`（恰 8/8 断言内）；客户端端存局部变量；Err 路 close 未 take 端（vnet :563-573 先例）。② gfxdemo 预拉 arm 紧随 guifbd：门 = guifbd spawn Ok + `cpio_find(H_ROOTFS, b"bin/gfxdemo")`；SpawnSpec stdout=console、stdin_feed=Some(&[])、stack_pages=8、extras=[present_cli, hdi_cli] 直递；**阻塞 reap 30s 帽收码**（berry-exec :54——非零码 eco 行续走）；**不 ps_reg**（PS ok n= 零漂移）；失败面 eco_skip |
| 4 | `user/bins/guifbd/src/main.rs` | 大增（服务端真身） | ② gift4/gift5 取端（0 值 eprintln+park 兜底——:81-91 同款）。③ serve 循环升格（:162-211）：三路确定性轮询（present/hdi/console）全 EAGAIN → `berry_object_wait_many`（n=3）；**deadline = min(下次软 vsync 点, now+10s) 进调用本身**（A1-a——醒来后检查不够，parked 最长迟答 10s 且 smoke 抓不住）；**每轮 poll 前重置 nb/nh 容量**（A1-b——漏重置 = 恒 -EINVAL 死循环）；present 路补 handle 出参 8 槽数组（现 :182 传 null）。④ present 六 op：GETINFO（ver 1/1/1、format_bits 实测、max_credits=2）；SUBMIT（**v1 = blit-at-SUBMIT**：vmo_get_size 校验 ≥ stride*h → damage 裁剪 CPU blit → px() 抽样 7 点（**全落 K=2 rect 并集内**——A5-b）对拍 vmo_read → present_seq++ → 8B token；立法两条 ST_BAD（K>0∧FULL / K=0∧无 FULL）；credits 满 → ST_FULL 1B；**一切拒收路径 close 已随行迁入的 handle**（A3-b——ST_BAD/ST_FULL/ST_UNSUPPORTED 全含））；FRAME_CALLBACK（token 校验 ST_NOENT → 16B FrameDoneRep，**禁断言精确 present_seq**——console 谱系共享抬高（A2-b））；RELEASE（应答 ids + 帧外别名重挂，≤4）；VOTE/POWER_MODE（记账 1B 回执——§0 不做面）。⑤ hdi 四 op 退化形：REG_VBLANK 存 event（**关闭形 = close 存量 + close 本次随行**——A3-d①）；VBLANK_WAIT park 到软 vsync 点（SOFT_VSYNC_NS ≈16.67ms 常量；**已 parked 再来 = ST_BUSY**——A3-d③）；PREPARE 槽记账 + 存别名 + 应答 slot + berry_event_create 空置 fence（**槽复用 = 覆盖时 close 旧别名**——A3-d②）；COMMIT 记账 1B。⑥ console 路升格：put_str **恒同步直写**（Fbcon::put_str 原路径零延迟变化）+ FULL **纯记账**（console_full_n++ + 共享 present_seq 谱系，不占客户端 credits，**禁入任何 vsync 排队呈现路径**——A2-a）。⑦ 首个 gfx 帧呈现时刻一次性打 `GFXF ok n=`（∧ 条件：抽样全过 ∧ console_full_n≥1——FBW ok 回流自身 pipe 结构保证）。⑧ VMO 消费 = vmo_read 行弹（静态 [u8;3200] 弹舱；guifbd 是 berry-libc musl 程序**有堆**——静态定容 = 工程选择非纪律，no_std/零 alloc 只执法 berry-gfx/guiproto 调用面——A4） |
| 5 | `user/bins/guifbd/Cargo.toml` | 小增 | + `berry-guiproto = { path = "../../berry-guiproto" }`；头注补段 |
| 6 | `user/bins/gfxdemo/`（**新 crate**） | 新建 | 独立小工作区（guifbd 同型：空 [workspace] + berry-libc/berry-sys/berry-gfx/berry-guiproto 四 path 依赖 + release z/lto/cgu=1/abort/strip + CRT0_SEED 锚）。main 流：`present_h()==0` → `"gfx: present absent"` + exit(0)；GETINFO 断 version_present==1 ∧ FMT_XRGB8888 ∧ max_credits==2；vmo_create → vmo_commit → vmar_map(self_vmar_h) → berry-gfx TextureDesc(Bgrx=1 与 ramfb 同源) + DisplayList（Clear 底色 + fill rect + glyph 串）+ Queue::submit 渲进映射；dup(VMO, RIGHT_READ) 别名 → SUBMIT(K=2 rect) → FRAME_CALLBACK 断 PRESENTED/credits → RELEASE 断 count==1 归还即关 → VOTE/POWER_MODE(DIM) 1B → hdi mini 序（REG_VBLANK(1)+event → VBLANK_WAIT 断 seq≥1∧ts_ns>0 → PREPARE **再 dup 新别名随行**（A6-b——send 即 take，旧别名已归服务端）断 slot<2 → COMMIT → REG_VBLANK(0)）→ `println!("GFX ok")` → exit(0)。**收侧 10s 帽**（fsclient fs_rpc 形态——A6-a）：每步 RPC wait_one READABLE 10s，失败 eprintln + exit 非零（svcmgr 收码 eco 行，FAIL loud） |
| 7 | `justfile` | 小增（两行） | build-user 段 guifbd 构建行后加 gfxdemo 同款行；mkcpio 按 `bin/gfxdemo=` 条目（mkcpio 内部 sorted，实参序无关产物） |
| 8 | `tools/smoke-image.sh` | 小增 | ECO_MARKERS 追加 `"GFX ok"`/`"GFXF ok n="`（27→29）；ALL_COUNT 动态算自动 67→69；人肉同步五处注释（:271/:7-8/:33-37/:291/:363）。子串审计已过：与现役 67 互非子串，裸 grep -qF 足够 |

契约顺序：#1/#2（库面）→ #3+#4+#5（服务端+分发）→ #6/#7（消费者+产链）→ #8（断言面开灯）。
中间态可验：S2 落成后 guifbd 带四 extras 起服务、无消费者，FBW/FBSERVE/PS 仍零漂移。

## §4 标记表（smoke-image EFI 相 67→69；smoke.sh rootfs 相零变化）

| 标记 | 相 | 打点者 | 恰一次语义 |
|---|---|---|---|
| `GFX ok` | 仅 EFI | gfxdemo（客户端闭环全断言过才打） | 结构性恰一次：svcmgr 单次预拉 + 阻塞 reap + 单 println；EFI RX 相 bsh park 在 fat 段命令不执行——无竞速面；rootfs 相 arm 随 guifbd 门静默跳过 |
| `GFXF ok n=` | 仅 EFI | guifbd（服务端真值：抽样 7 点全过 ∧ console_full_n≥1；n= 抽样数，前缀式同 `PS ok n=`） | 结构性恰一次：首呈现单发标志（served :198-204 同款）；稳态矩阵使 full_n 持续增长，自证行只打一次 |
| `gfx: present absent`（诊断行） | bsh 手跑形态（smoke 面不可达） | gfxdemo 降级门 | 永不进 MARKERS 数组 |

两标记分立 = 故障定位面（GFXF 缺 = 服务端 blit/抽样/记账炸；GFXF 在而 GFX 缺 = 客户端 RPC 链炸）。
ECO 族本就不走 assert_once 机器执法（smoke-image.sh:294-299 只盖 EFI_FIXED+CONT+ERE）——恰一次 =
结构保证 + §5 人工 grep -cF==1 复核，与 FBW/FBSERVE 同待遇。算术：4+6+3+27+**29** = **69**。
**不设「GFX absent」断言标记**：rootfs 相 guifbd arm 已实打 `eco: fb absent`（svcmgr:416-417），
gfxdemo arm 静默跳过零新行 = §0「静默」的字面落实。

## §5 验证面

**host 测试（just test-host）**：
1. guiproto parse_vblank_wait_req roundtrip + 拒面三连（长尾/截断/错 op 位恰 None）。
2. 既有面回归零漂移（guoproto roundtrip 族 + berry-gfx 全测试不触碰应全绿——berry-gfx 零改动的直接证据）。
3. guifbd/gfxdemo = musl guest 程序无 host 测试面（FBW/FBSERVE 先例——真值交 smoke）。

**smoke-image EFI 相（just smoke-image）**：
1. 69/69 到齐（DEADLINE_BOOT=360s，实测全链 ~115s 余量足）。
2. GFX ok / GFXF ok n= 恰一次复核：grep -cF 各 ==1。
3. 既有 67 零漂移：FBW ok / FBSERVE ok（guifbd 回归）、PS ok n= 前缀（账本零新增）、BSH ok。
4. 阶段 3 字节对拍动态自洽（rootfs.cpio +bin/gfxdemo 变容自动跟随）。
5. 顺手对账：EFI 相 `PS ok n=` 实值 == **15**（2026-11-02 正片双跑实测）；rootfs 相族实值 == **13**
   （同日主仓正片双跑同值——gui3 树登记面与主仓同源零差，本刀零登记行坐实）；B4 Boot C 相 == 14。
   设计底稿「EFI 13 / rootfs 15」系**两相读数对调**错值；本刀 gfxdemo reap 走 berry-exec
   `child.reap()` 不经 svcmgr 登记路径——§9 R1 备注⑤ 台账勘误依据。

**rootfs 相零扰动复核（just smoke）**：八跑全绿 + ROOTFS_MARKERS 46 零变化；
负向复核：rootfs 相日志 `grep -c "GFX"` == 0；`eco: fb absent` 仍恰一行。

**QEMU 串行纪律**：smoke-image / smoke / run-efi 主会话串行收割，先 pgrep qemu 确认无并行；
TCG 时率注记（规则/08）：软 vsync 16.67ms 阻塞应答壁钟放大属预期，GFX ok 时延不设断言门。

## §6 实施编排骨（impl 拆步 → review → fix → 主会话收割）

- **S1 库面**（#1+#2）：guiproto parse + libc 访问器/注释。验收线 = just test-host 绿 + just build 绿。
- **S2 服务端+分发**（#3+#4+#5）：svcmgr 双 arm + guifbd 服务端真身。验收线 = build-user 绿
  （smoke 中间态 67/67 由主会话终验——agent 不跑 QEMU）。
- **S3 消费者+产链**（#6+#7）：gfxdemo crate + justfile 两行。验收线 = build-user 绿
  （musl 链过 = berry-gfx 设备面首消费编译证）。
- **S4 断言面开灯**（#8）：smoke-image.sh 标记。验收线 = shell 语法 + 标记算术复核。

**review 四维**（对全 diff）：
1. 能力/rights：handle take/close 全路径——svcmgr Err 面 / guifbd **拒收面（A3-b）** / RELEASE
   重挂 / gfxdemo 归还即关 / hdi 槽覆盖；SUBMIT 随行必是 dup 别名且 rights ⊆ 源集；无全局权限位。
2. 恰一次/标记：两新标记单发结构、互非子串、ECO 不进 assert_once 面；诊断行不进断言数组。
3. wire 合规：应答恰形恰长（**1B 烘焙线：非 OK 应答恒 1B 不带数据**）；两条 ST_BAD 立法在位；
   ST_FULL 用于 credits/slots；SUBMIT_F_UNSQUASHABLE = v1 死路径（永不判弃即合规）。
4. 结构面：三路轮询序 + deadline min 进调用（A1-a）+ nb/nh 重置（A1-b）；console 恒同步直写
   （A2-a）；bsh 链/env 布局零触碰；PS 零注册；guifbd 恰 8 / gfxdemo 6 句柄；extras 恰 4 枚
   （第 9 插 = CHAN_HND_MAX 抬帽另刀——review 卡死）。

**fix**：按维度回写原步重跑验收线；wire 类发现（想加字段/改应答形）不修——升级主会话按 ADR #54 重裁。

**主会话收割**（QEMU 串行队列）：just test-host → just smoke-image（69/69 + 恰一次）→
just smoke（八跑 + rootfs 46 + grep GFX==0）→ just check → 文档收口（本稿状态行 +
阶段沿革追记 + 规则/07 加 GFX 两标记瘦行 + 雷册 §C 判例 + GUI-4段② §7-⑤ PS 台账勘误）→ 提交。
提交拆分裁量：S1 / S2 / S3+S4 三拆或两拆，随验证实况定。

## §7 开放问题 → 拍板定案（2026-11-02 设计 workflow 六工 + 主会话拍板；依据全文存档 §9）

1. **端点分发题 → 裁 b-2（svcmgr 持端点 + 预拉链直递）**：svcmgr 在 guifbd 预拉段
   channel_pair()×2，服务端端随 guifbd extras（2→4，恰 8/8），客户端端随 **gfxdemo 预拉 arm**
   直递作其 env extras（fresh 端直递非 dup——fsmgr fs_b/net_b 先例）。否决链：b-1 经 bsh
   三重死（bsh env 8/8 恰满 + bsh 只能 dup 自表 handle + 槽位撞 bsh 自验门）；a 鸡生蛋且
   intent 借入链断链；c 机制零存在（fsmgr 十 op 无返 handle 面）且自举递归。先 b 后 c 是
   **依赖序非妥协**——c（路径式命名空间）留 GUI-5+ 独立 ADR 刀。零内核改动、bsh 面零触碰。
2. **hdi v1 裁剪面 → 四 op 全应答退化形**：REG_VBLANK 记账存 Event；VBLANK_WAIT parked 到
   软 vsync 点一次性应答；PREPARE 槽记账 + 空置 fence；COMMIT 记账。wire 零改动；唯一库缺口
   parse_vblank_wait_req = 库代码非协议（guiproto :1098-1099 自注背书），本刀补全。
3. **frame_callback 时刻表 → 组合案**：FRAME_CALLBACK 完成即答（presented_ns=clock）；
   「未来时刻表」整体走 hdi 域 VBLANK_WAIT（ts_ns = clock 合成软 vsync 点）；定时器 = 阻塞应答
   隐式持有 + wait_many deadline 臂到点触发。**不内嵌 next_target_ns**——FrameDoneRep 16B 全
   分配无空位（guiproto :796-842），wire 改动禁入本刀；此解读链落册防 GUI-6+ 再议（R1 备注⑥）。
4. **console FULL 自证 → 加 `GFXF ok n=` 第二标记**：console_full_n 计数 + 共享 present_seq
   谱系 + 两条 ST_BAD 立法；**恒同步直写纯记账**（A2-a）；不动 FBSERVE ok 文案。
5. **gfxdemo 内容/断言 → 确定性 DisplayList + 双 guest 侧标记链**：内容 = Clear 底色 + fill
   rect + glyph 串（Bgrx 同源免转换）；K=2 damage rect 验裁剪路径；断言 = guifbd px() 抽样 7
   点对拍 vmo_read（交叉核对强于复算期望）+ gfxdemo 全链断言；宿主侧像素断言无通道不做；
   golden 整图对拍挂 SDK-1 不进本刀。
6. **编译链/挂账 → 全部预期成立**：gfxdemo 新 crate 照 guifbd 产链两行；PS 零注册（thr 先例）
   ——roster 实测 EFI 相 **15**/16、rootfs 相族 **13**/16（正片实测；底稿「EFI 13/rootfs 15」
   系两相对调错值）；guifbd 恰 8/8、gfxdemo 6/8 句柄；PROCESS_MAX
   =64 距离远。GUI-4段② §7-⑤「rootfs 相恰满 16/16」与实测 13/16 矛盾——勘误见 §5/§9。
7. **（并入 ①）** 8. **gui-wm（GUI-5）分界记录**：本刀 gfxdemo 直连 present 域 = 单客户端
   退化面；GUI-5 gui-wm 进场后 gfxdemo 改连 wm——协议同 op 不变，分发端点换手。挂账防丢。

## §8 scout 结论存档（2026-11-01，免下会话重走）

- berry-guiproto lib.rs（2185 行）public 面全核：PresentReq（Submit.rects `[Rect; 帽]+len`）/ GetinfoRep（version_present/input/hdi 三域 + format_bits + max_credits ≤ CREDITS_MAX）/ FrameDoneRep / ReleaseRep / input 族 / hdi 常量（OP_REG_VBLANK 0x40..COMMIT 0x43，HDI_SLOTS_MAX=2）。三域 = 三对独立 channel（能力即权限）。
- guifbd main.rs（222 行）全核：gift2 = fbinfo VMO（40B 五域 u64 LE）、gift3 = console pipe；自验三步 → FBW ok → serve 循环（recv 256B / EAGAIN→wait READABLE 10s 重臂 / 非 EAGAIN trap / 首条 FBSERVE ok / put_str lossy）。
- wait_many：NR 0x05 在册 + berry_object_wait_many（berry-sys lib.rs:130）+ libc 已有消费先例（io.rs:176 stdin 并监）。
- guifbd 只在 EFI 链活（boot main.rs fb 授能块 EFI-GOP 门）；smoke-image 现标记 67（EFI 十段+接续+裸相 27+生态 27+RX+对拍）。
- GUI-2 段①②（main 098d01b/5317e09）：berry-gfx 3082 行 12 模块（caps/cmd/color/font×4/icon/lru/pass/queue/raster）——pass 形态 API + golden 回归 host 面全备，设备面零消费。
- ADR #50-54 五行全读；规划 :80/:87/:88/:102 四锚全读。

## §9 设计 workflow 批判吸收清单（2026-11-02，六工：A1/A2/A3 调研 + 综合 + R1/R2 对抗）

**总裁决**：R1 = 0 阻断/1 应修/6 备注；R2 = 0 阻断/7 应修/6 备注——全部吸收进 §0/§2/§3–§6。
关键修订映射（实施/review 时逐条对照）：

| 发现 | 级 | 吸收落点 |
|---|---|---|
| R1 应修-1 SUBMIT_F_UNSQUASHABLE 未入 squash 语义 | 应修 | 与 R2 A3-a 一并消解：v1 钉死 blit-at-SUBMIT，squash/DISCARDED 分支 = 死路径留异步后端刀，永不判弃即旗标合规（§0-5/§3#4） |
| R2 A1-a parked vsync 点须进 wait_many 调用本身 | 应修 | §3#4③ + §6 review dim 4 断言面 |
| R2 A1-b 三路 nb/nh 容量每轮重置 | 应修 | §3#4③（漏重置 = 恒 -EINVAL 死循环） |
| R2 A2-a console 须立法恒同步直写、FULL 仅记账 | 应修 | §2 关键地形 + §3#4⑥（误读形 = 文本路 +16.67ms/队深 8 打满/屏显冻结） |
| R2 A3-a 状态机文本矛盾（blit-at-SUBMIT vs pending squash） | 应修 | §0-5 择一钉死：sync-blit，pending 槽仅 token 记账 |
| R2 A3-b 拒收路径须 close 已迁入 handle | 应修 | §3#4④ + §6 review dim 1 |
| R2 A5-a channel_pair()×2 建点钉 fb 探测门 else 分支内 | 应修 | §3#3①（门外 = rootfs 相白建对象 + 恒泄漏） |
| R2 A6-a「RPC 无超时挂死 boot 链」记述错误 | 应修 | 机制更正：fs_rpc 收侧 10s/90s 帽 + reap 30s 帽在册；gfxdemo 采收侧帽形（§3#6）——死法 = 标记缺失 FAIL loud 非挂死 |
| R1 备注① libc netd_h/input_h「独占槽」doc 失准 | 备注 | §3#2 一并同步 |
| R1 备注② §0-1「双路」措辞 | 备注 | 已改三路（console pipe 实为 channel——R2 A1-c 澄清） |
| R1 备注③ REG_VBLANK(0) 关闭形随行 Event 歧义 | 备注 | 裁定 = close 存量 + close 本次随行（§3#4⑤，同 R2 A3-d①） |
| R1 备注④ mkcpio 字典序锚不精确 | 备注 | 实序 fsmgr<gfxdemo<grep<gui-inputd；mkcpio 内部 sorted 无实害——§3#7 改准 |
| R1 备注⑤ PS 台账矛盾（GUI-4段② §7-⑤ 记 16/16 vs 实测 rootfs 15/16） | 备注 | §5 smoke 时取 `PS ok n=` 实值对账后勘误 GUI-4段② 草案 |
| R1 备注⑥ 「未来时刻表」v1 解读链落册 | 备注 | §7-3 已载（wire 无位的推理链，防 GUI-6+ 再议改 wire） |
| R2 A2-b 禁断言精确 present_seq | 备注 | §3#4④ |
| R2 A3-c/d 双缓冲别名合法 + 三处小生命周期 | 备注 | §3#4⑤（槽复用覆盖 close、第二 VBLANK_WAIT = ST_BUSY） |
| R2 A4 guifbd 有堆，no_std/零 alloc 是库纪律 | 备注 | §3#4⑧ 措辞改「静态定容 = 工程选择」 |
| R2 A5-b 抽样点须全落 damage 并集 + 恰一次人工复核同法 | 备注 | §3#4④/⑦ |
| R2 A6-b PREPARE 须再 dup 新别名（send 即 take） | 备注 | §3#6 |

**风险面十条**（主会话收割时逐条对照）：wire 改动需求 = 零（任何「应答加字段」滑向 = 停刀升 ADR
#54 重裁）；guifbd env 恰 8/8 零余量（第 9 插 = CHAN_HND_MAX 抬帽四处联动另刀——review 卡死
extras 恰 4）；PS 相位余量（勘误后真值：EFI 相 **15**/16、rootfs 相族 **13**/16——零注册解；底稿
对调错值已由 §5-⑤ 实测取代）；软 vsync 停车死状可被 smoke
抓（标记缺失）+ parked 超期路径留 eprintln；TCG 时率放大属预期；rootfs.cpio 增容字节对拍自洽；
QEMU 串行收割；bsh 手跑恒降级是设计内非缺陷；ramfb 格式面 Bgrx 既有依赖记录在案。

三路调研底稿与批判全文存 workflow transcript（run wf_57d970c3-74e，journal.jsonl）——本节为
执法摘要，锚点复走以底稿为准。
