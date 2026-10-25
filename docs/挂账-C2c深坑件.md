# C-2c 挂账移交材料（退场会话 → 挂账刀）

2026-10-25。裁决后退出口径：中批 6（head/wc/cut/tr/sort/uniq）留、深坑 6（tail/env/date/grep/find/which）挂账专项另刀。
原文档由退场会话（berryos-99）落 /tmp/c2c-drafts/，因 /tmp 易失由主会话转入仓库；源码归档随迁 `docs/挂账-C2c深坑件/`。
四节：① stubs 二分实证清单 ② fs gift 根因+修法 ③ onig 互斥实证 ④ wc/date 宿主钉板字节。

## ① berry-libc 十二桩二分（链接探针实证，2026-10-25）

方法：仓库工作区+lock 原样复制到探针目录，剥掉 C-2c 十二桩后按家族 recipe 链接，
正对照（date/tail/env 塞回 members）验证探针确能抓缺符号。探针可复现（同法重跑即可）。

**必留 4 桩（uu_sort 链真消费——不是 head/wc！）**：

| 桩 | 引用点（rcgu.o 符号级） |
|----|----|
| `nl_langinfo` | `uucore::features::i18n::datetime::get_locale_months`（sort -M 月份名本地化） |
| `setlocale` | 同上 |
| `exp2` | `bigdecimal::BigDecimalRef::inverse_with_context`（sort 数值排序的 bigdecimal 链） |
| `log10` | `bigdecimal::arithmetic::pow::pow_u64_with_context` |

head/wc/cut/tr/uniq 五件剥 12 桩后链接全绿、bin 产出实证——**零依赖**。
收敛刀已按此实证回补：保留 4 桩全归因 uu_sort（原注释「uu_date/uu_wc 可达」系批次并集 probe 的过度归因，已修正）。

**可退 8 桩（按消费命令归属，收敛刀已退）**：

| 桩 | 唯一消费方（正对照实证） |
|----|----|
| `inotify_init1` | uu_tail（-f 尾随面） |
| `epoll_create1` / `eventfd` | uu_tail（mio/inotify 链——原注释写「uu_date/std 面」不准，按此归属） |
| `sigprocmask` | uu_env（apply_signal_action/block_signal） |
| `__libc_current_sigrtmin` / `__libc_current_sigrtmax` | uu_env（realtime_signal_bounds） |
| `clock_getres` / `clock_settime`（sys.rs） | uu_date |

**挂账刀须知（tail）**：探针 lock 重解析后新 inotify/mio 还会要求 `inotify_rm_watch`、`epoll_ctl`
等原 12 桩之外的符号——tail 挂账刀的桩面比本批已知的更宽，届时按「loud 拒链 → 补到绿」现探现补。
date 全量四件 = clock_getres/clock_settime/nl_langinfo/setlocale（locale 双件与 sort 共享，见必留表）。

## ② fs gift 根因 + 修法（文件面 U3 步排障结论，未实施）

**根因**：bsh 的 `run_capture` / `pipe_capture` / `run_output` 三个 spawn 点
`extra_gifts: &[]`——子进程无 gift[2]（rootfs READ dup）/gift[3]（fs client SEND|RECV dup），
berry-libc `fs.rs` 头规则「无 fs gift（fs_h()==0）的进程：一切文件 op 优雅降级 ENOENT」→
uu_cat `File::open("/u3fix.txt")` ENOENT →（stderr 未接管）exit 1 → libc exit 钩子
`send_stdout_eof()`（零长 EOF）→ 管道末段空输入 exit 1。观测 code=1/len=0/eof=true 与此完全吻合。
（std→libc 符号面无缺口：rust-src `sys/fs/unix.rs` cfg 实证 musl 走 `open` 别名分支，非 open64。）

**修法（挂账刀直接可用）**：bsh 加 `fs_gifts()` 助手返回
`[berry_handle_duplicate(H_ROOTFS, RIGHT_READ), berry_handle_duplicate(fs_h, RIGHT_SEND|RIGHT_RECV)]`
（样板 = svcmgr `uu_run`，U1 九步含写路径已证两 dup 足量）；三个 spawn 点接线。
**berry-exec 契约**：`SpawnSpec.extra_gifts` send 时全被 take——调用方保留原 handle 必须先 duplicate；
pipe_capture 每段现 dup 现传（berry-exec 全部 Err 早退点均在 env send 前——gifts 未 take 仍留本表，Err-return 清理须照 uu_run 样板显式补关双 dup，与 pipe_capture 现存 Err 块关未 take 管道端/out 对同型；channel_send 本身失败则 berry-exec trap 自陷、无清理面）。
**v1 约束**：单管道至多一个 fs 消费段——共享单会话 fs 通道上双消费者 = 应答偷取/会话错位。

> 本刀七步 U3 全 stdin 驱动（feed 常量直灌，无文件 open 步）故未触发此雷；
> 该布线留给挂账刀或 bsh 通用 run 面扩展时一并做。

## ③ onig 互斥实证（自写 grep/find/which 的上游缺位记录）

- `uu_grep 0.2.0`（crates.io 线顶）与 `findutils 0.10.0` 均**硬依赖 onig/onig_sys**（C 正则库，
  经 cc 走 C 工具链构建），**无 feature 门**可关——2026-10-25 薄 crate probe 实证。
- berry-ld 链 = 纯 Rust（零 musl 字节、零 C 工具链）——结构性互斥，非配置问题。
- `uu_which`：crates.io 无发布线（无版本可锁步）。
- 故三件落地为自写语义子集（grep 字面子串 `-F` 默认 + `-i/-v/-c/-n/-q/-l`；
  find `-maxdepth/-name/-type`；which PATH 存在性——v1 STAT mode 恒 0644 无 x 位偏差头注明示）。
  自写三件源码已随仓库归档在 `docs/挂账-C2c深坑件/bins/{grep,find,which}`。

## ④ wc/date 宿主钉板字节（=0.12.0 同源直跑，冻结断言用）

- wc（`      4       8      42`——三列定宽，**六个前导空格**；4 行 8 词 42 字节输入）：
  纯格式纯函数，无分叉面。**wc 在保留批内，此钉板现役可用。**
- date（`Thu Jan  1 00:00:00 UTC 1970`——`date -u -d @0`）：date 挂账，钉板留档。

## 源码归档（docs/挂账-C2c深坑件/）

- `uu/{tail,env,date}/`：三件薄 harness（同保留六件模板——CRT0 锚 + uumain 直传；
  挂账刀直接拷回 `user/bins/uu/` 恢复 members 即用）。
- `bins/{grep,find,which}/`：自写三件完整源码（独立小 workspace 形态，含各自 Cargo.lock；
  挂账刀按当时树况决定接回形态）。
