#!/usr/bin/env python3
"""A2-3 EVS echo server 宿主对拍 client（`docs/事件循环A2设计草案.md` §3.4——
A2 整卡验收线的 host 侧组件）。

- guest 侧 bsh `evs_selftest` 经 netd LISTEN(18082) 起事件循环 echo server
  （hostfwd `tcp::18082-:18082` 专线——LSN 自验 18081 零交集）；本脚本
  **逐连接串行**对拍（4-5a 铁律：串行 = 状态谓词零竞速——不建任何竞速标记）。
- 两连接异型载荷（64B/24B，与 bsh `EVS_PAYLOAD_0/1` 同源定值）：
  conn0[i] = (i*7+3) & 0xff，conn1[i] = (i*11+5) & 0xff。每连接：connect →
  write 全量 → read 收齐等长回显 → 逐字节比对 → close。
- **重试环**：hostfwd 在 guest 未就绪时也接受 host 侧 connect（SLIRP 缓冲/
  guest RST 后断），单连接失败 sleep 2 重来——guest boot 到 bsh EVS 实测
  可达 ~5min（TCG 慢夜，A2-3 首跑实录 16:52:50→~16:55:30），总帽 480s。
- 无时刻输入、成功输出全定（mkcpio/mkfat/echod 同纪律）：仅启动一行 +
  `<TAG> client ok n=2` 一行；失败诊断进 stderr、退出码 1（smoke 相尾
  kill+断言收口，提前退出 = loud）。
- 用法：python3 tools/evsclient.py [port] [tag]   # 默认 18082 EVS
  A3-3 SKT 复用（fd 皮对拍——`evsclient.py 18083 SKT`，guest bsh
  `skt_selftest` server 段同源载荷）；EVS 默认调用字节不变。
  smoke/手动用法：QEMU 前后台起 + 记 PID + 相尾按 PID kill（多会话禁 pkill）。
  Self-check: ``python3 tools/evsclient.py --selftest`` —— 线上零 QEMU
  钉板（载荷黄金哈希 vs bsh 同源定值 + try_conn 全链 + 帽序契约 +
  main 重试环），见 ``selftest()``。
"""
import os
import socket
import sys
import time

HOST = "127.0.0.1"
PORT = 18082
# 帽 env 化（S-2b §5.1）：默认值 = 主 smoke 单核线现行定值不变；smoke-smp
# 线经 env 覆盖（TOTAL 960 / ATTEMPT 90——MTTCG 双核 0.61x 一切帽 ×2 铁律；
# 「client 帽 ≥ 相帽 + 60s」与「内帽 < 尝试帽」两道帽序见 SMP-S2 草案 §5.1）。
TOTAL_CAP_S = int(os.environ.get("BERRY_EVS_TOTAL_CAP_S", "660"))  # 总帽：guest boot→bsh EVS 实测 ~60s-5min（TCG 慢夜）；X-5 帽序勘正（2026-11-09）：相帽 DEADLINE_ROOTFS=600 后 480 已倒挂（client 先于 QEMU 起、帽序不能倒），升 660 = 600+60 序恢复——smoke-smp 同款 env 覆盖（960）不受影响
ATTEMPT_CAP_S = int(os.environ.get("BERRY_EVS_ATTEMPT_CAP_S", "45"))  # 单尝试帽：connect/write/read 全程；失败尝试留 guest CloseWait
                    # 尸槽（bsh 侧释槽 30s 帽 + 重入 accept）——45s 覆盖回收窗
RETRY_SLEEP_S = 2

PAYLOADS = [
    bytes((i * 7 + 3) & 0xFF for i in range(64)),
    bytes((i * 11 + 5) & 0xFF for i in range(24)),
]


def try_conn(port: int, payload: bytes) -> None:
    """单连接一轮：connect → write → read 等长 → 逐字节比对 → close。

    任何一步异常上抛（调用方重试环收）。Nagle 关闭：单写无影响、防未来
    多段场景尾延迟（echod 同款防御位）。
    """
    with socket.create_connection((HOST, port), timeout=ATTEMPT_CAP_S) as s:
        s.settimeout(ATTEMPT_CAP_S)
        s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        s.sendall(payload)
        got = bytearray()
        while len(got) < len(payload):
            chunk = s.recv(len(payload) - len(got))
            if not chunk:  # 提前 EOF = echo 短斤（guest 侧断——非重试可愈）
                raise RuntimeError(
                    f"echo short: got {len(got)}/{len(payload)} before EOF"
                )
            got += chunk
        if bytes(got) != payload:
            # 逐字节首差定位（诊断面；成功路径零输出）
            bad = next(i for i in range(len(payload)) if got[i] != payload[i])
            raise RuntimeError(
                f"echo mismatch at [{bad}]: {got[bad]:#x} != {payload[bad]:#x}"
            )


def main() -> int:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else PORT
    tag = sys.argv[2] if len(sys.argv) > 2 else "EVS"
    name = f"{tag.lower()}client"
    print(f"{name}: start on {HOST}:{port}", flush=True)
    t0 = time.monotonic()
    n_ok = 0
    for ci, payload in enumerate(PAYLOADS):
        while True:
            if time.monotonic() - t0 > TOTAL_CAP_S:
                print(
                    f"{name}: FAIL: total cap {TOTAL_CAP_S}s hit at conn {ci}",
                    file=sys.stderr,
                    flush=True,
                )
                return 1
            try:
                try_conn(port, payload)
                break
            except Exception as e:  # noqa: BLE001 —— 重试环吃一切网络态
                print(
                    f"{name}: conn {ci} attempt failed: {e}",
                    file=sys.stderr,
                    flush=True,
                )
                time.sleep(RETRY_SLEEP_S)
        n_ok += 1
    print(f"{tag} client ok n={n_ok}", flush=True)
    return 0


def selftest() -> int:
    """`--selftest`：线上零 QEMU 钉板（evsclient 宿主链零守卫收口）。

    仿 mkfat/echod --selftest 同位纪律（a-3 U4）：本件此前只在 smoke
    A2-3/A3-3 相被动覆盖（guest EVS/SKT 对拍 client），门禁自身坏（载荷
    定值漂/echo 短斤判错/帽序倒挂）= smoke 相静默劣化成超时排查。
    五面：
      ① 载荷黄金哈希——PAYLOADS[0/1] 与 bsh EVS_PAYLOAD_0/1（
        `user/bins/bsh/src/main.rs` `(i*7+3)`/`(i*11+5)` 同源定值）逐字节
        共识钉板：生成式断言 + sha256 冻结（漂则红——两侧任一改定值
        忘同步即被咬）；长度 64/24 契约（A2-3 草案 §3.4）。
      ② try_conn 全链环回——真 socket 上起最小 echo server（临时口 0），
        try_conn 两载荷全过（connect→write→read 等长→逐字节→close）；
        错误载荷回显 server 侧打 first-diff 诊断位（首差定位正确性）。
      ③ 短斤拒面——server 提前 close：try_conn 必抛 RuntimeError 含
        ``echo short``（guest 侧断链形态的红判据，非挂死）。
      ④ 帽序契约——TOTAL_CAP_S ≥ DEADLINE_ROOTFS(600)+60 且
        ATTEMPT_CAP_S < TOTAL_CAP_S（X-5 帽序勘正注：client 先于 QEMU
        起、帽序不能倒；SMP 线 env 覆盖 960/90 同序——env 改漂即咬）。
      ⑤ main 重试环——port 参数化 + 失败诊断 stderr 面 + 成功输出全定
        （``<TAG> client ok n=2``）三契约在环回 server 上整链复演（与
        smoke 相同调用形 ``main()``，argparse-free argv 直读不改产线）。
    """
    import hashlib
    import threading

    # ① 载荷黄金哈希（与 bsh main.rs EVS_PAYLOAD_0/1 同源定值对账）。
    want0 = bytes((i * 7 + 3) & 0xFF for i in range(64))
    want1 = bytes((i * 11 + 5) & 0xFF for i in range(24))
    assert len(PAYLOADS) == 2 and len(PAYLOADS[0]) == 64 and len(PAYLOADS[1]) == 24, (
        "selftest: PAYLOADS 形状漂（应 64B/24B 两连接——A2-3 §3.4）"
    )
    assert PAYLOADS[0] == want0, "selftest: conn0 载荷漂（(i*7+3)&0xff ×64）"
    assert PAYLOADS[1] == want1, "selftest: conn1 载荷漂（(i*11+5)&0xff ×24）"
    g0 = hashlib.sha256(PAYLOADS[0]).hexdigest()
    g1 = hashlib.sha256(PAYLOADS[1]).hexdigest()
    assert g0 == "39e3d7b6b5d075d37d053ad89b24b41bef4f3c29760c84447cab3f3be1882241", (
        f"selftest: conn0 黄金哈希漂 {g0}"
    )
    assert g1 == "40b374c7c9cb2c11959424fe31a8b2d7783d990143590a861452bd8a69e1a25e", (
        f"selftest: conn1 黄金哈希漂 {g1}"
    )

    # ② 环回 echo server（临时口 0——不碰 18080-83 互斥口）。
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.bind((HOST, 0))
    srv.listen(4)
    port = srv.getsockname()[1]

    def echo_once(conn: socket.socket) -> None:
        with conn:
            data = conn.recv(4096)
            conn.sendall(data)

    t = threading.Thread(
        target=lambda: [echo_once(srv.accept()[0]) for _ in range(2)], daemon=True
    )
    t.start()
    try_conn(port, PAYLOADS[0])
    try_conn(port, PAYLOADS[1])  # 全链绿 = ② 过

    # ③ 短斤拒面：server 收即 close → try_conn 必抛 echo short。
    srv2 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv2.bind((HOST, 0))
    srv2.listen(4)
    port2 = srv2.getsockname()[1]

    def short_close(conn: socket.socket) -> None:
        with conn:
            conn.recv(4096)
        # 收即 close = 提前 EOF（echo 短斤形态）

    t2 = threading.Thread(
        target=lambda: [short_close(srv2.accept()[0]) for _ in range(1)], daemon=True
    )
    t2.start()
    try:
        try_conn(port2, PAYLOADS[0])
        raise AssertionError("selftest: 短斤 server 未被拒（应抛 echo short）")
    except RuntimeError as e:
        assert "echo short" in str(e), f"selftest: 短斤诊断面漂：{e}"

    # ④ 帽序契约（X-5 勘正注 + SMP env 覆盖同序）。
    assert TOTAL_CAP_S >= 660, f"selftest: 总帽倒挂（{TOTAL_CAP_S} < 600+60）"
    assert ATTEMPT_CAP_S < TOTAL_CAP_S, (
        f"selftest: 单尝试帽 ≥ 总帽（{ATTEMPT_CAP_S} ≥ {TOTAL_CAP_S}）"
    )

    # ⑤ main 整链复演：环回 server + argv 注入 + 输出全定断言。
    srv3 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv3.bind((HOST, 0))
    srv3.listen(4)
    port3 = srv3.getsockname()[1]

    def echo_two(conn: socket.socket) -> None:
        with conn:
            got = bytearray()
            while len(got) < 4096:
                b = conn.recv(4096)
                if not b:
                    break
                got += b
                if len(got) >= 24:  # 两载荷皆 ≤64B，收齐即回
                    break
            conn.sendall(bytes(got))

    t3 = threading.Thread(
        target=lambda: [echo_two(srv3.accept()[0]) for _ in range(2)], daemon=True
    )
    t3.start()
    argv_saved = sys.argv
    import contextlib
    import io

    out = io.StringIO()
    try:
        sys.argv = ["evsclient.py", str(port3), "ST"]
        with contextlib.redirect_stdout(out):
            rc = main()
    finally:
        sys.argv = argv_saved
    assert rc == 0, f"selftest: main 整链 rc={rc}"
    lines = [l for l in out.getvalue().splitlines() if l.strip()]
    assert lines[-1] == "ST client ok n=2", f"selftest: 成功输出全定破：{lines[-1]!r}"

    for s in (srv, srv2, srv3):
        s.close()
    print("evsclient: selftest ok (黄金哈希×2/全链/短斤/帽序/main ×6)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    sys.exit(main())
