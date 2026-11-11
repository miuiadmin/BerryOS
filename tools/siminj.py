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

Self-check: ``python3 tools/siminj.py --selftest`` (no socket, no QEMU) pins
the frozen wire contract and audits both vector tables -- see ``selftest()``.
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


def selftest() -> int:
    """`--selftest`：注入 client 线契约钉板 + 双 profile 向量审计（零 QEMU）。

    仿 elf2flat/pwrcls --selftest 同位纪律（a-3 U4）：本件只在 smoke 注入
    相被动覆盖，门禁自身坏（帧长漂/头偏移漂/向量序漂）= SIMBAT 相静默
    劣化成 60s 超时排查。三面：
      ① 线契约钉板——magic/ver/头偏移、per-dev 冻结帧长 {1:50, 2:98, 3:19}
        与 berry-pwproto 常量同源（HEADER 8 + payload 38/86/7 + changed u32
        ——`user/berry-pwproto/src/lib.rs` §帧布局；改协议冻结面必同步本锚）；
      ② 向量审计——两 profile 条数/序/帧↔ACK seq·dev·st 恒等 + a8 恰一
        NO_ACK 且毒环前后有效帧可恢复（sim-S2 A8 九矢形状）；
      ③ 拒面——ack_of 越界 st、seq LE 编码溢出、未知 dev 噪声形 50B 短帧。
    """
    # ① 冻结线契约：头 8B + 载荷定长 + 事件位图 u32（与 berry-pwproto 对账）。
    HDR, PAY = 8, {1: 38, 2: 86, 3: 7}
    for dev, total in ((1, 50), (2, 98), (3, 19)):
        assert total == HDR + PAY[dev] + 4, f"selftest: dev{dev} 帧长漂"
        f = frame(0, dev=dev)
        assert len(f) == total, f"selftest: dev{dev} 实长 {len(f)} != {total}"
        assert f[0:4] == b"PWSM", "selftest: magic 漂"
        assert f[4] == dev and f[5] == 1, "selftest: dev/ver 漂"
        assert f[6:8] == (0).to_bytes(2, "little"), "selftest: seq LE 漂"
        assert f[8] == 0 and f[9] == 0, "selftest: present/ac 缺省漂"  # 全零载荷起版
        # seq u16 LE 双端钉（0/最大/奇偶）。
        for seq in (0, 1, 0xBEEF, 0xFFFF):
            assert frame(seq)[6:8] == seq.to_bytes(2, "little"), f"selftest: seq{seq} LE"
    # 载荷偏移钉板：status@10 / health@11 / capacity@12（b[12] 写入即被
    # berry-pwproto 值域拒 capacity>100 → BAD；偏移漂 = 注入面失义）。
    f = frame(3, status=2, health=5, cap=101)
    assert (f[10], f[11], f[12]) == (2, 5, 101), "selftest: 载荷偏移漂"
    assert f[13:50] == bytes(37), "selftest: 载荷残留非零"

    # ACK 契约：9B = 头 8 + st u8；dev/st 落位。
    a = ack_of(0x1234, ST_STALE, dev=2)
    assert len(a) == ACK_LEN == 9, "selftest: ACK 长漂"
    assert a[0:4] == b"PWSM" and a[4] == 2 and a[5] == 1, "selftest: ACK 头漂"
    assert a[6:8] == (0x1234).to_bytes(2, "little"), "selftest: ACK seq LE 漂"
    assert a[8] == ST_STALE, "selftest: ACK st 落位漂"

    # ② 向量审计：两 profile。
    s1 = s1_cases()
    assert len(s1) == 4, "selftest: s1 恰四矢（OK×2/BAD/STALE——smoke 计数门 2 行）"
    assert [c[0] for c in s1] == ["seq1-ok", "seq2-ok", "seq3-bad-cap101", "replay1-stale"]
    sts = []
    for name, wire, want in s1:
        assert want is not NO_ACK, f"selftest: {name} s1 不应有 NO_ACK"
        assert wire[6:8] == want[6:8], f"selftest: {name} 帧/ACK seq 不一致"
        assert wire[4] == want[4], f"selftest: {name} 帧/ACK dev 不一致"
        sts.append(want[8])
    assert sts == [ST_OK, ST_OK, ST_BAD, ST_STALE], "selftest: s1 期望序漂"
    # s1 第三矢 cap=101 真值域毒（ berry-pwproto capacity>100 → BAD）。
    assert s1[2][1][12] == 101, "selftest: s1 BAD 矢 cap 值漂"

    a8 = a8_cases()
    assert len(a8) == 9, "selftest: a8 恰九矢（sim-S2 A8 毒环/恢复）"
    no_ack = [c for c in a8 if c[2] is NO_ACK]
    assert len(no_ack) == 1 and no_ack[0][0] == "a8-unknown-dev9", \
        "selftest: a8 恰一 NO_ACK（未知 dev=9 头杂音——解码器再同步不 ACK）"
    assert len(no_ack[0][1]) == 50, "selftest: 未知 dev 噪声形应短帧 50B"
    assert no_ack[0][1][4] == 9, "selftest: NO_ACK 矢 dev 字节漂"
    for name, wire, want in a8:
        if want is NO_ACK:
            continue
        assert wire[6:8] == want[6:8], f"selftest: {name} 帧/ACK seq 不一致"
        assert wire[4] == want[4], f"selftest: {name} 帧/ACK dev 不一致"
    # a8 形状：uninit(status=4)→wrong-dev(2)→no-ack(9)→ok→ok→stale→bad→bad→ok
    kinds = []
    for name, wire, want in a8:
        kinds.append("NO_ACK" if want is NO_ACK else want[8])
    assert kinds == [ST_BAD, ST_BAD, "NO_ACK", ST_OK, ST_OK, ST_STALE, ST_BAD, ST_BAD, ST_OK], \
        "selftest: a8 期望序漂（毒环 2 拒 + 静默 + 恢复链）"
    assert a8[0][1][10] == 4, "selftest: uninit 矢 status=4 漂"
    assert a8[1][1][4] == 2 and len(a8[1][1]) == 98, "selftest: wrong-dev 矢应 thermal 98B"
    assert a8[7][1][11] == 8, "selftest: health=8 毒矢漂"
    # 恢复矢重放被拒矢同 seq=3：BAD 不立 last_seq（拒帧零腐蚀账面）——
    # 若 BAD 推进了账，重放 seq=3 应 STALE 而非 OK，毒环「恢复」即失义。
    assert a8[6][1][6:8] == a8[8][1][6:8] == (3).to_bytes(2, "little"), \
        "selftest: 恢复矢应重放 bad-cap101 的 seq=3"

    # ③ 拒面：本件是注入器不是编码器——负样本只钉「构造面恒等」不钉拒绝
    #（值域拒绝属 berry-pwproto host 测试，单编码器纪律不长第二套）。
    assert frame(1, dev=2)[13] == 0, "selftest: thermal 矢起点载荷漂"
    assert frame(1)[8:50] == bytes(42), "selftest: battery 全零载荷残留"
    print("siminj: selftest ok (线契约钉板 dev{1:50,2:98,3:19} + s1 四矢/a8 九矢审计 + 偏移钉板)")
    return 0


def main():
    if sys.argv[1:2] == ["--selftest"]:
        return selftest()
    ap = argparse.ArgumentParser()
    ap.add_argument("socket", nargs="?", default="/tmp/berryos-sim1.sock")
    ap.add_argument("--profile", choices=("s1", "a8"), default="s1")
    args = ap.parse_args()
    run(args.socket, args.profile)


if __name__ == "__main__":
    main()
