#!/usr/bin/env python3
"""sim-S1 M3 注入 client（docs/sim设备通道与契约设计-v1.1.md §7/§8 四流）。

连 QEMU serial1 unix sock → 逐帧注入电池帧 → 读回 9B ACK 字节对拍。
sock 路径 = argv[1]（smoke.sh RX 相传临时目录 SIM_SOCK）；缺省回落
/tmp/berryos-sim1.sock（run-rootfs 固定路径——手动 `just run-rootfs` +
本 client 复用）。黄金帧与 berry-pwproto golden_bytes_battery 同源（全零
快照）：PWSM + dev=1 + ver=1 + seq LE + 50B；capacity_pct wire 偏移 12。

四流冻结向量（§8 QEMU 面，smoke.sh RX 相段 i 消费）：
  seq1 OK      → ACK st=0（ST_OK）
  seq2 OK      → ACK st=0（递增）
  seq3 cap=101 → ACK st=1（ST_BAD 值域拒）——串口无 SIMBAT 行（段计数门）
  重放 seq1    → ACK st=2（ST_STALE 非单调）——同上静默

手跑：`just run-rootfs` 起场后 `python3 tools/siminj.py`；退出码非 0 =
对拍红（smoke 段 loud 失败——负例反转红即依赖本对拍有牙）。
"""
import socket
import sys
import time

SOCK = sys.argv[1] if len(sys.argv) > 1 else "/tmp/berryos-sim1.sock"
ST_OK, ST_BAD, ST_STALE = 0, 1, 2
ACK_LEN = 9


def frame(seq, cap=None):
    b = bytearray(50)
    b[0:4] = b"PWSM"
    b[4] = 1  # dev = battery
    b[5] = 1  # ver
    b[6:8] = seq.to_bytes(2, "little")
    if cap is not None:
        b[12] = cap  # capacity_pct：present/ac/status/health 四布尔后
    return bytes(b)


def ack_of(seq, st):
    return bytes([0x50, 0x57, 0x53, 0x4D, 1, 1]) + seq.to_bytes(2, "little") + bytes([st])


def connect(retries=10):
    # server=on,wait=off：QEMU 起即 bind；smoke 段在标记到齐后才跑本 client，
    # 正常零重试——重试环只吃手跑时 QEMU 未起/慢起态。
    for _ in range(retries):
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(15)
        try:
            s.connect(SOCK)
            return s
        except (FileNotFoundError, ConnectionRefusedError):
            s.close()
            time.sleep(1)
    print(f"siminj: {SOCK} {retries}s 不可连（QEMU serial1 未起？）", flush=True)
    sys.exit(1)


def main():
    s = connect()
    print("connected", flush=True)
    cases = [
        ("seq1-ok", frame(1), ack_of(1, ST_OK)),
        ("seq2-ok", frame(2), ack_of(2, ST_OK)),
        ("seq3-bad-cap101", frame(3, cap=101), ack_of(3, ST_BAD)),
        ("replay1-stale", frame(1), ack_of(1, ST_STALE)),
    ]
    for name, f, want in cases:
        s.sendall(f)
        got = b""
        while len(got) < ACK_LEN:
            chunk = s.recv(ACK_LEN - len(got))
            if not chunk:
                print(f"{name}: EOF（got {got.hex()}——simbattery 死/ACK 断流）", flush=True)
                sys.exit(1)
            got += chunk
        ok = got == want
        print(f"{name}: ack={got.hex()} want={want.hex()} {'PASS' if ok else 'FAIL'}", flush=True)
        if not ok:
            sys.exit(1)
        time.sleep(0.5)
    print("ALL-4-PASS", flush=True)


main()
