# GUI-5 实施设计草案（gui-wmd 窗口守护——wm 域协议 + 权威窗口表 + 命中分发）

> 版本：**1.0**（v1.0）｜2026-11-06

> 状态：**已收官**（2026-11-03——设计定稿 2026-11-02 后实施 workflow 九工
> wf_b4e9a409-273 全绿（S1 库面→S2 gui-wmd/svcmgr/libc→S3 inputd/gfxdemo→S4 开灯
> →四维审查→修复工）+ 主会话终验全绿：test-host 0 fail / smoke-image 70/70 恰一次
> + PS ok n=16 + kfile=3794640B/init=32245080B 字节对拍 / smoke 八跑零漂；审查抓
> R-wire 阻断 TABLE_WAIT 回声活锁一刀修复（§7 Q11-③ 在途门控注记）；收割期另破
> kfile 假 FAIL 判例（雷册 §B29 时序踩踏形态 + smoke-image.sh 基线入口快照加固）。
> 执行结果详见文末 §10）。
> 前置：GUI-2 段③ 已收官（`9ccf87a`——guifbd present/hdi 服务端真身 + gfxdemo 直连
> 单客户端退化面）；GUI-4 段①② 已收官（guiproto 三域 wire + inputd 接线）；
> ADR #61 已落册（`ce05b6e`——gui-wmd 形态终裁：独立常驻守护，呈现/窗口策略分离）。
> 钦定源：`docs/GUI里程碑规划-2026-10-v1.0.md` :91 GUI-5 行 + ADR #61（`docs/架构决策.md` :70）。

## §0 范围（本刀做 / 不做）

**本刀 = ADR #61 的落地刀**：gui-wmd 独立常驻守护进场，持权威窗口表（两层窗 + Z 序 +
避让 + parent/group + 焦点）；wm 域协议进 berry-guiproto（host 测编解码先行）；
客户端改连 wm 域（协议同 op 不变、分发端点换手——GUI-2 段③ §7-⑧ 分界记录兑现）；
窗口表→inputd 推送契约落地（命中分发查 wmd 推送快照）。

**做**：
1. **berry-guiproto wm 域 wire 定形**：0x60..=0x7F 互斥带六 op（GETINFO/CREATE/DESTROY/
   SET/TABLE_GET/TABLE_WAIT）+ VER_WM 自带版本面（零触碰既有字节全 add-only）+
   窗口表快照帧（12B 头 + 16B 表项 ×15 ≤252）+ wm_hit_test 纯函数单源 + 13 条测试义务；
2. **gui-wmd crate**（新建，三模块 main/table/relay）：权威窗口表（有效几何不变量式
   避让推导）+ present/hdi 双域透传中继（分发表执法 + 帧字节级零观测差）；
3. **svcmgr wmd arm**（EFI-only 门）：channel_pair×2（wm 对 + wmd↔inputd 推送对，
   建点钉门内）+ present/hdi 客户端端换手（gfxdemo → wmd，guifbd 零改动）+ ps_reg
   ALIVE + Err/cpio 缺分支 handle 显式回收；
4. **psproto 抬帽 16→17**（ADR #61 义务①；S1 先于/同提交 S2 硬序）；
5. **inputd 窗口表消费**：serve 两路 poll-first + wait_many n=1|2（wm_push_h()≠0 门控，
   ==0 现路径逐字节保留）+ 快照存储 + 命中内存旁注 + 超龄三层失效立法；
6. **gfxdemo 改连 wm 域**：present 六调用点 + hdi mini 序五连发保序经 wm 中继换端
   （op 编码与断言零改动）——GFX/GFXF 标记语义零漂移；
7. **smoke 标记**：单标记 `WM ok`（EFI 相；ECO 29→30、ALL_COUNT 69→70）。

**不做**（挂账）：
- 布局/窗口状态持久化（ADR #61 义务② = 形态前置记录非必做——形态前置三件成文
  见 §7 Q-义务② 档位行；GUI-5 无持久化需求 + fsproto 严格串行无请求号 → 常驻双读者
  = 应答跨客户错配结构性风险 → **wmd 不装 fs 端**，位序留 GUI-8）；
- 多客户端 SUBMIT 窗口 token 在册执法（多客户端刀；guifbd 零改动是本刀的地形依据）；
- latch/squash 落 guifbd（§7 Q6 明文另刀 + 沿革追记勘误 GUI-10 :105）；
- berry-launcher wiring（并线后 6c 接刀——wm 域端点就位即满足其前置）；
- PM 交互脉冲（GUI-6+；inputd :614-618 占位注释口径随本刀更新为三候选点）；
- hdi 域多客户端化 / virtio-gpu 后端 / Z 序动画 / smoke-image 加 virtio-tablet
  （Q14 裁不加——命中分发钦定走 host 测，M-GUI 主验收另逼 recipe）。

## §1 立项依据（钦定源）

- **规划 :91 GUI-5 行**：两层窗 + Z 序表 + 避让保留；WindowInfo 带 parent/group 字段
  （QNX 缝）；hit test 归 wmd/合成侧立法（「屏上有什么」只存在于合成侧——防 clickjacking）；
  窗口表→inputd 推送契约进 berry-guiproto；验收线 = 协议 crate 测 + 命中分发 host 测 +
  gui 联调标记；依赖 GUI-3/4（均已收官）。
- **ADR #61**：独立 gui-wmd 常驻守护（guifbd 纯呈现域零演进污染）；权威窗口表；客户端
  改连 wm 域（协议同 op 不变，分发端点换手）；hit test = wmd 权威 + 呈现侧最后防线；
  端点沿 b-2 延伸；连带义务三条（psproto 抬帽 16→17 / 持久化走 fsclient（形态前置
  记录）/ wire 遵协议面纪律）。
- **ADR #50**（协议纪律字面真源——「#54=协议纪律」惯用引勘误见 §8-①）：per-interface
  version 随定形刀必落；host 测编解码先行、消费者随刀接线。
- **GUI-2 段③ §7-⑧**：gfxdemo 直连 = 单客户端退化面；本刀改连 wm——端点换手。
- **PS 台账**（勘误后真值）：EFI 相 15/16、rootfs 相族 13/16——wmd 入册后 EFI 相
  16/16 **恰满帽内不丢**（ps_find_slot 判 `*n < PS_MAX` 追加，:1224）；静默丢仅在
  第 17 行（EFI 实值若漂 +1——rootfs 差 1 未定位的同源不确定）——S1 抬帽先行同时
  兜住两种实值（R2 备注⑥ 校准，义务①硬序不变）。

## §2 拓扑与通道（定稿）

```
（EFI 链，wmd 进场后）
gfxdemo（预拉）                gui-wmd（新常驻）                guifbd（零改动）
  │ wm 域 channel ─────────→  │ 权威窗口表 + hit test 立法      │ present 六 op
  │  WM_GETINFO/CREATE/       │ present 客户端端 ───────────→  │ hdi 四 op
  │  DESTROY/SET/TABLE_GET    │ hdi 客户端端 ──────────────→  │ （wire 逐字节同
  │  + present 六 op 经中继   │ （中继分发表：present 1..=6    │  gfxdemo 时代）
  │  + hdi 四 op 经中继       │  →present_cli；hdi 0x40..0x43  │
  └──────────────────────────┤   →hdi_cli；wm 0x60..=0x7F→本地；
      （push 对，wmd=server） │   其余一切 op→1B ST_BAD）       │
                             │ ←─ WM_TABLE_WAIT(last_seen)：   │
                             │    park 至 version>last_seen    │
                             ↓    应答全表快照 ≤252B
             gui-inputd（serve 两路 poll-first + wait_many n=1|2）
               ① input 域（既有——SUBSCRIBE/INPUT_POLL 零扰动=泵唯一 drain 者）
               ② wm 表推送通道（client 角色；快照存储 + wm_hit_test 内存旁注）
svcmgr 装配（b-2 延伸，EFI-only 门=present_cli≠0 ∧ cpio_find bin/gui-wmd）：
  wm 对 + 推送对 = channel_pair×2（门内 spawn 前）；推送对 inputd 端存 static
  WMD_PUSH_B（NET_A/INPUT_A 同形「留表即留命」）；present/hdi 客户端端换手 wmd arm。
  rootfs 相：fb 缺席 → guifbd 门跳 → present_cli=0 → wmd arm 静默跳过 →
  WMD_PUSH_B 恒 0 → inputd extras 仍单枚 → 46 标记面逐字节零漂移。
```

env 预算（base 4 = console/stdin/proc/vmar）：wmd extras=[wm_srv, wm_push_srv,
present_cli, hdi_cli] 4 枚 = **恰 8/8 零余量**（Q8 裁 hdi 同中继；Q12 冻结注记进头注，
第 5 插 = 内核 CHAN_HND_MAX 四处抬帽另刀）；gfxdemo extras=[wm_cli] 1 枚 = **5/8**
（勘误：§2 骨架原写 6/8 系 4+1 算术错）；inputd extras 1→2 = 6/8（rootfs 相仍 1 枚
5/8）；guifbd 8/8 不动；bsh 8/8 硬墙零触碰（wm 客户端端永不入 bsh）。

## §3 逐文件落点表（定稿——设计 workflow 综合 + R1/R2 修正全吸收）

| # | 文件 | 动作 | 改什么（精确段） |
|---|---|---|---|
| 1 | user/berry-guiproto/src/lib.rs | 大增（纯 add-only，既有三域字节零触碰） | ①:8 头注同步（present 域消费者改 gui-wmd 代理）+:16 后追加 wm 域条目（两对 channel 共享一 op 空间/库内首个带参阻塞请求立法）；②op 常量（:76 后）：OP_WM_GETINFO=0x60/CREATE=0x61/DESTROY=0x62/SET=0x63/TABLE_GET=0x64/TABLE_WAIT=0x65；③值常量：WM_LAYER_APP=0/SYSTEM=1；WM_CREATE_F_AVOID_SYS=1<<0；WM_SET_F_GEOM/VIS/Z=1<<0/1/2；WM_VIS_HIDE/SHOW=0/1；WM_WIN_F_VISIBLE=1<<0/FOCUSED=1<<1（未知位双侧拒）；④帽：WM_WINDOWS_MAX=15/WM_REQ_MAX=20/WM_REP_MAX=252 + VER_WM:u8=1（VER_HDI 旁）；⑤const assert 六条（:1260 后追加，含 12+15×16==252 与 ≤256）；⑥WmEntry{id,parent:u16,group:u8,x/y/w/h:u16,layer,z,flags:u8}（doc 注 x/y/w/h=**有效几何域**）+ WmSnapshot（derive 五件套 :39-47 惯例）；⑦编解码对十组（None 契约/恰长执法/值域对称拒——帧形见 §7 Q1/Q2/Q5/Q15/Q17；TABLE_GET/TABLE_WAIT 共用快照应答 encode/parse_wm_snapshot_rep，count=0 合法空表 + 三态分离解析 INPUT_POLL :1029-1070 形）；⑧wm_hit_test 纯函数（doc 注单源禁两处手写：F_VISIBLE 置位参选、layer 降序→z 降序→点入、平局表序靠前、w/h=0 恒不中）；⑨测试 13 条（roundtrip/golden（改布局必炸注）/截断尾随拒/值域相异值矩阵/帽边界（count=0 合法钉）/烘焙线三态/保留位 0xEE/错 op 混淆（拿 0x01/0x20/0x40 解析必 None）/assert 契约/LE/None 契约 + const_frame_caps_pinned 追加；:1262 tests 内追加既有行零改动） |
| 2 | user/berry-psproto/src/lib.rs | 小增（抬帽 17） | MAX_ENTRIES 16→17（:53）+ 名单注释枚举加 gui-wmd（7B≤NAME_MAX=8）+ REPLY_MAX 公式自动 240（:57-58 断言仍过）+ reply_full_table_boundary 17 臂名表（:313 `15=>b"svcmgr15", _=>b"svcmgr16"` 保 8B 恰满——重名不炸测试纯语义修正，R2 备注④校准）+ reply_mixed 测试加 (b"gui-wmd",STATE_ALIVE,0) 行钉真实名 + :294/:337/:368 注释字面同步 |
| 3 | user/bins/gui-wmd/Cargo.toml | 新建 | [package] gui-wmd 0.1.0 + GUI-5 头注（形态/拓扑/零 fs 端/**env 4 extras 恰 8/8 冻结注记（Q12）**）+ 空 [workspace] + path 依赖恰三枚 berry-libc/berry-sys/berry-guiproto（无 berry-gfx——wmd 不光栅化）+ [profile.release] z/lto/cgu=1/abort/strip（guifbd/gfxdemo 逐字同形） |
| 4 | user/bins/gui-wmd/src/main.rs | 新建 | CRT0_SEED 锚（guifbd :72-73 同款）+ 降级门（四访问器 0 值 = eprintln + **park 非 exit(0)**——svcmgr 已 ps_reg ALIVE，exit 留永久 ALIVE 谎账）+ 启动序五步（①收割四端 ②present 首发 ReleaseWait 清前世孤儿别名 :474-496 语义 ③present GETINFO 缓存几何 ④表初始化空表 version≥1 ⑤诊断行 `gui-wmd: up` 转 serve）+ serve 两路 poll-first（wm_srv→push_srv→全 EAGAIN 才 wait_many n=2，WMD_IDLE_CAP_NS=10s）+ nb/nh 每轮重置（:323-343 纪律）+ 头注结构注释：单 wm 客户端串行下中继期间表不可能变 = 零新鲜度损失（防后刀第二客户端踩雷） |
| 5 | user/bins/gui-wmd/src/table.rs | 新建 | 静态定容（guifbd Srv :205-246 先例）：entries:[WmEntry;WM_WINDOWS_MAX]+count+focus:Option<WindowId>+version:u32（Q3 编码）+ parked:(Option<Handle>,last_seen) **单车位语义钉死：后到 WAIT 现答现快照并覆盖 parked（不排队）；表变更向当前 parked 应答全表快照并清空**（R2 备注⑦注释钉）；**有效几何不变量式避让（R1 应修②）**：任何 CREATE/DESTROY/SET（含系统层窗变更）触发受影响表项重算（N≤15 O(N) 零成本）——命中只见有效矩形；wire 语义钉板：表项携有效几何，申请几何不回显 |
| 6 | user/bins/gui-wmd/src/relay.rs | 新建 | present+hdi 双域透传中继。**分发表执法（R1 应修①）**：op∈{present 1..=6}→present_cli 透传；op∈{hdi 0x40..=0x43}→hdi_cli 透传；op∈{wm 0x60..=0x7F}→wm 域处理（push 对只收 TABLE_WAIT，其余帧 1B ST_BAD）；其余一切 op（含 input 0x20/0x21 错投）→1B ST_BAD；随行 handle 数与两域协议定值不符 → ST_BAD+close。**失败面（R2 应修③）**：中继 recv 帽 10s 量级，超时/持续 EBUSY/应答随行数超窗 = trap()（fail loud——gfxdemo 10s 帽 + inputd 超龄降级 + GFX 缺三重兜底链注记；禁 wmd 合成 present 域应答）；recv 窗 [u8;256]+[Handle;8]（gfxdemo :190-191 同形）。GETINFO 逐调用中继（Q15——真值源恒 guifbd）；RELEASE=ReleaseWait 长 park 界说明（boot 期清账即答） |
| 7 | user/bins/svcmgr/src/main.rs | 中增（wmd arm+换手） | ①:243-249 区新增 static WMD_PUSH_B:AtomicU32（NET_A/INPUT_A 同形 +「留表即留命」注释——EFI 相 inputd 不拉起恒滞 static 属覆盖面）；②guifbd arm(:403-477) 与 gfxdemo arm(:478-542) 之间插 wmd arm：门=present_cli≠0 ∧ cpio_find(b"bin/gui-wmd")→门内 channel_pair()×2（options=0 借链激活，A5-a 建点钉门内，intent 注记照 inputd arm :677-682）→spawn extras=[wm_srv,push_srv,present_cli,hdi_cli] 4 枚 stack_pages=8 stdout=console→Ok：ps_reg(b"gui-wmd",STATE_ALIVE,0) 不 reap + WMD_PUSH_B.store；**Err 面（R2 应修②显式枚举）：close(wm_srv)+close(wm_cli)+close(push_srv)+close(push_cli)+close(present_cli)+close(hdi_cli) 六枚**（**实施修正：present_cli=0/hdi_cli=0 置位弃**——§3#7 原文钦定置 0，落地即 4 条 unused_assignments：换手后两柄已随 spawn 移交无读者，死存编译器即证；GUI-2段③ close-only 先例同形） + ps_reg SPAWNFAIL + eco_skip；**cpio 缺分支（R1 应修④）：close(present_cli)+close(hdi_cli) 两端 close-only（:531-541 gfxdemo cpio 缺分支同款；置 0 死存弃同上）**+零新行；③gfxdemo arm 改线：门 present_cli≠0→wm_cli≠0、extras [present_cli,hdi_cli]→[wm_cli]；④inputd arm(:673-731)：extra_gifts=[in_b]→[in_b,push_b 非 0 追加]（0=现行为逐字节不变）；⑤家名单(:1450-1466)不添 wmd 行（guifbd 不在册先例） |
| 8 | user/berry-libc/src/io.rs | 小增（六访问器） | 同槽多产惯例：wmd 侧 wm_srv_h/wm_push_srv_h/wm_present_h/wm_hdi_h 四（extras 0-3）+ 消费侧 wm_cli_h（gfxdemo extras[0]，同槽顶替 present_h）/wm_push_h（inputd extras[1]）；present_h() 保留 doc 注 GUI-5 起 gfxdemo 改 wm_cli_h |
| 9 | justfile | 小增（产链两处，随 S2 落地） | build-user 段 gfxdemo 块(:205-207)后追加 gui-wmd 构建行（--target aarch64-unknown-linux-musl + RUSTFLAGS berry-ld 链）；mkcpio 条目插 bin/gui-inputd(:255) 与 bin/guifbd(:256) 之间（字典序 gui-inputd<gui-wmd<guifbd） |
| 10 | drivers/bins/gui-inputd/src/main.rs | 中增（两路 serve+快照） | serve(:595-684)：poll input_h（SUBSCRIBE/INPUT_POLL 分派零扰动）→poll push（WAIT 应答到达：recv 快照→存储+age 打点→立刻重臂 send WAIT(new_version)；EBUSY 重臂 10s）→全 EAGAIN 才 wait_many n=1|2（wm_push_h()==0→n=1 现代码逐字节保留）deadline=SVC_TICK 100ms；**serve_poll(:555-589) :585 步进睡改形必须 wm_push_h()!=0 门控（R2 阻断①）**：非 0 时 thread_sleep 改 wait_many([push_h],deadline=min(now+1ms,poll_deadline))；==0 时 :585 现形逐字节保留；快照静态存储+version+age，超龄 10s→WM_TARGET_STALE 哨兵（记账不命中）+每 10s 重发 WAIT(0)；drain 后 to_wire 前逐事件 wm_hit_test **内存旁注**（Q16——INPUT_POLL 恒为泵唯一 drain 者，旁注非第二消费）；:614-618 PM 占位口径更新（候选点三）；:83-86 SCREEN_W/H 注释降级为无快照态兜底（快照头几何域对齐） |
| 11 | drivers/bins/gui-inputd/src/lib.rs | **不动（零 diff=显式裁决）** | hit_test 单源驻 guiproto（Q9）；若实施发现需薄包装必须 #[inline] 直调单源并 host 测同钉（草案骨架 #6 原行修正） |
| 12 | user/bins/gfxdemo/src/main.rs | 中改（wm 改线全量） | ①:34 import 改 wm 端访问器（present_h/hdi_h 删——两端随 svcmgr 换手归 wmd）；②:175-188 双降级门改 wm 单门（`gfx: wm absent` 一行诊断+exit(0) 永不进 MARKERS；hdi absent FAIL 门删——hdi 随 wm 中继）；③新增 wm 域往返块（插 present GETINFO :196 前）：WM_GETINFO（VER_WM=1+windows_max=15+几何回显）+CREATE ack id≠0+SET 1B OK+TABLE_GET 回显四组断言→单 println `WM ok`（TABLE_GET 回显与 CREATE 请求几何比对——注记：v1 单窗无系统层重叠有效几何==请求几何，GUI-8 加系统窗时本断言须随改，R2 备注⑧）；④present 六调用点(:196-359)换 wm 通道（op 编码与应答断言零改动）；⑤hdi mini 序(:361-414)保序换端（五连发 :372/:375/:396/:407/:414 首参换 wm 端，断言全保）；⑥:416-419 GFX ok 打点零改动；⑦头注 :1-26 重写（wmd 中继拓扑+WM ok 语义）。不动面=rpc 助手 :106-155+VMO 三连/渲染 :212-283 |
| 13 | tools/smoke-image.sh | 小增（开灯+对账） | :189 ECO_MARKERS 尾追加 `"WM ok"`（GFXF ok n= 后纯追加压并线冲突）；头注 :177-188 后追加 GUI-5 段（WM ok 语义/恰一次结构/中间态永不进 MARKERS/wmd 一 spawn ≈+5-10s TCG 360s 帽不动）；:119-121 PS 注释加对账句（EFI 15→16 恰满旧帽帽内不丢——S1 抬帽先行兜住实值漂 +1 角；rootfs 13 不变）；计数字面 29→30 三处（:36/:305/:377；ALL_COUNT :288 数组自算零改） |
| 14 | tools/smoke.sh | **不动（零 diff=显式裁决）** | rootfs 相 46 标记零变化；负向对账（rootfs `WM ok` 恰 0 次 + PS ok n= 实值==13）= 收割期读日志手动步进 §5 验证表，不进 smoke.sh 码（改动归主线串行，规划 :97——少一处并线冲突热点） |
| 15 | user/bins/guifbd/src/main.rs | **不动（零 diff=显式裁决）** | ADR #61 零演进污染 + Q6 latch/squash 明文另刀 + GFX/GFXF 零漂证明的构造前提；**收割期 diff 此文件出现任何改动 = 即刻停刀核对是否并线带入**（R1 备注③——并线对照锚） |

契约顺序：#1/#2（S1 库面）→ #3-#9（S2 wmd+装配+产链）→ #10-#12（S3 inputd+gfxdemo）
→ #13（S4 开灯）。**硬序：S1 先于/同提交 S2 合入**（wmd ps_reg 在帽 16 下 EFI 实值若
漂 +1 即表满静默丢——抬帽兜住两种实值）。

## §4 标记表（定稿——单标记）

| 标记 | 相 | 打点者 | 恰一次语义 |
|---|---|---|---|
| `WM ok` | 仅 EFI | gfxdemo（wm 客户端往返四组断言：GETINFO 三断言/CREATE id≠0/SET 1B/TABLE_GET 回显） | 结构性恰一次：svcmgr 单次预拉 + 阻塞 reap + 单 println（GFX ok :416-419 同款）；ECO_MARKERS 尾追加不进 assert_once（phase-2 仅 EFI_FIXED+CONT+ERE——GFX ok 同待遇）；裸 grep -qF；互非子串已审（最近形 BENCHWAKE n=/MANY ok/WAIT ok 均不含） |
| `gfx: wm absent`（诊断行） | 手跑形态 | gfxdemo 降级门 | 永不进 MARKERS 数组（netd 双门先例） |
| `gui-wmd: up`（诊断行） | EFI | wmd 启动序⑤ | 前缀统一小写防误配 WM ok；永不进 MARKERS |

算术：4+6+3+27+**30** = **70**（ECO 29→30）。双标记 WMF 否决（Q13）：EFI 相 inputd
未拉起（svcmgr H_MMIO 门先跳，家名单注释 :1451-1453 在册）⇒ 恒无发送者 = 断言面永空
死标记。wmd 存活性由 PS roster 行证、功能性由 WM ok 证、present 真值已有 GFXF——
分诊正交：WM 缺 = wm 域炸；GFX 缺 = 中继/present 链炸。

## §5 验证面（定稿）

**host 测试（just test-host）**：
1. guiproto wm 域 13 条义务全绿（帽钉板/roundtrip/黄金字节/截断全拒/值域相异值/帽边界
   count=0 合法/烘焙线三态/保留位 0xEE/错 op 混淆/should_panic/LE/None）+
   wm_hit_test 五组 host 测（空表 None/表边界/Z 遮盖/避让带/陈旧纪元记账不命中）；
2. psproto 抬帽绿（boundary 17 臂恰满 + mixed 加 gui-wmd 真实名行）；
3. 既有面回归零漂移（guoproto 既有测试行零改动、berry-gfx 全绿不触碰）。

**smoke-image EFI 相（just smoke-image）**：
1. 70/70 到齐（DEADLINE_BOOT=360s 不动；wmd 一 spawn ≈+5-10s TCG，实测链 ~115s 余量足）；
2. `WM ok` 恰一次复核 grep -cF==1；既有 69 零漂移（GFX ok/GFXF ok n= 恰一次）；
3. PS 对账：EFI 相 `PS ok n=` 实值 == **16**（15+wmd 恰满旧帽帽内；若实为 17 则 S1
   抬帽已兜住——差异即 rootfs 差 1 同源定位线索，停刀对账）；
4. 阶段 3 字节对拍动态自洽（rootfs.cpio +bin/gui-wmd 变容自动跟随）。

**smoke rootfs 相（just smoke 八跑）**：全绿 + ROOTFS_MARKERS 46 零变化；负向对账 =
rootfs 相日志 `grep -cF "WM ok"` == 0；`PS ok n=` 实值 == **13**（wmd 不在 rootfs 相
活——EFI-only 门构造性保证）；`eco: fb absent` 仍恰一行。

**设备面断层预判句（Q14，进档防丢）**：WM_TABLE_WAIT 推送机制在一切现役相 smoke 覆盖
= 零（EFI 相 inputd 未拉起；rootfs 相 wmd 缺席）——host 测独扛；GUI-8/M-GUI 主验收
（点击交互）将逼 smoke-image 加 virtio-tablet 或 rootfs+ramfb 新 recipe。

**QEMU 串行纪律**：smoke-image/smoke 主会话串行收割，先 pgrep qemu 确认无并行（6c/dd
线避让）；TCG 时率注记（规则/08）不设时延断言门。

## §6 实施编排骨（impl 拆步 → review → fix → 主会话收割）

- **S1 库面**（#1+#2）：guiproto wm 域（op/常量/帽/结构/编解码十组/wm_hit_test/13 测）
  + psproto 17。验收线 = just test-host 全绿 + just check 绿。
- **S2 wmd+装配+产链**（#3-#9）：gui-wmd 三模块 + svcmgr wmd arm/换手/WMD_PUSH_B +
  libc 六访问器 + justfile 两处。验收线 = just build-user 绿（gui-wmd 产物入
  rootfs.cpio、字典序位正确）。
- **S3 inputd+gfxdemo**（#10-#12）：两路 serve（**serve_poll 门控——R2 阻断①复述：
  wm_push_h()==0 时 :585 thread_sleep 现形逐字节保留**）+ gfxdemo 改线。验收线 =
  just build-user 绿 + just test-host 绿（wm_hit_test 五组）。
- **S4 断言面开灯**（#13）：smoke-image 标记。验收线 = bash -n + 标记算术复核
  （ECO 30/ALL_COUNT 70）。

**review 四维**（对全 diff）：
1. 能力/rights：wmd arm **Err 面六枚 + cpio 缺分支两枚**显式回收逐枚核对（R2 应修②/
   R1 应修④）；中继随行 handle take-close 全路径；**relay 分发表**（R1 应修①）白名单
   外 op 恒 ST_BAD；无全局权限位。
2. 恰一次/标记：WM ok 单发结构/互非子串/ECO 不进 assert_once 面；诊断行不进断言数组。
3. wire 合规：1B 烘焙线（非 OK 恒 1B）；帧算术（12+15×16=252≤256/REPLY_MAX=240/
   PS_WIN 493≤512）；count=0 合法空表；值域未知位双侧拒；三态分离解析。
4. 结构面：两路 poll-first + nb/nh 每轮重置 + deadline 进调用；**serve_poll push_h
   门控**（R2 阻断①）；bsh 链/env 布局零触碰；wmd extras 恰 4（第 5 插 = 抬帽另刀
   review 卡死）；guifbd 零 diff（#15 行）+ inputd lib.rs 零 diff + smoke.sh 零 diff。

**fix**：按维度回写原步重跑验收线；wire 类发现（想加字段/改应答形）不修——升级主会话
按协议面纪律重裁（真源 ADR #50）。

**主会话收割**（QEMU 串行队列）：just test-host → just smoke-image（70/70 + 恰一次 +
PS 16 对账）→ just smoke（八跑 + 46 零漂 + 负向 grep）→ just check → docs 收口
（本稿状态行 + §7 Q6 沿革勘误 + 索引升格 + 规则/07 加 WM ok 瘦行 + 雷册 §C）→
三拆提交 → **立即并线**（main merge 进 gui3，吸收 justfile/svcmgr/smoke-image
ECO_MARKERS 冲突 → 推平 → 通知 6c 接 wiring 刀）。

## §7 拍板定案（2026-11-02 设计 workflow 六工 + 主会话拍板核准；R1/R2 修正已内嵌）

**Q1 op 带**：0x60..=0x7F 32-号互斥带（与前域等宽对称），v1 六 op 0x60..0x65；命名
`OP_WM_<动词>`（跟 input 域前缀式）。错投帧在任何既有域解析器 = None → ST_BAD
结构性防错投；反向（它域帧投 wm 通道）由 relay 分发表执法（#6）。
**Q2 版本域**：wm 域自带 OP_WM_GETINFO（req 4B 无参；rep 12B [st@0][version_wm u8@1]
[windows_max u8@2][r@3][disp_w u16@4..6][disp_h u16@6..8][r u32@8..12]）+ VER_WM=1
（VER_HDI 旁）。塞 GetinfoRep 保留位 = 协议面变更（黄金测+const assert 零漂炸弹）弃。
**Q3 推送形**：WM_TABLE_WAIT 携 last_seen（req 8B [op@0][r][r16][last_seen u32@4..8]
——**库内首个带参阻塞请求**，头注立法）；wmd park 至 version>last_seen 应答快照，
表已新立即答。version u32=(epoch u8<<24)|counter u24，counter∈1..=0xFFFFFF 跳 0
（version 恒≥1 ⇒ WAIT(0)=冷启动恒立即答）；epoch 取 wmd 启动 CNTVCT 派生 u8
（无持久化下唯一可实现的跨启动区分源）。**表述校准（R1 应修③/R2 备注⑥）**：跃迁
>旧 last_seen 为大概率非必然（u8 截断无单调性）；纪元回退角由 Q11-③ 超龄 WAIT(0)
重探测兜底（陈旧窗上界 = SNAPSHOT_STALE_CAP 10s），不依赖「必>」。
**Q4 门条件**：EFI-only（门=present_cli≠0 ∧ cpio gui-wmd 在）；两对建点钉门内
spawn 前；推送对 inputd 端 static WMD_PUSH_B 交接（arm 顺序物理约束：wmd arm
:478 先、inputd arm :673 后）；门外预建 = rootfs 相白建对象恒泄漏（A5-a）。两链都活
案否决：rootfs 相 wmd 无 present 可代理整体死面 + roster/inputd env 扰动，真命中验收
钦定走 host 测。
**Q5 表项与帽**：16B 定宽 [window_id u16@0][parent u16@2][group u8@4][x u16@5][y@7]
[w@9][h@11][layer u8@13][z u8@14][flags u8@15]；WM_WINDOWS_MAX=15（快照 12B 头+
15×16=252≤256，WM_REP_MAX=252）。transform/hotAreas 不取（日后 = VER_WM 升版+表项
重排非 add-only）；几何 u16 无负坐标（v1 裁窗口恒屏上，离屏动画属视图内变换——
GUI-10 挂账显式记录）；**规划 :234 WWM 类型查表参照裁语（R1 备注⑤）**：两层窗 v1
下 layer 0..=1 即类型优先级一维表；多窗连续 z 竞争面现形（GUI-8+）需服务端归一化时
走 VER_WM 升版，z 平铺客户端自选语义不先烧死。
**Q6 latch/squash 张力**：**明文另刀**——#61（刀前 ADR）位阶高于 GUI-10 调研刀序
建议；本刀 GFX/GFXF 零漂证明的前提是 guifbd 字节不动（同刀落 = 时序化改造零漂证明
坍塌）；latch/squash 大概率动 present wire = 停刀升 ADR 面。处置 = 本稿 §0 挂账 +
docs/阶段沿革.md 追记勘误（research/GUI-10 原文不编辑——调研=证据快照）。
**Q7 psproto**：17 不留裕（#61 义务①字面；18 = 无 ADR 修订的偏离）。boundary 第 17
名 = b"svcmgr16"（8B 保恰满断言）；真实名 b"gui-wmd"（7B）钉 mixed 测试。
**Q8 hdi 代理**：**经 wmd 中继**（推翻 D2 直连案）——§0 冻结做-2/做-3 字面「present/
hdi 客户端端换手 + 双域中继」；hdi 直连留例外 = 未来 launcher 出帧时 hdi 归属裂两主；
砍序案违 GFX ok 语义注记零漂铁律；中继案断言损失 = 零（仅 rpc 首参换端）。wmd 恰
8/8；gfxdemo extras=[wm_cli] 5/8。**dissent 保留（D2）**：8/8 复刻 #61 否决反模式、
GUI-8 fs dup 落第 4 枚免抬帽的窗口损失——届时（Q12）按 #61 范式重裁，3/8 备选复活。
**Q9 命中拓扑**：**B 直落**（否决 A 与过渡形）——inputd 持快照本地命中；注解 v1 =
内存旁注（不进 wire）；PointerEvent rsv u16 载体明文否决（违三域字节不动）；wm 分发
票面立为 GUI-8 前瞻形。wm_hit_test 单源驻 **berry-guiproto**（双端唯一天然共享依赖，
算法挨着 WmEntry 数据类型随 wire 同刀演进；inputd lib 驻地案否决 = wmd 引 drivers
bin crate 跨树倒挂伪单源）；inputd 需薄包装必须 #[inline] 直调。
**Q10 焦点/粘滞**：inputd 本地粘滞零逐事件 wire 流量——DOWN→wm_hit_test 锁定
pointer_id 目标窗；MOVE→全投锁定窗（跟手）；UP/CANCEL（含合成取消）→解锁；锁定窗
不在最新快照→丢弃至解锁不重新命中（防手势跳窗 clickjacking）；多指各自独立锁定。
焦点 = wmd 策略面（建/毁/SET_VIS 置 WM_WIN_F_FOCUSED，单焦点窗+归还，规划 :235）。
避让 = CREATE 携 WM_CREATE_F_AVOID_SYS，wmd 写**有效几何**进表项——不变量式重推导
（R1 应修②，见 #5）。
**Q11 快照失效三层**：①纪元（快照头 version u32）；②超龄回退——age>10s 快照视同
缺席，注解降级 WM_TARGET_STALE 哨兵（记账不命中不派发主张）；③超龄探测重臂——降级
态每 10s 重发 WAIT(0)（v1 send 入死通道无应答 = 惰性无害；监督者进场当天即活）。
心跳案直接违零主动 send 纪律。**实施修正（R-wire 审查阻断一刀）**：原文「降级态每
10s 重发」在「wmd 活、表安静」稳态下 × parked 单车位位移语义 = 自持回声活锁
（inputd 重臂 WAIT ↔ wmd 同版本位移应答乒乓）。落地形 = **在途门控**：降级态**且无
在途 WAIT** 才每 10s 重发；outstanding 标志清位在 wm_on_reply 入口任意三态（Err 后
也清——有意加宽，防一次 Err 永久误门控探测）。纵深防御变体（同版本应答结构性禁
重臂 ≤3 行）裁量弃——触发点斩断已足，多一层 = 多一处时序耦合面。
**Q12 env 第 5 插预案**：8/8 冻结注记进 wmd 头注 + §0 护栏；本刀不预抬不预留；
GUI-8 fs dup 即触 CHAN_HND_MAX 8→9 四处抬帽另刀（Q8 dissent 的 3/8 备选届时重审）。
**Q13 标记形态**：单标记 WM ok（§4 定稿）。WMF 否决——EFI 相 inputd **未拉起**
（svcmgr H_MMIO 门先跳——家名单注释 :1451-1453 在册；R1 备注②勘误：非「park 于
find_input_device」）⇒ 推送契约在一切现役相结构性不可执行，WMF = 永空死标记。
WMD_PUSH_B EFI 相恒滞 static 属「留表即留命」注释覆盖面。
**Q14 tablet**：不加 virtio-tablet——钦定验收三件无一需要；加 tablet 本刀仍不闭环
端到端命中（gfxdemo 演完即退无常驻消费者）；M-GUI 主验收（GUI-8 点击）才是 recipe
归属。S3 原悬题「先核 :139-141」已由 svcmgr 门控源码定案：push 通道 EFI 不可达，
WM_TABLE_WAIT 机制 smoke 覆盖 = 零、host 测独扛。
**Q15 几何域**：快照头 12B 携 disp_w/disp_h（TABLE_GET/TABLE_WAIT 共用）；count=0 =
合法空表（区别于 INPUT_POLL count 0 拒——空批有 1B EAGAIN 替代形；窗口表空 = 真实
初始态，测试钉住防照抄）；1B [ST_EAGAIN] 仍合法（wmd 上线前/降级面），三态分离解析。
两域分立：WM_GETINFO = wmd 自答（几何 = 启动期一次缓存）；present GETINFO（gfxdemo
三断言真值源）= **逐调用中继非自答**（否决 §8-④ 建议——自答 = wmd 持第二份 present
常量副本，guifbd 演进时双源漂移面；wmd 门=present_cli≠0 已构造性保证 guifbd Ok，
零启动序耦合）。inputd 命中坐标系与归一化同域对齐，SCREEN_W/H 定值降为无快照态兜底
（挂账消解）。
**Q16 双消费**：INPUT_POLL 是泵唯一 drain 者（三选一消解为非题）；wm 命中 = to_wire
编码期内存旁注（双吃/丢失结构性不存在）。serve 重构 = 统一两路 poll-first（#10）；
门控判据 = wm_push_h()≠0（extras 位）；==0 时现单通道代码路径逐字节保留。
**Q17 SET op 族**：单 OP_WM_SET 掩码式（req 20B [op@0][mask u8@1][r16@2..4]
[window_id u16@4..6][r16@6..8][x u16@8][y@10][w@12][h@14][vis u8@16][z u8@17]
[r16@18..20]；mask 位 GEOM/VIS/Z=1<<0/1/2，mask=0→None，未知位双侧拒；masked-out
槽位值无语义仍编入=定长+「编什么收什么」禁实现侧偷偷清零；应答 1B OK）。移动端
典型变更 = 移动+缩放同帧原子生效 → 表版本恰 +1 → inputd 恰醒一次收一张快照。DESTROY
req 8B [op][r][r16][window_id u16@4..6][r16]，应答 1B OK。
**Q18 错误码**：复用 ST_* 七码零新增——id/parent 无效→ST_NOENT；表满→ST_FULL；
layer/mask/flags/vis 野值→ST_BAD（wire 层 None，服务端 1B）；ST_BUSY 头注预留多客户
端刀（v1 串行深 1 不可达）。分工：值域 wire 层执法，引用存在性服务端策略面。
**Q19 crate 结构**：三模块 main/table/relay + 启动序五步（#3-#6 定稿）。0 值端点
park 非 exit(0)（svcmgr 已 ps_reg ALIVE，exit 留永久谎账——v1 无终态回写路径）。

**Q-义务② 档位行（R1 应修⑤——GUI-8 持久化形态前置三件成文）**：
① wmd 装 fs 端 ⇒ env 第 5 插 ⇒ CHAN_HND_MAX 8→9 四处抬帽另刀（kernel/channel.rs
:34/:47/:61 + berry-exec lib.rs:424/:437-440 + berry-libc io.rs:43/:100-109 + ktest，
Q12 地形）；② fsmgr 多客户端三选项（多端点 serve / 分相错开 / 不落盘）分界记录
（§8-③ 全文在档）；③ 状态文件三约束（自校验/读失败回默认+重写自愈/写失败非致命）。
wmd 头注 env 冻结注记同步引此行。

## §8 六路侦察结论存档（2026-11-02，wf_077ab79c-0ac，免下会话重走）

### ① guiproto（协议面）
- op 布局：present 1..=6（lib.rs:54-64）/ input 0x20..0x21（:66-68）/ hdi 0x40..0x43
  （:70-76）三带互斥；0x60..=0xFF 全空闲——**wm 域开新互斥带**：错投帧在任何既有域
  解析器 = 未知 op → None → ST_BAD，结构性防错投（复用 1..n 起号 = 错投 present 通道的
  wm 帧被当合法 present op 解析掉）。命名跟 input 域前缀式 `OP_WM_<动词>`。
- 版本域：**wm 域自带 GETINFO 形 op + VER_WM 常量**（推荐案）——零触碰既有字节全
  add-only；塞 GetinfoRep 保留位 = 协议面变更（黄金字节测 :1359-1368 与 PRESENT_REP_MAX
  const assert :1249 是故意埋的零漂移炸弹，改即炸须停刀重裁）。
- 应答形惯例：非 OK 恒 1B（:30-32 烘焙线 + :893-913）；成功 per-op 定长；变长 = count
  前缀收在定长头内（INPUT_POLL 4B 头形）；SUBMIT 8B token ack 先例（:775-794）→ 建窗
  应答同形携 window_id u32；幂等 setter 1B OK；16B 定长先例 FrameDoneRep（:809-842）。
- **wmd→inputd 推送法定形态**：严格串行 RPC + 零服务端主动 send（:18-22）→ 推送只能
  建模为阻塞应答：inputd 发 WM_TABLE_WAIT，wmd park 到表变更应答全表快照。两形：无参
  （vblank_wait :1098-1116 先例）vs 携 last_seen 版本号（表已新立即答——**首个带参
  阻塞请求，wire 面新立**）。串行深 1 连带：inputd 需 wait_many 双通道并监。
- 快照 = count 前缀 + 定宽表项，**单帧 ≤256B 硬约束**（无多帧/续帧先例）；表项 16B →
  N≤15 顶格 244B（恰 INPUT_REP_MAX 先例值）；**按 GUI-8 形态定帽**（launcher 桌面 =
  真实上界消费者，勿按 v1）。
- 帽执法：12 条 const assert（:1246-1260）+ ≤CHAN_MSG_MAX=256（kernel channel.rs:28
  单源）；wm 域同款义务（WM_WINDOWS_MAX/WM_REQ_MAX/WM_REP_MAX + 帽钉板测试 :1313-1339
  同形追加）。owned 定容数组+derive 五件套惯例（:39-47）。
- 测试义务 13 条：帽钉板/全 op roundtrip/黄金字节钉板（改布局必炸注）/截断每切点全拒+
  尾随拒/值域拒矩阵（相异值防互换静绿，版本三元组 (1,2,3) 先例 :1345-1347）/帽边界恰满/
  烘焙线三态/保留位 0xEE 前向兼容（:2151-2206）/未知位不拒（钦定位域）/错 op 混淆拒/
  assert 契约 should_panic/LE 钉板/None 契约。
- **#54 引用勘误**：「host 测编解码先行、消费者随刀接线」字面真源 = **ADR #50 行**
  （架构决策 :59）；#54 本体 = SDK 序。「#54=协议纪律」惯用引语义锚 = 「协议先定形、
  无消费者不接线」+「改 wire 停刀升 ADR」——引用写准防漂。
- ST_* 七码复用（:81-93：窗口 id 无效 → ST_NOENT、表满 → ST_FULL 现成）；lib.rs:8 头注
  「present 域消费者=应用/gui-wm（GUI-5，段②消费者）」本刀同步拓扑。

### ② inputd（输入对端）
- 现役：serve 单通道 gift2_h（main.rs:596）；SUBSCRIBE 两任（svcmgr boot 自验 INPUT ok +
  bsh inputpoll INPUTP ok n=3）；INPUT_POLL 阻塞 30s 帽 1ms 步进（:555-589）；100ms
  SVC_TICK 心跳重臂；服务端永不主动 send（send_rep 受激面 :511-528）。
- 推送接入：**新开 channel pair = 唯一干净接收面**（input 域反向复用结构性不可行——
  串行 RPC + 角色已定 + 随行 handle 恒闭 ST_BAD :640-647）；inputd extras 1→2（env
  5→6/8）；serve 升格两路 poll-first + wait_many n=2（guifbd 三路 :923-958 先例 +
  nb/nh 每轮重置纪律 :323-343）。
- svcmgr 建对 = **2 对**：① wm 域对（服务端端→wmd extras；客户端端→svcmgr 持留按 b-2
  直递消费者）；② wmd↔inputd 推送对（inputd 端随 inputd spawn 迁入——inputd arm 先于
  wmd arm，建对后存 static 或 wmd arm 内建对存 static，NET_A/INPUT_A 先例 svcmgr:
  243-249；Err 路双端显式回收 :718-729）。
- 命中分发两拓扑：A（inputd 转发 wmd 裁决——每事件多一跳 + wmd 在热路径 + 重启丢在途）
  vs B（inputd 持快照本地命中直投——零跳 + wmd 不在逐事件路径 = 失败隔离诉求；代价 =
  新鲜度窗口 + 命中算法两址一致性义务——纯函数单源，禁两处手写）；**ADR #61 文面倾向
  B**（「命中分发查 wmd 推送快照」）；A 可作 B 的 v1 过渡（无消费者期只记账不命中）。
- 双消费语义：bsh INPUT_POLL 与 wm 命中 drain 归属须钉死（同批事件不能双吃也不能丢）；
  v1 兼容期门控「无 wm 消费者 = 现行为原样」显式立法。
- 快照失效盲区：v1 无对端关闭检测——wmd 崩溃后 inputd 陈旧快照无信号；版本纪元 +
  超龄回退（单窗直通/记账不命中）立法。
- SCREEN_W/H=800×600 定值 vs 真 fb 几何联动挂账（main.rs:83-86）——快照应携带几何域
  或引用 GETINFO 协商值，否则命中系统性偏移。
- intent：wm 对须 channel_pair()（options=0 借链激活）勿自造 options 面；当前惰性
  （全员 UNSPECIFIED 顶档实际抬不了档）；回溯点 GUI-4段② §7-8（:484）。
- PM 脉冲占位（main.rs:614-618）：本刀后候选点 tick/EV_SYN 收批/+快照到达 = 三选一。
- bsh 8/8 硬墙：wm 客户端端永不入 bsh——联调标记走预拉消费者（gfxdemo 形）或 svcmgr
  自验（input_selftest 同款）。

### ③ fsclient（持久化）
- **结构性硬约束**：fsproto 严格串行无请求号——同一 channel 对两并发活跃客户 = 应答
  跨客户错配；rootfs 相 bsh = 永久活跃 fs 客户 → wmd 并发用 fs_a dup = 混流。三选项：
  (a) fsmgr 扩多端点 serve (b) 分相错开 (c) 不落盘——**本刀裁：wmd 不装 fs 端**
  （GUI-5 无持久化需求；ADR 义务② = 形态前置记录非必做；位序留 GUI-8）。
- FAT 跨 boot 先例：写门 = 根级平 8.3 大写名（WMSTATE.BIN 形）≤16KiB、FAT 根 16 项帽
  （与既有条目共享预算）；每次保存 = 全文件整体重写（PUT 分片 + close/TRUNC）；TCG
  成本 ~10s/次——「每次窗口变化」不可行，现实 = boot 收尾 + 事件去抖（GUI-8 同款）。
- **结构性缺口**：无任何现役相同时「GUI 链活 ∧ FAT 活」（rootfs 相有盘无 ramfb；
  EFI 相有 ramfb 无 virtio 盘）——GUI-8 M-GUI 跨 boot 验收需新 recipe（rootfs+ramfb
  或 EFI+盘）；连带 mkfat 确定性硬线（状态文件写入破「重造盘字节零漂」——须走持久化
  相「复用已写盘」先例，smoke.sh:116-123）。
- 状态文件三设计约束（GUI-8 记账）：自校验（magic + schema 版本 + 总长 + 校验尾）/
  读失败回默认 + 一次全量重写自愈毒环（B4 ④ 守卫保证收敛）/写失败非致命（会话毒化
  进程静态无恢复——wmd 崩溃重启自然获新会话，只丢一次保存不连累窗口服务本身）。
- v1 无 rename/journaling/fsck；覆写撕裂三分 = 旧全/新全/毒环半文件。

### ④ guifbd（呈现链）
- 单客户端假设点九处（present_seq :214 / tokens 环 :216-218 / held 别名 :220-221 /
  vote+power :223-225 / vb_ev :235 / vb_park :237-238 / slots :239 / 每域单服务端
  channel :899/:904 / console 谱系共享 :399-400）——**形态 A（wmd = present+hdi 唯一
  客户端）下全部由构造满足，guifbd 零改动**；wire 契约与 gfxdemo 时代逐字节相同。
- 分发换手：svcmgr :430-431 两对不动；客户端端从 gfxdemo arm（:504）换手 wmd arm——
  b-2 面重定向非新对，guifbd 零新 handle（wait_many 仍 n=3，对 wm 域零感知）。
- **wmd 崩溃重启残留**：guifbd held 表持 wmd 前世在途别名（credits 永占 1-2 槽）——
  现协议可解：**新 wmd 启动序立法首发 ReleaseWait 全量清账**（:474-496，零 guifbd 改动）。
- console 互踩：v1 裁「接受交错」——console = 紧急/调试覆写通道，不进 Z 序不进 wmd 记账
  （收进合成 = 破坏失败隔离）；单线程确定性轮询下是确定性交错非竞态。
- gfxdemo 改线清单：:34 访问器 / :175-188 降级门双门改单 / present 六调用点换端
  （GETINFO :196-210 / SUBMIT :306-311 / FRAME_CALLBACK :314-326 / RELEASE :330-340 /
  VOTE :344-354 / POWER_MODE :355-359，op 编码零改动）/ hdi mini 序 :361-414 去向待裁
  （wm 代理 vblank vs 砍序）；不动面 = rpc 助手 :106-155 + VMO 三连 + 渲染管线
  :212-283。
- env 核算：wmd 4 extras（wm_srv + push_srv + present_cli + hdi_cli）= **恰 8/8 零
  余量**（第 5 插 = 内核 CHAN_HND_MAX 抬帽四处联动另刀）；hdi 不代理案 = 3 extras 7/8
  （但 hdi_cli 无人接 = mini 序砍、GFX 断言面缩）。gfxdemo 1 extra 5/8。
- gfxdemo 降级门语义：present absent → wm absent 改名；「guifbd Ok 而 wmd spawn fail」
  中间态退出口径 = 永不进 MARKERS（netd 双门先例）。

### ⑤ mandate（钦定面）
- 两层窗 = 状态栏系统窗 + 全屏桌面窗（规划 :314）；.bap 应用全屏独占窗起步（:66）；
  避让 = avoid area（状态栏避让）；QNX 缝 = parent/group 安全域聚合（域内自治+域外
  单窗）v1 用不上但字段缝成本低（GUI-05 :205-213/:357）；Z 序 = 类型→优先级一维数值表
  （免连续 z 值竞争，WWM 参照 :234）；hit test = Fuchsia RFC-0096 骨架（input pipeline
  唯一直读驱动输入防键盘记录器）；焦点 v1 = 单焦点窗+归还（:235）；GUI-8 分工 = 状态栏
  画在 launcher 进程内、wmd 管层位/避让/命中（:63/:65-66）。
- **张力点**：GUI-10 :105「latch/squash 语义随此刀落 guifbd」vs ADR #61「guifbd CI 面
  零演进污染」——§7 Q6 已裁：明文另刀 + 沿革追记勘误。

### ⑥ dispatch（分发产链）
- svcmgr arm 全景表（带行号）；wmd arm 刀形 = guifbd arm + inputd/vnet 组合。
- 门条件已裁 EFI-only（§7 Q4）；两链都活案否决记录在案。
- 产链：justfile 三行（gfxdemo 行 :205-207 后）+ mkcpio 条目插 gui-inputd(:255) 与
  guifbd(:256) 之间（字典序 gui-inputd<gui-wmd<guifbd）+ 最小三依赖（libc/sys/
  guiproto，无需 berry-gfx）+ CRT0_SEED 锚 + 降级门照 gfxdemo :177-181。
- psproto 抬帽清单：MAX_ENTRIES 16→17（lib.rs:53）+ REPLY_MAX=2+17×14=240≤256 +
  **reply_full_table_boundary 测试 16 臂硬码名表须补第 17 名**（现 `_ => b"svcmgr15"`
  兜底会重名）+ PS_WIN 512B（17×29=493≤512）+ smoke 断言零破（前缀式 PS ok n=）。
- **无任何现役 smoke 相同时有显示+输入**——M-GUI 主验收最终逼 smoke-image 加
  virtio-tablet；GUI-5 预判已进 §5（Q14：本刀不加，host 测独扛）。
- 并线冲突热点 = smoke-image.sh ECO_MARKERS（gui3 +GFX/GFXF vs main +APPUI 双改）/
  svcmgr/justfile——6c 承诺行级独立追加压冲突到零；本刀 ECO_MARKERS 尾追加 + 头注
  段追加均为纯追加形。
- rootfs roster 推数 14 vs 实测 13 差 1 未定位——smoke 时以 PS ok n= 实值对账
  （纪律沿 GUI-2段③ §5，不预判）。
- gfxdemo 改线保 GFX/GFXF 语义不漂（已上 CI）；wmd arm Err 路回收照 vnet/gfxdemo
  先例（svcmgr :518-528）。

## §9 设计 workflow 批判吸收清单（2026-11-02，wf_9293e8ee-9a4 六工：D1/D2/D3 起草
+ 综合 + R1/R2 对抗；R1 = 0 阻断/5 应修/5 备注，R2 = 1 阻断/2 应修/5 备注——全吸收）

| 发现 | 级 | 吸收落点 |
|---|---|---|
| R2-阻断① serve_poll 步进睡改 wait_many([push_h]) 缺 ≠0 门控——rootfs 相活锁自旋 | 阻断 | §3#10 显式门控 + §6 S3 复述 + review dim 4；==0 时 :585 现形逐字节保留 |
| R1-应修① wmd serve 面分发表未立法（白名单外 op 默认行为未定义） | 应修 | §3#6 relay 分发表四行成文 + review dim 1 |
| R1-应修② 避让有效几何只在建表时一次性推导，重推导不变量与回显语义未钉 | 应修 | §3#5 不变量式重推导 + Q10 补 wire 语义钉板（表项携有效几何，申请几何不回显）+ Q5/R1 备注⑤ z 裁语 |
| R1-应修③ Q3 纪元跃迁论断过强（「必>」非结构性保证） | 应修 | §7 Q3 表述校准（大概率非必然 + 超龄 WAIT(0) 兜底；R2 备注⑥ 同勘） |
| R1-应修④ wmd arm cpio 缺分支 present_cli/hdi_cli 2 槽滞留 | 应修 | §3#7 cpio 缺分支 close 两端 close-only（置 0 实施弃——死存警告，见 §3#7 修正注） |
| R1-应修⑤ ADR #61 义务② S4 docs 收口行缺回填项 | 应修 | §7 Q-义务② 档位行（GUI-8 形态前置三件成文）+ wmd 头注引用 |
| R2-应修② svcmgr Err 面 6 枚漏 2 + 变量置 0 | 应修 | §3#7 Err 面显式枚举六枚（置 0 实施弃——unused_assignments ×4，§3#7 修正注） |
| R2-应修③ relay.rs 失败面未定义（recv 帽/EBUSY/recv 窗） | 应修 | §3#6 失败面三句（trap + 三重兜底链注记 + recv 窗同形） |
| R1-备注① S1 硬序「表满静默丢」机制句与 EFI 15/16 不符 | 备注 | §1 PS 台账机制句改写（恰满帽内不丢；静默丢仅第 17 行；抬帽兜两种实值）+ §3#2 风险表述校准（R2 备注④ 重名不炸测试） |
| R1-备注② Q13 前提误植（EFI 相 inputd 未拉起非 park） | 备注 | §7 Q13 勘误 + S3 悬题定案（push 通道 EFI 不可达，host 测独扛） |
| R1-备注③ guifbd 缺显式零 diff 行 | 备注 | §3#15 显式零 diff 行（并线对照锚——diff 出现任何改动即停刀核对） |
| R1-备注④ §7 计数口径（十五项清单 vs 19 问） | 备注 | §7 定稿 = 十九问全回填 + 清单逐项映射（Q1-Q19 覆盖草案 §7 待拍清单全 15 项 + 三增问） |
| R2-备注⑦ parked 单车位交错语义两义均收敛防实现歧义 | 备注 | §3#5 单车位语义钉死（后到覆盖不排队）+ S2 实现注释同钉 |
| R2-备注⑧ gfxdemo TABLE_GET 断言比对面须钉有效几何 | 备注 | §3#12 断言注记（v1 有效==请求几何；GUI-8 加系统窗随改）+ §3#1 WmEntry doc 注 |
| R1-备注⑤ Q5 z 平铺未对 :234 参照作裁语 | 备注 | §7 Q5 裁语补（layer 即类型优先级表；归一化走 VER_WM 升版） |
| R2-备注⑤ Q3 epoch 内部矛盾勘误 | 备注 | 同 R1-应修③ 合并吸收 |

**风险面十条**（收割时逐条对照）：wmd env 恰 8/8（第 5 插 = 抬帽另刀）；WM_TABLE_WAIT
机制 smoke 覆盖 = 零（host 测独扛——断层预判句 §5 在档）；S1 先于 S2 硬序（PS 表满
静默丢兜底）；TCG +5-10s spawn 余量；rootfs.cpio 增容字节对拍自洽；并线三热点预案
（ECO 尾追加形）；中继阻塞期间零新鲜度损失结构注释防后刀踩雷；relay trap 失败链三重
兜底（gfxdemo 10s 帽/inputd 超龄/GFX 缺 FAIL）；guifbd/inputd lib/smoke.sh 三零 diff
行 = 收割 diff 审查白名单；rootfs 差 1 未定位（PS 实值对账）。

三路起草底稿与批判全文存 workflow transcript（run wf_9293e8ee-9a4，journal.jsonl）
——本节为执法摘要，锚点复走以底稿为准。

## §10 执行结果（2026-11-03 收官实录）

**实施链**：workflow wf_b4e9a409-273 九工全绿（S1 库面：guiproto wm 域 +1247 行纯
add-only 12 新测 + psproto 16→17 → S2 gui-wmd crate 三模块（main/table/relay）+
svcmgr wmd arm/换手 + libc 六访问器 + justfile 产链 → S3 inputd 双路 serve +
gfxdemo 改连 wm → S4 smoke-image 开灯 69→70 → 四维审查（cap/mark/wire/struct）→
修复工）。审查抓获 **R-wire 阻断一刀**：TABLE_WAIT 回声活锁（详见 §7 Q11-③ 实施修正
——在途门控）；另 R2-阻断① 复检 PASS（serve_poll 门控落地、==0 分支 :585 现形逐字节
保留）。三零 diff 白名单兑现：guifbd / inputd lib.rs / smoke.sh 收割 diff 全空。

**实施偏差两笔**：① §3#7 钦定「置 0」落地即 4 条 unused_assignments（换手后无读者）
——删 4 行改 close-only（GUI-2段③ 先例同形；终源复跑镜像 sha256 逐字节全同 = 零
产物影响）；② SVCMGR 家名单/PS 台账按 §1 预案走（EFI 16/16 恰满帽内不丢——预测命中）。

**终验**：test-host 全绿 0 fail；smoke-image PASS（70/70 恰一次 + PS ok n=16 +
kfile=3794640B / init=32245080B 字节对拍 + RX 注入 + 金丝雀严格递增）；smoke 八跑
零漂（rootfs 相 46 含 SIG、负向 grep `WM ok`==0——EFI-only 标记不入 rootfs 相）；
just check 过。数据行随三拆提交回填。

**kfile 假 FAIL 判例（雷册 §B29 时序踩踏形态）**：首跑 smoke-image 行为面全对唯
kfile 对拍挂（期望 3973440B）——根因非本刀：QEMU 运行窗（≈4.5 分钟）内共享
~/.cargo-target 的 berry-kernel 被兄弟会话（S-3c 线验证构建，27f0f6c 00:17:07 落库）
覆盖为 main 量级内核，smoke-image.sh 收尾 stat 换脸文件 → 镜像 3794640 ↔ 宿主
3973440 错位。FAT 直解 + mtime 钉序 + 提交时间三环实证；终源复跑（窗口干净）PASS
即证。**加固**：KERNEL_BYTES/INITRD_BYTES 基线快照前移至脚本入口（= 镜像打包同窗，
QEMU 窗内覆盖免疫）；**协议**：跨会话共享 target dir 写前 sock 通告 + 避让 QEMU 窗
（0c/6c/dd 三方钉板；同根异形于 K-1 指纹错链——判例扩 §B29 原条，不开新条）。

**reapq overflow (leak) ×3 归档**：串口尾部实录于 app churn 密集段（REAPQ cap 32
churn 峰值积压、首 4 loud 防风暴——sched.rs:621 设计内行为，不在标记清单）；终源
镜像 sha256 与首跑逐字节全同 ⇒ 同字节同行为确定性复现 ⇒ 非死存清理引入；与 wmd
常驻（16 进程）的因果未证伪——临界积压面候选，留调度线（S-4/容量）处置，不阻断本刀。

**三拆提交**：①库面（guiproto/psproto/libc）= `a35562a`；②服务与产链（gui-wmd/
svcmgr/inputd/gfxdemo/justfile/smoke-image 开灯+基线入口快照加固——同文件同刀）=
`a81714c`；③docs+判例（本稿收官/索引/规则 07·11/雷册 §B29 扩段+§C/阶段沿革）=
本笔。
