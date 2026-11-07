#!/usr/bin/env python3
"""sim battery-frame injector and ACK comparator (sim-S1 M3 / sim-S2 A8).

Wire contract is the frozen v1 layout in ``user/berry-pwproto``. The client
never treats an injection echo as success: every expected ACK is read back from
the same UART1 Unix socket and compared byte-for-byte.

Profiles:
  s1 (default): four frozen vectors from sim-S1 M3 -- OK x2, BAD, STALE.
  a8: power-category poison ring before/after recovery:
      uninitialized invalid category, wrong service device, unknown device with
      no ACK, valid sequence, stale replay, two more invalid samples, then a
      post-poison valid frame. Only the three valid battery frames may print
      ``SIMBAT``.

Usage:
  python3 tools/siminj.py [socket] [--profile s1|a8]
The socket defaults to /tmp/berryos-sim1.sock for manual ``just run-rootfs``.
"""

import argparse
import socket
import sys
import time

ST_OK, ST_BAD, ST_STALE = 0, 1, 2
ACK_LEN = 9
NO_ACK = object()


def frame(seq, *, dev=1, status=0, health=0, cap=None):
    # Device determines the frozen frame length. Thermal is a valid 98B frame
    # when used as the A8 wrong-service sample; unknown devices keep the short
    # noise form because the decoder must resynchronize without knowing length.
    size = {1: 50, 2: 98, 3: 19}.get(dev, 50)
    b = bytearray(size)
    b[0:4] = b"PWSM"
    b[4] = dev
    b[5] = 1
    b[6:8] = seq.to_bytes(2, "little")
    # payload offsets: present@8, ac@9, status@10, health@11, capacity@12.
    b[10] = status
    b[11] = health
    if cap is not None:
        b[12] = cap
    return bytes(b)


def ack_of(seq, st, dev=1):
    return bytes([0x50, 0x57, 0x53, 0x4D, dev, 1]) + seq.to_bytes(2, "little") + bytes([st])


def s1_cases():
    return [
        ("seq1-ok", frame(1), ack_of(1, ST_OK)),
        ("seq2-ok", frame(2), ack_of(2, ST_OK)),
        ("seq3-bad-cap101", frame(3, cap=101), ack_of(3, ST_BAD)),
        ("replay1-stale", frame(1), ack_of(1, ST_STALE)),
    ]


def a8_cases():
    return [
        # Uninitialized service: invalid battery status must be BAD and must not
        # establish last_seq.
        ("a8-uninit-status4", frame(1, status=4), ack_of(1, ST_BAD)),
        # Valid wire device but wrong service category: simbattery owns only
        # DEV_BATTERY in S1; thermal is a valid protocol frame and gets BAD.
        ("a8-wrong-dev-thermal", frame(1, dev=2), ack_of(1, ST_BAD, dev=2)),
        # Unknown device is header noise: decoder resynchronizes and sends no
        # ACK. A later ACK proves the decoder did not wedge on the poison frame.
        ("a8-unknown-dev9", frame(1, dev=9), NO_ACK),
        ("seq1-ok", frame(1), ack_of(1, ST_OK)),
        ("seq2-ok", frame(2), ack_of(2, ST_OK)),
        ("replay1-stale", frame(1), ack_of(1, ST_STALE)),
        ("a8-bad-cap101", frame(3, cap=101), ack_of(3, ST_BAD)),
        ("a8-bad-health8", frame(4, health=8), ack_of(4, ST_BAD)),
        # Recovery proof after the poison ring: stale/invalid samples did not
        # corrupt accounting; a later monotonic battery frame is accepted.
        ("a8-recovery-seq3", frame(3), ack_of(3, ST_OK)),
    ]


def connect(path, retries=10):
    last = None
    for _ in range(retries):
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            s.connect(path)
            s.settimeout(15.0)
            return s
        except (FileNotFoundError, ConnectionRefusedError) as exc:
            last = exc
            s.close()
            time.sleep(1.0)
    print(f"siminj: {path} {retries}s 不可连（QEMU serial1 未起？last={last}）", flush=True)
    raise SystemExit(1)


def read_ack(s, name):
    got = b""
    while len(got) < ACK_LEN:
        try:
            chunk = s.recv(ACK_LEN - len(got))
        except socket.timeout:
            print(f"{name}: ACK 超时", flush=True)
            raise SystemExit(1)
        if not chunk:
            print(f"{name}: EOF（simbattery 死/ACK 断流）", flush=True)
            raise SystemExit(1)
        got += chunk
    return got


def expect_no_ack(s, name):
    s.settimeout(0.5)
    try:
        got = s.recv(ACK_LEN)
    except socket.timeout:
        print(f"{name}: no-ACK PASS", flush=True)
        s.settimeout(15.0)
        return
    if got:
        print(f"{name}: 期望无 ACK，实得 {got.hex()}", flush=True)
    else:
        print(f"{name}: 期望无 ACK，实得 EOF", flush=True)
    raise SystemExit(1)


def run(path, profile):
    cases = s1_cases() if profile == "s1" else a8_cases()
    with connect(path) as s:
        print("connected", flush=True)
        for name, wire, want in cases:
            s.sendall(wire)
            if want is NO_ACK:
                expect_no_ack(s, name)
                continue
            got = read_ack(s, name)
            ok = got == want
            print(
                f"{name}: ack={got.hex()} want={want.hex()} "
                f"{'PASS' if ok else 'FAIL'}",
                flush=True,
            )
            if not ok:
                raise SystemExit(1)
            time.sleep(0.2)
    print(f"ALL-{('4' if profile == 's1' else 'A8')}-PASS", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("socket", nargs="?", default="/tmp/berryos-sim1.sock")
    ap.add_argument("--profile", choices=("s1", "a8"), default="s1")
    args = ap.parse_args()
    run(args.socket, args.profile)


if __name__ == "__main__":
    main()
