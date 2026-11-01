#!/bin/sh
# berrydump AAVMF 首验（V-5a-1）——自含脚本，不进 justfile（三不纪律同
# build.sh：不进 berryboot members / 不动 justfile / 不进 image 产链）。
# 用法：sh bootloader/berrydump/aavmf-firstboot.sh [盘img] [秒帽]
#
# 形态与 justfile run-efi / tools/smoke-image.sh 同参：qemu-xhci +
# usb-storage 可移介质路径 + ramfb 供 GOP + fresh vars.fd（无 BootOrder
# 残留，确定性）；串口 -nographic 收割 berrydump 行协议
# （fw:/gop:/cfg:/xsdt:/madt:/spcr:/dbg2:/fdt:/pci:/hcs:/BerryDump done）。
# berrydump dump 完不 EBS 返 EFI_SUCCESS → 固件续行——收割窗 = done 落
# 串口即 kill（macOS 无 GNU timeout：smoke.sh 后台 + kill 模式）。
#
# 多会话纪律：跑前先与在途 workflow 会话排程 QEMU 窗（TCG 互斥）。
set -e
cd "$(dirname "$0")/../.."   # 仓库根

IMG=${1:-out/berrydump-disk.img}
CAP=${2:-90}
AAVMF_CODE=${AAVMF_CODE:-/opt/homebrew/share/qemu/edk2-aarch64-code.fd}
AAVMF_VARS=${AAVMF_VARS:-/opt/homebrew/share/qemu/edk2-arm-vars.fd}
LOG=out/berrydump-firstboot.log

if [ ! -f "$IMG" ]; then
    echo "berrydump: FAIL: 盘 $IMG 缺席（先 sh bootloader/berrydump/build.sh，"
    echo "  再: python3 tools/mkfat.py out/berrydump-disk.img --put \\"
    echo "        bootloader/berrydump/out/BERRYDUMP.EFI:/EFI/BOOT/BOOTAA64.EFI --size-mib 8）"
    exit 1
fi

# fresh vars（确定性：不带上一轮 BootOrder 状态——run-efi 同款）。
cp "$AAVMF_VARS" bootloader/berrydump/out/vars.fd

qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic \
    -pflash "$AAVMF_CODE" -pflash bootloader/berrydump/out/vars.fd \
    -drive if=none,file="$IMG",format=raw,id=bb \
    -device qemu-xhci -device usb-storage,drive=bb \
    -device ramfb \
    > "$LOG" 2>&1 &
QPID=$!

i=0
while [ $i -lt $((CAP * 2)) ]; do
    if grep -q "BerryDump done" "$LOG" 2>/dev/null; then
        sleep 1   # 尾行落盘余量
        kill "$QPID" 2>/dev/null || true
        wait "$QPID" 2>/dev/null || true
        echo "berrydump: firstboot ok → $LOG"
        grep -E "^(fw|gop|cfg|xsdt|madt|spcr|dbg2|fdt|pci|hcs|BerryDump)" "$LOG" | head -40
        exit 0
    fi
    if ! kill -0 "$QPID" 2>/dev/null; then
        echo "berrydump: FAIL: QEMU 提前退出（未见 BerryDump done）——日志尾："
        tail -20 "$LOG"
        exit 1
    fi
    sleep 0.5
    i=$((i + 1))
done
kill "$QPID" 2>/dev/null || true
wait "$QPID" 2>/dev/null || true
echo "berrydump: FAIL: ${CAP}s 帽超时未见 BerryDump done——日志尾："
tail -20 "$LOG"
exit 1
