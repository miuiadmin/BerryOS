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

> **2026-10-27 C-2 刀 2 进展注**：fs 消费子进程的样板已立——bsh 新
> `run_capture_fs`（本节修法的 df 消费版：rootfs READ dup + fs SEND|RECV
> dup 四元装配 + stdin 补空 feeder 满足 berry-exec 在位断言；`DF ok` 步②
> df 经此路全绿）。**三个存量 spawn 点（run_capture/pipe_capture/
> run_output 裸形态）接线仍归本挂账刀**；另 du 表观尺寸步同刀实录死于
> std 线程面（uu_du 0.12.0 无条件 thread::spawn——uu_sort 同型，dump 与
> 处置详见 bsh df_selftest 头注 + `docs/阶段沿革.md` C-2 刀 2 条目），
> 执行面回收并入本刀 sort·head 批。

> **2026-10-28 T-1 收口注记**：上注「std 线程面缺口」归因被**法证推翻**
> （`docs/法证-TLS原位方案崩因归因.md`）——du/sort 启动期死的真因 =
> **crt0 .tbss 初值污染**（原位 TLS 方案不清零、.tbss 文件字节非零区被当
> 数据用），非 pthread/TLS 平台缺口。T-1（拷贝式 TLS + pthread/futex
> 真身 + 0x35 thread_start，`docs/T-1实施设计草案.md`）落地后：U3 sort
> 段恢复（五步→六步）、DF du 表观尺寸步恢复（三步→四步），双双入 smoke
> rootfs 相常绿。三个存量 spawn 点接线**仍挂账**（与线程面无关，归后续刀）。

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

---

## ⑤ 挂账刀收账实录（2026-10-27，四节全部归位，本文件转归档）

**裁决**：①②③④ 四节全部由 C-2c 挂账刀（2026-10-27）落地收账；源码档案照旧留档。

- **① 桩面归位**：tail 补桩七枚 ENOSYS（epoll_create1/epoll_ctl/epoll_wait/eventfd/
  inotify_init1/inotify_add_watch/inotify_rm_watch——§①「现探现补」预言应验：lock
  重解析后比本档已知 12 桩多出 epoll_ctl/epoll_wait/inotify_add_watch/
  inotify_rm_watch 四枚；
  -f 尾随链 v1 无实现、运行期不可达，U4 自验无 -f 步）；env = sigprocmask 空掩码
  手写桩 + `__libc_current_sigrtmin/max` 定值 34/64（musl 档）；date = clock_getres
  谎报 {0,1ns} 成功（上游 unwrap 档失败即 panic）+ clock_settime EPERM。uu_sort
  专属 exp2/log10 零新面不动。
- **② fs gift 接线**：C-2 刀 2 已立 `run_capture_fs` 样板（§② 修法的 df 消费版），
  本刀把三个存量 spawn 点接线：`run_output`/`pipe_capture` 走 gift 装配（v1 单
  fs 消费者约束——gifts 只随管道首段）；`run_capture` 保持裸形态（其消费面
  tail/env/date 均无文件 IO）。
- **③ 自写三件落地**：`user/bins/{grep,find,which}/`（stdhello 同款独立小工作区，
  berry-ld 直链）；uu 家族 members 20→23（tail/env/date 拷回恢复）。
- **④ date 钉板消费 + 一枚桩侧雷实录**：U4 步③ 断言即 §④ 钉板字面
  `Thu Jan  1 00:00:00 UTC 1970`。**首跑实录**：nl_langinfo 原「问啥都答 UTF-8」
  把 uu_date 的 _DATE_FMT（0x2006c——uu_date locale.rs 手卷的 glibc nl_item，libc
  crate 未收录故上游自定常量）查询也答成 "UTF-8"——get_locale_format_string 拿它
  当日期格式串、ensure_timezone_in_format 再补 " %Z" → date 默认输出恒
  "UTF-8 UTC"（u4_bad 四面 dump `hex=5554462d38205554430a` 实锤）。**修法 = 按
  item 分诊**：CODESET（14——libc crate linux 面 glibc 序）→ "UTF-8"，其余 → ""
  （C locale 之外的 locale 数据 v1 一律不支持；uu_date 的 !is_empty 过滤器吃空串
  落内建回退 `"%a %b %e %X %Z %Y"` = 钉板同序输出；uucore get_locale_months 的
  ABMON 面有 should_use_icu_locale() 前置门——LC_TIME 缺席 → "und" → false——
  运行期不可达，空串不会 poison 月份名）。
- **lock 实测**：uu/Cargo.lock 238→263 = 25 包新增、0 包移除 + 2 行依赖边
  再限定（三镜审查账目修正：原式「25 adds + 2 removals」把行级删除混进包级
  账目——238+25−2=261≠263）；adds 比计划 24 多一枚 = getrandom 0.2.17 第二版本
  （const-random-macro 链引入，与 0.4.3 共存）；2 行删除为良性再限定（tempfile
  的 getrandom 依赖收窄到 0.4.3）。存量 238 零漂移。
- **U4 八步**（bsh `u4_selftest`，`U4 ok` = rootfs 相第 30 标记，链序排 fat 族前
  同 U2/U3 契约）：⓪ `tail -n 2` → ① `head -c 8`（fd 克隆桩 fcntl F_DUPFD 活证）
  → ② `env U4V=zz`（environ=NULL 锚：空输出 rc0）→ ③ date 钉板 → ④ `grep -n
  bravo /u4.txt`（自写 grep 对 FILE 参恒带文件名前缀——**计划字面修正**：期望
  `"/u4.txt:2:bravo\n/u4.txt:4:bravo\n"` 非 GNU 单文件省名形态 `2:bravo\n…`）
  → ⑤ `cat|grep -c` 管道 → ⑥ `which wc`（PATH 缺席默认 /bin + stat 存在性）
  → ⑦ `find /etc -name 'mt*'`（fsmgr mtab 预建消费面复用 → `/etc/mtab`）。
  步④/⑥ 用后即 unlink（fstree 帽 9/17 窗口让位后续 dir/cd 自验；⑦ 纯只读
  消费预建树零创建零清理——三镜审查勘误：原记「④/⑥/⑦ 用后即 unlink」）。
- **sort/du 处置**：P4 实测判决双双不可行——崩点在 std `run_spawn_hooks`
  （早于 pthread_create），线程面平台缺口依旧；两件挂账维持（bin 仍随 rootfs
  发行，验收面留缺口解除另刀）。§① 预言的 tail 宽桩面已闭环，无新挂账。
  **（2026-10-28 T-1 段⓪法证勘误**：崩点定位不变——确在 run_spawn_hooks、
  早于 pthread_create——但根因归因改写：非「线程面平台缺口」，而是 berry-libc
  crt0 **原位 TLS 初值污染**（.tbss 窗 = 活 .data.rel.ro 文件字节，三方共违约
  .tbss 零初始化契约），家族全扫 24/38 带毒（uu 家族全军覆没）——详见
  `docs/法证-TLS原位方案崩因归因.md`。「挂账维持」结论不变；解锁钥匙 =
  TLS 提供面修复刀（crt0 拷贝式），修后 sort/du 才首次真正抵达
  pthread_create=EAGAIN 第二层。）
- **三镜审查修复轮（2026-10-27，18 findings——修 15 / 驳 3）**：
  grep 四修：退出码 GNU 口径（旧式 `any != o.inv` 在 -v 下两态恰反转——
  有选中退 1/零选中退 0；改 `-q 命中 0 最优先 → 错 2 → 选中存在（-v 同权）
  0 / 否则 1`）、不剥尾 `\r`（剥掉则含 \r 的 pattern 永不中且命中行输出少
  一字节）、文件参 read<0 loud（原 `n<=0` 当 EOF 静默截断——真码 1 与
  「无命中」同形，恰 C-2c 步 2 教训要防的盲区）、**幻影行首跑实抓**
  （`0..=buf.len()` 对 \n 收尾缓冲造第 N+1 空行——-v 回归钉板逼出：GNU
  口径 \n 收尾文件恰 N 行、空缓冲零行，修 `start==buf.len()` 边界不造行）；
  find 两修（走查错置 AtomicBool 退 1——原恒 0 且头注误标「GNU 同形」；
  尾斜杠 `trim_end_matches('/')` 消 `/etc//mtab` 双斜杠）+ glob 指数回溯
  局限头注明示（info 级只记录）；fcntl F_DUPFD 边界执法（arg<0 或 ≥
  FD_BASE+FD_MAX+ALIAS_MAX=15 → EINVAL——POSIX 口径，原负参被钳 3、超窗
  归 EMFILE）；U4 步④ 扩四断言回归钉板（`-n` 选中 + `-n -v` 反选 +
  `-v` 全空退 1 + `-q` 带不可读参仍退 0 且 stderr 入比对面——spawn 合计
  9→12）+ 步② 弱断言成因注记（environ=NULL 桩面，setenv/environ 落真时
  改常量 `"U4V=zz\n"`）；文档勘误五处（本档 §⑤ 差集三枚→四枚补
  epoll_wait、lock 账目 238+25−2=261≠263 包/行混账、⑦ 零创建零清理、
  AGENTS smoke 行 rootfs 29→30、uu/Cargo.toml 设计理由 20→23、CLAUDE/AGENTS
  run-efi 段字节数刷新（kfile 3394152B / init 28608660B / load pages=133 /
  MMAP pages=17426——主树工件注入副本 image 直靴实测）。驳三：仓库根
  cargo-target/ 游离目录（他人探针产物不碰——处置权在主会话）、build-user
  时点不可复现（并发 ps 刀在途编辑态，非本刀文件——本刀构建链独立闭环 +
  隔离副本复跑全绿）、smoke.sh PASS 行金丝雀计数为 RX 相残留变量（既有
  形态 info 记录，另刀改逐相计数）。修复轮验收：abi-check
  PASS + test-host 338/0 + 五相 smoke 全绿（U4 ok 含新四断言，rootfs.cpio
  28,608,660B）+ smoke-image 复跑全绿（EFI 十段 + 生态 16 含 U4 ok +
  字节对拍 + RX 活性复验）。
