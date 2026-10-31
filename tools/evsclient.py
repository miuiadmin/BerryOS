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
TOTAL_CAP_S = int(os.environ.get("BERRY_EVS_TOTAL_CAP_S", "480"))  # 总帽：guest boot→bsh EVS 实测 ~60s-5min（TCG 慢夜）+ 对拍余量
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


if __name__ == "__main__":
    sys.exit(main())
