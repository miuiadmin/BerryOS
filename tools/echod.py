#!/usr/bin/env python3
"""确定性 TCP 回显服务（`docs/TCP栈设计草案.md` §6 —— T-c 的 host 侧验收组件）。

- bind 127.0.0.1:18080（QEMU user 网：guest 主动 connect 10.0.2.2:18080
  由 SLIRP 转发到宿主本机同端口——vnet tcp.rs 自验的对端）。
- 收即原样回发（sendall 全量）；client 半关闭（FIN）后被动关闭本端
  （CLOSE_WAIT→LAST_ACK 侧——让 guest 恰好验**主动关闭**全路径）。
- 无时刻输入、输出全定（mkcpio/mkfat 同纪律）：仅启动一行，内容固定。
- 用法：python3 tools/echod.py [port]        # 默认 18080
  smoke/手动用法：后台起 + 记 PID + 相尾按 PID kill（多会话禁 pkill）。
  Self-check: ``python3 tools/echod.py --selftest`` —— 进程内环回（线程
  serve + 主线程 client 对拍），bind 临时口 0（不碰 18080-83 自托管
  互斥口），见 ``selftest()``。
"""
import socket
import sys
import threading

HOST = "127.0.0.1"
PORT = 18080


def serve_bind(port: int) -> socket.socket:
    """bind+listen（``--selftest`` 前置拆分：返回 listener 供探真实口）。

    产线调用面零改动（smoke.sh/justfile ``python3 tools/echod.py`` 照旧
    无限 serve）；selftest 拿到 listener 后探真实口起线程跑同一段
    accept 循环——被测的就是产线路径本体，不是影子副本。
    """
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    # SO_REUSEADDR：smoke 相位反复起停，上一连接的 TIME_WAIT 残留会让
    # bind 报 EADDRINUSE（client 侧 TIME_WAIT 不占 server bind 口，但
    # 复用同端口的连接组在 macOS 下有此残留面——开了无害，防御位）。
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((HOST, port))
    srv.listen(4)
    return srv


def _accept_loop(srv: socket.socket, announce: bool = True) -> None:
    """accept 主循环（serve 产线体，selftest 复用同一段）。

    announce=False 供 selftest 静默探口（无时刻输入、成功输出全定纪律
    ——临时口数字是时刻输入，不得进自检输出面）。
    """
    if announce:
        print(f"echod: listening on {HOST}:{srv.getsockname()[1]}", flush=True)
    while True:
        conn, _peer = srv.accept()
        with conn:
            # 回显即答不攒段：Nagle 对单写无影响，关掉防未来多段场景尾延迟。
            conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            while True:
                data = conn.recv(4096)
                if not data:  # client FIN → 被动关闭（close 发 FIN = LAST_ACK 侧）
                    break
                conn.sendall(data)


def serve(port: int) -> None:
    _accept_loop(serve_bind(port))


def selftest() -> int:
    """`--selftest`：进程内环回对拍（echod/evsclient 宿主链零守卫收口）。

    仿 mkfat/elf2flat --selftest 同位纪律（a-3 U4）：echod 此前只在
    smoke T-c 相被动覆盖（guest vnet TCP 自验对端），门禁自身坏（回显
    变形/半关断裂/bind 失败形态）= smoke 相静默劣化成 60s 超时排查。
    四面：
      ① 环回 echo 语义——随机长×随机段写、读回应恰等长逐字节；
      ② 半关闭路径——client shutdown(WR) 后 server 侧 recv 空 → 被动
        close（产线注释里的 LAST_ACK 侧契约），client 读到 EOF 非挂死
        （5s 帽防永久阻塞——selftest 红要 loud 不要僵）；
      ③ 并发连接——listen backlog 同口两条并发连接独立 echo（每连接
        独立 with conn 块——并发断裂面此前零守卫）；
      ④ 段化 delivery——单逻辑写拆多发 send，server recv(4096) 攒段
        回发必须无损（TCP 流语义钉板）。
    bind 临时口 0（内核分配闲置口）——绝不碰 18080-83 自托管互斥口
    （smoke.sh :337「echod/hostfwd 端口互斥」纪律），并发 CI runner
    与本地 smoke 会话零冲突。
    """
    import os

    srv = serve_bind(0)
    port = srv.getsockname()[1]
    # announce=False：临时口号是时刻输入，不进自检输出面（成功输出全定）。
    t = threading.Thread(target=_accept_loop, args=(srv, False), daemon=True)
    t.start()

    def one_conn(payload: bytes, chunks: int, half_close: bool) -> None:
        with socket.create_connection((HOST, port), timeout=5) as s:
            s.settimeout(5)
            if chunks <= 1:
                s.sendall(payload)
            else:  # ④ 段化：拆多发 send 钉流语义
                step = max(1, len(payload) // chunks)
                for off in range(0, len(payload), step):
                    s.sendall(payload[off : off + step])
            if half_close:  # ② client FIN → server recv 空 → 被动 close
                s.shutdown(socket.SHUT_WR)
            got = bytearray()
            while len(got) < len(payload):
                b = s.recv(4096)
                if not b:
                    raise AssertionError(
                        f"selftest: echo 短斤 EOF at {len(got)}/{len(payload)}"
                    )
                got += b
            assert bytes(got) == payload, (
                f"selftest: echo 变形 {len(got)}B 首差 "
                f"[{next(i for i in range(len(payload)) if got[i] != payload[i])}]"
            )
            if half_close:
                # ② 续：server 被动 close 后 client 再读必 EOF（非挂死）。
                extra = s.recv(4096)
                assert extra == b"", "selftest: 半关后应 EOF 却有数据"

    # ① 随机长×随机段（含 4096 边界两侧：recv 缓冲恰满/恰满+1）。
    lens = [1, 2, 4095, 4096, 4097, 65536]
    for n in lens:
        payload = os.urandom(n)
        one_conn(payload, chunks=1, half_close=False)
    # ④ 段化 delivery（64B/24B 与 evsclient PAYLOADS 同形——对拍链载荷
    # 形状钉板；smoke 相即这两条载荷过 echod）。
    one_conn(bytes((i * 7 + 3) & 0xFF for i in range(64)), chunks=8, half_close=False)
    one_conn(bytes((i * 11 + 5) & 0xFF for i in range(24)), chunks=5, half_close=False)
    # ② 半关闭：产线注释契约「client FIN → 被动关闭」。
    one_conn(os.urandom(3000), chunks=1, half_close=True)
    # ③ 并发两连接独立 echo。
    import concurrent.futures

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as ex:
        futs = [ex.submit(one_conn, os.urandom(5000 + 7 * k), 1, False) for k in range(2)]
        for f in futs:
            f.result(timeout=10)
    # 收尾：关 listener 前静默 accept 线程即将到来的 OSError（close 后
    # accept 抛 EBADF/OSError 是预期收尾，非测试红——但裸异常打 stderr
    # 会污染门禁输出；excepthook 单参 args 形，吞前留档）。
    def _swallow(args: threading.ExceptHookArgs) -> None:
        pass

    threading.excepthook = _swallow
    srv.close()
    t.join(timeout=2)
    print("echod: selftest ok (echo/半关/并发/段化 ×11)")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    port = int(sys.argv[1]) if len(sys.argv) > 1 else PORT
    serve(port)
