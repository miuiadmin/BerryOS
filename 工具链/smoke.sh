#!/usr/bin/env bash
# BerryOS 无人值守冒烟测试 —— CI 与本地同一入口（just smoke）。
#
# 做六件事：
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
#      P5 v0 起扩为编辑三段、P6 A-6 起四段）：三拉 QEMU 带
#      -fw_cfg rootfs + FIFO stdin（holder 子进程占住写端防 EOF），39 标记
#      （boot 23 + rootfs 16——PIPE ok/VIRTIO ok/VBLK ok/BLKD ok/FAT ok/
#      FATW ok 在列
#      = bsh 已进交互读循环/virtio 探测与块设备 IO/块设备服务消费链/FAT
#      只读底座与**落盘写**全过，状态谓词
#      门控注入时机，不赌 boot 秒数）后
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
#           消回声歧义的确定性状态谓词）。
#      b/c/d 三断言互非子串。随后再验 switches 金丝雀严格递增（IRQ 分流
#      破坏屏蔽态/EOI 漏发类 bug 的死状 = 冻而不死，kill -0 活性检抓不到）。
#   6. P6 A-6 FAT 持久化相（第四跑）：**不重造盘**（复用第 5 相已写盘——
#      mkfat 重产 = 持久化零验收），39 标记载明写过的盘再起（bsh FATW
#      自验走覆写路径 = 旧链回收/新链分配幂等重过）+ 注入 `cat OUT.TXT`
#      → grep 'f42t'——`>` 落的是真扇区、跨 boot 存活。盘 cache
#      writeback=off：QEMU 由 SIGTERM 终止，writeback 模式可能不刷页
#      缓存（所有 -drive 行统一，含 justfile run-rootfs）。
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
DEADLINE_ROOTFS=240  # 秒：rootfs/RX/持久化三相标记到齐——P6 A-6 起 FATW
                     # 自验含整链 commit（fsmgr 侧 ~13 次串行扇区 RPC，TCG
                     # 下每次 ~0.8s）+ GET 读回 ≈ +30-40s，120s 结构性不够
DEADLINE_CANARY=180  # 秒：等 ≥3 条 switches 金丝雀
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke.XXXXXX")
QEMU_PID=""
RX_FIFO=""
RX_HOLDER=""

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
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke: FAIL: $1" >&2
  # 失败必带现场：串口日志尾部直接进 stderr（CI 里即是诊断依据）。
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
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
python3 工具链/mkfat.py disk.img || fail "mkfat 造盘失败"
printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"  # 复用同一日志文件（cleanup 只删一份）
echo "smoke: QEMU 启动（rootfs 相：-fw_cfg opt/berry/rootfs + virtio-blk）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# 入机侧两行 + 编译侧全链：hello 自打 = 装载的程序真的在跑且能 syscall；
# stdhello 自打 STD 双标记 = **std 程序经 musl target + berry-libc shim +
# 流式装载全链跑通**（G-eco-d d-1c——sum/heap/argc 自检过了才轮到标记）；
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
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=")
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
  # 相内含 FATW 整链 commit（TCG ~+30-40s）——比较须用 DEADLINE_ROOTFS
  #（240s；与文案一致。写 BOOT 是笔误：比较 120s 文案 240s，>120s 到齐会被误杀）。
  [ $(( $(date +%s) - t2 )) -gt "$DEADLINE_ROOTFS" ] \
    && fail "rootfs 相 ${DEADLINE_ROOTFS}s 标记未到齐，缺：$missing"
  sleep 1
done
echo "smoke: rootfs 相 ${#ROOTFS_MARKERS[@]}/${#ROOTFS_MARKERS[@]} 到齐（ROOTFS 双行 + rootfs hello + ECO ok + STD 双标记 + HEXYL ok + BSH ok + EXIT ok + PIPE ok + FS ok n= + VIRTIO ok + VBLK ok + BLKD ok n= + FAT ok n= + FATW ok n=）"

# —— 阶段 4：P5 RX 交互相（rootfs+FIFO 合相：内核 RX 中断 + console 行编辑
# + bsh stdin 转发 + bsh 应答）——
# stdin 换 FIFO：holder（sleep 占写端）防 EOF 断流；boot 23 + rootfs 16 = 39
# 标记到齐（PIPE ok 在列 = bsh selftest 全过、已进交互读循环——注入的行才
# 有人收；VIRTIO/VBLK/BLKD/FAT/FATW ok 在列 = MMIO 探测、块设备 IO、块设备
# 服务消费链与 FAT 只读底座+落盘写全过；状态谓词门控——不赌 boot 秒数）
# 后经 fd3 逐字节注入编辑序列，每
# 字节隔 0.5s（FEN=0 FIFO 深 1 字节——慢速 = 单字节-per-handler 标记确定性；
# QEMU 流控不丢字，真机才有 overrun 面）：a. `rx\n` → `RX ok`（console 收齐
# 行 token 才打——内核侧状态断言，非注入即达回声）；b. `helll\x7f`+`o\n` →
# 退格编辑得 `hello` → endK2 转发 bsh → `bsh: hello: not found`；c. `\x1b[A`
# +`!\n` → ESC[A 历史召回 `hello`+`!` → `bsh: hello!: not found`（与 b 互非
# 子串，各自独立断言）；d. `run fail\n` → `echo f$?t > OUT.TXT\n` →
# `cat OUT.TXT\n` → grep 'f42t'（A-6 FAT 落盘链，见头部段 d 注）。注入后
# 仍跑金丝雀——RX 中断若破坏 IRQ 屏蔽契约
# （在屏蔽窗内置 Event/唤醒却漏 EOIR/漏开关），死状常为「冻而不死」（活性
# 检盲区），只有切换序列还动才真活着。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
RX_FIFO=$(mktemp "${TMPDIR:-/tmp}/berryos-rx.XXXXXX")
rm -f "$RX_FIFO"
mkfifo "$RX_FIFO" || fail "mkfifo 失败"
( sleep 900 ) > "$RX_FIFO" &
RX_HOLDER=$!
# 盘重造 + LBA0 印章重落（rootfs 相的 FATW 落盘写（RW.TXT）会脏卷面——
# 持久化相的基线须从确定性盘起：本相重造保证段 d 的 OUT.TXT 落在已知的
# 簇位（8）与根目录槽位；vblk LBA1/BLKD LBA5 仍在保留区）。**本相写过的
# 盘直接喂阶段 5**（持久化相不再重造）——cache.writeback=off 保证
# SIGTERM 终止前扇区已同步落宿主盘。
python3 工具链/mkfat.py disk.img || fail "mkfat 造盘失败"
printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
echo "smoke: QEMU 启动（RX 交互相：rootfs + FIFO stdin + virtio-blk）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!

# 注入口先备好读端存在的证据：39 标记全到 = QEMU 正常跑且 bsh 就位（exec 3>
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

# —— 阶段 5：P6 A-6 FAT 持久化相（第四跑：写过的盘再起）——
# **不重造盘**（复用 RX 相已写的 disk.img——mkfat 重产 = 持久化零验收）；
# 不关 fd3（阶段 5 还要注入 cat 行，exec 3>&- 移到本阶段尾）。39 标记载明
# 全链在「脏盘」上照跑：bsh FATW 自验走**覆写路径**（RW.TXT 旧链 collect →
# 新链分配 → 旧链释放——每次 boot 幂等重过），HELLO/PATTERN 基准照读。
# 注入 `cat OUT.TXT` → grep 'f42t'：上一相 `>` 落的扇区跨 boot 存活——
# fat:rw 关卡的「w」半边（读 = FAT ok，写 = FATW ok + 本断言）。
kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
: > "$LOG"
echo "smoke: QEMU 启动（FAT 持久化相：复用已写盘 + FIFO stdin）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
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
echo "smoke: PASS（金丝雀 ${n} 条严格递增 + rootfs 相全链 + RX 交互相（RX ok + bsh 退格/历史召回/落盘三应答）+ FAT 持久化相（跨 boot 读回 f42t））"
