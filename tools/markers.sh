# 主 smoke（smoke.sh）与 SMP 冒烟（smoke-smp.sh）共用标记单源。
# 由来：smoke-smp.sh 原将双数组**复制**自 smoke.sh 并以 awk 计数对拍防漂
# 移（§5.1 最低限度方案）；S-5 收官后测试基建收尾刀抽出本文件单源化，
# 对拍守卫随退役（两脚本 source 本文件，漂移面构造性消失）。
# 元素约束沿袭：元素 = 引号串，**元素内不得含 `)`/引号**（历史 awk 计数
# 器的解析约束；今虽无计数器，加标记仍守此约束防回归）。
# 消费方：smoke.sh 裸相 28 + rootfs 相 50；smoke-smp.sh 各相同双数组；
# smoke-image.sh KERNEL_MARKERS 硬拷贝（G4-1 三镜 P2-3 补列——改 MARKERS
# 须连 smoke-image.sh 同步，第三消费方漏同步即静默失覆盖）。

# 裸相 28 标记（boot 全链；G4-1 起 +DPG ok——demand 闭环自证）。
MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok" "DPG ok")

# rootfs 相 50 标记（G4-3 B 刀 +DPGN ok n=4——0x21 四早退负序（门/对齐/
# commit/身份验各断精确 errno）；C-lite +DPGS ok n=4——缺页风暴 N=4；G4-2
# +DPGP ok 跨进程供给面；THR 尾插随合并序）。
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "LSN ok n=7" "TCPS ok echo=" "APP ok n=" "APPUI ok n=" "DIR ok n=" "CD ok" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "DF ok" "U4 ok" "TAB ok" "INT ok" "POLL ok" "EVS ok n=2" "SKT ok n=3" "EPL ok n=8" "NETS ok n=2" "MMP ok n=9" "CLK ok n=6" "SIG ok n=6" "PS ok n=" "BC ok" "M3DEMO ok n=5" "THR ok" "DPGP ok" "INPUT ok" "DPGS ok n=4" "DPGN ok n=4")

# 防假绿兜底：单源被误清空 = 轮询零等待恒过——非空断言 loud 拒（P6 A-5
# 「基准重产义务要有机器守卫」族精神沿袭）。
[ "${#MARKERS[@]}" -gt 0 ] && [ "${#ROOTFS_MARKERS[@]}" -gt 0 ] || {
  echo "markers.sh: 标记数组为空（单源被误清？）——拒跑防假绿" >&2
  exit 1
}
