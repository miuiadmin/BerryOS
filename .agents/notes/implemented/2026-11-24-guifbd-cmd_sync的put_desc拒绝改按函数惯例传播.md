# guifbd `cmd_sync` 内静默吞掉的 put_desc 拒绝改为按函数惯例传播

Status: implemented

## Problem

`user/bins/guifbd/src/guest.rs` 的 `Vgpu::cmd_sync`（virtio-gpu 控制队列同步命令车，第 2421 行）契约是「任一面失败返回 `None`，调用方定死法」（第 2420 行文档注释）。函数内全部失败分支都是 `None`——`chain_put(..)?`（2431）、`get_u16(..)?`（2447）、`publish(..)?`（2448）、`try_into().ok()?`（2497）与七处 `return None`——唯独把 INDIRECT 头描述符写进 ring 的那一步写成 `let _ = vio::put_desc(...)`：`None` 被吞掉后函数照旧 `publish`、抬 `published` 记账、敲门铃并 poll 10 秒等一个设备永远不会处理的命令，最后以 `None` 返回——调用方只看到 `guifbd: vgpu init fail: <步骤>`，看不到「描述符根本没落环」；而 `published` 已多记 1，v2 异步路 reap 的 used.idx 期望值从此错位。这是 [vblk 死亡面加固](2026-11-24-vblk死亡面加固与display接线.md)与 [gui-inputd `post_events` 执法](2026-11-24-gui-inputd-put_desc拒绝loud化.md)清掉的同一形态（`let _ = vio::put_desc(`）；gui-inputd 笔记把 guifbd 这一处列入遗留，研究报告 §3.4 把它登记在 `put_desc` 静默吞拒绝项下。

## Decision

第 2437—2445 行：`let _ = vio::put_desc(..)` 改为 `vio::put_desc(..)?`——与同函数相邻分支（2431 `chain_put(..)?`、2447 `get_u16(..)?`、2448 `publish(..)?`）同一惯例，被拒即返回 `None`，诊断由唯一调用方 `vio_init` 既有的四行 `guifbd: vgpu init fail: <步骤>`（2794、2805、2821、2835）承担，且 `FB ok vgpu`（2838）缺席即 smoke 标记缺失。不新增控制流、不改文案、不 trap——`cmd_sync` 的契约是返回 `None` 由调用方决定回退 ramfb，与本文件 v2 路 `submit_frame` 的 `self.die("v2 ring 构表")`（2691—2705）不同：那是返回 `()` 的函数，死法只能在函数内定。

`None` 的两个条件来自 `drivers/berry-virtio/src/lib.rs:163—180`：`idx >= lay.qsize`，或四次 `put_u*` 的环页切片越界。本调用点 `idx = 0`、`VIO_QSIZE = 16`（2225）→ 第一条不可达；4096B ring 页、`RingLayout::new(16)` 的 desc 表 256B → 第二条不可达。Some 分支除少一次赋值外零操作差异：正常路径输出与行为逐位不变；`None` 分支不再发布陈旧 desc、不再抬 `published`。

## Alternatives considered

照 vblk 与 gui-inputd 形态打印诊断并 `trap()`（弃——`cmd_sync` 契约是 `None` 交调用方定死法，`vio_init` 已有逐步骤诊断与 ramfb 回退，trap 会把可回退的初始化失败变成进程死亡）；保留 `let _` 只把 `published` 记账挪到 `put_desc` 成功之后（弃——仍会 publish 一个没有描述符的 head 并空等 10 秒）；只打印不返回（弃——同上）；同批处理 vgpud、vnet 里的同款 `let _ = vio::put_desc(`（弃——vnet 属并行会话领地、vgpud 不在产线不可构建，仍列研究报告 §3.4 遗留）。

## Consequences

编译门：`just build-user` exit 0（尾行 `tlsscan: rootfs.cpio x62 ELF 全过`），日志有 `Compiling guifbd`；`cargo check --release --target aarch64-unknown-linux-musl`（guifbd 独立工作区）exit 0，warning 集与基线逐字相同（berry-libc `net.rs` 3 条既有 dead_code，guifbd 自身 0 条）。

端到端（desktop profile、QEMU TCG、256 MiB，带 virtio-gpu 后端即不设 `BERRY_NO_VGPU`，run `1795470798-432305000-59482-host`）：`LAUNCH ok` 第 304 行（起机 +211 s）；第 121 行 `FB ok vgpu slot=28 pmode=1280x800`——该行只在 `guest.rs:2838` 打印，前提是四处 `cmd_sync`（2789、2804、2820、2834）都返回期望的 `Some`，即改后行在那次开机里至少执行 4 次；`FBSERVE ok` 第 122 行、`GFX ok` 第 204 行、`VGPU2 reap/rect` 2 行、devmgr `4 devices: [slot 28] gpu → guifbd ✓, …`；`guifbd: vgpu init fail`、`停刷` 均 0 次。截图 1088×1920 RGB8 解码成功，非黑占比 99.77%；沉浸像素回归四点与参考值逐位相同（`px(100,20)=#01a186`、`px(200,600)=#06a896`、`px(500,600)=#05b67d`、`px(272,1830)=#0065c3`），x=544 列 y∈[1600,1800) 的 R>250 计 0。同机 `VBLK ok cap=2048 slot=31`（第 120 行）、`BLKD ok n=512`（第 205 行）；`wm WAIT 在途悬挂` 2、`err=-13` 0、`wmd died` 0、panic 0。

未覆盖面：`None` 分支在现几何下不可达，端到端只走到 Some 分支。vgpud、vnet 的同款形态未动，见研究报告 §3.4。

## Related

- [gui-inputd `post_events` 的 put_desc 拒绝 loud 化——vblk 同款执法](2026-11-24-gui-inputd-put_desc拒绝loud化.md)——同形态执法的前置刀；其遗留清单里的 guifbd 一项由本刀清偿。
- [vblk 死亡面加固：trap 先复位设备、put_desc 与授窗拒绝 loud 化、分区行接 display()](2026-11-24-vblk死亡面加固与display接线.md)——`let _ = put_desc` 形态的第一刀。
- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§3.4 健壮性审计后续，同类面的登记处。
