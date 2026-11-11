#!/usr/bin/env bash
# smoke-image：M-G1 V-4 img-boot 独立冒烟关卡（VM 草案 §V-4；smoke-x86 同款
# 形态——独立关卡，**不并入主 smoke**，ARM64 五跑零扰动；M-G1 收官时另裁
# 是否升第六跑）。零 -kernel / 零 fw_cfg：AAVMF 固件 + out/berryos.img 挂盘
# （qemu-xhci + usb-storage）启动——「盘自己把自己拉起来」的全链自动验收。
#
# 断言面（ EFI 十段/接续形态源自 V-3 收官 run；生态 39 = 对抗审查保留日志
# 标记普查实证面（B-1 起 +B1 ok n=4；C-2c 挂账刀起 +U4 ok——深坑六件执行
# 面八步；C-4 起 +TAB ok——编辑器九步 + 真源两钉，零 fs 盘面 EFI 路可达；
# T-1 起 +THR ok——进程内多线程消费面，thrchild 镜像内嵌 svcmgr 零
# rootfs 依赖，EFI 路同打；G4-2 起 +DPGP ok——pager 首形跨进程供给面
#（dpgp 内嵌镜像 bootstrap=0，0x21 阻塞收货，零 rootfs 依赖 EFI 路同打）；
# G4-3 起 +DPGS ok n=4——缺页风暴 N=4（dpg_storm 共享三 VMO × 4 子并发
# 首触自愈收敛，零 rootfs 依赖 EFI 路同打）；G4-3 B 刀起 +DPGN ok n=4——
# 0x21 四早退负序（门/对齐/commit/身份验各断精确 errno，join 9/7 双锚，
# 零 rootfs 依赖 EFI 路同打）；
# GUI-1 起 +FBW ok——guifbd 显示服务自验三步
# （clear 双相位差分/蜂格字形对拍/scroll 行移），EFI-GOP 路专属、-kernel
# 主路零插表零路径；GUI-1 段② 起 +FBSERVE ok——console fan-out 上屏
# serve 循环首条消息（双路上屏闭环）；GUI-2 段③ 起 +GFX ok/GFXF ok n=
# ——gfxdemo 消费侧首刀客户端/服务端双标记（present 域 SUBMIT 闭环 + hdi
# 软 vsync mini 序）；A7-1 起 +MMP ok、A7-3 起 n=9——
# bsh mmp_selftest 九步：匿名档 libc 桥 + 文件档 staging（⑥-⑧ 走 RAM 树
# fs 非盘面）+ mprotect dormant 环，EFI 路同打；
# A7-2 起 +CLK ok n=6——bsh clk_selftest 六步，timer 壁钟分档 + 补桩
# 五真身首活体，纯 clock syscall + `date -u` spawn 无文件参零 fs 盘面
# EFI 路同打；
# A7-4 起 +SIG ok n=6——bsh sig_selftest 六步（⓪raise 即时投递 ①表/掩码
# 回读 ②blocked 窗 held→unblock 补投 ③kill 分诊 ④sigchild 跨进程
# SIGTERM→DFL exit(143) ⑤修复轮回归钉板——嵌套补投恰一次/delivery-
# time 表变更/包络恢复/无残留双投），纯用户态信号面 + sigchild 零 gift
# spawn 零 fs 盘面，EFI 路同打）；GUI-5 起 +WM ok——gfxdemo 改连 wm 域往返
# 四组断言单 println（EFI 专属）
# ——非主观子集）：
#   1. EFI 十段恰一次：skeleton / fw / gop（成功形态 WxH，locate fail 不算）/
#      fs HELLO.TXT / kfile / init / elf / load / mmap / BERRYBOOT ok；
#   2. 内核接续恰一次：HANDOFF ok ver= / MMAP ok segs= / ROOTFS ok
#      **src=handoff**（handoff 源专属形态——fw_cfg/DTB 路不通于此）；
#   3. 裸相 32 标记（与 smoke.sh 同一清单——内核行为零漂移的证据；B-0 起
#      + BENCH ok n=6 微基准/churn 金丝雀关卡行；R-0 起 + BENCHWAKE n=
#      唤醒延迟直方图形状行；S-2 起 + BENCHXH ok 资源耗尽/恢复六面行）+ EFI 路
#      生态 39 标记（rootfs hello / ECO / STD 双 / FILE / HEXYL / BSH /
#      EXIT / APP / APPUI / PIPE / FS / U1 / U2 / U3 / B1 / U4 / TAB / INT /
#      POLL / PS / BC / THR / DPGP / DPGS / DPGN / MMP / CLK / SIG / FBW / FBSERVE /
#      GFX 双 / WM /
#      LAUNCH 双（GUI-8 段③ 起 EFI 专属——launcher 首帧 LAUNCH ok + 合成
#      swipe 两轮翻页 PAGE ok n=2，冻结相自测面）
#      ——除 WM 与 LAUNCH 双（EFI 专属，详 ECO_MARKERS 前注）外与 smoke.sh
#      rootfs 相断言 canon 同面；无 virtio 面：不挂 virtio-blk/net，
#      VIRTIO/FAT/NET/DIR/CD 族不在断言面）；
#   4. kfile/init 字节数与宿主 KERNEL.ELF / rootfs.cpio **精确对拍**（盘
#      内容 ↔ BerryBoot 读数 ↔ 内核消费三方一致性，镜像打包断裂即 FAIL）；
#   5. switches 金丝雀 ≥3 条严格递增（同 smoke.sh——冻而不死类 bug 的
#      唯一探测器）；注入后复验（RX 中断破坏 IRQ 契约的死状是冻而非崩）；
#   6. RX 注入相：FIFO stdin 逐字节注入 `rx\n` → RX ok（PL011 RX 中断 →
#      console 任务 → token 面全链；bsh 可交互的自动代理——提示符本身无
#      固定标记；C-4b 勘正：RX ok 实为内核 console 观察窗直打、不经 bsh
#      ——EFI 路 bsh park 在 fat 段（C-2b 契约），注入命令不执行，此相
#      只验内核 RX 链路非 bsh 交互）。
#      C-4b 裁定：EFI 路不设 Ctrl-C 交互杀段（i130t）——bsh 交互编辑循环
#      结构性不可达（fat park 挡断，详见阶段 5 后注记）；交互杀五环面由
#      主路 smoke.sh RX 相段 g 独占（有盘路 bsh 全功能），EFI 路杀链核心
#      由 boot 期 INT ok 覆盖（真机 spawn+terminate+reap 四步）。
#   7. boot banner 形态（G-ver-2 镜像路同面，版本管理标准 §3）：首匹配行
#      须为 `BerryOS boot <semver> (<ident>[ rustc<ver> dbg|rel])`——正则
#      与 smoke.sh 主路逐字同款（原前锚弱形升全形，三脚本同正则复制）；
#      ident=nogit 即红（标准 §6——nogit 标识产物不得进发布链）。
#   8. VER 哨兵执法（标准 §1 构建标识轨道，审计洞①闭环）：`eco: VER ok`
#      在席 + `eco: VER mix` 缺席——EFI 相（GOP 相门）是 VER 行唯一实现
#      场，混合构建 loud 行命中即红。
#
# UTM 手动点验四条（同镜像、UI 无法 CI——清单正文见 docs/开发体验.md §L3）：
#   ① 插盘：UTM 新建 ARM64 虚机（UEFI）驱动器挂 berryos.img；
#   ② 开机：固件自举 → BerryBoot 十段 → 内核标记流；
#   ③ 到提示符：串口/控制台终端出 bsh 提示符；
#   ④ 键盘输入：键入 rx（回显）→ ls → 截图留档。
#
# 用法：tools/smoke-image.sh <镜像> <AAVMF_CODE.fd> <AAVMF_VARS.fd>
#（三参均由 justfile 单一事实源注入——smoke-x86 先例：脚本不自带第二份
# 路径拷贝。第 3 参 AAVMF_VARS 现仅占位：vars 自产 blank，不读共享模板
# ——见下方 vars 生成段注。）
# 调试：SMOKE_KEEP_LOG=1 just smoke-image 保留串口日志不删。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"
KERNEL=$(berryos_kernel debug)

IMAGE=${1:?用法: smoke-image.sh <镜像> <AAVMF_CODE> <AAVMF_VARS>}
AAVMF_CODE=${2:?缺 AAVMF_CODE 路径}
AAVMF_VARS=${3:?缺 AAVMF_VARS 路径}

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke-image: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$IMAGE" ] || { echo "smoke-image: FAIL: 镜像不存在（先 just image）：$IMAGE" >&2; exit 1; }
[ -f "$AAVMF_CODE" ] || { echo "smoke-image: FAIL: AAVMF CODE 固件不存在：$AAVMF_CODE" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke-image: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }
[ -f "rootfs.cpio" ] || { echo "smoke-image: FAIL: rootfs.cpio 缺席（先 just build-user）" >&2; exit 1; }

# 字节对拍基线在入口快照（= just image 刚毕、与镜像打包同窗）：QEMU 运行窗内
# 共享 ~/.cargo-target 若被兄弟会话构建覆盖，收尾再 wc 会读到换脸文件 → 假
# FAIL（2026-11-03 实抓：镜像 3794640B ↔ 收尾 stat 3973440B——雷册 §B29
# 时序踩踏形态）。rootfs.cpio 虽在仓库根不在共享 target dir，同窗快照保持一致。
KERNEL_BYTES=$(wc -c < "$KERNEL" | tr -d ' ')
INITRD_BYTES=$(wc -c < "rootfs.cpio" | tr -d ' ')

# —— 构建产物断言（版本管理标准 §1 五轨道配对律·三方联动第一方；置
#    pre-flight 位 = 断链不烧 115s TCG 先红 + SMOKE_PHASES=boot 分层面同
#    受护）——manifest.txt = mkimage 侧产物（与镜像同目录、不进盘），其
#    platform_revision 列自 PR≡level 刀单源取 tools/abi.toml [platform]
#    .level。三方联动终态：manifest PR == version VMO level == rootfs
#    .berryver level——后两方由下方 VER 哨兵段在机内对拍（eco: VER），
#    本门闭第一方（宿主侧文件对拍）。不等即红 = mkimage 取值链断裂或
#    镜像与 manifest 异窗。
MANIFEST=$(dirname "$IMAGE")/manifest.txt
python3 - "$MANIFEST" "tools/abi.toml" <<'PYEOF' || exit 1
import re, sys

def die(msg):
    print(f"smoke-image: FAIL: {msg}", file=sys.stderr)
    sys.exit(1)

try:
    import tomllib
except ModuleNotFoundError:
    die("manifest PR 对拍需 Python ≥3.11（tomllib——berry_assemble/mkcpio"
        " 同依赖门）")
manifest, abi = sys.argv[1], sys.argv[2]

try:
    with open(abi, "rb") as f:
        level = tomllib.load(f)["platform"]["level"]
except Exception as e:
    die(f"读 {abi} [platform].level 失败：{e}")
try:
    with open(manifest, encoding="ascii") as f:
        text = f.read()
except Exception as e:
    die(f"读 {manifest} 失败（mkimage 侧产物应与镜像同目录）：{e}")
prs = re.findall(r"(?m)^item .* platform_revision=(\d{10})$", text)
if not prs:
    die(f"{manifest} 无 platform_revision 条目行（manifest v1 产物面断裂）")
bad = sorted({int(p) for p in prs if int(p) != level})
if bad:
    die(f"manifest platform_revision={bad} != tools/abi.toml level={level}"
        "——三方联动第一方断裂（manifest PR == version VMO level =="
        " rootfs .berryver level；后两方由 eco: VER 哨兵对拍）")
print(f"smoke-image: manifest platform_revision={level} == abi.toml level"
      "（三方联动第一方 ok）")
PYEOF

# 帽：EFI 链 + 内核全标记 V-3 实测 ~115s（TCG），360s ≈ 3x 余量（帽只兜
# 结构性挂死，不赌快）。金丝雀在 ktest 毕即开始出现，标记到齐后通常即刻满足。
# B-1 起 +B1 ok n=4（bench1 spawn + ~8.6k SVC ≈ +5-10s TCG）——360s 帽不动。
# C-3 起 +PS/BC（ps + berryctl 三拉起 = 两 musl spawn + 4 RPC ≈ +10-20s
# TCG）——360s 帽不动。
DEADLINE_BOOT=360
DEADLINE_CANARY=120
DEADLINE_RX=60

# EFI 十段恰一次面（4 固定串 + 6 ERE——ERE 钉成功形态：gop 必须带分辨率、
# kfile/init 必须带字节与 ok，失败诊断行（locate fail / open fail / absent）
# 一律不匹配 = loud）。
EFI_FIXED=("BerryBoot EFI skeleton" "fw: EDK II" "fs: HELLO.TXT 28B match" "BERRYBOOT ok")
EFI_ERE=(
  'gop: [0-9]+x[0-9]+'
  'kfile: [0-9]+B ok'
  'init: [0-9]+B ok'
  'elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok'
  'load: segs=[0-9]+ pages=[0-9]+ ok'
  'mmap: n=[0-9]+ ok'
)
# 内核接续恰一次面（src=handoff 是 handoff 路专属——fw_cfg 路打 src=fw_cfg
# 串，混源即 FAIL）。
# ver= 前缀式（同 MMAP segs=/ROOTFS src= 带参形态）：协议升版（v2 = 54fc9b6
# V10 度量槽留位，bootloader/内核双侧同写）不应炸标定——内核 probe 认
# ≤ 当前版，「接续恰一次」才是断言面。钉死 ver=1 曾致 54fc9b6 后 EFI 冒烟
# 全线红（run 42864829668 实证：唯缺此标记，MMAP/ROOTFS 同伴均到齐）。
CONT_MARKERS=("HANDOFF ok ver=" "MMAP ok segs=" "ROOTFS ok src=handoff")
# 裸相 32（与 smoke.sh MARKERS 逐字同源——改那边同步这边，改漂移即两关卡
# 各自失去「同内核零漂移」对照力；S-2 起 +BENCHXH ok 资源耗尽行为行；
# G4-1 起 +DPG ok demand 闭环行——P2-3 补同步；刀1 起 +WAKESRC acct=
# per-source 唤醒记账行；sim-S2 A2 起 +FRZCYC ok r=2 freeze/thaw 循环行；
# sim-S3 起 +PSCI susp=0（EFI 路探测行带原因后缀——子串可 grep）+
# PWRDEG ok d=1（ktest 在 EFI 路照跑，两行都在））。
KERNEL_MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "WAKESRC acct=" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok" "DPG ok" "FRZCYC ok r=2" "PSCI susp=0" "PWRDEG ok d=1")
# EFI 路生态 39（对抗审查保留日志逐标记普查实证——健康 boot 各恰一次为
# 日志观察面，本脚本断言为 grep -cF 存在性 ≥1；除
# WM ok 与 LAUNCH 双（GUI-5/GUI-8 起 EFI 专属）外与 smoke.sh rootfs 相断言
# canon 同面，EFI
# 路上 libc/spawn/退出链回归不静默
# 溜过。无 virtio 面——VIRTIO/FAT/NET/DIR/CD 族不在断言面。FS 用带参形态
# `FS ok n=` 避 `ROOTFS ok src=` 子串混叠——smoke.sh P6 A-1 同款陷阱；
# `STD ok` 与 `STD err ok` 互非子串已核，smoke.sh 双断言先例。C-2c 起
# +U3 ok（text 批执行验收——与 U1/U2 同为纯 RAM/spawn 面，EFI 路可达；
# T-1 起 sort|uniq 步与四段链回收同面）；
# B-1 起 +B1 ok n=4（EL0 四项微基准格式断言收口——零 fs 盘面纯 RAM/spawn，
# 链序在 bsh 内排 fat 族前 = EFI 路无盘亦打，smoke.sh rootfs 相同步面）；
# C-3 起 +PS ok n=/BC ok（ps 查询 RPC + berryctl 三子命令——svcmgr 直打、
# 零 fs 盘面（rootfs 内 ELF spawn + 纯 RAM RPC），EFI 路可达同面；账本
# 行数 EFI 路无 virtio 族与主 smoke 不同形，断言取 `PS ok n=` 前缀式；
# GUI-5 对账：wmd 入册后 EFI 相 PS ok n= 实值 15→16——恰满旧帽（16）帽内
# 不丢；若实为 17 则 S1 抬帽（16→17）已兜——差异即 rootfs 差 1 的同源定位
# 线索，停刀对账；rootfs 相 wmd 不在册（EFI-only 门）实值 13 不变）。
# GUI-8 段② 对账：launcher 入册（ps_reg ALIVE 不 reap）后 EFI 相 PS ok n=
# 实值 16→17——不抬帽先入账 = 表 17 恰满丢 berryctl 自身行（裁决五两件
# 同刀必做，抬帽 17→18 已兜——REPLY_MAX 254≤256 帧帽内）；rootfs 相
# launcher 不在册（EFI-only 门）实值不变。
# C-4 起 +TAB ok（bsh tab_selftest 九步——FakeSrc 状态机 + 真源两钉，零
# fs 盘面纯 RAM/cpio 直读，EFI 路可达；断言走 assert_marker 特例防
# TAGTAB ok 子串假阳性）。
# C-4b 起 +INT ok（bsh int_selftest 四步——editor 0x03 作废行/ESC 两态
# 共存/续用 + 前台杀链真机面：bin/hello 纯 RAM spawn + process_terminate
# (130) + collect_fg TERMINATED 收口 + reap 130；零 fs 盘面，链序排 fat
# 族前 = EFI 路可达；裸 grep -qF 足够——"INTENT ok" 不含 "INT ok" 子串）。
# A2-1 起 +POLL ok（bsh poll_selftest 四步——纯内核 Event 对象 + 真内核
# wait_many 批切分 16+4；链位置 int 后 fat 族前 = EFI 路可达，U2/U3/TAB/
# INT 契约）。"POLL ok" 与 EFI canon 现役标记互非子串（PIPE/PS 前缀互异）。
# T-1 起 +THR ok（svcmgr thr_selftest——thrchild 内嵌镜像（echo/talker 同
# 产链 include_bytes）bootstrap 自句柄五步链，零 rootfs/零 fs 盘面 = EFI
# 路结构性可达（链位置 bc_selftest 后 fat 族前同打）；裸 grep -qF 足够
# ——"THR ok" 无前邻混叠面（THR 前缀族唯一，TAB `(^|[^G])` 特例不需要）。
# G4-2 起 +DPGP ok（svcmgr dpg_selftest——dpgp 内嵌镜像跨进程供给形：本
# 服务即 pager，0x21 阻塞收货给子 demand 页；零 rootfs/零 fs 盘面 = EFI 路
# 结构性可达（链位置 thr_selftest 后 input 前同打）；裸 grep -qF 足够
# ——"DPGP ok" 前缀族唯一（DPG ok 裸相标记为子串反向——grep -qF 方向相反
# 不混叠：查 "DPGP ok" 不命中 "DPG ok" 行，反查亦然）。
# G4-3 起 +DPGS ok n=4（svcmgr dpg_storm——共享三 VMO × 4 子并发首触缺页
# 风暴 N=4：单槽盲覆写 + 广播错醒 → 重 fault 自愈收敛，4× join 7 才打；
# 零 rootfs/零 fs 盘面 = EFI 路结构性可达〔链位置 input_selftest 后同打〕；
# 裸 grep -qF 足够——"DPGS ok n=4" 位 4 S≠P 与 "DPGP ok"/"DPG ok" 互非
# 子串（正反两向不混叠，DPGP 同款论证）。
# GUI-1 起 +FBW ok（guifbd 显示服务自验三步全过自打——boot 授能 fb MMIO
# VMO 第 5 插映 FB_VA + 几何 info VMO 第 6 插随 env extras；stdout=console
# 发端 vblk 同路；EFI-GOP 路专属，-kernel 主路 svcmgr 探测 -EBADF 一行
# 降级不 spawn）。裸 grep -qF 足够——"FBW ok" 与 boot 期 "FB ok"（gop
# 携面打点）/"FB vmo="（授能打点）互非子串（F-B-W-space vs F-B-space）。
# GUI-1 段② 起 +FBSERVE ok（guifbd serve 循环首条消息自打——console
# fan-out 管道 endB_fb 第 7 插随 env extras[1] take 迁入；稳态矩阵打印
# 持续不断 → 首条必达 = 状态谓词非竞速。裸 grep -qF 足够——"FBSERVE ok"
# 与 "FBW ok"/"FB ok" 互非子串（F-B-S-E-R-V-E））。
# A7-1 起 +MMP ok、A7-3 起 n=9（bsh mmp_selftest 九步——⓪-④ 匿名档
# libc 桥：region 表 + vmo_create/整段 commit/vmar_map(self_vmar_h()) 桥，
# 首映射 volatile 写读对拍/munmap 三态/空洞复用回首址（first-fit 钦定）/
# 范围盒拒面三连 + mprotect 双裁；⑥-⑧ A7-3 文件档 staging——RAM 造源
# m/mmpf.bin + mmap(MAP_PRIVATE) 私有拷贝证/同 fd offset 档复映射/拒位
# 四连/fd 面 EBADF·EACCES；⑨ mprotect dormant 保内容环。⑥-⑧ 走 RAM 树
# fs 面（非盘面——父目录在 RAM 树 create 恒落 RAM 相位无关），链位置
# poll 后 fat 族前 = EFI 路可达，U2/POLL 契约；无拓扑降级门——gift[3]
# 是 T-1 spawn 通用装配，所有相恒在，失败即 park
# loud。裸 grep -qF 足够——"MMP ok"（大写）与 boot 期 'mmap: n=' ERE（小写）
# 及现役标记互非子串）。
# A7-2 起 +CLK ok n=6（bsh clk_selftest 六步——timer 壁钟分档：⓪mono
# 双读单调非减 ①REALTIME 合成一致（rt ≥ mono 差 ≤1s）②time()/
# gettimeofday() 互恰 ③nanosleep(50ms) 活性 ④settime 双档 EPERM 拒
# ⑤裸 `date -u` 漂移容忍钉板（前缀 `Thu Jan  1 00:` + MM:SS 数字域 +
# 尾 ` UTC 1970`——REALTIME 端到端活钟面）。实现面 = berry-libc
# clock_gettime 分诊（mono 族直读 / REALTIME 族 = EPOCH_BASE(0)+mono
# 合成 / 冷门 id EINVAL 诚实拒）+ 补桩五真身（time/gettimeofday/
# settimeofday/nanosleep/usleep）。零 fs 盘面（纯 clock syscall + date
# spawn 无文件参）= EFI 路可达，链位置 mmp 后 fat 族前（A 系契约位）；
# 无拓扑降级门，失败即 park loud。裸 grep -qF 足够——"CLK ok"（大写）
# 与现役标记互非子串（无 XCLK 形态在册）。
# A7-4 起 +SIG ok n=6（bsh sig_selftest 六步——信号面真身化验收：⓪raise
# 即时投递 ①表/掩码回读 ②blocked 窗 held→unblock 补投 ③kill 分诊
# self/ESRCH/EINVAL ④spawn bin/sigchild → berry_signal_send(hp,SIGTERM)
# → DFL io::exit(143) EOF 先行 → reap 143 ⑤修复轮回归钉板（位号序敏感
# 三形分诊：Call 位 1 < Hold 位 12 < 表变位 18——嵌套补投恰一次 +
# delivery-time 表重查 + 包络全量恢复 + 无残留双投）。实现面 = berry-libc
# signal.rs ③a+③b 真身 + 内核 0x86/0x87）。零 fs 盘面（纯用户态信号面 +
# sigchild 零 gift spawn）= EFI 路可达，链位置 clk 后 fat 族前（A 系契约
# 位）；无拓扑降级门，失败即 park loud。裸 grep -qF 足够——"SIG ok"
# （大写）与现役标记互非子串（无 XSIG 形态在册）。
# GUI-7：+ APPUI ok n=——appd [ui] 资产面谓词（EFI 路同打：appd 与
# bzImage 路同一内嵌二进制；语义详 smoke.sh ROOTFS_MARKERS 注）。
# GUI-2 段③ 起 +GFX ok / GFXF ok n=（消费侧首刀双标记——GFX ok = gfxdemo
# 客户端全链断言过自打：GETINFO 协商（ver/XRGB8888/credits=2）+ berry-gfx
# soft 渲帧 → VMO → SUBMIT（K=2 damage 随行 VMO READ 别名）→
# FRAME_CALLBACK PRESENTED → RELEASE count==1 归还即关 → VOTE/POWER_DIM
# 1B → hdi 软 vsync mini 序（REG_VBLANK/VBLANK_WAIT/PREPARE 再 dup/COMMIT）
# ；GFXF ok n= 由 guifbd 首呈现时打（服务端真值：px() 抽样 7 点全过 ∧
# console_full_n≥1，前缀式同 PS ok n=）——两分立 = 故障定位面（GFXF 缺 =
# 服务端炸；GFXF 在而 GFX 缺 = 客户端 RPC 链炸）。svcmgr EFI 相预拉直递
# extras 6/8（rootfs 相随 fb 门静默跳过零新行；bsh 手跑 present_h()==0
# 恒「gfx: present absent」降级行——永不进 MARKERS）。裸 grep -qF 足够
# ——"GFX ok" 与 "GFXF ok n=" 互非子串（G-F-X-空 vs G-F-X-F）且与现役
# 标记互非子串。
# GUI-5 起 +WM ok（窗口守护联调标记——gfxdemo 改连 wm 域后往返四组断言全过
# 单 println 自打：WM_GETINFO（VER_WM=1 + windows_max=15 + 几何回显）/
# CREATE ack id≠0 / SET 1B OK / TABLE_GET 回显（v1 单窗有效几何==请求几何）。
# 恰一次 = 结构保证（svcmgr 单次预拉 + 阻塞 reap + 单 println——GFX ok
# 同款），阶段 2 不另覆盖（仅 EFI 固定串/ERE/接续恰一次面——GFX/GFXF 同
# 待遇）。仅 EFI 相：svcmgr wmd arm 门 = present_cli≠0 ∧ cpio 在册
# bin/gui-wmd，rootfs 相 fb 缺席门静默跳过——负向对账（rootfs 日志 WM ok
# 恰 0 次）归主会话收割手动步进，不进 smoke.sh 码。中间态永不进 MARKERS：
# wmd spawn fail/cpio 缺 → svcmgr 显式回收 handle 降级续链，gfxdemo 降级
# 门打「gfx: wm absent」诊断行（netd 双门先例——present/hdi 端随换手归
# wmd，bsh 手跑恒 wm absent 形）；「gui-wmd: up」启动诊断行前缀统一小写
# 防误配。裸 grep -qF 足够——"WM ok" 与现役标记互非子串（最近形
# BENCHWAKE n=/MANY ok/WAIT ok 均不含）。GFX/GFXF 语义零漂移：present/hdi
# 调用点仅换端经 wmd 中继（op 编码与断言零改动——guifbd 字节不动）。时
# 序：wmd 一 spawn ≈+5-10s TCG，实测链 ~115s 余量足——DEADLINE_BOOT=360s
# 帽不动。LAUNCH 双（GUI-8 段③）：launcher 常驻进程自测——首帧呈现回执
# 过链 LAUNCH ok + 合成 swipe 两轮翻页各落一帧 PAGE ok n=2（frameloop.rs
# Selftest 状态机——**每进程**恰一次；svcmgr 一次重启帽重拉后两标记可在
# 同一 boot 日志合法重打，canon 断言为 grep -qF 存在性，勿升格恰一次）。
# 监督死亡面另由 eco 行盯——'launcher died'（eco_line 前缀 `eco: `，行首
# 无 \n 可与上帧 art 行尾残字拼成 `veco: …`——grep 用无前缀子串）不进
# canon。裸 grep -qF 足够——"LAUNCH ok"/"PAGE ok n="
# 与现役 34 枚互非子串（LAUNCH/PAGE 词头独有）。
ECO_MARKERS=("rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" "HEXYL ok" "BSH ok" "EXIT ok" "APP ok n=" "APPUI ok n=" "PIPE ok n=" "FS ok n=" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "U4 ok" "TAB ok" "INT ok" "POLL ok" "PS ok n=" "BC ok" "THR ok" "DPGP ok" "DPGS ok n=4" "DPGN ok n=4" "FROZEN ok n=1" "WAKE ok n=1" "TOMB ok n=1" "MMP ok n=9" "CLK ok n=6" "SIG ok n=6" "FBW ok" "FBSERVE ok" "GFX ok" "GFXF ok n=" "WM ok" "LAUNCH ok" "PAGE ok n=")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-image.XXXXXX")
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/berryos-smoke-image.XXXXXX")
QEMU_PID=""
RX_FIFO=""
RX_HOLDER=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  if [ -n "$RX_HOLDER" ] && kill -0 "$RX_HOLDER" 2>/dev/null; then
    kill "$RX_HOLDER" 2>/dev/null
    wait "$RX_HOLDER" 2>/dev/null
  fi
  [ -n "$RX_FIFO" ] && rm -f "$RX_FIFO"
  rm -rf "$RUN_DIR"
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke-image: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-image: FAIL: $1" >&2
  echo "---- 串口日志尾部（${LOG}）----" >&2
  tail -40 "$LOG" >&2
  echo "--------------------------------" >&2
  exit 1
}

# 标记断言（smoke.sh 同款）：DEMO ok 是 AIDEMO ok 的子串，特判负向；其余
# 两两互非子串裸 grep 足够。
assert_marker() {
  if [ "$1" = "DEMO ok" ]; then
    grep -qE '(^|[^I])DEMO ok' "$LOG"
  elif [ "$1" = "TAB ok" ]; then
    # C-4：boot 期 TAGTAB ok 恰含 "TAB ok" 子串——qF 假阳性。只排前邻 G
    # （TAGTAB 的 G）——真标记可嵌稳态矩阵巨行中段（前驱任意大写字母，
    # 实测 BTAB ok），排整个 A-Z 会拒真标记（DEMO ok 只排 I 同型）。
    grep -qE '(^|[^G])TAB ok' "$LOG"
  else
    grep -qF "$1" "$LOG"
  fi
}

# 恰一次断言：EFI 引导链无重入（BerryBoot 不跑两遍 = EBS 后固件不再拿回
# 控制权的结构性证据）；grep -c 空结果时 0（|| true 吞 grep 无匹配退出码）。
assert_once() {
  local c
  c=$(grep -cF -- "$1" "$LOG" || true)
  [ "$c" -eq 1 ] || fail "「$1」非恰一次（${c:-0} 次）——EFI 链重入或打印面漂移"
}
assert_once_re() {
  local c
  c=$(grep -cE -- "$1" "$LOG" || true)
  [ "$c" -eq 1 ] || fail "ERE「$1」非恰一次（${c:-0} 次）——EFI 链重入或打印面漂移"
}

# 金丝雀序列检查（smoke.sh 同款 awk：≥3 条且严格递增）。
canary_ok() {
  grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
    { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
    END { exit (bad || n<3) ? 1 : 0 }
  '
}

# vars.fd 自产 blank（64MiB 全 0xFF）进本跑临时目录——**不拷贝共享模板**
# （2026-10-29 实抓：手动 boot `-pflash` 直挂共享 vars 会往里持久化
# BootOrder/boot entry，拷贝继承陈旧项 → USB 形态下 BDS 试遍旧 entry 尾落
# EFI Internal Shell（标记全缺）；blank = 固件每跑重新枚举在位设备自举，
# 真零状态。第 3 参 AAVMF_VARS 仅为接口稳定保留（CI 零改动），不消费）。
python3 -c "import sys; sys.stdout.buffer.write(b'\xff'*(64<<20))" > "$RUN_DIR/vars.fd" || fail "vars.fd 生成失败"

# FIFO stdin（smoke.sh RX 相同款）：holder 持写端防 EOF；帽 1200s 盖全流程
# worst（360+120+60 + RX 注入段 + 余量 ~2x）。
RX_FIFO=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-image-fifo.XXXXXX")
rm -f "$RX_FIFO"
mkfifo "$RX_FIFO" || fail "mkfifo 失败"
( sleep 1200 ) > "$RX_FIFO" &
RX_HOLDER=$!

echo "smoke-image: QEMU 启动（AAVMF + ${IMAGE}——零 -kernel / 零 fw_cfg）"
# 与 justfile run-efi 同参（不挂 virtio——EFI 路无 virtio 面；ramfb 供 GOP）。
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic \
  -pflash "$AAVMF_CODE" -pflash "$RUN_DIR/vars.fd" \
  -drive if=none,file="$IMAGE",format=raw,id=bb \
  -device qemu-xhci -device usb-storage,drive=bb \
  -device ramfb \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：全部标记到齐（EFI 十段 + 接续 3 + 裸相 32 + 生态 39 = 84）——
# EFI 段/接续用 ERE 固定串混合面，统一进一个等待循环（到齐是状态谓词，
# 阶段间无独立验收语义；缺失清单在超时 fail 文案里分组可见）。
# —— 分相断言集（CI 分层用）：SMOKE_PHASES=boot 时只验引导面——
# EFI 十段 + 接续 3 + 裸相 32 = 45 标记（引导链完整验收面，慢 runner
# TCG 可达——run 42624077761 实证到齐）；生态 39 服务链 wall-clock 慢机
# 跑不完（同 run 实证 360s 帽缺 HE XYL/BSH/APP 族），只进全链相。
# 未设或非 boot = 原 84 全面行为不变。
if [ "${SMOKE_PHASES:-all}" = "boot" ]; then
  P1_MARKERS=("${EFI_FIXED[@]}" "${CONT_MARKERS[@]}" "${KERNEL_MARKERS[@]}")
  ALL_COUNT=$(( ${#EFI_FIXED[@]} + ${#EFI_ERE[@]} + ${#CONT_MARKERS[@]} + ${#KERNEL_MARKERS[@]} ))
else
  P1_MARKERS=("${EFI_FIXED[@]}" "${CONT_MARKERS[@]}" "${KERNEL_MARKERS[@]}" "${ECO_MARKERS[@]}")
  ALL_COUNT=$(( ${#EFI_FIXED[@]} + ${#EFI_ERE[@]} + ${#CONT_MARKERS[@]} + ${#KERNEL_MARKERS[@]} + ${#ECO_MARKERS[@]} ))
fi
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（固件/引导崩溃？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${P1_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  for e in "${EFI_ERE[@]}"; do
    grep -qE "$e" "$LOG" || missing="${missing:+$missing }ERE:$e"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "标记 ${DEADLINE_BOOT}s 未到齐（实测全链 ~115s），缺：$missing"
  sleep 1
done
# G-ver-2（版本管理标准 §3）镜像路同面：boot banner 形态断言——首匹配行
# 须为 `BerryOS boot <semver> (<ident>[ rustc<ver> dbg|rel])`（EFI 路与
# -kernel 主路同一内核 banner；正则与 smoke.sh 主路**逐字同款**——原前锚
# 弱形升全形，ident 段形态同步受锚，三脚本同正则复制非各写各的）。
# 上方轮询的 KERNEL_MARKERS "BerryOS boot" 子串断言原样保留——本行只加
# 形态边不松子串边；置轮询到齐后 = banner 行必已在席，断言零竞态；先行
# 于 SMOKE_PHASES=boot 早退 = 分层 CI 轻面同受 G-ver-2 护。
grep -m1 'BerryOS boot' "$LOG" | grep -Eq 'BerryOS boot [0-9]+\.[0-9]+\.[0-9]+ \([A-Za-z0-9.-]+( rustc[A-Za-z0-9.?]+ (dbg|rel))?\)' \
  || fail "boot banner 形态缺：首匹配行应为 'BerryOS boot <semver> (<ident>[ rustc<ver> dbg|rel])'（版本管理标准 §3 G-ver-2）；实得：$(grep -m1 'BerryOS boot' "$LOG" || echo '（日志无 BerryOS boot 行）')"
# 标准 §6 执法化（审计洞②：原纯人律无机器门）：ident 非 nogit 拒——nogit
# 标识产物不得对外发布（无归因能力的构建仅限本地调试），冒烟门是发布链
# 前闸，nogit 构建过门即违规。门 = 首 banner 行含 nogit 即红（行内唯一可
# 含该串的段 = ident——semver/rustc/profile 段均数字与固定词）；与
# smoke.sh 主路同形同正则。
grep -m1 'BerryOS boot' "$LOG" | grep -q 'nogit' \
  && fail "boot banner ident 为 nogit：nogit 标识产物不得对外发布/进发布链（版本管理标准 §6——无归因能力的构建仅限本地调试）；实得：$(grep -m1 'BerryOS boot' "$LOG")"
if [ "${SMOKE_PHASES:-all}" = "boot" ]; then
  echo "smoke-image: 引导面标记 ${ALL_COUNT}/${ALL_COUNT} 到齐（EFI 十段 + 接续 + 裸相 32 + banner 形态 + ident 非 nogit；SMOKE_PHASES=boot 分层 CI 轻面，生态 39 + VER 哨兵 + RX + sim 面留快机全链）"
  cleanup 2>/dev/null || true
  exit 0
fi
echo "smoke-image: 全标记 ${ALL_COUNT}/${ALL_COUNT} 到齐（EFI 十段 + 接续 + 裸相 32 + 生态 39)"

# —— VER 哨兵执法（版本管理标准 §1 构建标识轨道；审计洞①「哨兵哑火」闭环）——
# EFI 相是 `eco: VER` 行唯一实现场：svcmgr ver_check 门 = H_FB 在席（fb
# GOP 相 ∧ rootfs 在机）——-kernel 主路无 GOP 恒静默跳过，故 VER 执法只
# 挂本脚本。kernel sysver VMO × rootfs .berryver 烙印三元组全等打
# `eco: VER ok`、任一不等打 `eco: VER mix`（混合构建——kernel 与 rootfs
# 异窗的 loud 行）。原状两行均无断言：印了 mix 照 PASS = 哨兵哑火。本块
# 双门：ok 在席（缺 = 哨兵未跑或非 GOP 相异常——结构链断会 trap 杀
# svcmgr，eco 标记门已先行兜底）+ mix 缺席（命中即红——混合构建不得过
# 冒烟门）。grep 取无前缀子串（eco 行行首无 \n 可与上帧残字拼 `veco: …`
# ——launcher died 同款防御）；置全标记到齐后 = ver_check（boot 装载块）
# 必已执行，断言零竞态；不进 SMOKE_PHASES=boot 早退面（生态链未跑，VER
# 行结构性缺席）。
grep -qF 'eco: VER ok' "$LOG" \
  || fail "eco: VER ok 缺席——混合构建哨兵哑火或非 GOP 相异常（版本管理标准 §1 构建标识轨道：kernel × rootfs 三元组对拍行应在席）"
grep -qF 'eco: VER mix' "$LOG" \
  && fail "eco: VER mix 在席——混合构建（kernel 与 rootfs 异窗）哨兵命中不得过门：$(grep -m1 'eco: VER mix' "$LOG")"

# —— 阶段 2：恰一次（EFI 十段 + 接续——引导链重入探测）——
for m in "${EFI_FIXED[@]}" "${CONT_MARKERS[@]}"; do
  assert_once "$m"
done
for e in "${EFI_ERE[@]}"; do
  assert_once_re "$e"
done
echo "smoke-image: EFI 十段 + 接续恰一次到齐"

# —— 阶段 3：字节数对拍（盘内容 ↔ 引导链读数 ↔ 内核消费）——
# 基线已在脚本入口快照（见上：与镜像打包同窗，免疫 QEMU 运行窗内共享
# target dir 被兄弟会话构建覆盖的换脸读——雷册 §B29 时序踩踏形态）。
grep -qE "kfile: ${KERNEL_BYTES}B ok" "$LOG" \
  || fail "kfile 字节数与宿主内核不一致（期望 ${KERNEL_BYTES}B）——镜像打包或引导读链断裂"
grep -qE "init: ${INITRD_BYTES}B ok" "$LOG" \
  || fail "init 字节数与宿主 rootfs.cpio 不一致（期望 ${INITRD_BYTES}B）——镜像打包或引导读链断裂"
echo "smoke-image: 字节对拍 ok（kfile=${KERNEL_BYTES}B init=${INITRD_BYTES}B 与宿主精确同数）"

# —— 阶段 4：switches 金丝雀（≥3 条且严格递增）——
t1=$(date +%s)
while :; do
  n=$(grep -c 'switches=' "$LOG" || true)
  [ "${n:-0}" -ge 3 ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出"
  [ $(( $(date +%s) - t1 )) -gt "$DEADLINE_CANARY" ] \
    && fail "金丝雀 ${DEADLINE_CANARY}s 内不足 3 条（现有 ${n:-0}）"
  sleep 2
done
canary_ok || fail "switches 序列非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"
# 快照注入前末值——阶段 6 复验以「出现严格更大的新线」为谓词（对抗审查 F2：
# 全量重验会被注入前旧线满足，冻而不死零判别力）。
LAST_CANARY=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)

# —— 阶段 5：RX 注入相（bsh 可交互的自动代理）——
# 全标记到齐 = bsh 就位（BSH ok 即交互读循环在跑）；注入 `rx` 逐字节 0.5s
# （FEN=0 深 1 字节，打字速率确定性——smoke.sh RX 相同款）。
exec 3> "$RX_FIFO"
printf 'r' >&3; sleep 0.5
printf 'x' >&3; sleep 0.5
printf '\n' >&3
t2=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 RX ok 阶段退出（注入链断？）"
  grep -qF "RX ok" "$LOG" && break
  [ $(( $(date +%s) - t2 )) -gt "$DEADLINE_RX" ] \
    && fail "RX ok ${DEADLINE_RX}s 未出现（EFI 路 RX 中断链断——查 irq? 行）"
  sleep 1
done
echo "smoke-image: RX ok 到齐（注入 rx → 应答）"
# （C-4b 裁定：EFI 路不设 Ctrl-C 交互杀注入段（i130t）——bsh 按 C-2b 链序
#  契约 park 在 fat_selftest（无盘 get_file Err → park `-> !` 挡断其后），
#  交互编辑循环结构性不可达（RX ok 是内核 console 观察窗直打、不经 bsh，
#  故能到而注入命令永不执行——verify 首跑实抓，串口 13670B 纯稳态矩阵零
#  回显实证）。交互杀五环面由主路 smoke.sh RX 相段 g 独占（有盘路 bsh
#  全功能）；EFI 路杀链核心面由 boot 期 INT ok 覆盖（真机 spawn +
#  terminate(130) + collect_fg 收口 + reap 130 四步）。）

# —— 阶段 6：注入后活性复验（等严格大于 LAST_CANARY 的新线）——
# switches 单调累计，全量重验无判别力（对抗审查 F2）——RX 中断破坏 IRQ 契约
# 的死状 = RX ok 后冻结，只有「新线仍在出现」才是活性谓词。
t3=$(date +%s)
while :; do
  cur=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
  [ "${cur:-0}" -gt "$LAST_CANARY" ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在注入后复验阶段退出"
  [ $(( $(date +%s) - t3 )) -gt "$DEADLINE_CANARY" ] \
    && fail "注入后 ${DEADLINE_CANARY}s 无新金丝雀线（> ${LAST_CANARY}）——RX 注入后冻结（冻而不死面）"
  sleep 2
done

# —— 阶段 7：sim 面零计数（sim-S1 M3 负例镜——主 smoke RX 相正例镜的对偶）——
# EFI 路 launch 不挂 serial1/simdev 同线配对（GUI8 同款冻结纪律）→ fw_cfg
# 门键缺席 = 内核门闭（无 `SIM gate on`）+ svcmgr dup 探针门闭静默跳过
# （无 SIMBAT/无 spawn 噪声）。SIM 行 >0 = launch 误挂配对或门判定漏——
# loud 拒，不静默（B4 零扰动面在此成执法不变量）。
n_gate=$(grep -acF 'SIM gate on' "$LOG" || true)
[ "$n_gate" -eq 0 ] \
  || fail "EFI 路 SIM gate on ${n_gate} 行（应 0——launch 误挂 simdev/serial1 配对？）"
n_bat=$(grep -acF 'SIMBAT' "$LOG" || true)
[ "$n_bat" -eq 0 ] \
  || fail "EFI 路 SIMBAT ${n_bat} 行（应 0——门闭相注入面泄漏/静默跳过漂移？）"

echo "smoke-image: PASS（EFI 十段恰一次 + 接续 src=handoff + 裸相 32 + banner 形态 + ident 非 nogit + 生态 39 + VER 哨兵 ok 在席/mix 零席 + 字节对拍 + 金丝雀严格递增 + RX 注入后活性复验 + sim 面零计数）"
