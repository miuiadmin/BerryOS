# GUI-4 段②实施设计草案（gui-inputd 接线/集成刀）

> 版本：**1.0**（v1.0）｜2026-11-06

> 状态：已收官（2026-11-01：smoke 四跑三修全八相 PASS——rootfs 相 46/46（INPUT ok 在列）+ RX 相 73/73 + INPUTP 往返 + 金丝雀 39 递增；smoke-smp 双相 PASS（相 2 = 46）；§7-①/② 首跑实录闭环）｜ 日期：2026-11-01 ｜ 前置：GUI-4 段①已落（`docs/GUI-4实施设计草案-v1.0.md` §7.1 三件：berry-guiproto / berry-virtio input 框架 / gui-inputd——其 §9 即本刀台账）
> 行号口径：本草案全部行号 = gui3 worktree @ `0a30854` 基线（2026-11-01）。实施时以**锚文本**为准、行号辅助定位——前序 hunk 落地会使后续行号平移。

---

## §0 范围与禁改清单

**本刀 = 段①三件套的接线/集成**：把 berry-guiproto 协议面、berry-virtio input 框架、gui-inputd 服务进程接进系统——编译链入册、QEMU 挂 tablet、svcmgr 预拉 + boot 自验、bsh 消费端、smoke 注入往返关卡化。纯用户态 + 编排刀。

**台账映射**（段① §9 十项，:975-999）：

| # | 段① §9 条目 | 本刀裁定 |
|---|---|---|
| 1 | justfile test-host 登记（berry-guiproto / gui-inputd 两行） | §3.6 |
| 2 | build-user 链：gui-inputd musl release 行 + mkcpio `bin/gui-inputd=` | §3.6 |
| 3 | run-rootfs 增 `-device virtio-tablet-device`（+QMP 通道注记） | §3.6 / §3.7 |
| 4 | smoke 注入往返标记：QMP input-send-event → 状态谓词门控 → POLL 事件真值断言 | §3.7 / §4.2 |
| 5 | svcmgr 预拉段 + boot 期 SUBSCRIBE 自验标记 + gift 槽位解法 | §3.3 / §4.1 / §7-③ |
| 6 | intent 面 v1 可达接线（options=0 不 opt-out + 借链注记 + PM 脉冲占位注释） | §3.3 / §3.5 |
| 7-10 | gui-wm 消费侧、ABS 域真机标定、多输入设备枚举、PM 脉冲落地 | **不在本刀**，§7-⑦ 重列防丢 |

**禁改清单**（铁律）：`kernel/**`、`arch/**`、`.cargo/config.toml`、根 `Cargo.lock`（内核 workspace 锁；user/ 子级 lock 随 §3.3-⑥/§3.4-④ 的 berry-guiproto path 依赖最小增量更新入册，不属禁改——审查轮澄清）。内核零改动——`kernel/src/channel.rs:34` `CHAN_HND_MAX=8` 恰容本刀满配 8 插（见 §7-⑥ 挂账）。

**产出边界**：实施只落 §3 列出的文件；每步一个可独立验证的提交点（仓库小步纪律）。

---

## §1 立项依据

- `docs/GUI里程碑规划-2026-10-v1.0.md` GUI-4 行（:91-92）：**裁决 A**（输入服务化）+ PointerEvent v1 + intent INTERACTIVE + PM 交互脉冲——「GUI = AI 调度第一个真实消费者——**GUI 只投票不设频**」+ 即到即派留位。
- GUI-1 落地注记（:101-103）：guiproto 定形锚点 = 「GUI-4（输入事件往返）或 GUI-2 消费侧首刀，**先到先定**」——本刀即锚点时刻：输入事件往返打绿后，`user/berry-guiproto` API 进入冻结候选。
- 段① §9 台账 1-6 项（见 §0 映射）。
- ADR #42（intent 两段式）：v1/v2 intent = 查表键，消息借入链是 v1 唯一可达面——本刀以 `channel_create options=0` 保持借链激活（§3.3）。
- 纪律依据：雷册 A32（标记 = 状态谓词非竞速）、A60（多分片输出/标记单行原子）、A74（常驻子勿 reap + recv 容量富余 + stdin 哑喂）、A77（时限帽按 RPC 成本模型）、A78（QEMU kill+wait+sleep 3）、A79（注入断言防回声）、A33（boot 直插 handle 定值须 boot 断言——本刀**不触发**该形态，§7-③）。
- 规划 §4 纪律（:97）：「凡涉 smoke.sh 的改动归主线串行」——本刀 smoke.sh 全套改动归本刀一次性落，不与其它线并行开 smoke.sh。

---

## §2 拓扑与能力链

### 2.1 三条链

**链 A：input 域 channel（控制面，常设）**

```
svcmgr channel_pair()（options=0，intent 借链激活——§3.3 注记）
  ├─ in_b（服务端端）──extra_gifts[0]──► gui-inputd spawn env hs[4] ──► libc gift2_h()
  └─ in_a（客户端端）──INPUT_A 静态存表（boot 自验用）
                      └─ dup ──bsh extras[3]──► bsh spawn env hs[7] ──► libc GIFT_H[5] ──► input_h()
```

**链 B：QEMU 注入（数据面，smoke RX 相专用）**

```
宿主 python3 QMP client ─unix sock─► QEMU input-send-event（三批各一 SYN）
  ─► virtio-tablet eventq ─► gui-inputd harvest（100ms tick）─► PointerPump
  ─► QUEUE_CAP=32 缓冲（预缓冲，非实时流）─► bsh `inputpoll` 发 INPUT_POLL(max=3)
  ─► 事件应答 ─► 三事件真值断言 ─► `INPUTP ok n=3`
```

**链 C：boot 自验（每 rootfs 相 boot 一次）**

```
svcmgr input_selftest：SUBSCRIBE(DELIVERY_BATCHED) 严格串行 RPC ─ ST_OK ─► `INPUT ok`
```

### 2.2 进程环境 handle 槽位表（满配形态）

| 槽 | bsh（满配） | gui-inputd |
|---|---|---|
| [0] | console | console |
| [1] | stdin（H_BSH_STDIN） | stdin（哑喂 Some(&[])，A74） |
| [2] | self_proc | self_proc |
| [3] | self_vmar | self_vmar |
| [4] | rootfs dup | **in_b（gift2_h()）** |
| [5] | fs dup | — |
| [6] | netd dup（NET_A） | — |
| [7] | **input dup（input_h()）** | — |

- 前缀紧致律（`user/berry-exec/src/lib.rs:420` 注释）：extras 全前缀无 0 洞；bsh 满配 nhandles = 4+4 = **8 = CHAN_HND_MAX 恰满帽**（§7-⑥）。
- **槽序稳定纪律**：input 只随 netd 之后入列（入列条件含 `net_dup != 0`）——镜像「netd 只随 fs 之后入列」既有律（svcmgr :934-938 注释：前缀紧致会让消费者落错槽造成同槽不同名）。现役相无 tablet-without-netdev 拓扑，此纪律恒成立。

### 2.3 相位 × 设备门控总表

| 相 | tablet | QMP | INPUT ok 断言 | INPUTP 断言 | 说明 |
|---|---|---|---|---|---|
| 裸相（run / smoke 相 1） | 无 | 无 | 不在 MARKERS | 无 | 无 rootfs 无消费者（§7-④ 偏差记录） |
| rootfs 相（smoke 相 2 / run-rootfs） | 有 | 无 | **在**（ROOTFS_MARKERS） | 无 | boot 自验不依赖注入 |
| RX 注入相（smoke 相 3） | 有 | **有** | 在（ALL_MARKERS） | **专用等帽 90s**（§4.2） | 唯一注入相 |
| FAT 子目录相 | 有 | 无 | 在 | 无 | |
| FAT 持久化相 | 有 | 无 | 在 | 无 | |
| B4 掉电相（三 boot） | 无 | 无 | 单标记门不含（Boot A `FAT ok`+18s 杀 / Boot B `FATW ok`+15s 杀 / Boot C guest 集 SIGTERM——全 bsh 侧，无数组迭代） | 无 | gui-inputd park 非致命（§4.1） |
| EFI（smoke-image） | 无 | 无 | 不在 EFI 门（H_MMIO 缺席预拉跳过 = 拓扑真值，LSN/EVS 同款） | 无 | 同左 |
| smoke-smp 相 2 | 有 | 无 | 在（数组同步 46） | 无 | RX 注入不属本线 |

---

## §3 逐文件落点

### 3.1 `user/berry-exec/src/lib.rs`（gift 槽 7→8）

spawn env 构造块 :408-471，四处：

1. :422 `let mut gift = [0 as Handle; 7];` → `[0 as Handle; 8];`
2. :420-421 槽位注释 → 「8 槽（A2-3：[4] rootfs / [5] fs / [6] netd；GUI-4 段②：+ [7] input）」
3. :439-442 容量断言**逻辑零改动**（`4 + extra_gifts.len() <= gift.len()`，:438 已有 `extra_gifts.len() <= 4` 帽断言）——仅文案 `extra_gifts ≤ 3（gift[2..4] = T-1 自描述对占定）` → `extra_gifts ≤ 4（GUI-4 段②起 bsh 面 +input 端；T-1 自描述对历史文案退役）`
4. :456 注释 `nhandles = 4 + len ∈ 4..=7` → `∈ 4..=8`

### 3.2 `user/berry-libc/src/io.rs`（hs 7→8 + input_h()）

boot 扫描 :61-128，五处：

1. :39 `static mut GIFT_H: [u32; 5]` → `[u32; 6]`
2. :68 容量沿革注释「A2-3：6→7」尾追「；GUI-4 段②：7→8」
3. :77 `let mut hs = [0u32; 7];` → `[0u32; 8];`
4. :100-102 `if nh >= 7 { GIFT_H[4] = hs[6]; }` 之后**加**：
   ```rust
   if nh >= 8 { GIFT_H[5] = hs[7]; }   // GUI-4 段②：input 端（svcmgr 预拉分种）
   ```
5. nh 注释 `4..=7` → `4..=8`；并在 `netd_h()`（:229 一带）后加同型访问器：
   ```rust
   /// GUI-4 段②：input 域 channel 客户端端（hs[7]/GIFT_H[5]——svcmgr 预拉
   /// 交付）。0 = inputd 未起（裸/B4/EFI 拓扑），调用方静默降级。
   pub fn input_h() -> u32 { unsafe { GIFT_H[5] } }
   ```

### 3.3 `user/bins/svcmgr/src/main.rs`（预拉 + boot 自验 + bsh 分种）

**① 静态端点**——`static NET_A`（:242）后加：

```rust
/// GUI-4 段②：input 域 channel 客户端端（预拉成功后非 0；boot 自验 +
/// bsh 面 dup 分种——input_h()）。0 = inputd 未起，静默降级。
static INPUT_A: AtomicU32 = AtomicU32::new(0);
```

**② 预拉段**——vnet 预拉块（:518-569）之后、stdhello 拉起（:570）之前插入。**以 vnet 块为模板逐行镜像**，差异五处（名字/礼物/存表/失败面/头注）：

```rust
// GUI-4 段②：gui-inputd 预拉（virtio-tablet 输入服务）。门控同 vnet 两道：
// H_MMIO 在席（EFI/裸拓扑缺席 → eco_skip(b"mmio absent") 静默零扰动）+
// cpio_find(H_ROOTFS, b"bin/gui-inputd")。
//
// intent 注记（台账 6，v1 可达面）：input 域 channel 由本表 channel_pair()
// 创建、**options=0**——不设 CHAN_OPT_NO_INTENT，消息借入链（4-3c：收端按
// 每消息重导 eff = min(declared, msg_intent)）有意保持激活 = GUI 规划 GUI-4
// 行「intent INTERACTIVE」的 v1 唯一可达形态。UNSPECIFIED(0x00) 顶档语义下
// 借入恒不抑制（4-3c 数值序）；svcmgr/bsh 自身 declared 不变——内核侧
// berry_intent 自申报 = 另刀。ADR #42 / 段① §9-6。
//
// 常驻不 reap（A74：reap 首步 wait 30s 帽必超时）；stdin 哑喂 Some(&[])；
// recv 容量按「协议定值 1 op + 富余」判面（段① serve() 已落，本刀零改）。
// 失败面 = close(in_b) + close(in_a) + eco_skip(b"inputd spawn")——bsh
// inputpoll 面静默降级（input_h()==0 → "input: absent"）。
```

代码体照抄 vnet 块（channel_pair() → SpawnSpec{name: b"gui-inputd", data_off/data_len 取 cpio_find 结果, stdout: console, stack_pages: 8, stdin_feed: Some(&[]), stdin_handle: None, args: &[], extra_gifts: &[in_b], mmio_vmo: Some(H_MMIO), mmio_va: VIRTIO_MMIO_VA} → spawn_rootfs_elf）：Ok → `ps_reg(b"inputd", ALIVE, 0)` + `INPUT_A.store(in_a, Ordering::Relaxed)`；Err → close 两端 + eco_skip。

**③ boot 自验调用**——`_start` 自验链 :108-149 中 `thr_selftest`（:140）之后、steady 'v' 循环（:142-149）之前加：

```rust
input_selftest(console);   // GUI-4 段②：boot 期 input 域 SUBSCRIBE 自验（INPUT ok）
```

**④ input_selftest + input_rpc**——放 dns_selftest 族旁（:2557 dns_selftest 注释、:2560 函数、:2575 dns_rpc 附近）。`input_rpc` 逐行镜像 `dns_rpc`（:2575-2598：encode → send 循环（EBUSY→wait WRITABLE 10s 重臂，其它错 → None）→ wait_one(h, SIGNAL_READABLE, now()+帽, &mut obs) → recv → `Option<Vec<u8>>`）；帽 = **60s**（推导见 §4.4）。selftest：

```rust
/// GUI-4 段②：boot 期 input 域自验——SUBSCRIBE(DELIVERY_BATCHED) 严格
/// 串行 RPC，ST_OK 才打 INPUT ok（dns_selftest 样板：INPUT_A==0 静默
/// 跳过零扰动）。tablet 缺席（B4）= gui-inputd 已 spawn 但 find_input_device
/// None → park（段① :139-141），应答永不到 → 60s 帽到点 eco_line 诊断后
/// 放弃，**非致命**——B4「vnet 无 net 槽 trap 干净自死、标记自然缺席不设
/// 门」先例同款（:1255-1257）；墙钟零影响（B4 kill 全由 bsh 侧标记触发）。
fn input_selftest(console: Handle) {
    let h = INPUT_A.load(Ordering::Relaxed);
    if h == 0 { return; }
    let mut buf = [0u8; 16];
    let n = gp::encode_input_sub_req(&mut buf, gp::DELIVERY_BATCHED);
    match input_rpc(h, &buf[..n]) {
        Some(rep) if rep == vec![gp::ST_OK] => { /* 打 INPUT ok（THR ok 同一打印路径） */ }
        Some(_) => eco_line(b"input sub st"),   // 诊断非致命
        None => eco_line(b"input rpc"),          // 超时/链断——B4 形态，非致命
    }
}
```

（`gp` = berry-guiproto crate 别名；SUBSCRIBE 应答 = `encode_status_rep` 1B 状态——段① `parse_status_rep` 收口。）

**⑤ bsh 臂 gifts**——:929-1003：:946 `let mut gifts = [rootfs_dup, 0 as Handle, 0 as Handle];` → 4 元素（尾补 `0 as Handle`）；netd 槽位块（:934-940）后加 input 槽位块（入列条件 = `INPUT_A != 0 && net_dup != 0`，dup 失败置 0 不入列）；slice 选取链相应 `&gifts[..4]` 满配分支；Err 收尾 close 未入列 dup（含 input_dup）——全部照 netd 槽位既有形态镜像。:923-925 槽位注释尾追「；[7] GUI-4 段②：input 客户端端（INPUT_A dup——inputd 未起静默不装，只随 net 后入列守槽序）」。

**⑥ Cargo.toml**：`user/bins/svcmgr/Cargo.toml` 加 `berry-guiproto = { path = "../../berry-guiproto" }`（netdproto 同型）。

**⑦ ps_selftest ⑤ 家名单断言追加一行**（主会话审稿补——:1265-1278 has_sub 链）：H_MMIO 门控块内（vnet 行旁）追加 `&& has_sub(&cap[..got], b"inputd alive\n")`。审稿已核：⑤ 为逐行子串 presence-only（非全表比对），inputd 加行不破既有断言；PS_WIN 512B / REPLY_MAX 2+16×14=226B 双帽 16 行均余；表满后 m3demo（第 17 注册）被 ps_find_slot 满表丢弃 = 无断言面不可见（PS_MAX 零余量挂账 §7-⑤）。

### 3.4 `user/bins/bsh/src/main.rs`（inputpoll 消费端）

**① BUILTINS**（:402-405）：`[&[u8]; 11]` → 12，尾补 `b"inputpoll"`（pwd 之后——补全/派发面同册）。

**② render_cmd 臂**（:3057 起）：pwd 臂结束（~:3137）后加 `} else if cmd == "inputpoll" { inputpoll_output(out) }`。

**③ input_rpc + inputpoll_output**——放 netd_rpc（:904-944）旁逐行镜像（encode → send 循环 EBUSY→wait WRITABLE 10s 重臂 → recv 循环 EAGAIN→wait READABLE 帽内重臂 → `Option<Vec<u8>>`）；**客户端帽 35s**（> 服务端 POLL_BLOCK_CAP_NS 30s——「client 帽 > 服务帽」纪律，§4.4）。语义三分支：

```rust
/// GUI-4 段②：inputpoll——INPUT_POLL(max_events=3) RPC → 三事件真值
/// 断言 → `INPUTP ok n=3`。
/// - input_h()==0（inputd 未起：裸/B4/EFI）→ "input: absent" + status 1
/// - 应答 ST_EAGAIN（空态合法——手跑未注入时）→ "inputpoll: no events" + status 0
/// - count==3 且 §4.2 真值表逐字段全中 → 逐事件诊断行 + `INPUTP ok n=3` + status 0
/// - 任何字段不符 → "bsh: inputpoll: <差异面>" + status 1（loud）
/// 输出纪律（A60）：逐事件一行（writeln! 单行 ≤192B 原子消息），标记行单发。
fn inputpoll_output(out: &mut Vec<u8>) { /* 如上三分支 + 真值断言 */ }
```

**④ Cargo.toml**：`user/bins/bsh/Cargo.toml` 加 `berry-guiproto = { path = "../../berry-guiproto" }`。

### 3.5 `drivers/bins/gui-inputd/src/main.rs`（注释面，零逻辑改动）

serve() SVC_TICK 重臂段（:613-628 一带）加 PM 交互脉冲占位注释（台账 6 后半）：

```rust
// GUI-4 段②占位（无代码）：PM 交互脉冲——输入事件到达即向 PM 服务投交互
// hint（提频投票）的缝留这里（serve 循环 100ms tick 或 EV_SYN 收批点）。
// **GUI 只投票不设频**（GUI 规划 GUI-4 行裁决：PM 仍唯一决策者）；v1 无
// PM 服务面，本刀零代码零标记——GUI-6/+ 落地时回填。
```

段①已落的构件零改动：ScreenGeom 800×600 常量（:85-86）、QUEUE_CAP=32、park-before-serve（:139-141）、`gui-inputd: up slot=… q=… cal=x[min,max) y[…]` 诊断（:184-187——**ABS 域真值源**，§7-① 首跑核验义务）。

### 3.6 `justfile`（四面）

**① build-user：gui-inputd 编译行**——vnet 块（:122-130，`CARGO_TARGET_DIR=../drivers/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" cargo build --release --manifest-path ../drivers/bins/vnet/Cargo.toml --target aarch64-unknown-linux-musl`）之后加同型块，路径换 `../drivers/bins/gui-inputd/Cargo.toml`，头注「GUI-4 段②：gui-inputd（virtio-tablet 输入服务——段①已落 bins/，本刀入编译链；musl + berry-libc shim + berry-ld 同链）」。

**② mkcpio 条目**（:223-267 块内）——:241 `bin/grep=…` 与 :242 `bin/guifbd=…` 之间插：

```
        bin/gui-inputd=../drivers/cargo-target/aarch64-unknown-linux-musl/release/gui-inputd \
```

（排序注记：`-`(0x2D) < `f`，gui-inputd 恒排 guifbd 前；mkcpio 内部自排序，确定性不破。路径与 :263 `bin/vnet` 同型——drivers 工作区 cargo-target。）

**③ run-rootfs**（:360-398）——launch 尾 :398（netdev 行）后加：

```
        -device virtio-tablet-device,id=tab0
```

recipe 头注（:360 一带）追：「GUI-4 段②：+virtio-tablet（id=tab0——QEMU virtio-mmio 总线**倒序建**、首 -device 落最高槽，tablet 追加在 blk/net 之后取更低槽 = 既有设备槽位零漂移；gui-inputd 按设备类扫 input 槽恰一认领）。**手跑无 QMP**：INPUT ok 照打（boot 自验不依赖注入）；INPUTP 只属 smoke RX 注入相」。

**④ test-host**（:430-522）——:486 berry-ui 行后加两行 + 注释：

```
    # GUI-4 段②：guiproto 协议面 + gui-inputd 泵/服务单测入 host 台账
    cargo test --manifest-path user/Cargo.toml -p berry-guiproto --target {{host}}
    cargo test --manifest-path drivers/Cargo.toml -p gui-inputd --target {{host}}
```

### 3.7 `tools/smoke.sh`（本刀主刀面）

**① 变量区**（:213-229）尾加：`QMP_DIR=""; QMP_SOCK=""`（GUI-4 段② QMP 通道——RX 相专用）。

**② cleanup()**（:231-271）rm 族（:254-257 一带）加 `rm -rf "$QMP_DIR"`。

**③ 四处 QEMU launch 追 tablet**——`-device virtio-tablet-device,id=tab0`（注意续行符）：rootfs 相 :476 后、RX 相 :896 后、FAT 子目录相 :1129 后、持久化相 :1208 后（皆 netdev 行之后 = 槽位注记见 §3.6-③）。**B4 三 boot（:1294/:1337/:1380）不挂**——掉电相无输入面，gui-inputd park 优雅降级（§4.1）。

**④ ROOTFS_MARKERS**（:809-810）——`"THR ok"` 后尾插 `"INPUT ok"`（45→46）；数组前注记块（SIG 注记 :791-808 之后、:809 之前）加：

```bash
# GUI-4 段②：+ INPUT ok——svcmgr boot 期 input 域 SUBSCRIBE 自验
# （input_selftest：gui-inputd 预拉成功后严格串行 RPC，ST_OK 才打；
# tablet 缺席（B4）= gui-inputd park、60s 帽到点放弃非致命，标记自然
# 缺席不设门——B4 无 netdev 先例同款；EFI = H_MMIO 缺席预拉跳过，
# 静默不打 = 拓扑真值（LSN/EVS 同款））。"INPUT ok" 与现役标记两两互
# 非子串（"INT ok" 位 2 P≠T；RX 相专用行 "INPUTP ok n=3" 不含 "INPUT ok"
# ——位 5 P≠空格）；不进 assert_marker 特判面。
```

**⑤ 到齐 echo**（:831）——`M3DEMO ok n=5 + THR ok` 后追 ` + INPUT ok`。

**⑥ 头注计数与标记清单**——:46 `72 标记（boot 27 + rootfs 45` → `73 标记（boot 27 + rootfs 46`；:50-53 标记清单 `M3DEMO ok n=5/THR ok 在列` → `M3DEMO ok n=5/THR ok/INPUT ok 在列`（语义短语同段尾追「+ input 服务链（INPUT——GUI-4 段②，svcmgr input_selftest）」）；各相计数注释按**现值 +1**（根因 = ROOTFS_MARKERS +1）：:108 `71→72`、:117 `71→72`、:900 `71→72`、:1102 `71→72`、:1185 `71→72`。

**⑦ RX 相 launch**（:891-897）——tablet 行（:896 后）再追 QMP 行，launch 前赋值：

```bash
QMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/berryos-qmp.XXXXXX")
QMP_SOCK="$QMP_DIR/sock"   # unix socket 路径帽 107B——mktemp 短前缀必达
```

```bash
        -device virtio-tablet-device,id=tab0 \
        -qmp unix:"$QMP_SOCK",server=on,wait=off \
```

**⑧ qmp_inject helper**——定义于 inject_line 函数（:925-932）之后（同区相仿职能）：

```bash
# GUI-4 段②：QMP 注入 client（macOS 无 socat——python3 socket 直连；内联
# 不落工具文件：注入序列与 h 段断言同址演化）。协议序：连上先收 greeting
# （逐行 JSON——读到 {"QMP":…} 才发 qmp_capabilities，等到 {"return":…}
# 协商毕；异步事件行静默跳过）；此后每条 input-send-event = 一批
# SYN_REPORT，QEMU 在命令返回前同步把事件写进 virtqueue used 环（宿主侧
# 无竞速面）。设备路由主案 = 显式 id（tab0）；QEMU 对 mmio 输入设备 id
# 寻址不收时回退 = 省 device 键（VM 内唯一输入设备，路由无歧义）——首跑
# 裁定实录进阶段沿革（段②草案 §7-②）。
qmp_inject() {
  python3 - "$QMP_SOCK" <<'PYEOF'
import json, socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
f = s.makefile("rw", encoding="utf-8", newline="\n")
def rd():                          # 逐行读到命令应答；greeting 即发握手
    while True:
        line = f.readline()
        if not line:
            raise SystemExit("qmp: eof（QEMU 退出？）")
        m = json.loads(line)
        if "QMP" in m:             # greeting
            f.write('{"execute": "qmp_capabilities"}\n'); f.flush()
        elif "error" in m:
            raise SystemExit("qmp: {}".format(m))
        elif "return" in m:
            return m
def send(events):                  # 一命令 = 一批 SYN_REPORT
    f.write(json.dumps({"execute": "input-send-event",
                        "arguments": {"device": "tab0", "events": events}}) + "\n")
    f.flush()
    rd()
rd()                               # greeting → qmp_capabilities → return
# ① DOWN：btn 按下 + 绝对坐标中点（16384,16384）→ 归一 (399,299)
send([{"type": "btn", "data": {"down": True, "button": "left"}},
      {"type": "abs", "data": {"axis": "x", "value": 16384}},
      {"type": "abs", "data": {"axis": "y", "value": 16384}}])
# ② MOVE：两采样对（8192,24576）→(199,449) 与（24576,8192）→(599,149)
send([{"type": "abs", "data": {"axis": "x", "value": 8192}},
      {"type": "abs", "data": {"axis": "y", "value": 24576}},
      {"type": "abs", "data": {"axis": "x", "value": 24576}},
      {"type": "abs", "data": {"axis": "y", "value": 8192}}])
# ③ UP：btn 抬起（零采样 → 泵合成 UP@最后已知位 (599,149)）
send([{"type": "btn", "data": {"down": False, "button": "left"}}])
PYEOF
}
```

> **⑧ 落地版指针（首跑后）**：上文为设计预写形态——实际落地版 `send()` 内含 **device 键自动降级**（主案 `device:"tab0"` 失败即永降级省 device 键重发该批 + 裁定行落 stderr），`rd()` 分化 `rd_result()` 返 (kind, m)。以 `tools/smoke.sh` 落地版为准，裁定全录 → §7-②。

**⑨ h 段**——g 段（Ctrl-C 杀段，收尾 :1080）之后、金丝雀块（:1082）之前：

```bash
# h. GUI-4 段②输入注入往返（QMP → virtio-tablet → gui-inputd 泵 → bsh
#    inputpoll）：QMP 三批各一 SYN（①DOWN ②MOVE ③UP）——QEMU 命令返回
#    前同步写 used 环，泵 100ms tick 收割入 QUEUE_CAP=32（预缓冲）；随后
#    慢注 inputpoll（0.5s/字节 ≈5.5s 窗）→ bsh INPUT_POLL(max=3) → 三事件
#    真值断言（段②草案 §4.2 真值表）→ INPUTP ok n=3。**状态谓词非竞速
#    （A32）**：注入全部先于命令落位，POLL 时队列必非空；**无串口回声面
#    （A79）**：QMP 事件走 virtio 设备面不进串口，断言坐标字面量只能来自
#    协议应答渲染；'inputpoll' 小写回显与 'INPUTP ok n=3' 互非子串。
qmp_inject || fail "QMP 注入失败（查 QMP_SOCK 握手 / tab0 路由——段② §7-②回退省 device 键）"
inject_line 'inputpoll\n'
t4=$(date +%s)
while ! grep -qF 'INPUTP ok n=3' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 INPUTP 阶段退出"
  [ $(( $(date +%s) - t4 )) -gt 90 ] \
    && fail "INPUTP ok n=3 90s 未出现（查 gui-inputd: up 行（ABS 域）与 inputpoll 逐事件诊断）"
  sleep 1
done
echo "smoke: INPUTP 输入往返到齐（QMP 三批 → 泵 3 事件 → POLL 真值断言）"
```

（inject_line 入参转义形态照 a 段 `'rx\n'` 先例；QEMU 退出纪律 A78 全脚本既有 trap 覆盖，h 段零新增面。）

### 3.8 `tools/smoke-smp.sh`（同步三处）

1. **数组**（:141-142）`"THR ok"` 后尾插 `"INPUT ok"`（45→46）——cross_check_markers（:148-158）计数对拍**必须同步**，否则首跑即 FAIL（防假绿机器守卫，P6 A-5 族）。
2. **相 2 launch**（:354-360）：:359 netdev 行后加 `  -device virtio-tablet-device,id=tab0 \`（**无 QMP**——INPUTP 不属本线；tablet 在席 = INPUT ok 照打，数组同步后断言恰好覆盖）。
3. 计数注释 :4-5 / :22 / :138 / :318 `45 标记` → `46 标记`。

DEADLINE_ROOTFS=840 不动：tablet 在席时 INPUT ok 距 THR ok ≈ 一次 RPC（≤2s，§4.4）；×2 铁律余量已含。

### 3.9 记账面（收官随刀）

- `AGENTS.md:89`「rootfs 相 45 标记」→ 46。
- `docs/规则/07-测试与冒烟.md:23` 相构成表：rootfs 45→46、RX 注入 72→73（71 族注释随 §3.7-⑥ 同步 +1）。
- `docs/索引.md`：本草案入册（GUI 线在途拆解类）。
- `docs/阶段沿革.md` 末尾追记：GUI-4 段②落地实录（含 §7-① ABS 域首跑值 + §7-② QMP 路由首跑裁定——**首跑义务**）。
- 可选对齐 GUI-1 先例：`docs/GUI里程碑规划-2026-10-v1.0.md` GUI-4 行尾落地注记一句（guiproto 定形锚点兑现）。

---

## §4 标记与断言设计

### 4.1 `INPUT ok`（rootfs 相族标记，入数组）

- **语义**：svcmgr boot 期 input 域 `SUBSCRIBE(DELIVERY_BATCHED)` 严格串行 RPC 回 ST_OK = 预拉 + extra_gifts 交付 + gift2_h() 消费 + serve 循环 + 协议编解码**全链活**（状态谓词：RPC 已完成，非竞速）。
- **判定文本**：`INPUT ok`（grep -qF，无特判——互非子串推演见 §3.7-④）。
- **相位**：rootfs / RX / FAT 子目录 / 持久化四相断言；裸相、B4、EFI 自然缺席（§2.3 表）。
- **退化形态两档**（都非致命）：
  - B4（tablet 缺席但 MMIO 在席）：spawn 成功 → gui-inputd park（段① :139-141 `find_input_device()` None → park）→ INPUT_A≠0 → SUBSCRIBE 已入队但永无应答 → 60s 帽 → `eco_line(b"input rpc")` 诊断 → 无标记。墙钟零影响：烧的是 svcmgr steady 循环前的尾段，B4 三 boot 的 kill 全由 bsh 侧标记触发（FAT ok/FATW ok/PWR_MARKERS），进程并行不阻塞。
  - EFI（H_MMIO 缺席）：预拉 gate 1 即跳过 → INPUT_A=0 → selftest 静默 return。零输出 = 拓扑真值。
- **时序**：thr_selftest 后、steady 循环前——与数组末位一致。
- **帽**：60s（§4.4）。

### 4.2 `INPUTP ok n=3`（RX 相专用，专用等帽）

**真值表**（注入三批 → 泵输出三 PointerEvent → bsh 逐字段断言）：

| # | QMP 批（=一 SYN） | 泵输出 PointerEvent | 断言字段 |
|---|---|---|---|
| ① | btn left down + abs x=16384 + abs y=16384 | phase=`PHASE_DOWN`(1)，buttons=`BUTTON_L`，pointer_id=0，x=**399**，y=**299**，history_len=0 | 全字段 |
| ② | abs x=8192,y=24576 + abs x=24576,y=8192 | phase=`PHASE_MOVE`(2)，buttons=`BUTTON_L`，pointer_id=0，x=**599**，y=**149**，history_len=1，history[0]=(**199**,**449**) | 含 history 采样 |
| ③ | btn left up | phase=`PHASE_UP`(3)，buttons=0，pointer_id=0，x=**599**，y=**149**，history_len=0，flags=**0**（泵仅对 CANCEL 置合成位——gui-inputd lib.rs:606；初稿误写 ∋SYNTHESIZED，审查轮纠正） | 零采样合成 |

**坐标字面量推导**（段① PointerPump `normalize_axis`：clamp 后 `(v-min)*(L-1)/(max-min)` 整除；ScreenGeom L=800/600）：

| 原始 v | v×799/32767 | v×799/32768 | v×599/32767 | v×599/32768 | 取值 |
|---|---|---|---|---|---|
| 8192 | 199.74→199 | 199.74→199 | 149.74→149 | 149.73→149 | x=199 / y=149 |
| 16384 | 399.49→399 | 399.49→399 | 299.49→299 | 299.49→299 | x=399 / y=299 |
| 24576 | 599.24→599 | 599.22→599 | 449.24→449 | 449.23→449 | x=599 / y=449 |

**ABS 域两候选（0..32767 / 0..32768，QEMU 版本面）下六字面量同值**——已推演穷尽；首跑仍须实录 `gui-inputd: up … cal=x[0,…)` 行核定（§7-①）。

**判定与相位**：`INPUTP ok n=3`，RX 相 h 段专用 while 等帽循环（**不进任何标记数组**——FAT/持久化/smoke-smp 相不注入则永不出现，进数组 = 三相结构性假超时）；恰断言一次（h 段单次注入单次命令）。

**反竞速论证（A32）**：QMP 三批全部完成 →（QEMU 同步写 used 环）→ 泵 100ms tick 收割入 QUEUE_CAP=32 → 才开始 5.5s 慢注 `inputpoll` → bsh 发 POLL 时队列**必然非空**（预缓冲状态谓词，非时序赌注）；事件满 3 = 一应答全收（INPUT_EVENTS_MAX=3 恰容）。

**反回声论证（A79）**：QMP 事件走 virtio 设备面**不进串口**（无 echo 面——与 FIFO 注入回声面本质不同）；断言的坐标字面量（399/299/599/149/199/449）只能来自 bsh 对协议应答的渲染；注入词 `inputpoll` 小写回显与 `INPUTP ok n=3` 大写标记互非子串。

**bsh 客户端语义**（§3.4）：input_h()==0 → absent（status 1）；ST_EAGAIN → no events（status 0，空态合法）；真值全中 → 标记；不符 → loud 差异行（status 1）。

### 4.3 时限帽推导（A77：RPC 成本模型，非 syscall 外推）

| 帽 | 值 | 推导 |
|---|---|---|
| svcmgr input_rpc（SUBSCRIBE） | 60s | tablet 在席实测 ≈ 1-2 轮转（≤2s）；60s = 30x 余量 + 覆盖 B4 park 形态到点放弃（帽即烧时） |
| bsh input_rpc（POLL 客户端） | 35s | **> 服务端 POLL_BLOCK_CAP_NS 30s**（段①定值）——client 帽必须大于服务帽（服务慢 ≠ 服务死，A71/A76 族纪律）；本序列下预缓冲即答 ≤2s |
| 宿主 INPUTP 等帽 | 90s | 慢注窗 5.5s + bsh 客户端帽 35s + 1s 轮询粒度 + TCG ×2 起步铁律余量 |
| smoke DEADLINE_ROOTFS（420s 不动） | — | rootfs 相族新标记 INPUT ok 距 THR ok ≈ 1 次 RPC（≤2s），既有余量覆盖 |

### 4.4 A74 合规清单（gui-inputd 常驻面）

常驻子不进 reap（ps_reg(b"inputd", ALIVE, 0) 后 svcmgr 不 wait）；stdin 哑喂 `Some(&[])`；serve recv 容量「协议 1 op + 富余」判面（段①已落：nh!=0 → ST_BAD 不 trap）；预拉 Err 面 close 两端不泄漏。ps 账本（审稿亲证）：ps_selftest ⑤ 家名单断言 = 逐行子串 presence-only——inputd 行加入不破既有断言（⑦ 反向追加 inputd 行强化拓扑真值）；rootfs 相 roster **实测 13/16**（~~16/16 封顶~~ 设计期推演错值——2026-11-02 GUI-2 段③ 正片实测勘误：rootfs 相族 `PS ok n=13` 主仓/gui3 双树同值；「ps_selftest 时 15」实为 EFI 相读数误植（EFI 相 `PS ok n=15`）、「berryctl 后 16」无标记实证；表满丢弃机制记述保留作机制参考，实际余量 3——抬帽义务见 §7-⑤ 勘误）；PS_WIN 512B / REPLY_MAX 226B 双帽 16 行均余；`PS ok n=` 前缀式断言全相安全，零余量挂账见 §7-⑤。

---

## §5 验证命令（实施序末逐级跑）

```bash
just test-host      # 新两行绿（berry-guiproto + gui-inputd host 单测）
just build-user     # bin/gui-inputd 平像入 rootfs.cpio（mkcpio 确定性含新条目）
just run-rootfs     # 手核：INPUT ok 出现（无 QMP 也打）；"gui-inputd: up slot=… cal=" 行实录 ABS 域（§7-① 首跑义务）
                    #   串口手打 inputpoll → "inputpoll: no events"（空态合法面核验）
just smoke          # 八相全绿：rootfs 46 + RX 73（含 h 段 INPUTP ok n=3）+ FAT 子目录/持久化 + B4（INPUT 自然缺席零扰动）
just smoke-smp      # 相 2 rootfs 46 绿（tablet 在席 INPUT ok 照打；无 QMP 无 INPUTP）
```

首跑后两项实录进 `docs/阶段沿革.md` 追记：① `cal=x[0,N)` 的 N 值（32767/32768）；② QMP device 键路由裁定（tab0 收 / 回退省键）。

---

## §6 实施编排骨架（九步，每步独立可验证提交点）

1. **基建三处联动**：berry-exec gift 7→8（§3.1）+ berry-libc hs 7→8 + input_h()（§3.2）——`just test-host` 绿。
2. **svcmgr**：INPUT_A + 预拉段 + input_selftest/input_rpc + bsh gifts + Cargo dep（§3.3）——编译绿（运行面待 rootfs 相）。
3. **bsh**：BUILTINS + inputpoll + Cargo dep（§3.4）——编译绿。
4. **gui-inputd**：PM 占位注释（§3.5）——零逻辑。
5. **justfile 四面**（§3.6）——`just build-user` 产 rootfs.cpio 含 bin/gui-inputd。
6. **手跑核验点**：`just run-rootfs`——INPUT ok + ABS 域实录（§5）。
7. **smoke.sh 全套**（§3.7）——`just smoke` 八相绿。**归主线串行落**（规划 §4 :97）。
8. **smoke-smp 同步**（§3.8）——`just smoke-smp` 绿。
9. **记账面收官**（§3.9）：AGENTS.md/规则 07/索引/沿革追记（含两项首跑实录）。

---

## §7 开放问题与挂账

1. **ABS 域首跑核验（义务）**：absinfo max 32767 vs 32768 两域六字面量已推演同值，但真值表以首跑 `gui-inputd: up … cal=x[0,N)` 实录 N 为准；若域值意外（min≠0 或 N 它值）→ 重推字面量 + 改 §4.2 真值表 + 追记。**首跑实录（2026-11-01）：`gui-inputd: up slot=31 q=16x8B cal=x[0,32767) y[0,32767)`——N=32767 与推演字面量同值，真值表锁定，义务闭环**（smoke-smp 相 2 后补探针取证：rootfs+tablet 直启 75s 窗，正片日志无 SMOKE_KEEP_LOG 已清）。
2. **QMP 设备路由首跑裁定**：主案 `"device": "tab0"` 显式寻址；若 QEMU 对 mmio 输入设备 id 寻址报 error → 回退省 device 键（VM 内唯一输入设备，路由无歧义），qmp_inject 单行改动，裁定实录追记。**首跑裁定实录（2026-11-01）三录**：① brew 新版 QEMU 已废 `server=nowait` 简写（解析期即拒）→ `server=on,wait=off`（§3.7 ⑦ 同步）；② 主案 `device:"tab0"` 在 -nographic 无 console 拓扑下被拒——GenericError `Device tab0 (head 0) is not bound to a QemuConsole`（显式 id 寻址走 console 头绑定面，mmio 设备无此绑定）；③ 回退省 device 键实证可用（事件广播进 HID handler 链；探针 + smoke 正片双证）——落地非预判的「单行改动」而是 send() 内主案失败即永降级 + 裁定行落 stderr（smoke 正片实录：`qmp: device=tab0 被拒（…）→ 永降级省 device 键`），smoke run 4 全八相 PASS。
3. **段① §5.3「gift 第 8 插 0x0001_0008 + boot 表序 assert」被本设计取代**：vnet 形态胜出——运行时 `channel_pair()` + extra_gifts 交付 + 静态 INPUT_A/dup 分种，**无 boot 直插定值、无模板硬编**（雷册 A33 的 boot 断言义务只对模板硬编形态执法，本刀不触发即无 assert 义务）。段① 文档该节（:460-475）以本草案为准——偏差记录，不改段①原文（在册文档冻结惯例）。
4. **段① §9-3「run/run-rootfs 增 tablet」之裸 run 不挂**：裸相无 rootfs 无消费者，挂 tablet 只喂 dmesg 噪声；本刀只挂 run-rootfs + smoke 四相。偏差记录。
5. **PS_MAX=16 余量挂账（勘误后真值）**：inputd 入册后 rootfs 相 roster 实测 **13/16**（~~恰满 16/16~~ 设计期推演错值——2026-11-02 GUI-2 段③ 正片双跑实测 `PS ok n=13` 勘误；另 EFI 相 15/16、B4 Boot C 相 14/16 同批定值）。抬帽义务降档：**表满（第 17 注册）前**须先抬 `user/berry-psproto/src/lib.rs` `MAX_ENTRIES`（16→17；REPLY_MAX = 2+17×14 = 240 ≤ 256 仍容）+ ps 消费端渲染宽度复核——非「下一常驻服务立即先抬」。
6. **CHAN_HND_MAX=8 恰满帽挂账**（审稿机制校正）：`kernel/src/channel.rs:34` 的 8 = **channel 消息随行 handle 数帽**（`channel.rs:47` `Message.handles` 数组）——spawn env 礼物块走 channel 消息随行（ktest.rs:887 注释「A2-3 6→8 同步」即 netd 第 7 插时同步抬的同一数组），故 bsh 满配 env 8 插 = 恰满此帽；**子进程 handle 另有进程表，bsh spawn 子不占 env 槽**——满配后 bsh 交互面（run hello 等）零影响。第 9 插（若有）= 内核抬帽另刀 + berry-exec gift 数组 + berry-libc hs + ktest H_HND 四处联动——禁改清单护住本刀不动内核。
7. **段① §9 台账 7-10 重列（不在本刀，防丢）**：⑦ gui-wm 消费侧（GUI-5——gift 端点改经 GUI 服务树分发）；⑧ ABS 域真机标定（仿真域 ≠ 真触控板）；⑨ 多输入设备枚举与路由（现恰一认领，多设备 = 拒面而非竞抢）；⑩ PM 交互脉冲真落地（GUI-6/+，本刀仅占位注释）。
8. **intent 语义注记**：options=0 = 不设 CHAN_OPT_NO_INTENT——INTERACTIVE 借入经消息传播（4-3c 每消息重导 eff，非历史 max 累积）；svcmgr/bsh 自身 declared 恒 UNSPECIFIED（顶档 = 借入不抑制）。若 M4 后观测到 input 事件消息流抬升 bsh 档位干扰调度，回溯点 = svcmgr 预拉段注释（已记录，不预埋代码）。
9. **vprobe did=18 白名单补刀（实施偏差补账，smoke run 1 现场）**：§3 逐文件落点原未列 `drivers/bins/vprobe`——tablet 上车后占用槽 did=18 触 vprobe loud trap（rootfs 相 VIRTIO ok 断链真凶，run 1 炸点）。修复 = 设备类白名单收 did=18（头注 + `VIRTIO_DEV_INPUT` 常量 + 三分支同 A-7 net 先例：健康面由消费者自身标记 INPUT ok 承接）。挂载面契约修订：vprobe 的「合法同席设备类」= {block 恰一, net, input}，其它 did 仍 loud trap。
