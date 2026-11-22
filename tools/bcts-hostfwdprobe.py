#!/usr/bin/env python3
"""bcts-hostfwdprobe——CI netns 内 SLIRP hostfwd 入向段预检（#290 REJECT 修面）。

背景（run 43616188197 实证 + owner 第 6 次裁定定谳）：bcts 步在 netns +
mount-ns 内复跑 smoke 全相时，evsclient ×10 conn timed out——红面按分类器
收敛在 hostfwd 入向段（宿主 → 127.0.0.1:18081-83 → SLIRP → guest），但
「QEMU/SLIRP 侧死」还是「echod/环境侧死」在烧掉 16 分钟前无法区分。本
工具在 `just bcts` 前跑两条腿，5 秒内出定谳：

  腿 A（echod 侧锚）：ns 内 127.0.0.1:18080 起 TCP 回显（与 echod 同
      bind 面）+ 本机 connect 收发 8B——证 lo 起、18080 可 bind、ns TCP
      栈活。红 = 环境面（echod 同样起不来），与 QEMU 无关。
  腿 B（hostfwd 监听面）：最小 QEMU `-S` 冻结态（无内核无盘——CPU 冻在
      复位向量，零 TCG 执行）挂 smoke.sh 同款 netdev（hostfwd 18081-83
      + virtio-net-device），宿主侧逐口 connect 探活。SLIRP hostfwd 的
      监听 socket 在 guest 未就绪时也收 host 侧 connect（evsclient.py
      头注同源定谳）——accept 即监听面活证，与 guest 状态解耦。红 =
      QEMU/SLIRP hostfwd 侧（版本/绑定面），非 guest 非 echod。

绿 = 两腿全过（后续 evsclient 红只剩 guest 侧变量）；红 = 红面钉死在
腿面 + loud 定谳行，不再盲烧全相。

用法（须在 netns 内、lo 已起——ci.yml bcts 步 unshare 脚本内、`exec
just bcts` 前一行）：
  python3 tools/bcts-hostfwdprobe.py            # 两腿真跑（QEMU≈1s）
  python3 tools/bcts-hostfwdprobe.py --selftest # 零 QEMU 钉板（挂 test-host）

输出（无时刻输入、成功输出全定——dns-stub/echod 同纪律）：
  hostfwdprobe: A ok loopback-echo 127.0.0.1:18080 8B
  hostfwdprobe: B ok qemu hostfwd 18081-83 accept x3（SLIRP 入向监听面活）
退出码：0 全绿；10 腿 A 红；20 腿 B 红；2 用法错。

selftest 覆盖（零 QEMU、临时口——不碰 18080-83 自托管互斥口）：netdev
  单源对拍（smoke.sh 四处 -netdev 行与本件常量全等——端口面漂移即红）/
  QEMU argv 组装（含 -S 与 netdev 常量）/ 腿 A 回显环回 / 慢起监听者
  poll-until-accept / 恒拒面分类 / 定谳行字面钉板。
"""
import os
import re
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

HOST = "127.0.0.1"
# smoke.sh 四处 launch（rootfs/RX/FAT 子目录/FAT 持久化）同款 netdev 行——
# 端口/规格漂移由 --selftest 对拍咬住（单源纪律：改 smoke.sh hostfwd 面
# 必同步本常量，反之亦然）。
HOSTFWD_PORTS = (18081, 18082, 18083)
HOSTFWD_NETDEV = (
    "user,id=n0,hostfwd=tcp::18081-:18081,"
    "hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083"
)
ECHO_PORT = 18080  # echod 同 bind 口（腿 A 锚）
LISTEN_WAIT_S = 10.0  # 腿 B：QEMU 起到 listener 就绪帽（实测 <1s，帽只兜慢）
POLL_S = 0.25

EXIT_A_RED, EXIT_B_RED, EXIT_USAGE = 10, 20, 2

A_OK_LINE = f"hostfwdprobe: A ok loopback-echo {HOST}:{ECHO_PORT} 8B"
B_OK_LINE = (
    f"hostfwdprobe: B ok qemu hostfwd {HOSTFWD_PORTS[0]}-{HOSTFWD_PORTS[-1]} "
    "accept x3（SLIRP 入向监听面活）"
)


# —— 腿 A：ns loopback + 18080 bind + 回显环回（echod 侧环境锚） ——
def _echo_server(srv: socket.socket, got: list) -> None:
    conn, _ = srv.accept()
    with conn:
        data = conn.recv(64)
        conn.sendall(data)  # 收即回显（echod 同语义）
    got.append(True)


def leg_a() -> str:
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        srv.bind((HOST, ECHO_PORT))
        srv.listen(1)
    except OSError as e:
        return (
            f"hostfwdprobe: A FAIL 127.0.0.1:{ECHO_PORT} bind/listen {e}——"
            "ns loopback/端口面问题（echod 将同样起不来；非 QEMU 面）"
        )
    got: list = []
    th = threading.Thread(target=_echo_server, args=(srv, got), daemon=True)
    th.start()
    payload = b"PINGA1B2"  # 8B 定值
    try:
        with socket.create_connection((HOST, ECHO_PORT), timeout=5) as s:
            s.settimeout(5)
            s.sendall(payload)
            if s.recv(len(payload)) != payload:
                return (
                    f"hostfwdprobe: A FAIL 127.0.0.1:{ECHO_PORT} 回显不匹配"
                    "——ns TCP 栈面问题（非 QEMU 面）"
                )
    except OSError as e:
        return (
            f"hostfwdprobe: A FAIL 127.0.0.1:{ECHO_PORT} connect {e}——"
            "ns loopback 面问题（lo 未起？非 QEMU 面）"
        )
    finally:
        srv.close()
    th.join(timeout=5)
    return A_OK_LINE


# —— 腿 B：QEMU -S 冻结态 SLIRP hostfwd 监听探活 ——
def qemu_argv() -> list:
    return [
        os.environ.get("QEMU", "qemu-system-aarch64"),
        "-machine", "virt", "-cpu", "cortex-a72", "-m", "32M",
        "-S",  # 冻结在复位向量：listener 就绪与 guest 状态解耦
        "-display", "none", "-serial", "none", "-monitor", "none",
        "-netdev", HOSTFWD_NETDEV,
        "-device", "virtio-net-device,netdev=n0",
    ]


def _poll_accept(port: int, cap_s: float) -> None:
    """poll-until-accept：listener 就绪即返；帽内恒拒/超时上抛。"""
    t0 = time.monotonic()
    last: Exception | None = None
    while time.monotonic() - t0 < cap_s:
        try:
            socket.create_connection((HOST, port), timeout=2).close()
            return
        except OSError as e:  # refused = listener 未就绪，续 poll
            last = e
            time.sleep(POLL_S)
    raise TimeoutError(f"{port}: {LISTEN_WAIT_S}s 未就绪（last={last}）")


def leg_b() -> str:
    proc = subprocess.Popen(  # noqa: S603 - argv 单源 qemu_argv()
        qemu_argv(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    try:
        try:
            for port in HOSTFWD_PORTS:
                _poll_accept(port, LISTEN_WAIT_S)
        except (TimeoutError, OSError) as e:
            return (
                f"hostfwdprobe: B FAIL qemu hostfwd 监听 {e}——红面钉在 "
                "QEMU/SLIRP hostfwd 侧（非 guest 非 echod；查 QEMU 版本/"
                "netdev 绑定面）"
            )
        return B_OK_LINE
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


def main() -> int:
    if len(sys.argv) > 2 or (len(sys.argv) == 2 and sys.argv[1] != "--selftest"):
        print(__doc__)
        return EXIT_USAGE
    if len(sys.argv) == 2:
        return selftest()

    a = leg_a()
    print(a, flush=True)
    if not a.endswith("8B"):
        return EXIT_A_RED
    b = leg_b()
    print(b, flush=True)
    return 0 if b == B_OK_LINE else EXIT_B_RED


# —— selftest（零 QEMU、临时口——不碰 18080-83 互斥口） ——
def selftest() -> int:
    fails: list[str] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        print(f"  {'ok' if ok else 'FAIL'}  {name}" + (f"——{detail}" if detail else ""))
        if not ok:
            fails.append(name)

    # ① netdev 单源对拍：smoke.sh 全部 -netdev user 行与本件常量全等。
    smoke = Path(__file__).parent / "smoke.sh"
    text = smoke.read_text(errors="replace")
    found = re.findall(r"-netdev (user,id=n0,(?:hostfwd=tcp::\d+-:\d+,?)+)", text)
    check(
        "netdev 单源对拍 smoke.sh",
        bool(found) and all(f == HOSTFWD_NETDEV for f in found),
        f"{len(found)} 处全等" if found else "smoke.sh 无 -netdev 行（面漂移）",
    )

    # ② QEMU argv 组装：-S 冻结 + netdev 常量在列（QEMU 名可 env 覆盖）。
    argv = qemu_argv()
    check(
        "qemu argv 含 -S 与 netdev 常量",
        "-S" in argv and HOSTFWD_NETDEV in argv
        and any(a.startswith("virtio-net-device,netdev=n0") for a in argv),
    )

    # ③ 腿 A 回显环回（临时口版：腿 A 逻辑本体，bind 0 由内核分配）。
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.bind((HOST, 0))
    srv.listen(1)
    port = srv.getsockname()[1]
    got: list = []
    th = threading.Thread(target=_echo_server, args=(srv, got), daemon=True)
    th.start()
    with socket.create_connection((HOST, port), timeout=2) as s:
        s.sendall(b"12345678")
        echoed = s.recv(8)
    srv.close()
    th.join(timeout=2)
    check("腿 A 回显环回（临时口）", echoed == b"12345678" and got == [True])

    # ④ poll-until-accept：慢起监听者——bind 立即（口已知）、listen 延迟
    #    0.5s（未 listen 期间 connect 恒 refused，恰是被测 poll 路径）。
    late = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    late.bind((HOST, 0))
    late_port = late.getsockname()[1]
    th2 = threading.Timer(0.5, late.listen, args=(1,))
    th2.start()
    try:
        _poll_accept(late_port, 5.0)
        check("poll-until-accept 慢起 0.5s 收敛", True)
    except Exception as e:  # noqa: BLE001
        check("poll-until-accept 慢起 0.5s 收敛", False, str(e))
    finally:
        late.close()

    # ⑤ 恒拒面分类：无监听者口必红（0.1s 短帽验超时路径不烧 10s）。
    probe_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    probe_sock.bind((HOST, 0))
    dead_port = probe_sock.getsockname()[1]
    probe_sock.close()  # 已关口：恒 refused
    try:
        _poll_accept(dead_port, 0.1)
        check("恒拒面 0.1s 帽超时红", False, "未上抛")
    except (TimeoutError, OSError):
        check("恒拒面 0.1s 帽超时红", True)

    # ⑥ 定谳行字面钉板（改文案先改 selftest——CI 对账面消费者）。
    check(
        "定谳行字面钉板",
        A_OK_LINE == "hostfwdprobe: A ok loopback-echo 127.0.0.1:18080 8B"
        and B_OK_LINE.endswith("accept x3（SLIRP 入向监听面活）")
        and EXIT_A_RED == 10 and EXIT_B_RED == 20,
    )

    print(f"hostfwdprobe selftest: {6 - len(fails)}/6 腿过")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
