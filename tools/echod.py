#!/usr/bin/env python3
"""确定性 TCP 回显服务（`docs/TCP栈设计草案.md` §6 —— T-c 的 host 侧验收组件）。

- bind 127.0.0.1:18080（QEMU user 网：guest 主动 connect 10.0.2.2:18080
  由 SLIRP 转发到宿主本机同端口——vnet tcp.rs 自验的对端）。
- 收即原样回发（sendall 全量）；client 半关闭（FIN）后被动关闭本端
  （CLOSE_WAIT→LAST_ACK 侧——让 guest 恰好验**主动关闭**全路径）。
- 无时刻输入、输出全定（mkcpio/mkfat 同纪律）：仅启动一行，内容固定。
- 用法：python3 tools/echod.py [port]        # 默认 18080
  smoke/手动用法：后台起 + 记 PID + 相尾按 PID kill（多会话禁 pkill）。
"""
import socket
import sys

HOST = "127.0.0.1"
PORT = 18080


def serve(port: int) -> None:
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    # SO_REUSEADDR：smoke 相位反复起停，上一连接的 TIME_WAIT 残留会让
    # bind 报 EADDRINUSE（client 侧 TIME_WAIT 不占 server bind 口，但
    # 复用同端口的连接组在 macOS 下有此残留面——开了无害，防御位）。
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((HOST, port))
    srv.listen(4)
    print(f"echod: listening on {HOST}:{port}", flush=True)
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


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else PORT
    serve(port)
