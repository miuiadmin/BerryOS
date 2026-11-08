# 主 smoke（smoke.sh）与 SMP 冒烟（smoke-smp.sh）共用标记单源。
# 由来：smoke-smp.sh 原将双数组**复制**自 smoke.sh 并以 awk 计数对拍防漂
# 移（§5.1 最低限度方案）；S-5 收官后测试基建收尾刀抽出本文件单源化，
# 对拍守卫随退役（两脚本 source 本文件，漂移面构造性消失）。
# 元素约束沿袭：元素 = 引号串，**元素内不得含 `)`/引号**（历史 awk 计数
# 器的解析约束；今虽无计数器，加标记仍守此约束防回归）。
# 消费方：smoke.sh 裸相 32 + rootfs 相 54；smoke-smp.sh 各相同双数组；
# smoke-image.sh KERNEL_MARKERS 硬拷贝（G4-1 三镜 P2-3 补列——改 MARKERS
# 须连 smoke-image.sh 同步，第三消费方漏同步即静默失覆盖）。

# 裸相 32 标记（boot 全链；G4-1 起 +DPG ok——demand 闭环自证；刀1 起
# +WAKESRC——per-source 唤醒记账 G1.5 单行，尾随 G1.5 ok；sim-S2 A2 起
# +FRZCYC——ktest freeze/thaw 两轮循环（SUS-NEG 之后、BL-OVF 之前）；
# sim-S3 起 +PSCI susp=0——boot 期 PSCI 深睡探测（EFI/EL2 门与 TCG
# FEATURES 拒三路同打）+PWRDEG——ktest 深睡降级证据链（ktest done 前））。
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "WAKESRC acct=" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok" "DPG ok" "FRZCYC ok r=2" "PSCI susp=0" "PWRDEG ok d=1")

# rootfs 相 54 标记（G4-3 B 刀 +DPGN ok n=4——0x21 四早退负序（门/对齐/
# commit/身份验各断精确 errno）；C-lite +DPGS ok n=4——缺页风暴 N=4；G4-2
# +DPGP ok 跨进程供给面；THR 尾插随合并序；sim-S2 A2 +FRZ 两轮冻结循环
#——EL0 载体：thrchild 冻结轮数以退出码带出，svcmgr 收码打标（防子进程
# 标记字面量被冻结路径吞掉）；EL1 ktest 载体 = 裸相 FRZCYC，双载体执法）。
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "LSN ok n=7" "TCPS ok echo=" "APP ok n=" "APPUI ok n=" "DIR ok n=" "CD ok" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "DF ok" "U4 ok" "TAB ok" "INT ok" "POLL ok" "EVS ok n=2" "SKT ok n=3" "EPL ok n=8" "NETS ok n=2" "MMP ok n=9" "CLK ok n=6" "SIG ok n=6" "PS ok n=" "BC ok" "M3DEMO ok n=5" "THR ok" "FRZ ok n=2" "DPGP ok" "INPUT ok" "DPGS ok n=4" "DPGN ok n=4" "FROZEN ok n=1" "WAKE ok n=1" "TOMB ok n=1")
# app-1 段 a-1 +FROZEN ok n=1（51→52）：freeze-first 自验——tomb_selftest 通道
# 判据（UP → suspend → 8s 零收 → thaw → RESUMED 全过才打）。与 T-1 的
# 「FRZ ok n=2」互非子串（F-R-O-Z-E-N 插元音），canon 机查已复核（对抗审查 L2）。
# app-1 段 b-1 +TOMB ok n=1（52→53，草案 v1.2 §6）：墓碑协议 v0 自验——
# PAUSESV1 → SAVED → reap0 → 记表 → relaunch → restored c=N+1 → reap1 →
# 表空全过才打（与 FROZEN ok 独立判）。canon：对全表 grep -F 互查过（含
# APP 族/FRZCYC）；备胎 TOMBST ok n=1 同过；APPTOMB ok n=1 含本串不可用。
# AM-1.5 +WAKE ok n=1（53→54，wake-pending 消费闭环）：tomb_selftest 冻结
# 窗 send PROBE → token 持有者 wait_one(SIGNAL_WAKE_PENDING) 收通知 →
# close(token) 解冻（Q1「唤醒即解冻」默认策略）→ PROBED 回环全过才打
#（与 FROZEN/TOMB 独立判互不连坐——发射序居两者之间）。canon：对全表
# grep -F 互查过（WAKESRC acct=/BENCHWAKE n= 均不含本串——'WAKE ok'
# 词中空格独有；本串亦非任何现役标记子串）。

# sim-S1/sim-S2 RX 专属标记（smoke.sh RX 相 serial1 注入后才出现；rootfs 相
# 无注入 client，不并入 ROOTFS_MARKERS——否则 rootfs 相标记门恒假红）。
# 消费点 = smoke.sh RX 相 sim 段（对拍 sweep，防死单源）；PWRKEY = sim-S2
# A4② PM 微相（QMP system_powerdown 注入 PL061 无主 IRQ）。
RX_MARKERS=("SIMBAT ok seq=" "PWRKEY unowned n=")

# 防假绿兜底：单源被误清空 = 轮询零等待恒过——非空断言 loud 拒（P6 A-5
# 「基准重产义务要有机器守卫」族精神沿袭）。
[ "${#MARKERS[@]}" -gt 0 ] && [ "${#ROOTFS_MARKERS[@]}" -gt 0 ] && [ "${#RX_MARKERS[@]}" -gt 0 ] || {
  echo "markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿" >&2
  exit 1
}
