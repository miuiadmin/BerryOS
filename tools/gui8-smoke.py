#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gui8-smoke —— GUI-8 段④「EFI GUI 交互相」双 boot 冒烟 rig。

为什么单独立脚本（不并回 smoke-image.sh）
----------------------------------------
冻结相 smoke-image 的断言面是「EFI 十段恰一次 + 裸相 27 + 生态 33 + 字节
对拍 + 金丝雀」，其 QEMU 拓扑**结构性无 virtio**（不挂 blk/tablet/QMP）。
交互相需要另一套拓扑（FAT12 数据盘 + virtio-tablet + QMP unix sock）和
另一套标记族（fs 端点让给 launcher 后 bsh 在 fs_selftest park，ECO 33 里
FS/U1/U2/U3/B1/U4/TAB/INT/POLL/MMP/CLK/SIG/PS/BC/THR/EXIT 均结构性缺席）。
因此本脚本 = 独立关卡，**零改 smoke-image.sh 行为**；`just gui8-smoke`
单口调用。

验收面（V2/V3/V4）
------------------
- Boot A：AAVMF + out/berryos.img + 独立 FAT12 数据盘（LBA0 印章
  `BERRYOSVBLK!` + 根目录 `GUI8.SMK`），display fw_cfg 800x600@60，
  virtio-blk + virtio-tablet + QMP。
  - 等 `LAUNCH ok` / `PAGE ok n=2` / `APP ok n=2`；
  - 「launcher: click ready」在席则用之，缺席回退 PAGE + settle 秒；
  - QMP 三批注入（abs → btn down → btn up）单击 APPDEMO 格中心；
  - 断言 `CLICK ok n=1` + `LAYOUTW ok`；宿主侧再经 FAT12 读回
    `LAYOUT.JSN` 对拍格数。
- Boot B：同一镜像、同一数据盘（不重造），fresh vars/QMP。
  - 断言 `LAYOUT ok n=<N>` + `LAUNCH ok`；
  - 断言零 `LAYOUTW ok`、零 `CLICK ok`（写路径一次性状态门）；
  - 宿主侧 sha256 前后相等 = 数据盘零重写。

**依赖（段④ 落地顺序）**：本脚本消费 launcher 段④ 标记
`CLICK ok n=1` / `LAYOUTW ok` / `LAYOUT ok n=`。基线 014623d 的 launcher
只有 LAUNCH/PAGE，三个新标记尚未合并；此时全跑会在等待阶段以**显式
缺标记** FAIL（不是静默跳过）。合并前 plumbing 自检：

    python3 tools/gui8-smoke.py --self-test
    python3 tools/gui8-smoke.py --dry-run [--image out/berryos.img]

标记 canon（per-process once；监督重启可合法重打）：一律存在性断言，
不升格「恰一次」——`CLICK ok n=1` 用负向前瞻防 `n=10` 子串假阳性。

macOS 纪律：serial 落文件（不依赖 terminal）、QMP 只用 unix socket
（零 ambient TCP 端口）、每 boot fresh blank vars.fd、SIGTERM/QMP quit
双路收尾、无 Docker/UTM 依赖。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# —— 被断言的 launcher 标记（段④ 合并后由 launcher println 单行打出）——
MARK_BERRYBOOT = "BERRYBOOT ok"
MARK_ROOTFS = "ROOTFS ok src=handoff"
MARK_APP = "APP ok n="
MARK_LAUNCH = "LAUNCH ok"
MARK_PAGE = "PAGE ok n=2"
MARK_CLICK = "CLICK ok n=1"
MARK_CLICK_ANY = "CLICK ok n="
MARK_LAYOUTW = "LAYOUTW ok"
MARK_LAYOUT = "LAYOUT ok n="
MARK_CLICK_READY = "launcher: click ready"
MARK_INPUT_ARMED = "launcher: input poll armed"
# 负向前瞻：`n=10` 不得命中 `n=1`（CLICK 恰一发的期望形态）。
RE_CLICK_N1 = r"CLICK ok n=1(?![0-9])"

# —— 数据盘约定（与 tools/smoke.sh / just desktop 同源）——
DISK_SEAL = b"BERRYOSVBLK!"
SMK_NAME11 = b"GUI8    SMK"
SMK_CONTENT = b"BERRYOS GUI8 INTERACTIVE SMOKE MARKER v1\n"
LAYOUT_NAME11 = b"LAYOUT  JSN"
RESERVED_ZONE = 16

# —— 桌面几何（launcher desktop.rs 常量镜像；仅用于算 cell(0,0) 中心，
#    屏幕宽高从串口 LAUNCH/FB 行动态解析——裁决 D：不把 800x600 写死）——
STATUS_H = 44
DOT_H = 8
BOTTOM_PAD = 8
GRID_COLS = 4
GRID_ROWS = 3
GRID_GAP = 12

DEFAULT_CODE = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
DEFAULT_VARS = "/opt/homebrew/share/qemu/edk2-arm-vars.fd"


def log(msg: str) -> None:
    print(f"gui8-smoke: {msg}", flush=True)


def warn(msg: str) -> None:
    print(f"gui8-smoke: WARN: {msg}", file=sys.stderr, flush=True)


class SmokeError(RuntimeError):
    """可预期失败（带可选日志路径，main 统一打印尾部）。"""

    def __init__(self, msg: str, log_path: Path | None = None):
        super().__init__(msg)
        self.log_path = Path(log_path) if log_path else None


class MissingMarkers(SmokeError):
    def __init__(self, phase: str, what: str, timeout: float, log_path: Path):
        super().__init__(
            f"{phase}: {timeout:.0f}s 内未等到 {what}", log_path
        )
        self.phase = phase
        self.what = what
        self.timeout = timeout


def tail_text(path: Path, n: int = 60) -> str:
    try:
        data = path.read_bytes()
    except OSError:
        return ""
    # 只取尾部 ~64KiB，避免大日志全量解码。
    chunk = data[-65536:]
    lines = chunk.decode("utf-8", "replace").splitlines()
    out = []
    for line in lines[-n:]:
        # 稳态矩阵是无换行长行——截断免得 fail 输出淹没诊断。
        out.append(line if len(line) <= 400 else line[:400] + " …<truncated>")
    return "\n".join(out)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for blk in iter(lambda: f.read(1 << 20), b""):
            h.update(blk)
    return h.hexdigest()


# =====================================================================
# 串口日志观察器
# =====================================================================


class LogWatcher:
    """增量读串口文件（QEMU stdout 重定向）——状态谓词轮询，非竞速。"""

    def __init__(self, path: Path):
        self.path = Path(path)
        self.text = ""
        self.pos = 0

    def poll(self) -> None:
        if not self.path.exists():
            return
        try:
            with self.path.open("rb") as f:
                f.seek(self.pos)
                chunk = f.read()
                self.pos = f.tell()
        except OSError:
            return
        if chunk:
            self.text += chunk.decode("utf-8", "replace")

    def count(self, marker: str) -> int:
        self.poll()
        return self.text.count(marker)

    def count_re(self, pattern: str) -> int:
        self.poll()
        return len(re.findall(pattern, self.text))

    def has(self, marker: str) -> bool:
        return self.count(marker) > 0

    def wait_until(
        self,
        proc: "subprocess.Popen[bytes]",
        predicate,
        description: str,
        timeout: float,
        phase: str,
    ) -> None:
        deadline = time.monotonic() + timeout
        started = time.monotonic()
        last_report = 0.0
        while True:
            self.poll()
            if predicate(self.text):
                log(f"{phase}: 已就位（{description}）")
                return
            rc = proc.poll()
            if rc is not None:
                raise SmokeError(
                    f"{phase}: QEMU 提前退出 rc={rc}（等待 {description}）",
                    self.path,
                )
            now = time.monotonic()
            if now >= deadline:
                raise MissingMarkers(phase, description, timeout, self.path)
            if now - last_report >= 15.0:
                log(
                    f"{phase}: {int(now - started)}s/{timeout:.0f}s 等待 {description}"
                )
                last_report = now
            time.sleep(0.25)

    def wait_markers(
        self,
        proc: "subprocess.Popen[bytes]",
        markers,
        timeout: float,
        phase: str,
    ) -> None:
        markers = list(markers)
        self.wait_until(
            proc,
            lambda t: all(m in t for m in markers),
            ", ".join(markers),
            timeout,
            phase,
        )

    def wait_regex(
        self,
        proc: "subprocess.Popen[bytes]",
        pattern: str,
        timeout: float,
        phase: str,
    ) -> None:
        self.wait_until(
            proc,
            lambda t: re.search(pattern, t) is not None,
            f"正则 /{pattern}/",
            timeout,
            phase,
        )

    def wait_any(
        self,
        proc: "subprocess.Popen[bytes]",
        markers,
        timeout: float,
        phase: str,
    ) -> bool:
        """任一标记出现即 True；超时 False（调用方决定是否回退）。"""
        markers = list(markers)
        deadline = time.monotonic() + timeout
        while True:
            self.poll()
            if any(m in self.text for m in markers):
                return True
            rc = proc.poll()
            if rc is not None:
                raise SmokeError(
                    f"{phase}: QEMU 提前退出 rc={rc}（等待任一 {markers}）",
                    self.path,
                )
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.25)


# =====================================================================
# QMP 客户端（unix socket；不占任何 ambient TCP 端口）
# =====================================================================


class QmpError(SmokeError):
    pass


class QmpClient:
    def __init__(self, path: Path, timeout: float = 15.0):
        self.path = str(path)
        self.sock: socket.socket | None = None
        self.f = None
        # 不传 device：QMP 的 device= 是 QemuConsole 查找键（不是输入设备
        # 选择器）；本拓扑只有 unbound 的 virtio-tablet，广播即它。
        self.device = None
        last: Exception | None = None
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.settimeout(10.0)
            try:
                s.connect(self.path)
                self.sock = s
                break
            except (FileNotFoundError, ConnectionRefusedError, OSError) as e:
                last = e
                try:
                    s.close()
                except OSError:
                    pass
                time.sleep(0.1)
        if self.sock is None:
            raise QmpError(f"QMP 连接失败（{self.path}）：{last}")
        self.f = self.sock.makefile("rw", encoding="utf-8", newline="\n")
        greeting = self._read_json()
        if "QMP" not in greeting:
            raise QmpError(f"QMP greeting 缺失：{greeting!r}")
        self.execute("qmp_capabilities")

    def _read_json(self) -> dict:
        assert self.f is not None
        line = self.f.readline()
        if not line:
            raise QmpError("QMP EOF（QEMU 已退出？）")
        try:
            return json.loads(line)
        except json.JSONDecodeError as e:
            raise QmpError(f"QMP 非 JSON 行：{line!r}（{e}）") from e

    def execute(self, cmd: str, arguments: dict | None = None):
        assert self.f is not None
        req: dict = {"execute": cmd}
        if arguments is not None:
            req["arguments"] = arguments
        self.f.write(json.dumps(req) + "\n")
        self.f.flush()
        while True:
            msg = self._read_json()
            if "event" in msg:
                continue  # 异步事件静默跳过
            if "error" in msg:
                err = msg["error"] or {}
                desc = err.get("desc", json.dumps(err))
                raise QmpError(f"QMP {cmd} 被拒：{desc}")
            return msg.get("return")

    def input_send(self, events):
        # 省 device 键 = 广播到 unbound handler 链（QEMU ui/input.c:
        # qemu_input_find_handler 第二圈）。device=tab0 会走
        # qemu_console_lookup_by_device_name（console 查找），在 -display none
        # 的 qemu-fixed-text-console 上触发 QEMU 11.0.3 的 QOM 崩溃——勿用。
        return self.execute("input-send-event", {"events": events})

    def click(self, x: float, y: float, w: int, h: int) -> None:
        """三批单击：①abs 定位 ②btn down（带坐标）③btn up。"""
        ax = max(0, min(32767, int(round(x * 32768.0 / max(1, w)))))
        ay = max(0, min(32767, int(round(y * 32768.0 / max(1, h)))))
        batches = [
            [
                {"type": "abs", "data": {"axis": "x", "value": ax}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
            ],
            [
                {"type": "btn", "data": {"down": True, "button": "left"}},
                {"type": "abs", "data": {"axis": "x", "value": ax}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
            ],
            [{"type": "btn", "data": {"down": False, "button": "left"}}],
        ]
        for batch in batches:
            self.input_send(batch)
            time.sleep(0.25)  # 让 guest 输入泵逐批消费（非时序断言）

    def quit(self) -> None:
        try:
            self.execute("quit")
        except (QmpError, OSError):
            pass
        self.close()

    def close(self) -> None:
        try:
            if self.f is not None:
                self.f.close()
        except OSError:
            pass
        try:
            if self.sock is not None:
                self.sock.close()
        except OSError:
            pass
        self.f = None
        self.sock = None


# =====================================================================
# FAT12 数据盘（mkfat 产物 + 根目录注入 GUI8.SMK / 读回 LAYOUT.JSN）
# =====================================================================


class Fat12Volume:
    """最小 FAT12 读写器——只服务本 rig 的根目录单层文件。

    几何从 BPB 动态解析（bps/spc/rsvd/nfats/root_entries/fat_sz），不硬编
    mkfat 的 4 扇区簇。卷起点 = 保留区 RESERVED_ZONE 扇区（mkfat 默认盘
    布局；BPB hidden 同时记录 16，这里以印章布局为准）。
    """

    def __init__(self, path: Path, reserved_zone: int = RESERVED_ZONE):
        self.path = Path(path)
        self.data = bytearray(self.path.read_bytes())
        self.rz = reserved_zone
        self.vol_off = self.rz * 512
        if len(self.data) < self.vol_off + 512:
            raise SmokeError(f"数据盘过短：{self.path}")
        v = self.data[self.vol_off :]
        if v[510:512] != b"\x55\xaa":
            raise SmokeError(f"卷 boot 签名缺失（{self.path}）")
        self.bps = struct.unpack_from("<H", v, 11)[0]
        self.spc = v[13]
        self.reserved = struct.unpack_from("<H", v, 14)[0]
        self.nfats = v[16]
        self.root_entries = struct.unpack_from("<H", v, 17)[0]
        self.total16 = struct.unpack_from("<H", v, 19)[0]
        self.fat_sz = struct.unpack_from("<H", v, 22)[0]
        self.total32 = struct.unpack_from("<I", v, 32)[0]
        self.total = self.total16 or self.total32
        if not (self.bps and self.spc and self.reserved and self.nfats and self.fat_sz):
            raise SmokeError(f"BPB 几何非法（{self.path}）")
        self.root_secs = (self.root_entries * 32 + self.bps - 1) // self.bps
        self.fat1_off = self.vol_off + self.reserved * self.bps
        self.root_off = self.vol_off + (
            self.reserved + self.nfats * self.fat_sz
        ) * self.bps
        self.data_start = self.reserved + self.nfats * self.fat_sz + self.root_secs
        self.clusters = (self.total - self.data_start) // self.spc
        if self.clusters <= 0:
            raise SmokeError(f"卷簇数为 0（{self.path}）")
        self.cluster_bytes = self.spc * self.bps
        fat_len = self.fat_sz * self.bps
        self.fat = bytearray(self.data[self.fat1_off : self.fat1_off + fat_len])

    # —— FAT12 12 位紧包（与 tools/mkfat.py fat12_set 同编码）——
    def _fat_get(self, n: int) -> int:
        off = n * 3 // 2
        if n % 2 == 0:
            return self.fat[off] | ((self.fat[off + 1] & 0x0F) << 8)
        return (self.fat[off] >> 4) | (self.fat[off + 1] << 4)

    def _fat_set(self, n: int, val: int) -> None:
        off = n * 3 // 2
        if n % 2 == 0:
            self.fat[off] = val & 0xFF
            self.fat[off + 1] = (self.fat[off + 1] & 0xF0) | ((val >> 8) & 0x0F)
        else:
            self.fat[off] = (self.fat[off] & 0x0F) | ((val & 0x0F) << 4)
            self.fat[off + 1] = (val >> 4) & 0xFF

    def _cluster_off(self, clus: int) -> int:
        return self.vol_off + (self.data_start + (clus - 2) * self.spc) * self.bps

    def _commit_fats(self) -> None:
        fat_len = len(self.fat)
        for i in range(self.nfats):
            off = self.fat1_off + i * self.fat_sz * self.bps
            self.data[off : off + fat_len] = self.fat

    def _write(self) -> None:
        self._commit_fats()
        self.path.write_bytes(self.data)

    def _iter_root(self):
        for i in range(self.root_entries):
            off = self.root_off + i * 32
            entry = self.data[off : off + 32]
            if entry[0] == 0x00:
                break
            yield off, entry

    def _find_root(self, name11: bytes):
        for off, entry in self._iter_root():
            if entry[0] == 0xE5:
                continue
            if entry[11] == 0x08:  # 卷标
                continue
            if bytes(entry[0:11]) == name11:
                first = struct.unpack_from("<H", entry, 26)[0]
                size = struct.unpack_from("<I", entry, 28)[0]
                return off, entry, first, size
        return None

    def list_root(self):
        out = []
        for _, entry in self._iter_root():
            if entry[0] == 0xE5 or entry[11] == 0x08:
                continue
            out.append(bytes(entry[0:11]).decode("ascii", "replace"))
        return out

    def read_file(self, name11: bytes) -> bytes | None:
        found = self._find_root(name11)
        if found is None:
            return None
        _, _, first, size = found
        if size == 0:
            return b""
        chunks = []
        clus = first
        guard = 0
        while 2 <= clus < 0xFF8 and guard < 100000:
            off = self._cluster_off(clus)
            chunks.append(bytes(self.data[off : off + self.cluster_bytes]))
            nxt = self._fat_get(clus)
            if nxt == 0 or nxt >= 0xFF8:
                break
            if nxt == clus:
                raise SmokeError(f"FAT12 自环 clus={clus}（{self.path}）")
            clus = nxt
            guard += 1
        blob = b"".join(chunks)
        if len(blob) < size:
            raise SmokeError(
                f"FAT12 文件链过短 {name11!r}: {len(blob)}<{size}（{self.path}）"
            )
        return blob[:size]

    def _alloc_chain(self, nbytes: int):
        nclus = max(1, (nbytes + self.cluster_bytes - 1) // self.cluster_bytes)
        free = [
            n for n in range(2, self.clusters + 2) if self._fat_get(n) == 0
        ]
        if len(free) < nclus:
            raise SmokeError(
                f"FAT12 空闲簇不足：需 {nclus}，余 {len(free)}（{self.path}）"
            )
        chain = free[:nclus]
        for i, c in enumerate(chain[:-1]):
            self._fat_set(c, chain[i + 1])
        self._fat_set(chain[-1], 0xFFF)
        return chain

    def add_file(self, name11: bytes, content: bytes) -> None:
        if len(name11) != 11:
            raise SmokeError(f"8.3 名长度非 11：{name11!r}")
        if self._find_root(name11) is not None:
            raise SmokeError(f"FAT12 文件已存在：{name11!r}")
        slot = None
        for i in range(self.root_entries):
            off = self.root_off + i * 32
            if self.data[off] in (0x00, 0xE5):
                slot = off
                break
        if slot is None:
            raise SmokeError(f"FAT12 根目录已满（{self.path}）")
        chain = self._alloc_chain(len(content))
        padded = content + b"\x00" * (len(chain) * self.cluster_bytes - len(content))
        for i, clus in enumerate(chain):
            off = self._cluster_off(clus)
            self.data[off : off + self.cluster_bytes] = padded[
                i * self.cluster_bytes : (i + 1) * self.cluster_bytes
            ]
        entry = bytearray(32)
        entry[0:11] = name11
        entry[11] = 0x20  # archive
        struct.pack_into("<H", entry, 22, 0x0000)  # time
        struct.pack_into("<H", entry, 24, 0x5021)  # date 2020-01-01
        struct.pack_into("<H", entry, 26, chain[0])
        struct.pack_into("<I", entry, 28, len(content))
        self.data[slot : slot + 32] = entry
        self._write()


def create_data_disk(path: Path) -> None:
    """mkfat 默认 FAT12 盘 + LBA0 印章 + 根目录 GUI8.SMK。"""
    path = Path(path)
    if path.exists():
        raise SmokeError(f"数据盘已存在（拒绝覆盖）：{path}")
    mkfat = ROOT / "tools" / "mkfat.py"
    if not mkfat.is_file():
        raise SmokeError(f"缺 mkfat：{mkfat}")
    p = subprocess.run(
        [sys.executable, str(mkfat), str(path)],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
    )
    if p.returncode != 0:
        raise SmokeError(
            f"mkfat 失败 rc={p.returncode}：{p.stderr.strip() or p.stdout.strip()}"
        )
    with path.open("r+b") as f:
        f.seek(0)
        f.write(DISK_SEAL)
    vol = Fat12Volume(path)
    vol.add_file(SMK_NAME11, SMK_CONTENT)
    verify_data_disk(path)


def verify_data_disk(path: Path) -> None:
    path = Path(path)
    if not path.is_file():
        raise SmokeError(f"数据盘缺席：{path}")
    with path.open("rb") as f:
        seal = f.read(len(DISK_SEAL))
    if seal != DISK_SEAL:
        raise SmokeError(f"数据盘 LBA0 印章缺失/不符：{seal!r}（{path}）")
    vol = Fat12Volume(path)
    got = vol.read_file(SMK_NAME11)
    if got != SMK_CONTENT:
        raise SmokeError(
            f"GUI8.SMK 读回不符：{got!r} != {SMK_CONTENT!r}（{path}）"
        )


def read_layout_cells(path: Path) -> list | None:
    vol = Fat12Volume(path)
    blob = vol.read_file(LAYOUT_NAME11)
    if blob is None:
        return None
    try:
        obj = json.loads(blob.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise SmokeError(f"LAYOUT.JSN 非法 JSON：{e}（{path}）") from e
    cells = obj.get("cells") if isinstance(obj, dict) else None
    if not isinstance(cells, list):
        raise SmokeError(f"LAYOUT.JSN 缺 cells 数组（{path}）")
    return cells


def write_layout_cells_for_selftest(path: Path) -> int:
    """自检用：注入一份 2 格 canonical 布局并读回。"""
    vol = Fat12Volume(path)
    payload = (
        b'{"schema":1,"cells":[{"app":"APPDEMO","page":0,"row":0,"col":0},'
        b'{"app":"APPQUIET","page":3,"row":0,"col":1}]}'
    )
    vol.add_file(LAYOUT_NAME11, payload)
    cells = read_layout_cells(path)
    if cells is None:
        raise SmokeError("自检 LAYOUT.JSN 读回 None")
    return len(cells)


# =====================================================================
# 几何 / QEMU 命令 / Boot 生命周期
# =====================================================================


def cell0_center(w: int, h: int) -> tuple[int, int]:
    """cell(0,0) 中心（status 栏之下，4×3 网格第一格）。"""
    grid_h = max(0, h - (STATUS_H + DOT_H + BOTTOM_PAD))
    cell_w = max(1, (w - (GRID_COLS - 1) * GRID_GAP) // GRID_COLS)
    cell_h = max(1, (grid_h - (GRID_ROWS - 1) * GRID_GAP) // GRID_ROWS)
    return cell_w // 2, STATUS_H + cell_h // 2


RE_LAUNCH_GEOM = re.compile(r"LAUNCH v=\d+ wmax=\d+ (\d+)x(\d+)")
RE_FB_GEOM = re.compile(r"FB ok (\d+)x(\d+)")


def geometry_from_log(watcher: LogWatcher) -> tuple[int, int, str]:
    watcher.poll()
    for rx, name in ((RE_LAUNCH_GEOM, "LAUNCH"), (RE_FB_GEOM, "FB")):
        m = rx.search(watcher.text)
        if m:
            return int(m.group(1)), int(m.group(2)), m.group(0)
    return 800, 600, "fallback 800x600"


def qemu_command(
    args, disk: Path, vars_path: Path, sock_path: Path, serial_path: Path
) -> list[str]:
    cmd = [
        args.qemu,
        "-machine",
        "virt",
        "-cpu",
        "cortex-a72",
        "-m",
        "256M",
        # 与生产 berry 宿主同形（qemu.rs machine_args）：-display none +
        # -monitor none + -serial file。**QMP input-send-event 必须省 device
        # 键**：device= 走 QemuConsole 查找（不是输入设备选择器），在
        # -display none 的 qemu-fixed-text-console 上触发 QEMU 11.0.3 QOM
        # 崩溃（2026-11-05 实抓）；省键 = 广播到 unbound handler 链。
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        f"file:{serial_path}",
        "-no-reboot",
        # code 只读（`-pflash` 会以读写打开；sandbox/只读介质下用
        # if=pflash,readonly=on 同款语义且免 "format not specified" 警告）。
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={args.code}",
        "-drive",
        f"if=pflash,format=raw,file={vars_path}",
        # 系统盘只读快照：绝不让 smoke 改 out/berryos.img。
        "-drive",
        f"if=none,file={args.image},format=raw,id=bb,snapshot=on",
        "-device",
        "qemu-xhci",
        "-device",
        "usb-storage,drive=bb",
        # inputd 只认 VERSION_1（GUI-4 判例：virtio-mmio 默认 legacy）。
        "-global",
        "virtio-mmio.force-legacy=false",
        "-drive",
        f"if=none,id=drv0,format=raw,file={disk},cache.writeback=off",
        "-device",
        "virtio-blk-device,drive=drv0",
        "-device",
        "virtio-tablet-device,id=tab0",
        "-device",
        "ramfb",
        "-fw_cfg",
        "name=opt/berry/display,string=800x600@60",
        "-qmp",
        f"unix:{sock_path},server=on,wait=off",
    ]
    # 调试开关（GUI8_QEMU_TRACE=1）：QEMU 输入子系统 trace 落 run 目录。
    if os.environ.get("GUI8_QEMU_TRACE"):
        cmd += [
            "-trace",
            "enable=qmp_enter_input_send_event",
            "-trace",
            "enable=input_event_abs",
            "-trace",
            "enable=input_event_btn",
            "-trace",
            "enable=input_event_sync",
            "-trace",
            "enable=virtio_input_queue_full",
            "-D",
            str(sock_path.parent / f"trace-{sock_path.stem}.log"),
        ]
    return cmd


class Boot:
    """一次 QEMU boot 的 serial/QMP/生命周期封装。"""

    def __init__(self, phase: str, args, run_dir: Path, disk: Path, suffix: str):
        self.phase = phase
        self.args = args
        self.run_dir = run_dir
        self.disk = disk
        self.suffix = suffix
        self.vars_path = run_dir / f"vars-{suffix}.fd"
        self.sock_path = run_dir / f"qmp-{suffix}.sock"
        self.log_path = run_dir / f"boot-{suffix}.log"
        self.cmd_path = run_dir / f"boot-{suffix}.cmd.txt"
        self.log = LogWatcher(self.log_path)
        self.proc: subprocess.Popen[bytes] | None = None
        self.fh = None
        self.qmp: QmpClient | None = None

    def start(self) -> None:
        # fresh blank vars.fd（64MiB 全 0xFF）——不继承上一轮 BootOrder。
        self.vars_path.write_bytes(b"\xff" * (64 << 20))
        if self.sock_path.exists():
            self.sock_path.unlink()
        cmd = qemu_command(
            self.args, self.disk, self.vars_path, self.sock_path, self.log_path
        )
        self.cmd_path.write_text(shlex.join(cmd) + "\n", encoding="utf-8")
        # 串口走 -serial file；QEMU 自身诊断另落 .qemu.log（不污染 guest 流）。
        self.qemu_log_path = self.run_dir / f"boot-{self.suffix}.qemu.log"
        self.fh = self.qemu_log_path.open("wb")
        log(
            f"boot {self.phase}: QEMU 启动（serial→{self.log_path.name}，"
            f"QMP→{self.sock_path.name}，m=256M，display none）"
        )
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=self.fh,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    def wait_markers(self, markers, timeout: float) -> None:
        assert self.proc is not None
        self.log.wait_markers(self.proc, markers, timeout, f"boot {self.phase}")

    def wait_regex(self, pattern: str, timeout: float) -> None:
        assert self.proc is not None
        self.log.wait_regex(self.proc, pattern, timeout, f"boot {self.phase}")

    def wait_any(self, markers, timeout: float) -> bool:
        assert self.proc is not None
        return self.log.wait_any(
            self.proc, markers, timeout, f"boot {self.phase}"
        )

    def count(self, marker: str) -> int:
        return self.log.count(marker)

    def count_re(self, pattern: str) -> int:
        return self.log.count_re(pattern)

    def qmp_connect(self) -> None:
        self.qmp = QmpClient(self.sock_path)

    def stop(self) -> None:
        quit_sent = False
        if self.qmp is not None:
            self.qmp.quit()
            self.qmp = None
            quit_sent = True
        if self.proc is not None:
            # 早失败（从未连上 QMP）不必干等 15s——直接 SIGTERM。
            if not quit_sent and self.proc.poll() is None:
                self.proc.terminate()
            try:
                self.proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                warn(f"boot {self.phase}: QMP quit 后 15s 未退 → SIGTERM")
                try:
                    self.proc.terminate()
                    self.proc.wait(timeout=10)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    warn(f"boot {self.phase}: SIGTERM 未退 → SIGKILL")
                    try:
                        self.proc.kill()
                        self.proc.wait(timeout=10)
                    except (ProcessLookupError, subprocess.TimeoutExpired):
                        pass
            self.proc = None
        if self.fh is not None:
            try:
                self.fh.close()
            except OSError:
                pass
            self.fh = None


# =====================================================================
# 预检 / 依赖探测
# =====================================================================


def launcher_marker_status() -> dict[str, bool | None]:
    """读 launcher 源码探测段④ 标记是否已合并（None = 源不在工作区）。"""
    src = ROOT / "user" / "bins" / "berry-launcher" / "src"
    checks = {
        "CLICK": (src / "frameloop.rs", "CLICK ok n="),
        "LAYOUTW": (src / "layout_io.rs", "LAYOUTW ok"),
        "LAYOUT": (src / "layout_io.rs", "LAYOUT ok n="),
    }
    out: dict[str, bool | None] = {}
    for key, (path, needle) in checks.items():
        try:
            out[key] = needle in path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            out[key] = None
    return out


def print_marker_status() -> bool:
    status = launcher_marker_status()
    text = " ".join(
        f"{k}={'yes' if v else ('missing' if v is False else 'n/a')}"
        for k, v in status.items()
    )
    log(f"launcher 段④ 标记依赖：{text}")
    complete = all(v is True for v in status.values())
    if not complete:
        warn(
            "launcher 段④ 标记（CLICK/LAYOUTW/LAYOUT）尚未合并——"
            "全跑会在等待阶段以显式缺标记 FAIL；plumbing 自检用 "
            "--self-test / --dry-run"
        )
    return complete


def require_file(path: Path, what: str) -> None:
    if not Path(path).is_file():
        raise SmokeError(f"{what} 不存在：{path}")


def preflight(args) -> None:
    if not shutil.which(args.qemu):
        raise SmokeError(f"找不到 QEMU：{args.qemu}（PATH 或 --qemu）")
    require_file(args.image, "镜像（先 just image）")
    require_file(args.code, "AAVMF CODE 固件")
    # vars 模板仅接口占位（每 boot 自产 fresh blank），缺席只警告不拦。
    if args.vars and not Path(args.vars).is_file():
        warn(f"AAVMF VARS 模板不存在（{args.vars}）——不消费，忽略")
    require_file(ROOT / "tools" / "mkfat.py", "mkfat.py")


# =====================================================================
# Boot A / Boot B 主流程
# =====================================================================


def run_dual_boot(args) -> int:
    preflight(args)
    print_marker_status()
    if args.require_markers:
        status = launcher_marker_status()
        missing = [k for k, v in status.items() if v is not True]
        if missing:
            raise SmokeError(
                f"--require-markers：launcher 段④ 标记缺席 {missing}"
                "（合并前请用 --self-test/--dry-run 验 plumbing）"
            )

    run_dir = Path(args.work_dir) if args.work_dir else Path(
        tempfile.mkdtemp(prefix="gui8-smoke.", dir=args.tmp_dir)
    )
    run_dir.mkdir(parents=True, exist_ok=True)
    log(f"run dir：{run_dir}")

    disk = Path(args.data_disk) if args.data_disk else run_dir / "layout.img"
    if disk.exists():
        if not args.reuse_data_disk:
            raise SmokeError(f"数据盘已存在：{disk}（覆盖请删或 --reuse-data-disk）")
        verify_data_disk(disk)
        log(f"复用数据盘：{disk}（sha256={sha256_file(disk)[:16]}…）")
    else:
        disk.parent.mkdir(parents=True, exist_ok=True)
        create_data_disk(disk)
        log(
            f"新数据盘：{disk}（{disk.stat().st_size}B，"
            f"sha256={sha256_file(disk)[:16]}…，LBA0 印章 + GUI8.SMK）"
        )

    boot_a: Boot | None = None
    boot_b: Boot | None = None
    success = False
    try:
        # ---------------- Boot A ----------------
        boot_a = Boot("A", args, run_dir, disk, "a")
        boot_a.start()
        boot_a.wait_markers(
            [
                MARK_BERRYBOOT,
                MARK_ROOTFS,
                MARK_APP,
                MARK_LAUNCH,
                MARK_PAGE,
            ],
            args.boot_a_timeout,
        )
        # launcher 已挂 INPUT_POLL（请求已入 inputd 队列）。这是状态谓词，
        # 不保证 inputd 已 harvest——点击后的 CLICK 等待窗兜住后者。
        if not boot_a.wait_any([MARK_INPUT_ARMED], 60.0):
            warn(
                f"未等到「{MARK_INPUT_ARMED}」——QMP 事件可能早于 guest "
                "挂缓冲而被设备丢弃；仍继续"
            )
        # click-ready（段④ 新标记）在席 = snap-to-0 已收敛；缺席回退 settle。
        if not boot_a.wait_any([MARK_CLICK_READY], 25.0):
            warn(
                f"未等到「{MARK_CLICK_READY}」——回退 PAGE 后静置 "
                f"{args.settle_seconds:.1f}s（snap-to-0 收敛）"
            )
            time.sleep(args.settle_seconds)

        w, h, src = geometry_from_log(boot_a.log)
        cx, cy = cell0_center(w, h)
        log(f"boot A: display {w}x{h}（{src}）→ cell(0,0) 中心 ({cx},{cy})")

        boot_a.qmp_connect()
        log("boot A: QMP 注入单击（abs → btn down → btn up）")
        boot_a.qmp.click(cx, cy, w, h)

        # CLICK / LAYOUTW 同一等待窗：两者都是「点击后应已成立」的状态。
        try:
            boot_a.log.wait_until(
                boot_a.proc,
                lambda t: re.search(RE_CLICK_N1, t) is not None
                and MARK_LAYOUTW in t,
                f"{MARK_CLICK} + {MARK_LAYOUTW}",
                args.click_timeout,
                "boot A click",
            )
        except MissingMarkers:
            # 逐标记列出真缺项（合并前就是这两个的缺席证据）。
            miss = []
            if re.search(RE_CLICK_N1, boot_a.log.text) is None:
                miss.append(MARK_CLICK)
            if MARK_LAYOUTW not in boot_a.log.text:
                miss.append(MARK_LAYOUTW)
            raise MissingMarkers(
                "boot A click",
                ", ".join(miss) or f"{MARK_CLICK} + {MARK_LAYOUTW}",
                args.click_timeout,
                boot_a.log_path,
            ) from None
        n_click = boot_a.count_re(RE_CLICK_N1)
        n_layoutw = boot_a.count(MARK_LAYOUTW)
        if n_click == 0:
            raise SmokeError(
                f"boot A: 缺 {MARK_CLICK}（QMP 点击未命中 APPDEMO 格？）",
                boot_a.log_path,
            )
        if n_layoutw == 0:
            raise SmokeError(
                f"boot A: 缺 {MARK_LAYOUTW}（布局变体未落盘/回读不自洽）",
                boot_a.log_path,
            )
        if n_click > 1:
            warn(
                f"boot A: {MARK_CLICK} 出现 {n_click} 次（监督重启合法重打；"
                "canon 为存在性）"
            )
        log(f"boot A: CLICK×{n_click} LAYOUTW×{n_layoutw}")

        boot_a.stop()
        boot_a = None
        verify_data_disk(disk)
        cells_a = read_layout_cells(disk)
        if cells_a is None:
            raise SmokeError(
                "boot A: LAYOUT.JSN 未落盘（LAYOUTW 已打但宿主读回缺席）"
            )
        if args.expected_cells and len(cells_a) != args.expected_cells:
            raise SmokeError(
                f"boot A: LAYOUT.JSN 格数 {len(cells_a)} != "
                f"--expected-cells {args.expected_cells}"
            )
        log(f"boot A: LAYOUT.JSN 宿主读回 {len(cells_a)} 格（与回读自洽）")

        if args.only_a:
            success = True
            log(
                "PASS（--only-a）：Boot A 单击 + LAYOUTW + 落盘读回；"
                "Boot B 未跑"
            )
            return 0

        # ---------------- Boot B ----------------
        hash_before_b = sha256_file(disk)
        boot_b = Boot("B", args, run_dir, disk, "b")
        boot_b.start()
        layout_pat = rf"LAYOUT ok n={args.expected_cells}(?![0-9])"
        boot_b.wait_markers(
            [MARK_BERRYBOOT, MARK_ROOTFS, MARK_APP, MARK_LAUNCH, MARK_PAGE],
            args.boot_b_timeout,
        )
        boot_b.wait_regex(layout_pat, args.boot_b_timeout)
        n_layout = boot_b.count_re(layout_pat)
        n_layoutw_b = boot_b.count(MARK_LAYOUTW)
        n_click_b = boot_b.count(MARK_CLICK_ANY)
        if n_layoutw_b != 0:
            raise SmokeError(
                f"boot B: 出现 {n_layoutw_b} 次 {MARK_LAYOUTW}——"
                "已有布局不应再走写路径（一次性状态门破）",
                boot_b.log_path,
            )
        if n_click_b != 0:
            raise SmokeError(
                f"boot B: 出现 {n_click_b} 次 {MARK_CLICK_ANY}——"
                "Boot B 未注入点击（自动点击面破）",
                boot_b.log_path,
            )
        log(
            f"boot B: LAYOUT ok n={args.expected_cells}×{n_layout}、"
            "LAYOUTW×0、CLICK×0"
        )

        boot_b.stop()
        boot_b = None
        hash_after_b = sha256_file(disk)
        if hash_before_b != hash_after_b:
            raise SmokeError(
                "boot B: 数据盘字节发生变化——跨 boot 读路径不应重写 "
                f"(before={hash_before_b[:16]}… after={hash_after_b[:16]}…)"
            )
        verify_data_disk(disk)
        cells_b = read_layout_cells(disk)
        if cells_b is None:
            raise SmokeError("boot B: LAYOUT.JSN 消失（跨 boot 存活失败）")
        if len(cells_b) != args.expected_cells:
            raise SmokeError(
                f"boot B: 宿主读回格数 {len(cells_b)} != {args.expected_cells}"
            )
        log(
            f"boot B: 数据盘 sha256 前后一致（{hash_after_b[:16]}…），"
            f"LAYOUT.JSN {len(cells_b)} 格跨 boot 存活"
        )

        success = True
        log(
            "PASS：EFI GUI 交互相双 boot（A: CLICK ok n=1 + LAYOUTW ok；"
            f"B: LAYOUT ok n={args.expected_cells} + 零重写）"
        )
        return 0
    finally:
        if boot_a is not None:
            boot_a.stop()
        if boot_b is not None:
            boot_b.stop()
        if success and not args.keep:
            shutil.rmtree(run_dir, ignore_errors=True)
        else:
            log(f"artifacts 保留：{run_dir}")
            log(f"  串口：{run_dir}/boot-a.log{'' if args.only_a else f' / {run_dir}/boot-b.log'}")
            log(f"  数据盘：{disk}")
            log(f"  命令：{run_dir}/boot-a.cmd.txt{'' if args.only_a else f' / {run_dir}/boot-b.cmd.txt'}")


# =====================================================================
# dry-run
# =====================================================================


def dry_run(args) -> int:
    preflight(args)
    print_marker_status()
    run_dir = Path(args.work_dir) if args.work_dir else Path(
        tempfile.mkdtemp(prefix="gui8-smoke-dry.", dir=args.tmp_dir)
    )
    run_dir.mkdir(parents=True, exist_ok=True)
    try:
        disk = Path(args.data_disk) if args.data_disk else run_dir / "layout.img"
        if disk.exists():
            verify_data_disk(disk)
        else:
            create_data_disk(disk)
        log(f"dry-run: image = {args.image}")
        log(f"dry-run: code  = {args.code}")
        log(f"dry-run: vars  = {args.vars or '(未给；每 boot 仍产 fresh blank)'}")
        log(f"dry-run: disk  = {disk}（sha256={sha256_file(disk)}）")
        log(f"dry-run: disk 根目录 = {Fat12Volume(disk).list_root()}")
        for suffix, markers in (
            (
                "a",
                [
                    MARK_BERRYBOOT,
                    MARK_ROOTFS,
                    MARK_APP,
                    MARK_LAUNCH,
                    MARK_PAGE,
                    MARK_CLICK,
                    MARK_LAYOUTW,
                ],
            ),
            (
                "b",
                [
                    MARK_BERRYBOOT,
                    MARK_ROOTFS,
                    MARK_APP,
                    MARK_LAUNCH,
                    MARK_PAGE,
                    f"LAYOUT ok n={args.expected_cells}",
                ],
            ),
        ):
            cmd = qemu_command(
                args,
                disk,
                run_dir / f"vars-{suffix}.fd",
                run_dir / f"qmp-{suffix}.sock",
                run_dir / f"boot-{suffix}.log",
            )
            log(f"dry-run: boot {suffix.upper()} QEMU = {shlex.join(cmd)}")
            log(f"dry-run: boot {suffix.upper()} 期望标记 = {markers}")
        w, h = 800, 600
        cx, cy = cell0_center(w, h)
        log(f"dry-run: 800x600 cell(0,0) 中心 = ({cx},{cy})")
        log("DRY-RUN PASS（未启动 QEMU）")
        return 0
    finally:
        if not args.keep:
            shutil.rmtree(run_dir, ignore_errors=True)
        else:
            log(f"dry-run artifacts 保留：{run_dir}")


# =====================================================================
# --self-test（无 QEMU：FAT12 / 几何 / 日志谓词 / QMP 协议）
# =====================================================================


class DummyProc:
    def poll(self):
        return None


class MockQmp:
    """最小 QMP server：greeting/握手 + input-send-event（首轮拒 device）。"""

    def __init__(self, path: Path):
        self.path = str(path)
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(self.path)
        self.srv.listen(1)
        self.srv.settimeout(10.0)
        self.batches: list[list] = []
        self.error: Exception | None = None
        self.thread = threading.Thread(target=self._serve, daemon=True)

    def start(self):
        self.thread.start()
        return self

    def _send(self, conn, obj):
        conn.sendall((json.dumps(obj) + "\n").encode("utf-8"))

    def _serve(self):
        try:
            conn, _ = self.srv.accept()
            conn.settimeout(10.0)
            f = conn.makefile("rw", encoding="utf-8", newline="\n")
            self._send(conn, {"QMP": {"version": {"qemu": {"major": 0}}}})
            while True:
                line = f.readline()
                if not line:
                    break
                msg = json.loads(line)
                cmd = msg.get("execute")
                args = msg.get("arguments") or {}
                if cmd == "qmp_capabilities":
                    self._send(conn, {"return": {}})
                elif cmd == "input-send-event":
                    if "device" in args:
                        self.error = AssertionError(
                            "input-send-event 不应带 device 键"
                        )
                        self._send(conn, {"return": {}})
                    else:
                        self.batches.append(args.get("events") or [])
                        self._send(conn, {"return": {}})
                elif cmd == "quit":
                    self._send(conn, {"return": {}})
                    break
                else:
                    self._send(conn, {"return": {}})
            conn.close()
        except Exception as e:  # noqa: BLE001 —— 自检要把异常带回主线程
            self.error = e
        finally:
            try:
                self.srv.close()
            except OSError:
                pass

    def join(self):
        self.thread.join(timeout=10)
        if self.error:
            raise self.error


def self_test(args) -> int:
    tmp = Path(tempfile.mkdtemp(prefix="gui8-smoke-selftest.", dir=args.tmp_dir))
    try:
        # 1) FAT12 + 印章 + GUI8.SMK + LAYOUT.JSN 往返
        disk = tmp / "layout.img"
        create_data_disk(disk)
        verify_data_disk(disk)
        cells = write_layout_cells_for_selftest(disk)
        if cells != 2:
            raise SmokeError(f"自检 LAYOUT 格数 {cells} != 2")
        roots = Fat12Volume(disk).list_root()
        if "GUI8    SMK" not in roots or "LAYOUT  JSN" not in roots:
            raise SmokeError(f"自检 FAT12 根目录异常：{roots}")
        log(f"self-test: FAT12 ok（根目录 {roots}，LAYOUT 2 格）")

        # 2) 几何
        if cell0_center(800, 600) != (95, 130):
            raise SmokeError(f"cell0_center 异常：{cell0_center(800, 600)}")
        if cell0_center(480, 1000) != (55, 196):
            raise SmokeError(f"cell0_center 480x1000 异常：{cell0_center(480, 1000)}")
        log("self-test: geometry ok（800x600→(95,130)，480x1000→(55,196)）")

        # 3) 日志谓词 + 缺标记显式失败
        lp = tmp / "fake.log"
        lp.write_text("A\nLAUNCH ok\nPAGE ok n=2\n", encoding="utf-8")
        lw = LogWatcher(lp)
        dp = DummyProc()
        lw.wait_markers(dp, [MARK_LAUNCH, MARK_PAGE], 1.0, "self-test log")
        try:
            lw.wait_markers(dp, [MARK_LAUNCH, MARK_LAYOUTW], 0.4, "self-test log")
        except MissingMarkers as e:
            if MARK_LAYOUTW not in str(e):
                raise SmokeError(f"缺标记报错未含 {MARK_LAYOUTW}：{e}") from e
        else:
            raise SmokeError("缺标记未抛 MissingMarkers")
        log("self-test: log predicate ok（缺标记显式 MissingMarkers）")

        # 3b) Boot B 断言形态：n=2 不得吃 n=20；CLICK n=1 不得吃 n=10；
        #     读路径日志必须零 LAYOUTW/零 CLICK。
        layout_pat = r"LAYOUT ok n=2(?![0-9])"
        fake_b = "LAUNCH ok\nLAYOUT ok n=2\n"
        if re.search(layout_pat, fake_b) is None:
            raise SmokeError("Boot B LAYOUT 正则未命中正常形态")
        if re.search(layout_pat, "LAYOUT ok n=20"):
            raise SmokeError("Boot B LAYOUT 正则误吞 n=20")
        if re.search(RE_CLICK_N1, "CLICK ok n=10"):
            raise SmokeError("CLICK 正则误吞 n=10")
        if "LAYOUTW ok" in fake_b or "CLICK ok n=" in fake_b:
            raise SmokeError("Boot B 负向断言形态自检失败")
        log("self-test: boot-B 断言形态 ok（n=2 不吃 n=20 / n=1 不吃 n=10 / 零重写）")

        # 4) QMP 协议 + no-device 广播 + 三批单击
        sock = tmp / "qmp.sock"
        mock = MockQmp(sock).start()
        cli = QmpClient(sock, timeout=5.0)
        cli.click(95, 130, 800, 600)
        cli.quit()
        mock.join()
        if len(mock.batches) != 3:
            raise SmokeError(f"QMP 批数 {len(mock.batches)} != 3")
        abs_events = [e for e in mock.batches[0] if e["type"] == "abs"]
        got = {e["data"]["axis"]: e["data"]["value"] for e in abs_events}
        if got.get("x") != 3891 or got.get("y") != 7100:
            raise SmokeError(f"QMP 坐标异常：{got}（期望 x=3891 y=7100）")
        if mock.batches[2][0]["data"]["down"] is not False:
            raise SmokeError("QMP 第三批非 btn up")
        log("self-test: QMP ok（no-device 广播 3 批，坐标 3891/7100）")

        log("SELFTEST PASS（FAT12/geometry/log/QMP；零 QEMU）")
        return 0
    finally:
        if not args.keep:
            shutil.rmtree(tmp, ignore_errors=True)
        else:
            log(f"self-test artifacts 保留：{tmp}")


# =====================================================================
# CLI
# =====================================================================


def parse_args(argv=None):
    ap = argparse.ArgumentParser(
        prog="gui8-smoke.py",
        description="GUI-8 段④ EFI GUI 交互相双 boot 冒烟 rig（QMP unix + "
        "virtio-blk 数据盘 + virtio-tablet；无 Docker/UTM）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "示例：\n"
            "  python3 tools/gui8-smoke.py --self-test\n"
            "  python3 tools/gui8-smoke.py --dry-run\n"
            "  python3 tools/gui8-smoke.py --image out/berryos.img\n"
            "  GUI8_KEEP=1 just gui8-smoke\n"
        ),
    )
    ap.add_argument("--image", default="out/berryos.img", help="EFI 系统盘（默认 %(default)s）")
    ap.add_argument(
        "--code",
        default=os.environ.get("AAVMF_CODE", DEFAULT_CODE),
        help="AAVMF CODE 固件（默认 env AAVMF_CODE 或 brew 路径）",
    )
    ap.add_argument(
        "--vars",
        default=os.environ.get("AAVMF_VARS", DEFAULT_VARS),
        help="AAVMF VARS 模板（接口占位；每 boot 仍产 fresh blank，不消费模板）",
    )
    ap.add_argument("--qemu", default=os.environ.get("QEMU", "qemu-system-aarch64"))
    ap.add_argument("--data-disk", help="数据盘路径（默认 <run-dir>/layout.img）")
    ap.add_argument(
        "--reuse-data-disk",
        action="store_true",
        help="允许复用已存在数据盘（缺失 GUI8.SMK/印章即 FAIL）",
    )
    ap.add_argument("--work-dir", help="指定 run 目录（默认 mktemp）")
    ap.add_argument(
        "--tmp-dir",
        default="/tmp" if Path("/tmp").is_dir() else tempfile.gettempdir(),
        help="临时目录（默认 /tmp；QMP sock 路径帽）",
    )
    ap.add_argument("--boot-a-timeout", type=float, default=420.0)
    ap.add_argument("--boot-b-timeout", type=float, default=300.0)
    ap.add_argument("--click-timeout", type=float, default=120.0)
    ap.add_argument(
        "--settle-seconds",
        type=float,
        default=5.0,
        help="无 click-ready 时 PAGE 后静置秒数（snap 收敛）",
    )
    ap.add_argument("--expected-cells", type=int, default=2)
    ap.add_argument("--only-a", action="store_true", help="只跑 Boot A（plumbing）")
    ap.add_argument("--dry-run", action="store_true", help="只建盘并打印 QEMU 命令")
    ap.add_argument("--self-test", action="store_true", help="无 QEMU 自检")
    ap.add_argument("--require-markers", action="store_true", help="launcher 标记未合并即提前 FAIL")
    ap.add_argument("--keep", action="store_true", help="保留 run 目录/日志/数据盘")
    return ap.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    try:
        if args.self_test:
            return self_test(args)
        if args.dry_run:
            return dry_run(args)
        return run_dual_boot(args)
    except MissingMarkers as e:
        print(f"gui8-smoke: FAIL: {e}", file=sys.stderr, flush=True)
        print(
            "  hint: 基线 014623d 的 launcher 尚未合并段④ 标记"
            "（CLICK/LAYOUTW/LAYOUT）；先跑 --self-test/--dry-run 验 plumbing，"
            "或等待段④ launcher 合并。",
            file=sys.stderr,
            flush=True,
        )
        if e.log_path:
            print(f"---- {e.log_path} 尾部 ----", file=sys.stderr)
            print(tail_text(e.log_path), file=sys.stderr)
            print("----", file=sys.stderr)
            qlog = e.log_path.with_name(e.log_path.stem + ".qemu.log")
            if qlog.is_file() and qlog.stat().st_size:
                print(f"---- {qlog} 尾部 ----", file=sys.stderr)
                print(tail_text(qlog), file=sys.stderr)
                print("----", file=sys.stderr)
        return 1
    except SmokeError as e:
        print(f"gui8-smoke: FAIL: {e}", file=sys.stderr, flush=True)
        if e.log_path:
            print(f"---- {e.log_path} 尾部 ----", file=sys.stderr)
            print(tail_text(e.log_path), file=sys.stderr)
            print("----", file=sys.stderr)
            qlog = e.log_path.with_name(e.log_path.stem + ".qemu.log")
            if qlog.is_file() and qlog.stat().st_size:
                print(f"---- {qlog} 尾部 ----", file=sys.stderr)
                print(tail_text(qlog), file=sys.stderr)
                print("----", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("gui8-smoke: FAIL: interrupted", file=sys.stderr, flush=True)
        return 130
    except Exception:  # noqa: BLE001 —— 未预期异常也要可见失败
        print("gui8-smoke: FAIL: 未预期异常", file=sys.stderr, flush=True)
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
