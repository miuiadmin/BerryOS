#!/usr/bin/env bash
# BerryOS 无人值守冒烟测试 —— CI 与本地同一入口（just smoke）。
#
# 做七件事：
#   1. 后台拉起 QEMU virt（-kernel ELF 直加载），串口重定向到临时日志；
#   2. 断言 boot 标记全部出现（BerryOS boot / GIC ok / … / ktest done）；
#   3. 断言 switches=N 金丝雀 ≥3 条且严格递增（IRQ 抢占分时活着、计数不冻）；
#   4. G-eco rootfs 相（G-eco-c 起）：重拉 QEMU 带 -fw_cfg rootfs.cpio，断言
#      入机侧（ROOTFS ok/vmo 行）+ 编译侧全链（rootfs hello = hello 进程
#      自打、ECO ok = svcmgr 流式 cpio/ELF 拉起成功、STD ok/STD err ok =
#      stdhello 经 musl target + berry-libc shim + 流式装载全链自打——
#      G-eco-d d-1c）标记到齐——「rootfs 程序经全链跑通出字」的关卡化
#      （裸跑相 rootfs 缺席 = svcmgr 静默降级，23 标记零扰动，两相互证）。
#   5. P5 RX 交互相（**rootfs+FIFO 合相**，P5 A-1 起为第三跑；注入序列
#      P5 v0 起扩为编辑三段、P6 A-6 起四段、C-0 刀 4 起五段、C-1 刀 5 起六段）：三拉 QEMU 带
#      -fw_cfg rootfs + FIFO stdin（holder 子进程占住写端防 EOF），47 标记
#      （boot 23 + rootfs 24——PIPE ok/VIRTIO ok/VBLK ok/BLKD ok/FAT ok/
#      FATW ok/NET ok ping=/DNS ok addr=/TCP ok echo=/TCPS ok echo=/APP ok n=/
#      DIR ok n= 在列 = bsh 已进交互读循环/virtio 探测与块设备 IO/块设备服务
#      消费链/FAT 只读底座与**落盘写**/网络往返与网络服务/TCP 客户端与 TCP
#      服务/.bap 应用拉起消费链/RAM 目录树 v2 五 op 自验全过，
#      状态谓词门控注入时机，不赌 boot 秒数）后
#      **逐字节慢速注入**编辑序列（FEN=0
#      FIFO 深 1 字节——慢速 = 单字节-per-handler 的标记确定性，消竞速；
#      QEMU chardev 流控押住输入不 overrun——真机才有突发丢字）：
#        a. `rx\n` → console token 面 `RX ok`（A-1）；
#        b. `helll\x7f`+`o\n` → 退格编辑得 `hello` → 转发 bsh →
#           `bsh: hello: not found`（A-2 行编辑 + 转发 + bsh 应答）；
#        c. `\x1b[A`+`!\n` → 历史召回 `hello` + `!` → 转发 →
#           `bsh: hello!: not found`（P5 v0 mini-readline：ESC[A 上翻）；
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
#      不破；独立盘 disk_dirs.img，与 disk.img 链零交集），47 标记载明全链
#      在目录盘上照跑后注入三段（断言与注入行互非子串 + 顺序防回声）：
#      `ls DATA` → grep 'NOTE.TXT'（READDIR FAT 回落链走查——断言先于一切
#      含 NOTE.TXT 的注入行回显）→ `cat DATA/NOTE.TXT` → grep
#      'subdir note'（多段 GET 全链：bsh→fsmgr resolve→子目录表→簇读→vblk）
#      → `cat DATA` → grep 'bsh: cat: DATA: bad path'（目录 GET → ST_BAD
#      类型冲突面）。
#   7. P6 A-6 FAT 持久化相（第五跑）：**不重造盘**（复用第 5 相已写的
#      disk.img——mkfat 重产 = 持久化零验收；disk_dirs.img 不复用——无
#      跨盘盘混），47 标记载明写过的盘再起（bsh FATW 自验走覆写路径 =
#      旧链回收/新链分配幂等重过）+ 注入 `cat OUT.TXT` → grep 'f42t'——
#      `>` 落的是真扇区、跨 boot 存活。盘 cache writeback=off：QEMU 由
#      SIGTERM 终止，writeback 模式可能不刷页缓存（所有 -drive 行统一，
#      含 justfile run-rootfs）。
#
# 注意：QEMU TCG 下 CNTV tick 实测 ≈ SLICE_HZ/2（见 CLAUDE.md 陷阱节），
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
DEADLINE_ROOTFS=300  # 秒：rootfs/RX/持久化三相标记到齐——P6 A-6 起 FATW
                     # 自验含整链 commit（fsmgr 侧 ~13 次串行扇区 RPC，TCG
                     # 下每次 ~0.8s）+ GET 读回 ≈ +30-40s，120s 结构性不够；
                     # T-d 起 svcmgr TCPS 三 op（每 op 一轮 TCP 会话相位，
                     # TCG 下三 op ≈ +15-25s）叠加，240s 余量见底 → 300s
                     #（帽只兜结构性挂死，常态全链 ~150s 内到齐）
DEADLINE_CANARY=180  # 秒：等 ≥3 条 switches 金丝雀
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke.XXXXXX")
QEMU_PID=""
RX_FIFO=""
RX_HOLDER=""
# T-c：宿主 TCP 回显服务（echod.py）进程与日志（cleanup 兜底收）。
ECHO_PID=""
ECHO_LOG=""

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
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke: 串口日志保留：$LOG" >&2
    # T-c 对抗审查实抓：ECHO_LOG 同为失败现场（echod traceback 只落它）——
    # SMOKE_KEEP_LOG=1 只保串口日志时宿主侧死因恒被抹。
    [ -n "$ECHO_LOG" ] && echo "smoke: echod 日志保留：$ECHO_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
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
  echo "--------------------------------" >&2
  exit 1
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
echo "smoke: QEMU 启动（rootfs 相：-fw_cfg opt/berry/rootfs + virtio-blk + virtio-net）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
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
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "TCPS ok echo=" "APP ok n=" "DIR ok n=" "CD ok")
t2=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（rootfs 相）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${ROOTFS_MARKERS[@]}"; do
    grep -qF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  # vmo 行须非降级形态（=none 即 seam 断：内核没把 rootfs 包进 VMO）。
  if [ -z "$missing" ] && grep -q 'ROOTFS vmo=none' "$LOG"; then
    fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
  fi
  [ -z "$missing" ] && break
  # 相内含 FATW 整链 commit + T-d TCP 三 op 全走真网卡——比较须用
  # DEADLINE_ROOTFS（现 300s；与文案一致。历史上写 BOOT 是笔误：比较 120s
  # 文案当时 240s，>120s 到齐会被误杀）。
  [ $(( $(date +%s) - t2 )) -gt "$DEADLINE_ROOTFS" ] \
    && fail "rootfs 相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: rootfs 相 ${#ROOTFS_MARKERS[@]}/${#ROOTFS_MARKERS[@]} 到齐（ROOTFS 双行 + rootfs hello + ECO ok + STD 双标记 + FILE ok + HEXYL ok + BSH ok + EXIT ok + PIPE ok + FS ok n= + VIRTIO ok + VBLK ok + BLKD ok n= + FAT ok n= + FATW ok n= + NET ok ping= + DNS ok addr= + TCP ok echo= + TCPS ok echo= + APP ok n= + DIR ok n= + CD ok）"

# app-0 a-2 证据层：APPDEMO（console=true）子自证输出——谓词 `APP ok n=`
# 证明「全部槽拉起 + reap 真码 0」，本行证明 console 能力面真把 stdout
# 接到了串口（reap 0 而输出丢失 = 能力面静默断，谓词层抓不到）。小写
# `app ` 前缀与全部大写标记面互非子串（与谓词分立的两层——HEXYL ok 的
# 「私有通道收齐 + 基准对拍才打」同款分层思想）。
grep -q 'app APPDEMO up' "$LOG" \
  || fail "app APPDEMO up 缺失（console=true 能力面断——appdemo 未见自证输出；查 eco: app 行）"

# —— 阶段 4：P5 RX 交互相（rootfs+FIFO 合相：内核 RX 中断 + console 行编辑
# + bsh stdin 转发 + bsh 应答）——
# stdin 换 FIFO：holder（sleep 占写端）防 EOF 断流；boot 23 + rootfs 24 = 47
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
# 退格编辑得 `hello` → endK2 转发 bsh → `bsh: hello: not found`；c. `\x1b[A`
# +`!\n` → ESC[A 历史召回 `hello`+`!` → `bsh: hello!: not found`（与 b 互非
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
# 每跑 DEADLINE_ROOTFS=300s 帽 + 注入段 0.5s/字节——900s 只盖两跑结构性
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
echo "smoke: QEMU 启动（RX 交互相：rootfs + FIFO stdin + virtio-blk + virtio-net）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!

# 注入口先备好读端存在的证据：47 标记全到 = QEMU 正常跑且 bsh 就位（exec 3>
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
# b. 退格编辑：`helll` + \x7f（回退 → `hell`）+ `o` + \n → 完整行 `hello` →
# 转发 bsh → `bsh: hello: not found`。
printf 'h' >&3; sleep 0.5
printf 'e' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf 'l' >&3; sleep 0.5
printf '\x7f' >&3; sleep 0.5
printf 'o' >&3; sleep 0.5
printf '\n' >&3
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 hello 应答阶段退出"
  grep -qF 'bsh: hello: not found' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: hello: not found 60s 未出现（转发链断？查 echo 字节与 fdrop 计数）"
  sleep 1
done
echo "smoke: bsh hello 应答到齐"
# c. 历史召回：ESC [ A（上翻 → 最新历史 `hello`）+ `!` + \n → `hello!` →
# 转发 → `bsh: hello!: not found`。
printf '\x1b' >&3; sleep 0.5
printf '[' >&3; sleep 0.5
printf 'A' >&3; sleep 0.5
printf '!' >&3; sleep 0.5
printf '\n' >&3
t4=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 hello! 应答阶段退出"
  grep -qF 'bsh: hello!: not found' "$LOG" && break
  [ $(( $(date +%s) - t4 )) -gt 60 ] \
    && fail "bsh: hello!: not found 60s 未出现（ESC[A 历史召回链断？）"
  sleep 1
done
echo "smoke: bsh hello! 应答到齐（退格 + ESC[A 召回双验过）"
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
# （自验面 CD ok 已在 47 标记列）。
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
# 模式，正是刀 3 的设计面）。47 标记载明全链在目录盘上照跑（DIR/CD ok 自验
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
echo "smoke: QEMU 启动（FAT 子目录相：fixture 盘 disk_dirs.img + FIFO stdin）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk_dirs.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
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
# 不关 fd3（阶段 5 已注入完 cat 行，exec 3>&- 移到本阶段尾）。47 标记载明
# 全链在「脏盘」上照跑：bsh FATW 自验走**覆写路径**（RW.TXT 旧链 collect →
# 新链分配 → 旧链释放——每次 boot 幂等重过），HELLO/PATTERN 基准照读。
# 注入 `cat OUT.TXT` → grep 'f42t'：RX 相 `>` 落的扇区跨 boot 存活——
# fat:rw 关卡的「w」半边（读 = FAT ok，写 = FATW ok + 本断言）。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
echo "smoke: QEMU 启动（FAT 持久化相：复用已写盘 + FIFO stdin + virtio-net）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
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
echo "smoke: FAT 持久化相 ${#ALL_MARKERS[@]}/${#ALL_MARKERS[@]} 到齐，注入 cat OUT.TXT"
inject_line 'cat OUT.TXT'
t6=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在持久化 cat 阶段退出"
  grep -aq 'f42t' "$LOG" && break
  [ $(( $(date +%s) - t6 )) -gt 60 ] \
    && fail "持久化 f42t 60s 未出现（盘写没落/没跨 boot 存活？查 cache.writeback 与 FATW ok）"
  sleep 1
done
echo "smoke: FAT 持久化到齐（跨 boot 读回 f42t）"

exec 3>&-
echo "smoke: PASS（金丝雀 ${n} 条严格递增 + rootfs 相全链 + RX 交互相（RX ok + bsh 退格/历史召回/落盘/RAM 目录链/cd-pwd 五应答）+ FAT 子目录相（fixture 盘 ls/cat DATA 活覆盖）+ FAT 持久化相（跨 boot 读回 f42t））"
