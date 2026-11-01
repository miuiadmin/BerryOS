#!/usr/bin/env bash
# BerryOS 无人值守冒烟测试 —— CI 与本地同一入口（just smoke）。
#
# 做八件事：
#   1. 后台拉起 QEMU virt（-kernel ELF 直加载），串口重定向到临时日志；
#   2. 断言 boot 标记全部出现（BerryOS boot / GIC ok / … / ktest done）；
#   3. 断言 switches=N 金丝雀 ≥3 条且严格递增（IRQ 抢占分时活着、计数不冻）；
#   4. G-eco rootfs 相（G-eco-c 起）：重拉 QEMU 带 -fw_cfg rootfs.cpio，断言
#      入机侧（ROOTFS ok/vmo 行）+ 编译侧全链（rootfs hello = hello 进程
#      自打、ECO ok = svcmgr 流式 cpio/ELF 拉起成功、STD ok/STD err ok =
#      stdhello 经 musl target + berry-libc shim + 流式装载全链自打——
#      G-eco-d d-1c）标记到齐——「rootfs 程序经全链跑通出字」的关卡化
#      （裸跑相 rootfs 缺席 = svcmgr 静默降级，26 标记零扰动，两相互证；
#      B-0 起裸相含 BENCH ok n=6——内核微基准 + churn 泄漏金丝雀关卡化；
#      R-0 起含 BENCHWAKE n=——唤醒延迟直方图 boot 尾形状行；
#      S-2 起含 BENCHXH ok——资源耗尽/恢复六面确定性关卡化；
#      B-1 起含 B1 ok n=4——EL0 用户态四项微基准（bench1 spawn + bsh
#      格式断言后 relay 收口，rootfs 相第 28 标记）；
#      C-2 刀 2 起含 DF ok——df + statfs 真值面（statfs 几何定值 +
#      RW.TXT 重定尺寸差分（相位无关）+ df 表格三步全过才打，rootfs 相
#      第 29 标记；原案步③ du 表观尺寸首跑实录裁出挂账——uu_du 0.12.0
#      无条件 thread::spawn 踩 std 线程面平台缺口（C-2c uu_sort 同型）；
#      T-1 起 du 步回收（crt0 TLS 初值污染破案 + L1/L2b pthread 面落地，
#      `du -b PATTERN.BIN` 原案字面回嵌，见 bsh df_selftest 头注））；
#      C-2c 挂账刀起含 U4 ok——深坑六件 tail/env/date/grep/find/which
#      执行面八步（tail -n 2 末两行/head -c 8 fd 克隆活证（挂账 §①
#      fcntl F_DUPFD 面闭环）/env U4V=zz 空环境弱断言/date -u -d @0 epoch
#      字面/grep -n 文件参数（fs gift）/cat|grep -c 管道/which wc 定位
#      /find /etc -name 走查），rootfs 相第 30 标记。
#      C-3 起含 PS ok n=/BC ok——ps 查询 RPC + berryctl 三子命令（svcmgr
#      ps_selftest/bc_selftest：账本全表编码 + 家名单断言 + ver/ps/mem
#      三 op 渲染行逐字节对拍，rootfs 相第 31/32 标记）。
#      C-4 起含 TAB ok——bsh Tab 补全 + 行编辑接管自验九步（console 退化
#      逐字节透传、编辑/echo/历史/Tab 全归 bsh editor：FakeSrc 状态机
#      回归面 ⓪-⑥ + 真源两钉 ⑦⑧（cpio bin/ps + libc /etc/mtab）），
#      rootfs 相第 33 标记。
#      C-4b 起含 INT ok——Ctrl-C kill 面自验四步（editor 0x03 作废行/
#      ESC 序列两态共存/Interrupt 后续用 + 前台杀链真机面：spawn
#      bin/hello → process_terminate(130) → collect_fg wait_many
#      TERMINATED 收口（EOF 缺口活证——内核杀不走 libc exit 族）→
#      reap 130 即 $? 真码），rootfs 相第 34 标记（链序同 TAB 排 fat
#      族前——EFI 路无盘亦打）。
#   5. P5 RX 交互相（**rootfs+FIFO 合相**，P5 A-1 起为第三跑；注入序列
#      P5 v0 起扩为编辑三段、P6 A-6 起四段、C-0 刀 4 起五段、C-1 刀 5 起六段、
#      C-4b 起七段、GUI-4 段② 起八段（+h QMP 注入往返））：三拉 QEMU 带
#      -fw_cfg rootfs + FIFO stdin（holder 子进程占住写端防 EOF），73 标记
#      （boot 27 + rootfs 46——PIPE ok/VIRTIO ok/VBLK ok/BLKD ok/FAT ok/
#      FATW ok/NET ok ping=/DNS ok addr=/TCP ok echo=/LSN ok n=7/TCPS ok echo=/
#      APP ok n=/
#      DIR ok n=/U1 ok/U2 ok/U3 ok/B1 ok n=4/DF ok/U4 ok/TAB ok/INT ok/POLL ok/
#      EVS ok n=2/SKT ok n=3/EPL ok n=8/NETS ok n=2/MMP ok n=9/CLK ok n=6/
#      SIG ok n=6/PS ok n=/BC ok/
#      M3DEMO ok n=5/THR ok/INPUT ok 在列 = bsh 已进交互读循环/virtio 探测与块设备 IO/块设备服务
#      消费链/FAT 只读底座与**落盘写**/网络往返与网络服务/TCP 客户端与 TCP
#      服务/TCP 服务端监听面（A2-2 段③ hostfwd 环回自连自验）/.bap 应用拉起
#      消费链/RAM 目录树 v2 五 op 自验/uutils core 家族
#      空文件元数据九步链 + echo/true/false 执行六步链 + text 批六步执行链
#      （sort|uniq 步与四段链 T-1 回收）+ EL0 四项微基准格式断言收口（B-1）
#      + 深坑六件八步执行链（U4——C-2c 挂账刀）+ Tab 补全九步（TAB——C-4）
#      + Ctrl-C kill 链四步（INT——C-4b）+ 多路等待聚合器四步（POLL——A2-1，
#      bsh poll_selftest）+ 事件循环 echo server 两连接对拍（EVS——A2-3，
#      bsh evs_selftest 经 netd 会话面）+ socket fd 皮三段整链（SKT——A3-3，
#      bsh skt_selftest：client/非阻塞错误面/server 段对拍）+ epoll+DNS 八步
#      整链（EPL——A4-3，bsh epl_selftest：epoll 三桩真身槽满/拒面/LT 复报/
#      短帽/非 socket 拒 + getaddrinfo 快慢双路/非法名本地拒/close 释槽复用）
#      + std::net spawn 面两步整链（NETS——A5-2，bsh nets_selftest 经
#      net_gifts 三元 gift 链 + std::net 深水面：正步 10.0.2.2:18080 echod
#      逐字节对拍 + localhost 负控——SLIRP 环回不可达定谳形状，std DNS 链
#      活证）+ mmap 匿名档五步整链（MMP——A7-1，bsh mmp_selftest：双页
#      首映射两页写读/hint 忽略档落窗/munmap 三态（内部地址 EINVAL·恰长
#      0·重复 0）/空洞复用回首址（first-fit 钦定）/拒面三连+mprotect 双档
#      ——extern 声明链接期解析 berry-libc mmap 真身，零 fs 盘面纯
#      vmo/vmar 桥，链序 A 系契约排 fat 族前）
#      + 里程碑演示
#      五步（M3DEMO——C-5）
#      + 进程内多线程消费面（THR——T-1，svcmgr thr_selftest）
#      + input 服务链（INPUT——GUI-4 段②，svcmgr input_selftest）
#      全过，
#      状态谓词门控注入时机，不赌 boot 秒数）后
#      **逐字节慢速注入**编辑序列（FEN=0
#      FIFO 深 1 字节——慢速 = 单字节-per-handler 的标记确定性，消竞速；
#      QEMU chardev 流控押住输入不 overrun——真机才有突发丢字）：
#        a. `rx\n` → console token 面 `RX ok`（A-1）；
#        b. `helll\x7f`+`q\n` → 退格编辑得 `hellq` → 转发 bsh →
#           `bsh: hellq: not found`（A-2 行编辑 + 转发 + bsh 应答；注入词
#           刻意非 cpio 在档名——C-2a 起裸名 = bin/ PATH 派发，真名会真跑）；
#        c. `\x1b[A`+`!\n` → 历史召回 `hellq` + `!` → 转发 →
#           `bsh: hellq!: not found`（P5 v0.5 mini-readline ESC[A 上翻；C-4
#           起编辑归 bsh editor.rs——语义逐字对齐，rx 行亦入 bsh 史故
#           newest 恒 = hellq）；
#        d. `run fail\n` → `$?`=42 → `echo f$?t > OUT.TXT\n`（大写 8.3 名
#           → FAT 层落盘，内容 "f42t\n"）→ `cat OUT.TXT\n` 读回——断言
#           grep 'f42t'：命令文本只含未展开的 `f$?t`，串口回显逐字节散布
#           非连续，f42t 只能来自真实文件内容（A-6 `$?` 展开造内容 =
#           消回声歧义的确定性状态谓词）；
#        e. `mkdir d1` → `cat NOPE.TXT`（FAT Miss → $?=127）→
#           `echo d$?x > d1/Z.TXT`（多段 `>` = RAM 树写，内容 "d127x\n"
#           ——`$?` 展开造内容消回声）→ `cat d1/Z.TXT`（grep 'd127x'）→
#           `cat d1`（目录 GET → ST_BAD → grep 'bsh: cat: d1: bad path'
#           ——`bsh: ` 前缀回显不可达）→ `rm d1/Z.TXT` → `cat d1/Z.TXT`
#           （grep 全行 'bsh: cat: d1/Z.TXT: not found'——UNLINK 生效）；
#        f. `cd d1` → `pwd` → grep '/d1'（C-1 刀 5：注入行不含 '/d1'，
#           段 e 断言串均无前导斜杠——只能来自 getcwd 渲染应答 = libc
#           CWD 全链交互面活覆盖）。
#      b/c/d/e/f 五段断言互非子串。随后再验 switches 金丝雀严格递增（IRQ 分流
#      破坏屏蔽态/EOI 漏发类 bug 的死状 = 冻而不死，kill -0 活性检抓不到）。
#   6. C-0 刀 4 FAT 子目录相（第四跑）：盘 = mkfat **--fixture 模式**（fixture
#      专列 DATA/ 目录 + DATA/NOTE.TXT——默认盘不含，默认盘字节零漂移硬线
#      不破；独立盘 disk_dirs.img，与 disk.img 链零交集），73 标记载明全链
#      在目录盘上照跑后注入三段（断言与注入行互非子串 + 顺序防回声）：
#      `ls DATA` → grep 'NOTE.TXT'（READDIR FAT 回落链走查——断言先于一切
#      含 NOTE.TXT 的注入行回显）→ `cat DATA/NOTE.TXT` → grep
#      'subdir note'（多段 GET 全链：bsh→fsmgr resolve→子目录表→簇读→vblk）
#      → `cat DATA` → grep 'bsh: cat: DATA: bad path'（目录 GET → ST_BAD
#      类型冲突面）。
#   7. P6 A-6 FAT 持久化相（第五跑）：**不重造盘**（复用第 5 相已写的
#      disk.img——mkfat 重产 = 持久化零验收；disk_dirs.img 不复用——无
#      跨盘盘混），73 标记载明写过的盘再起（bsh FATW 自验走覆写路径 =
#      旧链回收/新链分配幂等重过）+ 注入 `cat OUT.TXT` → grep 'f42t'——
#      `>` 落的是真扇区、跨 boot 存活。盘 cache writeback=off：QEMU 由
#      SIGTERM 终止，writeback 模式可能不刷页缓存（所有 -drive 行统一，
#      含 justfile run-rootfs）。
#   8. B4 掉电相（第六-八跑 = 采样 ×2 + 注入 + 恢复终验，独立盘
#      disk.pwr.img）：掉电安全首刀关卡化——SIGKILL 直拟掉电
#      （cache.writeback=off 下已写扇区已落宿主盘 = **扇区间**撕裂前缀；
#      扇区内撕裂 = 真机面，不在本相覆盖）。Boot A：FAT ok 后 +18s
#      SIGKILL 落 fatw 新文件写窗（staging ≈16 PUT ≈13s + commit ≈10s）
#      → pwrcls 采样行，断言按 kill 时相分两档（状态谓词、非时序赌）：
#      FATW ok 未见于日志 → ∈ {noent, clean}（新文件 ④ 空转、⑤FAT 先于
#      ⑥′根——粗检 canary：执法「根在场而 FAT 撕裂」类现序下不可达
#      态，对写序回归零证伪力）；已见 → ∈ {clean, poison}（已进覆写面，
#      同 Boot B 口径）。Boot B：FATW ok（首写完）后 +15s SIGKILL 落 DF
#      覆写 resize commit 窗 → 采样行断言 ∈ {clean, poison}（整扇区 FAT
#      写下 midchain 结构性不可达——扇区原子性面的负向断言）；随后
#      pwrcls --poison **确定性注入毒环态**（RW.TXT 整链双 FAT 置
#      Free、根目录原样 = 覆写 commit 撕裂窗「FAT 已写、根未写」的盘面
#      形状——自然采样撞窗是概率事件，注入 = 每轮必达的关下面）；
#      Boot C：毒环盘上全链重跑——FATW 覆写走恢复 commit（smoke 单文
#      件覆写负载下旧首簇 = 最低 Free 必复用进新链头；一般负载可中段
#      复用/不复用——fsmgr ④ 守卫按 ∈ 新链成员制设计，自愈不依赖头部
#      复用）+ DF/CD 照跑，盘侧 pwrcls 终验必 clean（FAT0 侧——FAT1
#      分歧不在 oracle 面，评估 §5.2；guest 标记与宿主盘面双面不变
#      量）。零新 guest 标记（全 script 侧门控，标记数组零耦合）；三
#      boot 均不带 netdev（vnet 无 net 槽 trap 自死为干净死，NET 族标
#      记自然缺席不设门——EFI 路「设备缺席优雅降级」同款先例）。
#
# 注意：QEMU TCG 下 CNTV tick 实测 ≈ SLICE_HZ/2（见 规则/08-QEMU与仿真.md / 雷册 §A8），
# 切换率 ≈5/s，100 次切换 ≈20s，故金丝雀截止 180s 足够宽。
# 调试：SMOKE_KEEP_LOG=1 just smoke 保留串口日志不删。
set -u

# 内核路径解析收敛在 paths.sh（单一事实源，justfile/CI 同源；含非交互 PATH 修补）。
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"
KERNEL=$(berryos_kernel debug)

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }

# 2b-1 高半翻转程序头断言（内存管理设计 §5.1：VMA≠LMA 防漂移哨兵——
# boot 段 vaddr==paddr、主体 vaddr-paddr==KERNEL_VA_BASE，任一漂移即 FAIL）。
python3 "$(dirname "$0")/size.py" --assert-flip "$KERNEL" \
  || { echo "smoke: FAIL: 高半程序头断言失败" >&2; exit 1; }

# ABI 单源守卫：abi.toml 生成的三面产物（内核 abi_gen.rs / berry-sys abi_gen.rs /
# ABI 文档 marker 块）逐字节比对——改了源忘再生成 / 手改产物即 FAIL（便宜且早响，
# QEMU 之前）。
python3 "$(dirname "$0")/gen_abi.py" --check \
  || { echo "smoke: FAIL: ABI 单源守卫失败（改了 abi.toml 跑 just abi-gen）" >&2; exit 1; }

DEADLINE_BOOT=120    # 秒：boot 全部标记到齐（裸相）
DEADLINE_ROOTFS=420  # 秒：rootfs/RX/持久化三相标记到齐——P6 A-6 起 FATW
                     # 自验含整链 commit（fsmgr 侧 ~13 次串行扇区 RPC，TCG
                     # 下每次 ~0.8s）+ GET 读回 ≈ +30-40s，120s 结构性不够；
                     # T-d 起 svcmgr TCPS 三 op（每 op 一轮 TCP 会话相位，
                     # TCG 下三 op ≈ +15-25s）叠加，240s 余量见底 → 300s；
                     # C-2a 起 U1 九 spawn 串行链再 +~40s（实测 U1 ok 落在
                     # switches=400 后，TCG ~5 switches/s），300s 帽常态仍
                     # 余 ~100s（帽只兜结构性挂死，常态全链 ~190s 内到齐）；
                     # C-2b 起 U2 六步再 +8 spawn（实测全 26 标记 ~165s 到
                     # 齐，TCG 波动域内），300s 帽不动；C-2c 起 U3 再 +8
                     # spawn（原案七步门审裁形为五步——步⓪①② 各 1 + 步⑤
                     # cut|tr 双段 2 + 步⑥' 三段链 3；head/sort 深坑挂账
                     # 裁出，见 U3 注块）——run-rootfs 实测 U3 ok @91s
                     # （含造盘/boot 走查 + U1/U2/U3 全链；fat/网络族在其
                     # 后），smoke 五跑全绿 27/27，300s 帽常态余量充足；
                     # B-1 起再 +1 spawn（bench1）+ ~8.6k 次 SVC（四项
                     # iters 合计——TCG 下 ≈ +5-10s，boot 相当量级），
                     # 300s 帽不动；C-5 起 m3demo 五步链每相再 +~45s
                     # （⓪cpio 走查 ~2s + ①ls ~6s + ②uu_cp 实测 ≈17s
                     # 慢而活——collect 帽 30s 定标实录（U1_EOF_CAP 同
                     # 口径）+ ③④ ~18s，run-rootfs 定向 CD ok→M3DEMO ok
                     # ≈45s），×3 boots 常态 ~240s 仍 < 300s，帽不动
                     # （五相复跑若显压力再升）；**A2-3 起 300→420**：
                     # EVS 段入 bsh 链（LISTEN+accept-wait+两连接
                     # RT/DRAIN/CLOSE——CLOSE 释槽走 vnet TCPS_CLOSE_CAP
                     # 30s + 尸槽回收每条 ~30s，worst ~60-130s）+ TCG
                     # 慢夜（持久化相 boot→EVS ok 实测 ~200s）叠加，
                     # 300s 帽首跑实录近失：64/65 到齐唯缺链尾 M3DEMO
                     #（bsh 阻在 m3demo 内、无 bail、switches 爬至 2400
                     #——脏盘相 FATW/T3.TXT 覆写 + 旧 staging 隐式
                     # commit 本就是最慢相；rootfs/RX/子目录三相同链
                     # 全绿 = 预算题非正确性题）；420 与 evsclient 总帽
                     # 480 保持 60s 余量序（client 先于 QEMU 起、帽序
                     # 不能倒）；**A3-3 起 SKT 链入 bsh 尾**（evs 后：
                     # ⓪ client 段单连接 RT ≈ T-c TCP 同量级 + ①纯本地
                     # 面 + ②两连接对拍 ≈ EVS 同量级——合计每相估
                     # +10-30s，420 帽首跑盯实测，压力显性再升）
DEADLINE_CANARY=180  # 秒：等 ≥3 条 switches 金丝雀
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke.XXXXXX")
QEMU_PID=""
RX_FIFO=""
RX_HOLDER=""
# T-c：宿主 TCP 回显服务（echod.py）进程与日志（cleanup 兜底收）。
ECHO_PID=""
ECHO_LOG=""
# A2-3：EVS echo server 宿主对拍 client（evsclient.py）——**每相一实例**
#（rootfs/RX/子目录/持久化四相各起一记：guest 每次 boot 各跑一遍 bsh
# evs_selftest；B4 三 boot 无 netdev 不起——EVS 结构性缺席不设门）。
EVS_PID=""
EVS_LOG=""
# A3-3：SKT socket fd 皮宿主对拍 client（evsclient.py 18083 SKT 参数化
# 复用——guest bsh skt_selftest server 段两连接对拍；生命周期/收口与
# EVS 对同款每相一实例，B4 无 netdev 不起）。
SKT_PID=""
SKT_LOG=""
# GUI-4 段②：QMP 通道——RX 相专用（QMP_DIR/QMP_SOCK，RX 相 launch 前赋值）。
QMP_DIR=""
QMP_SOCK=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  # P5 A-1 交互相资源：holder 子进程 + FIFO（exec 3>&- 由阶段尾自关）。
  if [ -n "$RX_HOLDER" ] && kill -0 "$RX_HOLDER" 2>/dev/null; then
    kill "$RX_HOLDER" 2>/dev/null
    wait "$RX_HOLDER" 2>/dev/null
  fi
  [ -n "$RX_FIFO" ] && rm -f "$RX_FIFO"
  # T-c：echod 三相共用，统一在此收（正常流四跑毕也走 trap）。
  if [ -n "$ECHO_PID" ] && kill -0 "$ECHO_PID" 2>/dev/null; then
    kill "$ECHO_PID" 2>/dev/null
    wait "$ECHO_PID" 2>/dev/null
  fi
  # A2-3：evsclient 兜底收（正常流每相 evs_client_wait 已收清——这里只
  # 兜失败中断时的在途实例）。
  if [ -n "$EVS_PID" ] && kill -0 "$EVS_PID" 2>/dev/null; then
    kill "$EVS_PID" 2>/dev/null
    wait "$EVS_PID" 2>/dev/null
  fi
  # A3-3：sktclient 兜底收（同上）。
  if [ -n "$SKT_PID" ] && kill -0 "$SKT_PID" 2>/dev/null; then
    kill "$SKT_PID" 2>/dev/null
    wait "$SKT_PID" 2>/dev/null
  fi
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke: 串口日志保留：$LOG" >&2
    # T-c 对抗审查实抓：ECHO_LOG 同为失败现场（echod traceback 只落它）——
    # SMOKE_KEEP_LOG=1 只保串口日志时宿主侧死因恒被抹。
    [ -n "$ECHO_LOG" ] && echo "smoke: echod 日志保留：$ECHO_LOG" >&2
    [ -n "$EVS_LOG" ] && echo "smoke: evsclient 日志保留：$EVS_LOG" >&2
    [ -n "$SKT_LOG" ] && echo "smoke: sktclient 日志保留：$SKT_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
    [ -n "$EVS_LOG" ] && rm -f "$EVS_LOG"
    [ -n "$SKT_LOG" ] && rm -f "$SKT_LOG"
    # GUI-4 段②：QMP 临时目录（-rf = 目录连 sock 一起收）。
    [ -n "$QMP_DIR" ] && rm -rf "$QMP_DIR"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke: FAIL: $1" >&2
  # 失败必带现场：串口日志尾部直接进 stderr（CI 里即是诊断依据）。
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
  # T-c 对抗审查补：echod 宿主侧死因（无异常处理，traceback 只落 ECHO_LOG）——
  # 无 SMOKE_KEEP_LOG 的 CI 失败 run 也要看见，随失败即打印不留文件。
  if [ -n "$ECHO_LOG" ] && [ -s "$ECHO_LOG" ]; then
    echo "---- echod 日志尾部（$ECHO_LOG）----" >&2
    tail -10 "$ECHO_LOG" >&2
  fi
  # A2-3 同款：evsclient 死因（重试诊断只落 EVS_LOG）。
  if [ -n "$EVS_LOG" ] && [ -s "$EVS_LOG" ]; then
    echo "---- evsclient 日志尾部（$EVS_LOG）----" >&2
    tail -10 "$EVS_LOG" >&2
  fi
  # A3-3 同款：sktclient 死因。
  if [ -n "$SKT_LOG" ] && [ -s "$SKT_LOG" ]; then
    echo "---- sktclient 日志尾部（$SKT_LOG）----" >&2
    tail -10 "$SKT_LOG" >&2
  fi
  echo "--------------------------------" >&2
  exit 1
}

# A2-3 EVS client 对（每相起/收）：起 = QEMU 前后台拉起（client 自带重试
# 环——guest LISTEN 未起时 connect 失败重试，240s 总帽内必达）；收 =
# guest 侧 EVS ok 已到齐后等 client 对拍完成行（compare 与 guest 打标
# 几乎同时——30s 帽只盖透传余量），成功后收进程 + 清日志槽（下一相重起）。
evs_client_start() {
  EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
  python3 tools/evsclient.py 18082 EVS > "$EVS_LOG" 2>&1 &
  EVS_PID=$!
}
evs_client_wait() {
  local tag="$1" tc
  tc=$(date +%s)
  while ! grep -q 'EVS client ok n=2' "$EVS_LOG" 2>/dev/null; do
    kill -0 "$EVS_PID" 2>/dev/null \
      || fail "evsclient 提前退出（$tag）：$(tail -5 "$EVS_LOG" | tr '\n' ' ')"
    [ $(( $(date +%s) - tc )) -gt 30 ] \
      && fail "$tag EVS client 30s 未对拍完成（guest EVS ok 已到——查 EVS_LOG 重试诊断）"
    sleep 1
  done
  kill "$EVS_PID" 2>/dev/null
  wait "$EVS_PID" 2>/dev/null
  rm -f "$EVS_LOG"
  EVS_PID=""
  EVS_LOG=""
  echo "smoke: $tag EVS client 对拍完成（两连接 64B/24B 逐字节）"
}

# A3-3 SKT client 对（与 EVS 对同款生命周期）：guest bsh skt_selftest
# server 段（hostfwd 18083 专线——echod 18080/LSN 18081/EVS 18082 零
# 交集）经 evsclient.py 18083 SKT 参数化复用（载荷同源公式、完成行
# `SKT client ok n=2`）；收 = guest SKT ok 到齐后 30s 帽等对拍完成行。
# **懒启动（A3-3 首跑实抓修正）= guest `EVS ok` 标记后才起**：QEMU 前起
# 的 18083 SYN 重试流（SLIRP ~7-13s 一发，filter-dump pcap 实证 19 发）
# 在 boot→EVS 全程与 18082 帧同 RX 环排队——vnet 泵逐帧消化，SYN 垃圾
# 把 EVS 握手/载荷/FIN 可见性拖迟数十秒，放大「对端 FIN 迟到落进
# drain→RT 窗」尸槽竞速（bsh 侧另有 revive 兜底——本处削触发面）。
# watcher 子壳 exec 成 python：SKT_PID 直杀两形态皆中；相失败提前退场
# 由 cleanup 兜底收。
skt_client_start() {
  SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
  ( until grep -q 'EVS ok' "$LOG" 2>/dev/null; do sleep 2; done
    exec python3 tools/evsclient.py 18083 SKT ) > "$SKT_LOG" 2>&1 &
  SKT_PID=$!
}
skt_client_wait() {
  local tag="$1" tc
  tc=$(date +%s)
  while ! grep -q 'SKT client ok n=2' "$SKT_LOG" 2>/dev/null; do
    kill -0 "$SKT_PID" 2>/dev/null \
      || fail "sktclient 提前退出（$tag）：$(tail -5 "$SKT_LOG" | tr '\n' ' ')"
    [ $(( $(date +%s) - tc )) -gt 30 ] \
      && fail "$tag SKT client 30s 未对拍完成（guest SKT ok 已到——查 SKT_LOG 重试诊断）"
    sleep 1
  done
  kill "$SKT_PID" 2>/dev/null
  wait "$SKT_PID" 2>/dev/null
  rm -f "$SKT_LOG"
  SKT_PID=""
  SKT_LOG=""
  echo "smoke: $tag SKT client 对拍完成（fd 皮 server 段两连接 64B/24B 逐字节）"
}

# 标记断言：DEMO ok 是 AIDEMO ok 的子串（4-6 起既有混叠）——裸 grep -qF 会被
# 先行的「J AIDEMO ok: …」行满足，DEMO ok 缺失时验收盲区（talker 链唯一关卡
# 失去鉴别力）。对该一项负向断言（前缀非 I 即真 DEMO）；其余标记两两互非
# 子串，裸 grep 足够。RX ok n= / ROOTFS ok src= 等行前缀形态不可上通用词
# 边界（会误杀）——只特判 DEMO ok（P5 对抗审查实抓）。FS 标记用
# 「FS ok n=」真实打印形态避 `ROOTFS ok src=` 子串混叠（P6 A-1 对抗审查
# 实抓，DEMO ok 同族——不改用特判，改用带参形态即恢复鉴别力）。
assert_marker() {
  if [ "$1" = "DEMO ok" ]; then
    grep -qE '(^|[^I])DEMO ok' "$LOG"
  elif [ "$1" = "TAB ok" ]; then
    # C-4：boot 期 TAGTAB ok 恰含 "TAB ok" 子串——qF 假阳性。只排前邻 G
    # （TAGTAB 的 G）——真标记嵌稳态矩阵巨行中段、前驱可为任意大写矩阵
    # 字母（实测 BTAB ok），排整个 A-Z 会把真标记一并拒掉（DEMO ok 只排
    # I 同型教训：排除集 = 假阳性源的唯一直接前驱字符）。
    grep -qE '(^|[^G])TAB ok' "$LOG"
  else
    grep -qF "$1" "$LOG"
  fi
}

echo "smoke: QEMU 启动（$KERNEL）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 标记（全部出现才算过）——
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（崩溃？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${MARKERS[@]}"; do
    # 累积全部缺失（旧写法 missing="$m" 只剩最后一个，诊断误导）。
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] && fail "boot 标记 ${DEADLINE_BOOT}s 未到齐，缺：$missing"
  sleep 1
done
echo "smoke: boot 标记 ${#MARKERS[@]}/${#MARKERS[@]} 到齐"

# —— 阶段 2：switches 金丝雀（≥3 条且严格递增）——
t1=$(date +%s)
while :; do
  n=$(grep -c 'switches=' "$LOG" || true)
  [ "${n:-0}" -ge 3 ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出"
  [ $(( $(date +%s) - t1 )) -gt "$DEADLINE_CANARY" ] \
    && fail "金丝雀 ${DEADLINE_CANARY}s 内不足 3 条（现有 ${n:-0}）"
  sleep 2
done

grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
  { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
  END { exit (bad || n<3) ? 1 : 0 }
' || fail "switches 序列非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"

last=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)

# —— 阶段 3：G-eco rootfs 全链（G-eco-c 起）——
# rootfs.cpio 由 build-user 产出（justfile mkcpio 步）；缺席 = 回归（loud）。
[ -f rootfs.cpio ] || fail "缺 rootfs.cpio（build-user 应产出——G-eco-c 编排回归）"
# A1-3b：rootfs fw_cfg 卷动态递送——build-user 起 mkcpio --split 32 旁路产
# rootfs0.cpio..（内核连续前缀发现序 opt/berry/rootfs0..9 优先，单卷帽
# 32MiB）；rootfs0 在席走多卷名、缺席回落旧名单单卷（混跑兼容）。串一次
# 构建、七位点（rootfs/RX/FAT 子目录/持久化/B4 三 boot）同源代入；不加
# 引号代入 = 词切分即多 -fw_cfg 参数（卷名无空格）。
FWCFG=""
vi=0
while [ -f "rootfs$vi.cpio" ]; do
  FWCFG="$FWCFG -fw_cfg name=opt/berry/rootfs$vi,file=rootfs$vi.cpio"
  vi=$((vi+1))
done
[ -n "$FWCFG" ] || FWCFG="-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio"
# P6 A-2：virtio-blk 盘（vprobe 的探测对象——设备在席 = `-device
# virtio-blk-device` 落位的充要条件；缺席现场造，gitignored。
# force-legacy=false = 传输层报现代版 Version=2（QEMU 默认报 1，vprobe
# 断言 ver==2 不收 legacy））。
# P6 A-5：盘源 = mkfat.py 确定性 FAT12 卷（2048 扇区 = [16 扇区零保留区]
# [卷 @LBA16]——HELLO.TXT 28B + PATTERN.BIN 5000B 簇链，fsmgr FAT 只读层
# 的读对象；每次重造 = 跨相/跨 run 零残留、PATTERN 逐字节可对拍）。
# P6 A-3：LBA0 印章 `BERRYOSVBLK!`（notrunc 只写 12B 落保留区——vblk
# 读 LBA0 对拍；写 LBA1/BLKD LBA5 同在保留区，FAT 卷面（LBA16+）测试
# 永不写——A-6 起卷面有 FATW/RW.TXT 落盘写，保留区与卷面仍零交集）。
# P6 A-6：-drive 全线 cache.writeback=off——QEMU 由 SIGTERM 终止，writeback
# 模式可能不刷页缓存；本相盘随后重造无持久化需求，但与 RX/持久化相统一。
python3 tools/mkfat.py disk.img || fail "mkfat 造盘失败"
printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
# T-c：宿主 TCP 回显服务（vnet TCP 自验的对端——guest 主动 connect 10.0.2.2
# 由 SLIRP 转发宿主 127.0.0.1:18080）。三相共用（每相一个连接、用完被动
# 关闭，单线程串行 accept 无冲突）。启动确认 = 轮询 listening 行——端口被
# 占（手动实证残留）/脚本报错即 loud，不静默续跑。
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
ECHO_PID=$!
te=$(date +%s)
while ! grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null \
    || fail "echod 启动失败（18080 被占？）：$(cat "$ECHO_LOG")"
  [ $(( $(date +%s) - te )) -gt 5 ] && fail "echod 5s 未 listening"
  sleep 0.2
done
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"  # 复用同一日志文件（cleanup 只删一份）
# A2-3：EVS client 先于 QEMU 起（client 自带重试环吃 hostfwd 未就绪态）。
evs_client_start
skt_client_start
echo "smoke: QEMU 启动（rootfs 相：fw_cfg rootfs 卷（A1-3b 多卷名）+ virtio-blk + virtio-net）"
# GUI-4 段②：rootfs/RX/FAT 子目录/FAT 持久化四相 launch 皆在 netdev 行后
# 追 -device virtio-tablet-device,id=tab0（QEMU virtio-mmio 总线倒序建、
# tablet 追加在 blk/net 之后取更低槽 = 既有设备槽位零漂移——槽位注记见
# justfile run-rootfs；B4 三 boot 不挂：掉电相无输入面，gui-inputd park
# 优雅降级、INPUT ok 自然缺席不设门）。
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# 入机侧两行 + 编译侧全链：hello 自打 = 装载的程序真的在跑且能 syscall；
# stdhello 自打 STD 双标记 = **std 程序经 musl target + berry-libc shim +
# 流式装载全链跑通**（G-eco-d d-1c——sum/heap/argc 自检过了才轮到标记）；
# FILE ok = **std File::* 端到端往返**（C-1 刀 3：std open/write/close→
# commit/read → berry-libc fs.rs fd 表游标 → berry-fsclient RPC → fsmgr
# RAM 树；roundtrip 全等 + unlink 还原树才自打）；
# HEXYL ok = **上游逐字未动的生态程序跑通**（d-3c，D4 路线①终验）——
# svcmgr 经私有 stdout 通道收齐 hexyl 全量输出（EOF = libc park 零长
# 消息，d-3b 约定写侧），与内嵌宿主基准（上游 0.17.0 --color=never 同
# 输入管道输出）**逐字节比对全等**才打此标记（单消息原子、每 boot
# 确定性——直接 grep 表行会被稳态矩阵在 192B 分片边界插花，属时序
# 竞速，不可为标记）。
# BSH ok = **交互 shell 自驱自验**（P5 A-2）：bsh 启动即内部 `run hexyl`
# （同 256B 输入 + --color=never + 私有 stdout + EOF 收齐），字节数 ==
# 2156 基准才自打——rootfs 相第 8 标记，不赌交互输入（stdin=/dev/null）。
# EXIT ok = **子进程真退出 + 父回收全链**（Plan B v0.5）：stdhello main
# 返回 → libc exit → 0x33（内核置 Exited + 唤醒 TERMINATED + drain + REAPQ
# 回收任务体）→ svcmgr wait TERMINATED → reap（unmap×2 + close×5，映像/
# 栈帧真还）——rootfs 相第 9 标记（确定性：stdhello 恒从 main 返回）。
# PIPE ok = **channel 即 pipe 管道全链**（P5 v0.5）：bsh 自驱三跑对拍——
# `hexyl | hexyl` 管道输出 vs 同字节直喂 hexyl 参照**逐字节全等**（EOF 链
# = 前段 exit 零长消息逐级传导；feeder 192B 分片阻塞泵）——rootfs 相第
# 10 标记（hexyl 纯函数 + isatty 恒 false = 确定性，不赌竞速）。
# FS ok n= = **RAM-fs 服务全链**（P6 A-1）：fsmgr（svcmgr 预拉起，fs 通道
# 经 env 块 gift 交付）+ bsh 自驱双写读——PUT 64B 单片 → GET 全等；
# PUT 2156B 多片（12×192 + TRUNC）→ GET 逐字节全等（more 链全盖）才自打
# （协议 berry-fsproto，数据 = hexyl 输出纯函数快照，无竞速面）——rootfs
# 相第 11 标记。标记取「FS ok n=」带参形态（bsh 实打 `FS ok n=2156`）——
# 裸「FS ok」是 boot 行 `ROOTFS ok src=` 的子串，零鉴别力（对抗审查实抓）。
# VIRTIO ok = **用户态驱动地基第一刀**（P6 A-2）：boot 授能 MMIO 窗口 VMO
# （`VIRTIO vmo=` 行 = 第 4 插定值交付面）→ svcmgr 经 SpawnSpec 映进子空间
# 固定 VA → vprobe 扫 32 槽窗定位设备（QEMU 倒序建总线、首个 -device 落
# 最高槽且落位随 -device 顺序漂移——绑死槽号首版实踩空槽）——每槽验规范
# magic、恰一占用槽且 version=2（-global force-legacy=false 的现代版）/
# device_id=2（block）才自打——**设备探测全在用户态**，内核只授窗口——
# rootfs 相第 12 标记（短于 192B 分片界，单消息原子，无竞速面）。
# VBLK ok = **真块设备 IO 全链**（P6 A-3）：vblk 用户态驱动——modern v2
# 特性协商（收 VERSION_1）+ split virtqueue（qsize=16 单页环，三址经
# 0x1B `vmo_get_phys` 查自建 VMO 页后备）+ LBA0 读 justfile/smoke 落盘
# 印章逐字节对拍 + LBA1 写/读回 pattern 对拍（poll used ring，10s 帽）
# 全过才自打——**DMA 缓冲从不进地址空间**（vmo_read/write + get_phys
# 三件套，零 vmar 面）——rootfs 相第 13 标记（状态谓词，无竞速面）。
# BLKD ok n= = **块设备驱动服务消费链**（P6 A-4）：vblk 自验毕转常驻
# 服务（channel 一端随 env 块 gift[2] 迁入，fsmgr 同槽）——svcmgr 作
# client 经 berry-blkproto 严格串行 RPC（请求 9B + 随行 1 枚 VMO 别名
# = **数据面走能力迁移**：单扇区 512B > inline 帽 256B，数据不走消息）
# 写 LBA5 确定性 pattern → 读回 → 独立缓冲逐字节对拍全等才打——
# 「驱动服务化」分层（A-5 FS 服务的前一层）活证——rootfs 相第 14 标记
# （状态谓词，无竞速面；30s 应答帽盖 vblk 启动自验在途）。
# FAT ok n= = **FAT12 只读底座全链**（P6 A-5）：svcmgr 预拉 vblk（自验门
# BLKD 后端点交 fsmgr，blk 服务唯一 client）→ fsmgr eager init（探测
# LBA0/LBA16 + BPB hidden 对账定位卷、整读 FAT0 + 根目录）→ bsh 自驱
# fs GET：HELLO.TXT 28B 全等 + PATTERN.BIN 5000B 逐字节对拍（(i*31+7)%256
# ——mkfat 生产者/ berry-fatfs fixture/bsh 对拍三面同源 pattern）全过才自打
# ——**盘上真文件系统的读路径全在用户态服务栈**（bsh→fsmgr→vblk→盘），
# rootfs 相第 15 标记（状态谓词；盘内容确定性字节，无竞速面。标记取带参
# 形态防子串混叠（与「FS ok n=」/「ROOTFS ok」两族互非子串已核——
# grep -qF 全标记两两互非子串纪律）。
# FATW ok n= = **FAT 落盘写全链**（P6 A-6）：RW.TXT 3000B pattern
# （(i*17+3)%256，跨 2 簇）经 bsh fs_put → fsmgr staging + 尾随 TRUNC 即
# commit——簇分配/数据扇区/双 FAT/根目录项真写盘（rpc_write 经 vblk
# OP_WRITE）→ fs_get 读回逐字节对拍才自打——`>` 重定向从此对 8.3 大写名
# 是**真落盘**（RAM 物化退役）——rootfs 相第 16 标记（状态谓词；每次
# boot 重写 = 覆写路径幂等：旧链 collect → 新链分配 → 旧链释放）。
# NET ok ping= = **网络面第一刀全链**（P6 A-7）：vnet 用户态驱动——扫窗
# 按设备类认领 net 槽（did=1 恰一，block 槽合法同席不扰——A-7 起多 virtio
# 设备同窗）→ modern v2 协商（收 NET_F_MAC + VERSION_1）→ config 读 MAC →
# 双队列（rx q0 + tx q1）+ 16 个 rx 单描述符缓冲 → ARP 解 SLIRP 网关
# （10.0.2.2）MAC → ICMP echo 32B payload 往返逐字节对拍全过才自打——
# `-netdev user`（SLIRP）对用户态驱动的应答活证，零内核网络栈——rootfs
# 相第 17 标记（状态谓词；协议解析 berry-netproto 恰长执法，无竞速面）。
# DNS ok addr= = **网络服务消费链**（P6 A-8）：vnet 自验毕转常驻服务
# （A-8b，vblk A-4 同款）——svcmgr 作 client 经 berry-netdproto 严格串行
# RPC（DNS_QUERY("localhost") → st==ST_OK 且 addr==127.0.0.1——SLIRP
# 虚拟 DNS 10.0.2.3 → 宿主 /etc/hosts，离线确定无外网依赖）往返对拍全过
# 才打——「驱动服务化」网络版分层活证——rootfs 相第 18 标记（状态谓词，
# 无竞速面；60s 应答帽盖 vnet 自验尾巴 + 服务端 20s DNS 帽，帽不押时序）。
# TCP ok echo= = **最小 TCP 客户端全链**（T-c，`docs/TCP栈设计草案.md`）：
# vnet 自验（NET ok）后、转常驻服务前，guest 主动 connect SLIRP 宿主
# echod（10.0.2.2:18080）——berry-tcp 状态机（握手/序号/RTO/TIME_WAIT
# 纯函数）+ tcp.rs IO 驱动：三次握手（我方 MSS 128 通告）→ 64B pattern
# 回显逐字节对拍 → 主动 FIN 四次挥手到 Closed 才自打——**连接建立/数据
# 可靠交付/连接拆除三阶段全在用户态**，零内核 TCP——rootfs 相第 19 标记
# （状态谓词；对端 echod 由本相前脚本拉起，pcap 实证 SLIRP 遵守我方
# MSS：64B 回显单段不分片；无竞速面）。
# TCPS ok echo= = **TCP 服务化消费链**（T-d，`docs/TCP栈设计草案.md` §7）：
# svcmgr 作 client 经 berry-netdproto TCP 三 op 严格串行 RPC——
# OPEN(18080)（在席会话 ST_BUSY 单会话独占）→ ROUNDTRIP(64B pattern) 逐
# 字节对拍 → CLOSE（TIME_WAIT 收口）三步全 ST_OK 才打；与 vnet 自验
# `TCP ok` 打的是同一宿主 echod——「TCP ok = 栈本身能跑，TCPS ok = 栈经
# 服务化 RPC 面能被消费」（§9 话术「最小 TCP/IP 栈服务」的两半）——
# rootfs 相第 20 标记（状态谓词，无竞速面；两标记互非子串已核：第 4 字符
# 'S' vs ' '）。
# APP ok n= = **.bap 应用拉起消费链**（app-0 a-2，ADR #43/#44：应用 = 纯
# 用户态协议、内核零「应用」概念）：svcmgr 兼任 appd 雏形——主走查收集
# `app/<ID>.bap` 条目几何（'a'<'b' 排序在 bin/ 前），走查毕逐槽 .bap 内层
# cpio 走查（manifest 直读 + ELF 只记几何）→ berry-manifest 解析 → 双守卫
# （manifest.id ↔ 文件名、manifest.entry ↔ 包内条目名）→ spawn_rootfs_elf
#（console=true 直打 / false 私有对端 + EOF 收集）→ reap 真码 0——**全部
# 槽全过才打**（任一降级 = `eco: app <id> <why>` 行 + 标记缺失 loud）——
# rootfs 相第 21 标记（确定性状态谓词：n ≤ APP_SLOTS=4 单位数字；裸跑
# app_n==0 零扰动）。
# DIR ok n= = **RAM 目录树 v2 五 op 全链**（C-0 刀 4）：bsh dir_selftest
# 自驱——MKDIR `bdir` → 多段 PUT/GET（`bdir/n.bin` 64B 逐字节全等）→
# READDIR 恰一名 → LOOKUP 命中 → STAT 双面（TYPE_DIR 子项数/TYPE_FILE
# 字节数）→ UNLINK 文件 → 空目录 READDIR → UNLINK 空目录 → LOOKUP
# Noent 收口，全过才自打——`cat/ls/mkdir/rm` 内建走的就是这台机器
# （fs_rpc 同路不长第二套），FAT 子目录的 QEMU 活覆盖在阶段 6 注入段
# ——rootfs 相第 22 标记（数组尾插：打印时序在 FATW 后、与 svcmgr NET
# 族并行面无序；状态谓词，无竞速面）。
# C-1 刀 3：FILE ok（stdhello File::* 往返——libc fd 3+ 文件面端到端，
# rootfs 相第 23 标记；svcmgr 预拉 stdhello 于 bsh spawn 前 = fs 通道单
# 客户窗口，RAM 路由不破盘）。
# C-1 刀 5：CD ok（bsh cd 自验——chdir/pwd + 相对名经 CWD 解析落点铁证，
# rootfs 相第 24 标记）。
# C-2a：U1 ok（uutils core 12 命令家族单子验收——crates.io uu_* crate
# 0.12.0 锁步薄 harness 逐字接入。九步串行空文件元数据链：mkdir/touch/
# cp/mv → ls（含 src.txt+mvd.txt 子串）→ cat（恰空）→ rm×2 → rmdir，每
# 步 = spawn + fs gift + 私有 stdout EOF + 阻塞 reap 真码 0；分片数据面
# 留 text 批（顺延 C-2c/U3）——rootfs 相第 25 标记；svcmgr 预拉窗内 fs 单活跃客户纪律同
# stdhello）。
# C-2b：U2 ok（uutils echo/true/false 执行面验收——C-2a 12 命令族 11/12
# 覆盖：U1 八命令 + 本刀三命令；uu_pwd 派发被 bsh 内建优先所掩，盘上二
# 进制无验收面，归 text 批裁处）。宿主 bsh u2_selftest 六步：echo 多参 /
# echo -n（无尾 \n 恰长）/ true 真码 0 / false 真码 1 / && 链退出码传播 /
# echo|cat 管道对拍——全在 bsh 进程内私有通道收齐逐字节比对 + 真码断言，
# 零 fs 面（恰为 U1 元数据链未盖的执行面，互补不重叠）——rootfs 相第
# 26 标记（状态谓词，无竞速面）；bsh 链排 fat 族前 = EFI 路亦可达（无盘
# fat park 不掩 U2——smoke-image ECO 14 canon 同面）。
# C-2c：U3 ok（uutils text 批执行验收——uu 家族 12→18 命令：中批六件
# head/wc/cut/tr/sort/uniq =0.12.0 锁步接入 + uu_pwd 收编（run_capture
# 直 spawn bin/pwd，绕开 U2 挂账的「内建优先所掩」——盘上二进制自此有
# 验收面））。bsh u3_selftest 五步：pwd `/\n` / cat 32B feed 逐字节全等 /
# wc -c `32\n` / cut -c 2-3|tr a-z A-Z 双段管道 `BC\nZ\nEL\n` /
# cat|uniq|wc -l 三段链 `7\n`——期望值全字面常量（P6 A-5 基准漂移雷），
# spawn 合计 8（1+1+1+2+3），零 fs 盘面（纯 RAM/spawn）。原案七步门审
# 两处裁形（首跑定标实录，不硬凑假绿）：步③ head 裁出——uu_head unix
# stdin try_clone_to_owned → fcntl(F_DUPFD) 桩 -EINVAL（fd 克隆面 libc
# 挂账）；步④ sort|uniq 与步⑥ sort 段裁出——uu_sort 启动期
# ctrlc/signal-hook 后台线程踩 std 线程面悬垂指针（pthread/TLS 平台缺口
# 挂账），步⑥ 授权裁三段（cat,uniq,wc -l）——深坑六件 tail/env/date/
# grep/find/which 挂账另刀（docs/挂账-C2c深坑件.md）。链序契约同 U2 排
# fat 族前（EFI 路无盘亦打 U3——smoke-image ECO 14 同面）——rootfs 相第
# 27 标记（状态谓词，无竞速面）。
# B-1：B1 ok n=4（EL0 用户态基准验收——docs/基准与压力测试设计.md §8）：
# bsh bench1_selftest spawn bin/bench1（stdhello 同链 std 面，四项基准
# sys_null/sys_vmo_rw4k/ipc_enqueue8/evt_signal 全 syscall 自造 = 零新
# 管道）→ 私有通道收齐 BENCH1 行 → **格式断言**（行数恰 4 + 名字-iters
# 对字面常量 + 行 ≤192B + cyc 可解析非零；**cyc 数值零断言**——公约 2
# 「统计归统计」，数值对拍归 bench.py --el0 L1 宽松因子）→ 全过才 relay
# 四行 + 打原子标记。链序契约同 U2/U3 排 fat 族前（bench1 零 fs 盘面
# ——纯 RAM/spawn，EFI 路无盘亦打 B1）——rootfs 相第 28 标记（状态谓词，
# 无竞速面；~8.6k SVC ≈ +5-10s，DEADLINE_ROOTFS 帽不动）。
# U4（C-2c 挂账刀）：深坑六件 tail/env/date/grep/find/which 执行面八步
# （spawn 合计 9；期望值全字面常量私有通道逐字节对拍）。步① head -c 8
# = fd 克隆面活证（挂账 §①：uu_head stdin try_clone_to_owned →
# fcntl F_DUPFD_CLOEXEC → berry-libc 别名槽——C-2c 裁出件闭环回收）；
# 步④⑥⑦ 走 run_capture_fs gift 四元（挂账 §②）；步④ 期望值按自写
# grep 实测口径（FILE 参数恒带名前缀，GNU 单文件无前缀是差异面）。
# 链序契约同 U2/U3/B1 排 fat 族前（tail/head/env/date 四步纯管道 +
# grep/which/find 三步 RAM 树 fs 面，EFI 路无盘亦打 U4）——rootfs 相第
# 30 标记（状态谓词，无竞速面）。
# PS/BC（C-3 段①②）：ps 查询 RPC + berryctl 三子命令执行验收——svcmgr
# ps_selftest（bin/ps spawn + PS_LIST 请求 → 账本全表编码 → 私有 stdout
# 收齐渲染行 → 家名单断言（常驻拓扑真值）→ 打 `PS ok n=`）+ bc_selftest
# （bin/berryctl 三拉起：ver 版本串/ps 账本锚行/mem 汇总，渲染行逐字节
# 对拍后打 `BC ok`）。零 fs 盘面（rootfs 内 ELF spawn + 纯 RAM RPC），
# EFI 路可达同面——rootfs 相第 31/32 标记（状态谓词；两 spawn + 4 RPC
# ≈ +10-20s TCG，DEADLINE_ROOTFS 帽不动）。
# TAB（C-4）：bsh tab_selftest 九步——⓪-⑥ FakeSrc 喂键断言编辑器状态机
# （唯一补全/多候选列示/历史召回环/退格/Ctrl-U/ESC 非 final 复位落下/
# 路径位），⑦⑧ 真源两钉（cpio bin/ 含 ps、libc /etc/ 含 mtab）。零 fs
# 盘面、零 spawn（纯内存面），链序排 fat 族前（EFI 路亦打）——rootfs 相
# 第 33 标记。断言走 assert_marker 特例（TAGTAB ok 子串假阳性面）。
# INT（C-4b）：bsh int_selftest 四步——⓪ editor 常态 0x03（作废行/清缓
# 冲/不入历史）① ESC 序列两态共存（esc=1/esc=2 复位落下直达 Interrupt）
# ② Interrupt 后编辑器续用 ③ 前台杀链真机面：spawn bin/hello（10s 滚动
# 长睡不自退）→ process_terminate(130)==0 → 重复杀 -EBADSTATE →
# collect_fg wait_many TERMINATED 收口（eof=false+killed=true——EOF 缺口
# 活证：内核杀不走 libc exit 族，stdout 无零长 EOF）→ reap==130（$?
# 真码）。零 fs 盘面（hello = rootfs ELF 纯 RAM spawn），链序排 fat 族前
# （EFI 路亦打）——rootfs 相第 34 标记。裸 grep -qF 足够（"INTENT ok"
# 不含 "INT ok"——INT 后是 E 非空格，两两互非子串已核）。
# M3DEMO（C-5）：M-G3 里程碑演示相 smoke 化（VM里程碑设计草案 §2）——
# bsh m3demo_selftest 五步全状态谓词（零竞速）：⓪ 生态名单（cpio_names
# 直数 bin/ ≥38 + bsh/grep/hello/ls/cp/cat/ps 七锚——/bin 不在 fsmgr
# 面，cpio 真源 = 「ls /bin」等价真值）① ls 消费链（fs gift 面 `ls /`
# 断言 HELLO.TXT + PATTERN.BIN 子串——mkfat 恒在件相位无关）② cp 复制
# 链（mkdir t + put demo.txt → uu_cp 双操作数 → 读回逐字节对拍）③
# cat|grep 管道（首段 fs gift，期望字面 `m3 beta\n`）④ 状态码跨命令
# 传递落盘：grep -q zzz rc1 → `echo x$?y > T3.TXT`（run_line 真行
# 执行器，$? 展开造内容 x1y\n 消回声）→ 读回全等；T3.TXT 故意留盘
# （持久化相注入 cat T3.TXT 断言 x1y——覆写幂等弱化面见持久化相注）。
# ps 名单步由 PS ok n= 承接（拆解决策留档 VM里程碑 §2）。链序契约同
# DF **排 fat 族后**（①步依赖挂载卷在位）——EFI 路无盘 fat park 挡断
# → M3DEMO 结构性不打（smoke-image ECO canon 零变化）——rootfs 相第
# 35 标记（"M3DEMO ok n=5" 与现役标记两两互非子串——M3 前缀族唯一）。
# T-1 §2（多线程首刀消费面）：+ THR ok——svcmgr thr_selftest 拉起 thrchild
# （内嵌镜像 bootstrap 自句柄形态：dup(hp, MANAGE|DUPLICATE) 经
# process_start x0 递入，进程内 thread_create/thread_start/join/复启
# -EBADSTATE/末线程收口五步链）等 TERMINATED + 真码 7 才打，错码打
# `eco: thr exit <n>` 降级行——rootfs 相第 36 标记。镜像内嵌零 rootfs
# 依赖（echo/talker 同产链）= 裸跑亦打（裸相只断言 MARKERS 27 子集，
# 多打无害）；链位置 bc_selftest 后 fat 族前（EFI 路 ECO 同面）。
# "THR ok" 与现役标记两两互非子串——THR 前缀族唯一（TAB `(^|[^G])` 前邻
# 特例不需要：无任何 `XTHR ok` 形态在册，/usr/bin/grep 复核零碰撞）。
# A2-1（事件循环聚合器 guest 冒烟）：+ POLL ok——bsh poll_selftest 四步
# （⓪ N=20 Event 全空纯 poll ①批 1 成员跨批发现+全满足集 ②收满 20
# +LT 复报 ③摘槽 19 ④清位后 30ms 阻塞超时收口；berry-poll 库 host 13 测之上
# 的真 syscall 绑定面）全过才打——rootfs 相第 37 标记。纯内核对象零 fs
# 盘面 = 链位置 int_selftest 后 fat 族前（EFI 路无盘亦打，U2/U3/TAB/INT
# 契约）。"POLL ok" 与现役标记两两互非子串（PIPE/PS/APP 前缀族互异，
# /usr/bin/grep 复核零碰撞——无 `XPOLL ok` 形态在册，前邻特例不需要）。
# A2-3（事件循环 echo server 整链）：+ EVS ok n=2——bsh evs_selftest 经
# netd LISTEN(18082)/ACCEPT/POLL/DRAIN/RT/CLOSE 全链收两连接（每连接
# 64B/24B 异型载荷逐字节对拍）全过才打——rootfs 相第 39 标记。netdev 依赖
# 面 = 链位置 poll_selftest 后 fat 族前（**EFI/裸路 netd_h()==0 静默跳过**
# ——有 netdev 的四相照打；「串行非竞速」4-5a 铁律：逐连接串行对拍零竞速
# 标记）。"EVS ok n=2" 与现役标记两两互非子串（无 `XEVS ok` 形态在册）。
# A3-3（socket fd 皮整链）：+ SKT ok n=3——bsh skt_selftest 三段
# （⓪ client 段 connect/send/recv 对拍 echod 32B 回显 + ① 非阻塞
# EAGAIN 节拍/F_GETFL 旗/SO_TYPE/send-on-listen EPIPE 错误面（18084 无
# hostfwd 专线 = accept 空队结构性恒真）+ ② server 段 evs 七步链的 fd 皮
# （hostfwd 18083 + 宿主 evsclient 18083 SKT 两连接对拍：阻塞 accept/
# 出参钉板/poll 尸槽闸门/recv 恰长对拍/send 回发/close））全过才打——
# rootfs 相第 40 标记，**A3 整卡验收线**。链位置紧随 evs_selftest 后
# fat 族前（拓扑降级同 EVS：netd_h()==0 / ENODEV 静默跳过——EFI canon
# 与 B4 三 boot 零变化）。"SKT ok n=3" 与现役标记两两互非子串（无
# `XSKT ok` 形态在册，/usr/bin/grep 复核零碰撞）。
# A4-3（epoll+DNS 整链）：+ EPL ok n=8——bsh epl_selftest 八步（⓪ create1
# flags EINVAL/4 槽满 EMFILE ① ctl 拒面全表（EBADF/EFAULT/EINVAL op·mask/
# ENOENT MOD·DEL）+ 段满·帽满形态 = Exists（7 socket+cfd 恰满 socket 段、
# 第 8 socket EMFILE、8/8 interest 满、重 ADD EEXIST——ENOSPC 结构性不可达
# A4-1 定谳）② wait(0) 快照 LT 复报两次连收恰 1 项 EPOLLOUT+data 透传
# （Client 恒 WRITABLE 状态谓词非竞速）③ wait(1500) 空表到点返 0 ④ 非
# socket fd（console 1）ADD EINVAL ⑤ getaddrinfo 双查（"127.0.0.1" 字面量
# 快路 + "localhost" DNS 慢路 SLIRP 离线确定 127.0.0.1——A4-2 接线首次
# guest 活体）⑥ 非法名（下划线）/超长名（65B）EAI_NONAME 本地拒零发包
# ⑦ close 释槽同号回填）全过才打——rootfs 相第 41 标记，**A4 整卡验收
# 线**。链位置紧随 skt_selftest 后 fat 族前（拓扑降级同 EVS/SKT：
# netd_h()==0 / connect ENODEV 静默跳过——EFI canon 与 B4 三 boot 零变化）。
# "EPL ok n=8" 与现役标记两两互非子串（无 `XEPL ok` 形态在册，/usr/bin/grep
# 复核零碰撞）。
# A5-2（std::net spawn 面整链）：+ NETS ok n=2——bsh nets_selftest 两步
# （① 正步 netprobe 10.0.2.2 18080：net_gifts 三元装配（A5-1）下经未修改
# 的 std::net TcpStream connect→write→echo 恰长对拍 echod，rc=0 + 输出
# 逐字节 `NETPROBE ok n=14\n`——std::net 深水面第一次 guest 全链活体
# （A5-1 净室 probe 定谳：无桩判死面）；② 负控 netprobe localhost 18080：
# std ToSocketAddrs→getaddrinfo→A4-2 DNS 档经 std::net 活证 + SLIRP guest
# 环回不绕宿主 loopback（ENETUNREACH——A5-0 草案 "localhost:18083" 原案
# 证伪的定谳形状），rc=1 + 尾行 `NETPROBE fail: connect errno=<n>` 前缀锚
# （errno 值是 SLIRP 行为面随 QEMU 版本可漂——值不锚；strerror 文案同理
# 不锚））全过才打——rootfs 相第 42 标记，**A5 整卡验收线**。链位置紧随
# epl_selftest 后 fat 族前（拓扑降级**双门**同 EVS/SKT/EPL：netd_h()==0
# / connect ENODEV(19) 静默跳过——EFI canon 与 B4 三 boot 零变化；**A5
# 终验五相 smoke 首跑实抓**：单门版（仅 netd_h()==0）在 B4 Boot A 把
# deviceless ENODEV 当链断 park → fat 族全断 → 240s FAT ok 未到——
# netprobe stdout 失败行携 errno=N 即为此门的确定性信号通道）。对端 =
# echod 18080（SLIRP 网关别名 10.0.2.2 直达宿主 loopback——T-c TCP ok
# 同源专线串行复用，零新端口）。"NETS ok n=2" 与现役标记两两互非子串
# （NET ok ping= 之 "NET " 与 NETS 之 "NETS" 首四字即异——前邻特例不
# 需要；无 `XNETS ok` 形态在册）。
# A7-1（mmap 匿名档 libc 桥）：+ MMP ok——bsh mmp_selftest（⓪
# mmap(NULL,8192,…) 双页首映射 + 两页各写读对拍 ①hint 0x5500_0000 忽略
# 档成功落窗 [0x5000_0000,0x6000_0000) ②munmap 三态——内部地址 -1+
# EINVAL（Partial 诚实拒）/恰长 0/重复 0（POSIX NotFound 追认档）③再
# mmap 同长 **== p1 首址**（first-fit 空洞复用——A7-0 §3.1 钦定段内
# 分配策略）+ 再写读 0x4D4D5033 + munmap ④拒面三连（fd=0 / +MAP_SHARED
# / 单 PROT_READ 各断 MAP_FAILED+EINVAL）+ PROT_EXEC 忽略档放行 +
# mprotect 命中 EACCES / 收口后未命中 EINVAL）全过才打——**A7-1 验收线**。
# A7-3（mmap 文件映射 staging + mprotect dormant）起 n=5→n=9 扩四步：
# ⑥ 文件档 staging 真身——RAM 造源 m/mmpf.bin（4396B，父目录在 RAM 树
# create 恒落 RAM 相位无关、避开 M3DEMO 的 t/ 零交叉）→ open(O_RDONLY)
# → mmap(MAP_PRIVATE, offset=0, len=4200)（跨页 staging + want<文件长
# 收尾截断臂 take<n 活证）+ 首尾对拍 + [4200,8192) 尾零区零值验（整段
# commit 含尾零页）→ 映射面改写一字节 → munmap → get_file 验源未变
# （急切私有拷贝证）→ 同 fd 复映射 offset=4096 len=300（单 PROT_READ
# 文件档放行 + EOF 恰边）→ 读回对拍；⑦ 文件档拒位四连（MAP_SHARED/
# offset 非页对齐/超 EOF 原始 length 判/prot 缺 READ 各 EINVAL）；⑧
# fd 面拒位（fd=1 与 fd=99 各 EBADF / O_WRONLY fd EACCES——v1 只收恰
# O_RDONLY）；⑨ mprotect dormant 保内容环（哨兵 → PROT_NONE 撤翻译 →
# 幂等 → RW 复映复活 → 哨兵原值 + 追写读 → dormant 条目 munmap 收口）。
# 造源收尾还零残留（unlink 文件 + m 目录）。
# mmap/munmap/mprotect 经 bsh 本地 extern 声明链接期解析 berry-libc
# `#[no_mangle]` 真身（六参/两参/三参与 libc crate aarch64-musl 声明逐参
# 对齐——berry-libc rlib 先于 -lc 归档，shim 全定义纪律）。⓪-④⑨ 纯
# vmo/vmar syscall 桥、⑥-⑧ 走 RAM 树 fs 面（非盘面）= 链位置
# nets_selftest 后 fat 族前（A 系契约位，EFI 路无盘亦打——self_vmar
# gift[3] 是 spawn 通用装配，无拓扑降级门，失败即 park loud）。
# "MMP ok n=9" 与现役标记两两互非子串（无 `XMMP ok` 形态在册，
# /usr/bin/grep 复核零碰撞——前邻特例不需要）。
# A7-2（timer 壁钟分档 + 补桩五真身）：+ CLK ok n=6——bsh clk_selftest
# 六步（⓪mono 双读单调非减 + nsec 域 [0,1e9) ①REALTIME 合成一致——
# 同窗双钟 rt ≥ mono 且差 ≤1s（base=0 下逐 ns 同值，容忍取样间隙）
# ②time()/gettimeofday() 互恰 ≤1s + tv_usec 域合法 + time() 返回值 ==
# 出参回填 ③nanosleep(50ms) 活性——前后 mono 差 ∈ [50ms,5s] TCG 宽带
# ④clock_settime 双档（REALTIME/MONOTONIC）各 -1 + errno==EPERM 拒
# ⑤裸 `date -u` 子进程漂移容忍钉板——29B = 前缀 `Thu Jan  1 00:` +
# MM:SS 数字域 + 尾 ` UTC 1970`，REALTIME 端到端活钟面（boot→clk 数分钟
# TCG，小时域 00 结构性稳定；>59min 破界预案 = 中段放宽任意 \d{2}:\d{2}，
# 设计稿 §5 #2））全过才打——rootfs 相第 44 标记，**A7-2 验收线**。
# 实现面 = berry-libc clock_gettime Linux 编号分诊（mono 族 1/4/6/7 直读 /
# REALTIME 族 0/5 = EPOCH_BASE(0)+mono 纯合成（静态零写者）/ 冷门 id
# EINVAL 诚实拒——首个真实 errno 路径）+ 补桩五真身
# time/gettimeofday/settimeofday(EPERM)/nanosleep/usleep（timer_create 族
# 维持零符号，A7-0 #10 深水后置）。clk_selftest 经 bsh 本地 extern 声明
# 链 berry-libc #[no_mangle] 真身（mmp 先例）；纯 clock syscall + date
# spawn 无文件参 = 零 fs 盘面，链位置 mmp 后 fat 族前（A 系契约位，EFI 路
# 无盘亦打）；无拓扑降级门，失败即 park loud。"CLK ok" 与现役标记两两互
# 非子串（无 `XCLK ok` 形态在册——前邻特例不需要）。设计稿「五步/n=5」
# 系 ⑤ 增补后计数未同步勘误，实现定案六步 n=6。
# A7-4（信号面真身化）：+ SIG ok n=6——bsh sig_selftest 六步（⓪sigaction
# 装 SIGUSR1 + raise 即时投递计数 1 ①表回读==handler + sigprocmask
# BLOCK/查询双向 ②blocked 窗 raise held→UNBLOCK 补投计数 1（③b 归并式
# 分诊核心环）③kill 分诊三连 self≡raise/ESRCH/EINVAL ④spawn bin/sigchild
# → berry_signal_send(hp,SIGTERM) → DFL io::exit(143) EOF 先行（INT 案
# 内核杀 no-EOF 的对照面）→ reap 真码 143 ⑤修复轮回归钉板（位号序敏感
# 三形分诊：Call 位 SIGHUP=1 < Hold 位 SIGUSR2=12 < 表变位 SIGCONT=18
# ——h3 内换表+解封：嵌套补投恰一次 =「Hold 即刻落 held」可见性主钉板
# + delivery-time 表重查（冻结 plan 形态跑旧表 h4）+ 包络全量恢复掩码
# + 收尾 UNBLOCK 无残留双投（冻结 plan 外层 Hold 步重落 held））全过才打
# ——rootfs 相第 45
# 标记，**A7-4 段④验收线**。实现面 = berry-libc signal.rs ③a+③b 真身
# （sigset/sigaction/sigprocmask 表面 + 投递循环 kill/raise/pause）+ 内核
# 0x86/0x87 跨进程位面；信号面经 bsh 本地 extern 声明链 berry-libc
# #[no_mangle] 真身（clk/mmp 先例）。纯用户态信号面 + sigchild 零 gift
# spawn = 零 fs 盘面，链位置 clk 后 fat 族前（A 系契约位，EFI 路无盘亦打
# SIG）；无拓扑降级门，失败即 park loud。"SIG ok" 与现役标记两两互非子串
# （现役无 `XSIG ok` 形态在册——前邻特例不需要）。
# GUI-4 段②：+ INPUT ok——svcmgr boot 期 input 域 SUBSCRIBE 自验
# （input_selftest：gui-inputd 预拉成功后严格串行 RPC，ST_OK 才打；
# tablet 缺席（B4）= gui-inputd park、60s 帽到点放弃非致命，标记自然
# 缺席不设门——B4 无 netdev 先例同款；EFI = H_MMIO 缺席预拉跳过，
# 静默不打 = 拓扑真值（LSN/EVS 同款））。"INPUT ok" 与现役标记两两互
# 非子串（"INT ok" 位 2 P≠T；RX 相专用行 "INPUTP ok n=3" 不含 "INPUT ok"
# ——位 5 P≠空格）；不进 assert_marker 特判面。
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "LSN ok n=7" "TCPS ok echo=" "APP ok n=" "DIR ok n=" "CD ok" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "DF ok" "U4 ok" "TAB ok" "INT ok" "POLL ok" "EVS ok n=2" "SKT ok n=3" "EPL ok n=8" "NETS ok n=2" "MMP ok n=9" "CLK ok n=6" "SIG ok n=6" "PS ok n=" "BC ok" "M3DEMO ok n=5" "THR ok" "INPUT ok")
t2=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（rootfs 相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${ROOTFS_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  # vmo 行须非降级形态（=none 即 seam 断：内核没把 rootfs 包进 VMO）。
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  # 相内含 FATW 整链 commit + T-d TCP 三 op 全走真网卡——比较须用
  # DEADLINE_ROOTFS（现 420s；与文案一致。历史上写 BOOT 是笔误：比较 120s
  # 文案当时 240s，>120s 到齐会被误杀）。
  [ $(( $(date +%s) - t2 )) -gt "$DEADLINE_ROOTFS" ] \
    && fail "rootfs 相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: rootfs 相 ${#ROOTFS_MARKERS[@]}/${#ROOTFS_MARKERS[@]} 到齐（ROOTFS 双行 + rootfs hello + ECO ok + STD 双标记 + FILE ok + HEXYL ok + BSH ok + EXIT ok + PIPE ok + FS ok n= + VIRTIO ok + VBLK ok + BLKD ok n= + FAT ok n= + FATW ok n= + NET ok ping= + DNS ok addr= + TCP ok echo= + LSN ok n=7 + TCPS ok echo= + APP ok n= + DIR ok n= + CD ok + U1 ok + U2 ok + U3 ok + B1 ok n=4 + DF ok + U4 ok + TAB ok + INT ok + POLL ok + EVS ok n=2 + SKT ok n=3 + EPL ok n=8 + NETS ok n=2 + MMP ok n=9 + CLK ok n=6 + SIG ok n=6 + PS ok n= + BC ok + M3DEMO ok n=5 + THR ok + INPUT ok）"

# app-0 a-2 证据层：APPDEMO（console=true）子自证输出——谓词 `APP ok n=`
# 证明「全部槽拉起 + reap 真码 0」，本行证明 console 能力面真把 stdout
# 接到了串口（reap 0 而输出丢失 = 能力面静默断，谓词层抓不到）。小写
# `app ` 前缀与全部大写标记面互非子串（与谓词分立的两层——HEXYL ok 的
# 「私有通道收齐 + 基准对拍才打」同款分层思想）。
grep -q 'app APPDEMO up' "$LOG" \
  || fail "app APPDEMO up 缺失（console=true 能力面断——appdemo 未见自证输出；查 eco: app 行）"
# A2-3：guest EVS ok 已在标记面到齐——收本相 client 对拍结果（30s 帽只盖
# 透传余量；compare 与 guest 打标几乎同时）。
evs_client_wait "rootfs 相"
# A3-3：guest SKT ok 已到齐——收本相 fd 皮 client 对拍结果（30s 帽同上）。
skt_client_wait "rootfs 相"

# —— 阶段 4：P5 RX 交互相（rootfs+FIFO 合相：内核 RX 中断 + console 行编辑
# + bsh stdin 转发 + bsh 应答）——
# stdin 换 FIFO：holder（sleep 占写端）防 EOF 断流；boot 27 + rootfs 46 = 73
# 标记到齐（PIPE ok 在列 = bsh selftest 全过、已进交互读循环——注入的行才
# 有人收；VIRTIO/VBLK/BLKD/FAT/FATW/NET ok 在列 = MMIO 探测、块设备 IO、块
# 设备服务消费链、FAT 只读底座+落盘写与网络往返全过（DNS ok addr= = 网络
# 服务消费链、TCP ok echo= = TCP 客户端全链、TCPS ok echo= = TCP 服务化消费
# 链、APP ok n= = .bap 应用拉起消费链、DIR ok n= = RAM 目录树 v2 五 op
# 自验，同批）；状态谓词门控——不赌 boot 秒数）
# 后经 fd3 逐字节注入编辑序列，每
# 字节隔 0.5s（FEN=0 FIFO 深 1 字节——慢速 = 单字节-per-handler 标记确定性；
# QEMU 流控不丢字，真机才有 overrun 面）：a. `rx\n` → `RX ok`（console 收齐
# 行 token 才打——内核侧状态断言，非注入即达回声）；b. `helll\x7f`+`o\n` →
# 退格编辑得 `hellq` → endK2 转发 bsh → `bsh: hellq: not found`；c. `\x1b[A`
# +`!\n` → ESC[A 历史召回 `hellq`+`!` → `bsh: hellq!: not found`（与 b 互非
# 子串，各自独立断言）；d. `run fail\n` → `echo f$?t > OUT.TXT\n` →
# `cat OUT.TXT\n` → grep 'f42t'（A-6 FAT 落盘链，见头部段 d 注）；e.
# `mkdir d1`/`cat d1/Z.TXT`/`cat d1`/`rm` RAM 目录链（C-0 刀 4，见头部段
# e 注）。注入后
# 仍跑金丝雀——RX 中断若破坏 IRQ 屏蔽契约
# （在屏蔽窗内置 Event/唤醒却漏 EOIR/漏开关），死状常为「冻而不死」（活性
# 检盲区），只有切换序列还动才真活着。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
RX_FIFO=$(mktemp "${TMPDIR:-/tmp}/berryos-rx.XXXXXX")
rm -f "$RX_FIFO"
mkfifo "$RX_FIFO" || fail "mkfifo 失败"
# holder 帽 1800s：C-0 刀 4 起 FIFO 面横跨三跑（RX + FAT 子目录 + 持久化，
# 每跑 DEADLINE_ROOTFS=420s 帽 + 注入段 0.5s/字节——900s 只盖两跑结构性
# 见底；30min 盖三跑 worst 仍有 ~2x 余量）。
( sleep 1800 ) > "$RX_FIFO" &
RX_HOLDER=$!
# 盘重造 + LBA0 印章重落（rootfs 相的 FATW 落盘写（RW.TXT）会脏卷面——
# 持久化相的基线须从确定性盘起：本相重造保证段 d 的 OUT.TXT 落在已知的
# 簇位（8）与根目录槽位；vblk LBA1/BLKD LBA5 仍在保留区）。**本相写过的
# 盘直接喂阶段 5**（持久化相不再重造）——cache.writeback=off 保证
# SIGTERM 终止前扇区已同步落宿主盘。
python3 tools/mkfat.py disk.img || fail "mkfat 造盘失败"
printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
# A2-3：本相（及子目录/持久化相）每次 boot 各跑一遍 evs_selftest——client
# 每相一实例（上相已收清，重起零残留）。
evs_client_start
skt_client_start
echo "smoke: QEMU 启动（RX 交互相：rootfs + FIFO stdin + virtio-blk + virtio-net）"
# GUI-4 段②：QMP 通道（RX 相专用）——h 段 qmp_inject 经 unix sock 注入
# virtio-tablet 事件（三批各一 SYN）；launch 挂 -qmp server=on,wait=off
#（brew 新版 QEMU 已废 server=nowait 简写——解析期即拒，首跑实录）。
QMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/berryos-qmp.XXXXXX")
QMP_SOCK="$QMP_DIR/sock"   # unix socket 路径帽 107B——mktemp 短前缀必达
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  -qmp unix:"$QMP_SOCK",server=on,wait=off \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!

# 注入口先备好读端存在的证据：73 标记全到 = QEMU 正常跑且 bsh 就位（exec 3>
# 对 FIFO 阻塞直至有读者——QEMU 死则上面 marker 轮询先 loud 失败，不卡在这）。
ALL_MARKERS=("${MARKERS[@]}" "${ROOTFS_MARKERS[@]}")
t3=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（RX 相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${ALL_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  # vmo 行非降级形态（与 rootfs 相同款守卫）。
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  # 比较用 DEADLINE_ROOTFS（同上 rootfs 相注——RX 相含 FATW 落盘写）。
  [ $(( $(date +%s) - t3 )) -gt "$DEADLINE_ROOTFS" ] && fail "RX 相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: RX 相 ${#ALL_MARKERS[@]}/${#ALL_MARKERS[@]} 到齐，注入编辑序列（逐字节 0.5s 间隔）"
evs_client_wait "RX 相"
skt_client_wait "RX 相"
# 逐字节注入助手（段 d 专用——a/b/c 保留既有显式 printf 形态）：单引号传参
# 保字面（`$?` 不在 smoke 侧展开）；每字节 0.5s（FEN=0 深 1 字节确定性）。
inject_line() {
  local s=$1 i
  for ((i = 0; i < ${#s}; i++)); do
    printf '%s' "${s:i:1}" >&3
    sleep 0.5
  done
  printf '\n' >&3
}
# GUI-4 段②：QMP 注入 client（macOS 无 socat——python3 socket 直连；内联
# 不落工具文件：注入序列与 h 段断言同址演化）。协议序：连上先收 greeting
# （逐行 JSON——读到 {"QMP":…} 才发 qmp_capabilities，等到 {"return":…}
# 协商毕；异步事件行静默跳过）。**设备路由首跑裁定实录（2026-11-01）**：
# 主案显式 id（tab0）在 -nographic 无 console 拓扑下被拒——
# GenericError "Device tab0 (head 0) is not bound to a QemuConsole"
# （input-send-event 显式 device 寻址走 console 头绑定面）；回退 = 省
# device 键（事件广播进 HID handler 链，VM 内唯一输入设备路由无歧义，
# 探针实证 {"return":{}}）——send() 内自动降级 + 裁定行落 stderr（探针
# 复现路径 = /tmp 直启同配方；段②草案 §7-② / 阶段沿革同源记录）。
qmp_inject() {
  python3 - "$QMP_SOCK" <<'PYEOF'
import json, socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
f = s.makefile("rw", encoding="utf-8", newline="\n")
def rd_result():                   # 逐行读到命令应答；greeting 即发握手
    while True:
        line = f.readline()
        if not line:
            raise SystemExit("qmp: eof（QEMU 退出？）")
        m = json.loads(line)
        if "QMP" in m:             # greeting
            f.write('{"execute": "qmp_capabilities"}\n'); f.flush()
        elif "error" in m:
            return ("err", m)
        elif "return" in m:
            return ("ok", m)
use_device = True                  # 主案 tab0 → 首命令不收即永降级
def send(events):                  # 一命令 = 一批 SYN_REPORT
    global use_device
    args = {"events": events}
    if use_device:
        args["device"] = "tab0"
    f.write(json.dumps({"execute": "input-send-event", "arguments": args}) + "\n")
    f.flush()
    kind, m = rd_result()
    if kind == "err" and use_device:
        print("qmp: device=tab0 被拒（{}）→ 永降级省 device 键".format(
            m.get("error", {}).get("desc", "?")), file=sys.stderr)
        use_device = False
        args.pop("device")
        f.write(json.dumps({"execute": "input-send-event", "arguments": args}) + "\n")
        f.flush()
        kind, m = rd_result()
    if kind == "err":
        raise SystemExit("qmp: {}".format(m))
rd_result()                        # greeting → qmp_capabilities → return
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
exec 3> "$RX_FIFO"
# a. token 行：RX ok（A-1 console token 面；`rx` 行入历史但不转发）。
printf 'r' >&3; sleep 0.5
printf 'x' >&3; sleep 0.5
printf '\n' >&3
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 RX ok 阶段退出（注入链断？）"
  grep -qF "RX ok" "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "RX ok 60s 未出现（注入丢失/overrun/中断号错——查 irq? 行与 echo 字节）"
  sleep 1
done
echo "smoke: RX ok 到齐"
# b. 退格编辑：`helll` + \x7f（回退 → `hell`）+ `q` + \n → 完整行 `hellq` →
# 转发 bsh → `bsh: hellq: not found`（C-2a 裸名派发只认 cpio 在档名，
# hellq 不在档 = not found 面仍在）。
printf 'h' >&3; sleep 0.5
printf 'e' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf '\x7f' >&3; sleep 0.5
printf 'q' >&3; sleep 0.5
printf '\n' >&3
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 hellq 应答阶段退出"
  grep -qF 'bsh: hellq: not found' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: hellq: not found 60s 未出现（转发链断？查 echo 字节与 fdrop 计数）"
  sleep 1
done
echo "smoke: bsh hellq 应答到齐"
# c. 历史召回：ESC [ A（上翻 → 最新历史 `hellq`）+ `!` + \n → `hellq!` →
# 转发 → `bsh: hellq!: not found`。
printf '\x1b' >&3; sleep 0.5
printf '[' >&3; sleep 0.5
printf 'A' >&3; sleep 0.5
printf '!' >&3; sleep 0.5
printf '\n' >&3
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 hellq! 应答阶段退出"
  grep -qF 'bsh: hellq!: not found' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: hellq!: not found 60s 未出现（ESC[A 历史召回链断？）"
  sleep 1
done
echo "smoke: bsh hellq! 应答到齐（退格 + ESC[A 召回双验过）"
# d. FAT 落盘链（P6 A-6）：`run fail`（bin/fail exit 42 → $?=42）→
# `echo f$?t > OUT.TXT`（大写 8.3 名 → fsmgr FAT 层 staging + TRUNC 即
# commit 落盘，内容 "f42t\n"）→ `cat OUT.TXT` 读回打印 f42t。断言
# grep -a 'f42t'：注入/回显文本只含未展开的 `f$?t`（console 逐字节回显
# 散布在稳态矩阵输出间，本就非连续），f42t 只能来自真实文件内容——
# `$?` 展开造内容 = 消回声歧义的确定性状态谓词（标记不建在竞速上的
# 注入版）。行间不等待：FIFO/通道排队天然串行，bsh 逐行处理。
inject_line 'run fail'
inject_line 'echo f$?t > OUT.TXT'
inject_line 'cat OUT.TXT'
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 FAT 落盘链阶段退出"
  grep -aq 'f42t' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "f42t 60s 未出现（FAT 写链断？查 bsh: 行与 fsmgr: fat 诊断）"
  sleep 1
done
echo "smoke: FAT 落盘链到齐（run fail + \$? 展开 > OUT.TXT + cat 读回）"
# e. RAM 目录链（C-0 刀 4）：`mkdir d1`（静默成功——下游 cat 证）→
# `cat NOPE.TXT`（FAT Miss → $?=127）→ `echo d$?x > d1/Z.TXT`（多段 `>` =
# RAM 树写，内容 "d127x\n"——`$?` 展开造内容，消回声）→ `cat d1/Z.TXT` →
# grep 'd127x' → `cat d1`（目录 GET → ST_BAD 类型冲突）→ grep 全行
# 'bsh: cat: d1: bad path'（`bsh: ` 前缀回显不可达）→ `rm d1/Z.TXT` →
# `cat d1/Z.TXT` → grep 全行 'bsh: cat: d1/Z.TXT: not found'（UNLINK 生效
# ——全行断言与 b/c 段 'not found' 族互非子串）。
inject_line 'mkdir d1'
inject_line 'cat NOPE.TXT'
inject_line 'echo d$?x > d1/Z.TXT'
inject_line 'cat d1/Z.TXT'
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 RAM 目录链阶段退出"
  grep -aqF 'd127x' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "d127x 60s 未出现（多段重定向/读链断？查 bsh: 行）"
  sleep 1
done
inject_line 'cat d1'
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在目录类型面阶段退出"
  grep -aqF 'bsh: cat: d1: bad path' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: cat: d1: bad path 60s 未出现（目录 GET 的 ST_BAD 面断？）"
  sleep 1
done
inject_line 'rm d1/Z.TXT'
inject_line 'cat d1/Z.TXT'
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 UNLINK 验证阶段退出"
  grep -aqF 'bsh: cat: d1/Z.TXT: not found' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: cat: d1/Z.TXT: not found 60s 未出现（UNLINK 断？查 bsh: 行）"
  sleep 1
done
echo "smoke: RAM 目录链到齐（mkdir + 多段 > + cat + 类型面 + rm）"
# f. cd/pwd 链（C-1 刀 5）：`cd d1`（段 e 留下的空目录——chdir 进去）→
# `pwd` → grep '/d1'。防回声：注入行 'cd d1'/'pwd' 均不含 '/d1'（console
# 逐字节回显散布非连续），段 e 的 'd1' 族断言串（'d127x'/'cat d1'）均无
# 前导斜杠形态——'/d1' 只能来自 pwd 的 getcwd 渲染应答 = libc CWD 全链
# （chdir resolve+STAT 目录型检 → CWD 静态 → getcwd 渲染）交互面活覆盖
# （自验面 CD ok 已在 58 标记列）。
inject_line 'cd d1'
inject_line 'pwd'
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 cd/pwd 阶段退出"
  grep -aqF '/d1' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "/d1 60s 未出现（cd/pwd 链断？查 bsh: 行）"
  sleep 1
done
echo "smoke: cd/pwd 应答到齐（chdir + getcwd 渲染）"
# g. Ctrl-C 交互杀链（C-4b）：`run hello`（前台子进程 = bin/hello——打一拍
# `rootfs hello` 后 10s 滚动长睡不自退，kill 目标确定性在位；`run` 走 cpio
# `bin/<名>` 直查，段 f 后 CWD=/d1 无碍）→ 前台期注入 `echo i$?t`
# （collect_fg stdin 臂：非 0x03 字节进 TYPEAHEAD 不丢键）→ 0x03 → 杀集
# `process_terminate(130)`（POSIX 128+SIGINT 惯例码）+ `^C` 回显 →
# TERMINATED 项收口（被杀者零长 EOF 永不到达——kill 不走 libc exit 族的
# EOF 缺口面）→ typeahead 回放 `echo i$?t` 真执行。断言 grep -aqF
# 'i130t'（P6 A-6 展开造内容纪律：注入/回显只含未展开 `i$?t`，i130t 只能
# 来自 $?=130 的真实展开——editor/collect_fg/terminate/typeahead/状态码
# 五环全过才可达，状态谓词非竞速；boot 期 `INT ok` 只盖自验面，本段是
# 交互杀真面）。行间不等待：FIFO/通道排队天然串行（段 d 同款）。
inject_line 'run hello'
inject_line 'echo i$?t'
printf '\x03' >&3; sleep 0.5
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 Ctrl-C 杀链阶段退出"
  grep -aqF 'i130t' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "i130t 60s 未出现（Ctrl-C 交互杀链断？查 TERM/^C 行与 bsh: 诊断）"
  sleep 1
done
echo "smoke: Ctrl-C 交互杀到齐（run hello 前台 + 0x03 → terminate 130 → \$? 展开回放）"

# h. GUI-4 段②输入注入往返（QMP → virtio-tablet → gui-inputd 泵 → bsh
#    inputpoll）：QMP 三批各一 SYN（①DOWN ②MOVE ③UP）——QEMU 命令返回
#    前同步写 used 环，泵 100ms tick 收割入 QUEUE_CAP=32（预缓冲）；随后
#    慢注 inputpoll（0.5s/字节 ≈5.5s 窗）→ bsh INPUT_POLL(max=3) → 三事件
#    真值断言（段②草案 §4.2 真值表）→ INPUTP ok n=3。**状态谓词非竞速
#    （A32）**：注入全部先于命令落位，POLL 时队列必非空；**无串口回声面
#    （A79）**：QMP 事件走 virtio 设备面不进串口，断言坐标字面量只能来自
#    协议应答渲染；'inputpoll' 小写回显与 'INPUTP ok n=3' 互非子串。
qmp_inject || fail "QMP 注入失败（tab0 主案 + 省 device 键回退两路皆拒——查 QMP_SOCK 握手；§7-② 裁定实录）"
inject_line 'inputpoll'
t4=$(date +%s)
while ! grep -qF 'INPUTP ok n=3' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 INPUTP 阶段退出"
  [ $(( $(date +%s) - t4 )) -gt 90 ] \
    && fail "INPUTP ok n=3 90s 未出现（查 gui-inputd: up 行（ABS 域）与 inputpoll 逐事件诊断）"
  sleep 1
done
echo "smoke: INPUTP 输入往返到齐（QMP 三批 → 泵 3 事件 → POLL 真值断言）"

# 注入后金丝雀（fresh 日志从 RX 相起算，无需清）。
t5=$(date +%s)
while :; do
  n=$(grep -c 'switches=' "$LOG" || true)
  [ "${n:-0}" -ge 3 ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 RX 金丝雀阶段退出"
  [ $(( $(date +%s) - t5 )) -gt "$DEADLINE_CANARY" ] \
    && fail "RX 相金丝雀 ${DEADLINE_CANARY}s 内不足 3 条（现有 ${n:-0}）——RX 路径冻住了调度？"
  sleep 2
done
grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
  { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
  END { exit (bad || n<3) ? 1 : 0 }
' || fail "RX 相 switches 非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"

# —— 阶段 5：C-0 刀 4 FAT 子目录相（第四跑：fixture 盘 + FIFO 注入）——
# 盘 = mkfat --fixture 模式（ROOT_LISTING + fixture 专列 DATA/ 目录 +
# DATA/NOTE.TXT = `berryos fat subdir note\n`——与 berry-fatfs host 共识
# 基准同源字节）；**独立盘 disk_dirs.img**：disk.img（RX 相已写、阶段 6
# 持久化相将复用）零扰动——默认盘字节零漂移硬线不破（子目录只在 fixture
# 模式，正是刀 3 的设计面）。73 标记载明全链在目录盘上照跑（DIR/CD ok 自验
# + FATW/RW.TXT 覆写幂等 + FAT 基准读）后注入三段，断言三重防回声：
# ① `ls DATA` → grep 'NOTE.TXT'——断言先于一切含 NOTE.TXT 的注入行回显
#   （顺序防线：本注入行自身只含 'ls DATA'，NOTE.TXT 只能来自 READDIR
#   真应答 = FAT 目录链走查（fsmgr resolve 根→DATA→簇读逐项））；
# ② `cat DATA/NOTE.TXT` → grep 'subdir note'——注入行不含该串，内容
#   只能来自多段 GET 全链（bsh→fsmgr resolve→子项表→NOTE.TXT 簇→vblk）；
# ③ `cat DATA` → grep 'bsh: cat: DATA: bad path'——目录 GET → ST_BAD
#   类型冲突面，`bsh: ` 前缀回显不可达。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
# QEMU 由 SIGTERM 终止后仍存活数秒并继续刷盘——盘取证/复用前等真退
#（P6 A-6 实测纪律；此处 disk.img 阶段 6 要复用）。
sleep 3
: > "$LOG"
FX_BIN=$(mktemp "${TMPDIR:-/tmp}/berryos-fx.XXXXXX")
python3 tools/mkfat.py disk_dirs.img --fixture "$FX_BIN" || fail "mkfat 造目录盘失败"
rm -f "$FX_BIN"
printf 'BERRYOSVBLK!' | dd of=disk_dirs.img conv=notrunc status=none
evs_client_start
skt_client_start
echo "smoke: QEMU 启动（FAT 子目录相：fixture 盘 disk_dirs.img + FIFO stdin）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk_dirs.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（FAT 子目录相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${ALL_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  # 比较用 DEADLINE_ROOTFS（同 rootfs 相注——本相含 FATW 覆写路径）。
  [ $(( $(date +%s) - t6 )) -gt "$DEADLINE_ROOTFS" ] \
    && fail "FAT 子目录相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: FAT 子目录相 ${#ALL_MARKERS[@]}/${#ALL_MARKERS[@]} 到齐，注入三段（ls/cat/cat）"
evs_client_wait "FAT 子目录相"
skt_client_wait "FAT 子目录相"
inject_line 'ls DATA'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 ls DATA 阶段退出"
  grep -aqF 'NOTE.TXT' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "NOTE.TXT 60s 未出现（READDIR FAT 回落链断？查 bsh: ls: 行）"
  sleep 1
done
inject_line 'cat DATA/NOTE.TXT'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 cat DATA/NOTE.TXT 阶段退出"
  grep -aqF 'subdir note' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "subdir note 60s 未出现（多段 GET 链断？查 bsh: cat: 行）"
  sleep 1
done
inject_line 'cat DATA'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 cat DATA 阶段退出"
  grep -aqF 'bsh: cat: DATA: bad path' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "bsh: cat: DATA: bad path 60s 未出现（目录 GET 的 ST_BAD 面断？）"
  sleep 1
done
echo "smoke: FAT 子目录到齐（READDIR + 多段 GET + 目录类型面）"

# —— 阶段 6：P6 A-6 FAT 持久化相（第五跑：写过的盘再起）——
# **不重造盘**（复用 RX 相已写的 disk.img——mkfat 重产 = 持久化零验收；
# disk_dirs.img 本相不复用——两盘链零混）；
# 不关 fd3（本相注入完 cat 行后 FIFO 再无消费者——阶段 7 掉电相三 boot
# 均 stdin=/dev/null，exec 3>&- 移到阶段 7 尾）。73 标记载明
# 全链在「脏盘」上照跑：bsh FATW 自验走**覆写路径**（RW.TXT 旧链 collect →
# 新链分配 → 旧链释放——每次 boot 幂等重过），HELLO/PATTERN 基准照读；
# DF 自验的差分面同走覆写路径（RW.TXT 3000→2048→3000 双向，绝对 free 跨
# 相漂移（重造盘 500/脏盘 499）已被相位无关化消解）。
# 注入 `cat OUT.TXT` → grep 'f42t'：RX 相 `>` 落的扇区跨 boot 存活——
# fat:rw 关卡的「w」半边（读 = FAT ok，写 = FATW ok + 本断言）。
# C-5 追加：注入 `cat T3.TXT` → grep 'x1y'——M3DEMO ④步落盘件的交互
# cat 面（注入行不含 x1y、$? 展开造内容消回声）。**弱化面如实记载**：
# 本相 boot 自验 M3DEMO 会覆写 T3.TXT（RW.TXT 同款覆写幂等路径），故
# 本断言验证「交互 cat 读 FAT 根文件」活面而非跨 boot 字节存活——
# 后者正证仍是 OUT.TXT f42t（自验链不写它，本相读回的必是 RX 相字节）。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
evs_client_start
skt_client_start
echo "smoke: QEMU 启动（FAT 持久化相：复用已写盘 + FIFO stdin + virtio-net）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（FAT 持久化相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${ALL_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  # 比较用 DEADLINE_ROOTFS（同 rootfs 相注——持久化相含 FATW 覆写路径）。
  [ $(( $(date +%s) - t6 )) -gt "$DEADLINE_ROOTFS" ] \
    && fail "FAT 持久化相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: FAT 持久化相 ${#ALL_MARKERS[@]}/${#ALL_MARKERS[@]} 到齐，注入 cat OUT.TXT + cat T3.TXT"
evs_client_wait "FAT 持久化相"
skt_client_wait "FAT 持久化相"
inject_line 'cat OUT.TXT'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在持久化 cat 阶段退出"
  grep -aq 'f42t' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "持久化 f42t 60s 未出现（盘写没落/没跨 boot 存活？查 cache.writeback 与 FATW ok）"
  sleep 1
done
inject_line 'cat T3.TXT'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 T3.TXT cat 阶段退出"
  grep -aqF 'x1y' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "x1y 60s 未出现（M3DEMO T3.TXT 落盘链断？查 M3DEMO ok 与 bsh: 行）"
  sleep 1
done
echo "smoke: FAT 持久化到齐（跨 boot 读回 f42t + M3DEMO T3.TXT 交互 cat 面）"

# —— 阶段 7：B4 掉电相（第六-八跑：独立盘 disk.pwr.img + 采样 ×2 + 注入 +
# 恢复终验）——
# 掉电模拟 = SIGKILL 直杀 QEMU（cache.writeback=off 下已完成扇区已在宿主
# 盘、在途扇区蒸发 = **扇区间**撕裂前缀；扇区内撕裂 = 真机面不在本相覆盖，
# 见 docs/FAT写路径原子性评估-2026-10.md）。三 boot 均无 netdev：vnet 无
# net 槽 = trap 干净自死（NET 族标记自然缺席不设门——EFI 路设备缺席优雅
# 降级同款先例）；bsh 的 fat 族自验（fat→fatw→df 相邻三连）不依赖 net。
# 盘重造 + LBA0 印章（vblk 自验读它）——每次 run 从确定性盘起。
# Boot A（新文件窗采样）：FAT ok 后 +18s SIGKILL 落 fatw 新文件写窗（造源
# staging ≈13s + commit ≈10s；新文件 ④ 无旧链可释放 = 无毒环窗）。采样
# 断言按 kill 时相分两档（kill 后查终局日志——状态谓词，不赌 kill 落点
# 与 TCG 提速赛跑）：FATW ok 未见 → ∈ {noent, clean}（根槽未写 = noent /
# commit ⑥′ 已过 = clean——commit 序 ⑤FAT 先于 ⑥′根，无「根在 FAT 不
# 在」中间态；粗检 canary：执法「根在场而 FAT 撕裂」类现序下不可达
# 态，对 ⑤/⑥′ 写序回归零证伪力——写序执法见评估 §6）；已见（快机已
# 进 DF 覆写面）→ ∈ {clean, poison}（同 Boot B 口径）。
# Boot B（覆写窗采样）：FATW ok（首写毕）后 +15s SIGKILL 落 DF resize
# commit 窗（fatw→df 相邻，resize 3000→2048→3000 ≈ 两次 commit 各 ~10s
# ≈ [+3,+25]s；覆写 commit ④ 旧链释放→⑤ FAT→⑥′根之间存在毒环窗）→
# 采样行断言 ∈ {clean, poison}（整扇区 FAT 写 = 中间簇撕裂 midchain 结构
# 不可达——扇区原子性面的负向断言）。
# 随后 pwrcls --poison **确定性注入毒环态**（RW.TXT 整链双 FAT 置 Free、
# 根目录原样 = 覆写 commit 撕裂窗「FAT 已写、根未写」的盘面形状——自然
# 采样撞窗是概率事件，注入 = 每轮必达）+ 注入后分类必须 poison。
# 已知缺口（如实入册，评估 §6）：Boot A/B 的 kill 前无活性复验——无输出
# 挂死与合法采样不可区分（kernel panic 有 loud 串口、由标记帽兜）；
# switches= 金丝雀每 50 切一行（1x ≈10s），18s/15s 窗内零增长断言在慢机
# 上是假红，故不做。
# Boot C（恢复终验）：毒环盘上全链重跑——fatw 覆写走恢复 commit（①collect
# 走旧首簇即撞 Free 止步 → ②升序分配把旧首簇复用进新链头（smoke 单文
# 件覆写负载下它是最低 Free；一般负载可中段复用/不复用——守卫按成员制
# 不依赖此）→
# ④守卫跳过 ∈ 新链的释放——毒环自愈，user/bins/fsmgr/src/fat.rs 头注）+
# DF resize + DIR/CD 照跑；guest 标记到齐后 SIGTERM 优雅退 + sleep 3 →
# **盘侧终验必 clean**（FAT0 侧——FAT1 分歧不在 oracle 面，评估 §5.2；
# guest 标记绿 + 宿主盘面 clean 双面不变量——标记绿而盘面毒 = 恢复链
# 假绿，单面都测不出）。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
python3 tools/mkfat.py disk.pwr.img || fail "mkfat 造掉电盘失败"
printf 'BERRYOSVBLK!' | dd of=disk.pwr.img conv=notrunc status=none
echo "smoke: QEMU 启动（B4 掉电 Boot A：新文件窗采样，无 netdev）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.pwr.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!
t7a=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（掉电 Boot A）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  grep -qF 'FAT ok n=5000' "$LOG" && break
  [ $(( $(date +%s) - t7a )) -gt 240 ] \
    && fail "掉电 Boot A 240s FAT ok 未到（bsh fat 链断？查 BSH/TAB/INT ok）"
  sleep 1
done
grep -q 'ROOTFS vmo=none' "$LOG" \
  && fail "掉电 Boot A: ROOTFS vmo=none（rootfs VMO 交付缺席——入机侧 seam 断）"
# +18s 落 fatw staging/commit 窗（FAT ok 后 fatw 即刻起步——bsh 链相邻）。
sleep 18
kill -9 "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
# SIGKILL 即时死（无 SIGTERM 的数秒刷盘拖尾）——盘面 = 撕裂前缀原样。
PWR_A=$(python3 tools/pwrcls.py --classify disk.pwr.img) \
  || fail "掉电 Boot A 分类失败（pwrcls 退出非 0——盘面畸形或几何不符）"
echo "smoke: 掉电采样 A（fatw 新文件窗）: $PWR_A"
if grep -qF 'FATW ok n=3000' "$LOG"; then
  # 快机已进 DF 覆写面（FATW ok 已见于终局日志）——按 Boot B 口径放行。
  case "$PWR_A" in
    "PWR cls=clean "*|"PWR cls=poison "*) ;;
    *) fail "掉电 Boot A 采样越界：$PWR_A（FATW 已毕、覆写窗口径 {clean, poison}——midchain/malformed = 扇区原子性假设破）" ;;
  esac
else
  # fatw 未毕——新文件窗口径（④ 空转无毒环面；粗检 canary，见块注）。
  case "$PWR_A" in
    "PWR cls=noent "*|"PWR cls=clean "*) ;;
    *) fail "掉电 Boot A 采样越界：$PWR_A（新文件路径应 ∈ {noent, clean}——出现毒环 = ④ 旧链释放被无端执行）" ;;
  esac
fi

# Boot B：覆写 resize 窗采样 + 确定性毒环注入。
: > "$LOG"
echo "smoke: QEMU 启动（B4 掉电 Boot B：DF resize 窗采样 + 注入）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.pwr.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!
t7b=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（掉电 Boot B）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  grep -qF 'FATW ok n=3000' "$LOG" && break
  [ $(( $(date +%s) - t7b )) -gt 240 ] \
    && fail "掉电 Boot B 240s FATW ok 未到（fatw 链断？Boot A 留盘 $PWR_A）"
  sleep 1
done
grep -q 'ROOTFS vmo=none' "$LOG" \
  && fail "掉电 Boot B: ROOTFS vmo=none（rootfs VMO 交付缺席——入机侧 seam 断）"
# +15s 落 DF resize commit 窗（FATW ok 后 df 即刻起步——bsh 链相邻；两次
# resize commit ≈ [+3,+25]s，15s 居中）。
sleep 15
kill -9 "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
PWR_B=$(python3 tools/pwrcls.py --classify disk.pwr.img) \
  || fail "掉电 Boot B 分类失败（pwrcls 退出非 0）"
echo "smoke: 掉电采样 B（DF resize 窗）: $PWR_B"
case "$PWR_B" in
  "PWR cls=clean "*|"PWR cls=poison "*) ;;
  *) fail "掉电 Boot B 采样越界：$PWR_B（整扇区 FAT 写下应 ∈ {clean, poison}——midchain/malformed = 扇区原子性假设破）" ;;
esac
python3 tools/pwrcls.py --poison disk.pwr.img \
  || fail "毒环注入失败（pwrcls --poison 自检未过）"
PWR_P=$(python3 tools/pwrcls.py --classify disk.pwr.img) \
  || fail "注入后分类失败（pwrcls 退出非 0）"
case "$PWR_P" in
  "PWR cls=poison "*) ;;
  *) fail "注入后非毒环态：$PWR_P（--poison 自检与分类分歧——工具面 bug）" ;;
esac
echo "smoke: 毒环注入就位（$PWR_P）"

# Boot C：毒环盘恢复终验——guest 标记（fsmgr/bsh fat 族 + DF/DIR/CD）+ 盘面双面。
: > "$LOG"
echo "smoke: QEMU 启动（B4 掉电 Boot C：毒环恢复终验，无 netdev）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.pwr.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!
# FAT/FATW 精确形（盘确定性：PATTERN 5000B / RW 3000B）；DF/CD 证明 resize
# 与目录链在恢复后的盘上照常。DIR ok 由 CD ok 承接（bsh 链 dir→cd 相邻）。
PWR_MARKERS=("FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=5000" "FATW ok n=3000" "DF ok" "CD ok")
t7c=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（掉电 Boot C 恢复相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${PWR_MARKERS[@]}"; do
    grep -qF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "掉电 Boot C: ROOTFS vmo=none（rootfs VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t7c )) -gt 300 ] \
    && fail "掉电 Boot C 300s 标记未到齐（毒环恢复链断？），缺：$missing"
  sleep 1
done
# 盘取证前优雅退 + 等真退（P6 A-6 纪律）。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
sleep 3
PWR_C=$(python3 tools/pwrcls.py --classify disk.pwr.img) \
  || fail "掉电终验分类失败（pwrcls 退出非 0）"
case "$PWR_C" in
  "PWR cls=clean "*) ;;
  *) fail "掉电终验非 clean：$PWR_C（guest 标记全绿而盘面未恢复——恢复 commit 假绿或 ④ 守卫失效）" ;;
esac
echo "smoke: B4 掉电相到齐（采样 A/B + 毒环注入 + 恢复终验 $PWR_C）"

exec 3>&-
echo "smoke: PASS（金丝雀 ${n} 条严格递增 + rootfs 相全链 + RX 交互相（RX ok + bsh 退格/历史召回/落盘/RAM 目录链/cd-pwd 五应答）+ FAT 子目录相（fixture 盘 ls/cat DATA 活覆盖）+ FAT 持久化相（跨 boot 读回 f42t + M3DEMO T3.TXT 交互 cat 面）+ B4 掉电相（采样 A/B ∈ 合法集 + 毒环注入 + Boot C 恢复终验盘面 clean））"
