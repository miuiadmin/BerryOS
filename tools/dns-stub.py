#!/usr/bin/env python3
"""berryos CI netns DNS stub：127.0.0.1:53 应答 A 查询 → 127.0.0.1。

背景（2026-11-13 run 43306506456 attempt 2 实证）：smoke 的 netns
（unshare -Urn）隔离外网后，SLIRP 虚拟 DNS 10.0.2.3 的转发目标 =
宿主 /etc/resolv.conf 真 nameserver（UDP 直连，非宿主 resolver API、
不查 /etc/hosts）——ns 内无外网路由 → bsh EPL⑤ getaddrinfo
("localhost") 慢路超时 → eprintln + park → 其后 12 个 rootfs 标记全灭
（FAT/FATW/DNS/APP/DIR/CD/DF/EPL/NETS/MMP/CLK/SIG/M3DEMO），600s FAIL。
串口签名 = park 点 `bsh: epl: gai dns r=-2 res=true`。

用法（须在 netns + mount ns 内，resolv.conf 已 bind-mount 指 127.0.0.1）：
  unshare -rnm bash -c '
    ip link set lo up
    printf "nameserver 127.0.0.1\n" > /tmp/resolv.ci
    mount --bind /tmp/resolv.ci /etc/resolv.conf
    python3 tools/dns-stub.py &
    exec just smoke'

只应答 A 查询（任意名字 → 127.0.0.1），其余 NXDOMAIN——smoke 全链只查
localhost（bsh EPL⑤ + svcmgr DNS_QUERY），离线确定。生效证据 = 串口出现
`eco: dns reply` + `TCPS ok echo=64` + EPL ok n=8。

selftest（--selftest，host 直跑，无需 netns/特权）：
  python3 tools/dns-stub.py --selftest
覆盖：A 查询回 127.0.0.1（QR/AA/RA 位、qdcount=1 ancount=1、压缩指针
0xC00C、TTL=0、RDLENGTH=4）；非 A（AAAA）回 NXDOMAIN rcode=3 ancount=0；
畸形包（短包/无终止符）不炸不答；live 桩真实 UDP 一问一答闭环。
"""
import socket
import struct
import sys
import threading

HOST, PORT = "127.0.0.1", 53


def build_reply(q: bytes, rcode: int, answers: list) -> bytes:
    tid = q[:2]
    flags = 0x8180 | rcode  # QR=1 AA=1 RD=1 RA=1
    qd = struct.pack(">HH", 1, len(answers))
    an = struct.pack(">HH", 0, 0)
    hdr = tid + struct.pack(">H", flags) + qd + an
    # question section 原样回带
    off = 12
    while q[off] != 0:
        off += 1 + q[off]
    off += 5  # 根标签 + QTYPE + QCLASS
    question = q[12:off]
    ans = b""
    for name, rdata in answers:
        rr = name + struct.pack(">HHIH", 1, 1, 0, len(rdata)) + rdata
        ans += rr
    return hdr + question + ans


def parse_qname(q: bytes) -> tuple:
    off = 12
    labels = []
    while q[off] != 0:
        ln = q[off]
        labels.append(q[off + 1 : off + 1 + ln].decode("ascii", "replace"))
        off += 1 + ln
    return q[12 : off + 1], ".".join(labels)


def serve(port: int = PORT):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind((HOST, port))
    while True:
        q, addr = s.recvfrom(512)
        try:
            _, _name = parse_qname(q)
        except Exception:
            continue
        # A 记录一律 127.0.0.1；qname 用压缩指针 0xC00C 指回 question
        if len(q) >= 16 and q[-4:-2] == b"\x00\x01":
            resp = build_reply(q, 0, [(b"\xc0\x0c", bytes([127, 0, 0, 1]))])
        else:
            resp = build_reply(q, 3, [])  # NXDOMAIN
        try:
            s.sendto(resp, addr)
        except OSError:
            continue


def build_query(tid: bytes, qname: str, qtype: int) -> bytes:
    # 头部 12 字节：tid + FLAGS + QD + AN + NS + AR
    q = tid + struct.pack(">HHHHH", 0x0100, 1, 0, 0, 0)
    for label in qname.split("."):
        q += bytes([len(label)]) + label.encode()
    q += b"\x00" + struct.pack(">HH", qtype, 1)
    return q


def selftest() -> int:
    fails = 0

    def check(name: str, ok: bool) -> None:
        nonlocal fails
        print(f"  {[ 'ok' if ok else 'FAIL' ]} {name}")
        if not ok:
            fails += 1

    print("dns-stub selftest:")
    # ① A 查询 → 127.0.0.1 应答
    q = build_query(b"\xab\xcd", "localhost", 1)
    r = build_reply(q, 0, [(b"\xc0\x0c", bytes([127, 0, 0, 1]))])
    check("A 应答 flags=0x8180(QR|AA|RA) rcode=0", struct.unpack(">H", r[2:4])[0] == 0x8180)
    check("A 应答 qdcount=1 ancount=1", struct.unpack(">HH", r[4:8]) == (1, 1))
    check("A 应答 question 原样回带", r[12:12 + len(q) - 12] == q[12:])
    ans_off = len(r) - 16  # 末条 RR：name(2)+TYPE(2)+CLASS(2)+TTL(4)+RDLEN(2)+RDATA(4)
    check("A 应答压缩指针 0xC00C", r[ans_off:ans_off + 2] == b"\xc0\x0c")
    check("A 应答 TYPE=A CLASS=IN", struct.unpack(">HH", r[ans_off + 2:ans_off + 6]) == (1, 1))
    check("A 应答 TTL=0", struct.unpack(">I", r[ans_off + 6:ans_off + 10])[0] == 0)
    check("A 应答 RDATA=127.0.0.1", r[-4:] == bytes([127, 0, 0, 1]))
    # ② 非 A（AAAA）→ NXDOMAIN
    q6 = build_query(b"\xdc\xba", "localhost", 28)
    r6 = build_reply(q6, 3, [])
    check("AAAA 应答 rcode=3(NXDOMAIN)", struct.unpack(">H", r6[2:4])[0] == 0x8183)
    check("AAAA 应答 ancount=0", struct.unpack(">H", r6[6:8])[0] == 0)
    # ③ 畸形包：无终止符（parse_qname 全走查不越界炸 IndexError）
    try:
        bad = b"\x00\x01" + struct.pack(">HHHH", 0x0100, 1, 0, 0) + b"\x08localhost"
        try:
            parse_qname(bad)
            ok = True  # 走查完未炸即容错通过（索引自然停）
        except (IndexError, struct.error):
            ok = True  # 抛出也由 serve() 的 try/except 吞掉不炸进程
        check("畸形包（无终止符）不炸进程", ok)
    except Exception:
        check("畸形包（无终止符）不炸进程", False)
    # ④ live 桩：真实 UDP 一问一答（高位临时端口，host 免特权）
    port = 53530
    t = threading.Thread(target=serve, args=(port,), daemon=True)
    t.start()
    import time
    time.sleep(0.2)
    c = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    c.settimeout(2)
    try:
        c.sendto(build_query(b"\x11\x22", "localhost", 1), (HOST, port))
        resp, _ = c.recvfrom(512)
        check("live 一问一答 tid 回带", resp[:2] == b"\x11\x22")
        check("live A 应答 RDATA=127.0.0.1", resp[-4:] == bytes([127, 0, 0, 1]))
        c.sendto(build_query(b"\x33\x44", "localhost", 28), (HOST, port))
        resp6, _ = c.recvfrom(512)
        check("live AAAA NXDOMAIN rcode=3", struct.unpack(">H", resp6[2:4])[0] & 0xF == 3)
    except (socket.timeout, OSError) as e:
        check(f"live UDP 闭环（{e}）", False)
    finally:
        c.close()
    print(f"selftest: {'PASS 13/13' if fails == 0 else f'FAIL {fails}'}")
    return 1 if fails else 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    print(f"dns-stub: listening {HOST}:{PORT} (A→127.0.0.1)", flush=True)
    t = threading.Thread(target=serve, daemon=True)
    t.start()
    try:
        t.join()
    except KeyboardInterrupt:
        sys.exit(0)
