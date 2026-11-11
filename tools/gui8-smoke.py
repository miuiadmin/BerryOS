#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gui8-smoke —— GUI-8 段④「EFI GUI 交互相」双 boot 冒烟 rig。

为什么单独立脚本（不并回 smoke-image.sh）
----------------------------------------
冻结相 smoke-image 的断言面是「EFI 十段恰一次 + 裸相 28 + 生态 36 + 字节
对拍 + 金丝雀」，其 QEMU 拓扑**结构性无 virtio**（不挂 blk/tablet/QMP）。
交互相需要另一套拓扑（FAT12 数据盘 + virtio-tablet + QMP unix sock）和
另一套标记族（fs 端点让给 launcher 后 bsh 在 fs_selftest park，ECO 36 里
FS/U1/U2/U3/B1/U4/TAB/INT/POLL/MMP/CLK/SIG/PS/BC/THR/EXIT 均结构性缺席）。
因此本脚本 = 独立关卡，**零改 smoke-image.sh 行为**；`just gui8-smoke`
单口调用。

验收面（V2/V3/V4）
------------------
- 数据盘哨兵：根目录 `GUI8.SMK` 的 payload 必须**逐字节恰等于**
  `BERRYOS-GUI8-SMOKE-V1\\n`（文件名本身用户可创建，故只认 payload；
  缺席/错 payload 一律 fail closed）。launcher 侧同一串才进冒烟支路。
- Boot A：AAVMF + out/berryos.img + 独立 FAT12 数据盘（LBA0 印章
  `BERRYOSVBLK!` + 精确 magic 的 `GUI8.SMK`），display fw_cfg 800x600@60，
  virtio-blk + virtio-tablet + QMP。
  - 等基础 5 + boot 批 `app APPDEMO up`（先把 boot 批 app 证据吃完）；
  - `launcher: click ready` **强制在席恰一次**（缺席即 FAIL，不 sleep
    回退——这是 QMP 注入前的状态谓词门）；
  - QMP 三批注入（abs → btn down → btn up）单击 APPDEMO 格中心；
  - 精确计数：`CLICK ok n=1` == 1、`LAYOUTW ok` == 1、`LAYOUT ok n=`
    == 0（重复/顺序假绿一律 FAIL，不 warn）；
  - 点击后**新增** `app APPDEMO up` 恰 1 行（点击真 spawn），
    `app APPQUIET up` 零新增（未误点/误 spawn）；
  - 宿主侧再经 FAT12 读回 `LAYOUT.JSN`：必须恰 **109 canonical bytes**
    且 cells 逐项 == `APPDEMO(0,0,0)` + `APPQUIET(page=3,row=0,col=1)`。
- Boot B：同一镜像、同一数据盘（不重造），fresh vars/QMP。
  - 精确计数：`LAYOUT ok n=2` == 1、`LAYOUTW ok` == 0、
    `CLICK ok n=` == 0（写路径一次性状态门）；
  - 宿主侧 sha256 前后相等 = 数据盘零重写；LAYOUT.JSN 仍为 canonical。
- `--normal-boot`（常态隔离 + BerryUI app，可选）：同一镜像/tablet/
  数据盘拓扑但 **不造 `GUI8.SMK`**，display 1080×1920。BROWSER 首格 +
  六个 BerryUI app 真实 APP_SPAWN：Browser/Settings/Files/Photos/Contacts/
  UITEST 分别完成窗口/帧、状态或数据链、layout-driven 点击、截图像素检查与
  返回销毁。
  `CLICK`/`LAYOUTW`/`LAYOUT`/`launcher: click ready` 仍零打印——证明
  GUI8 冒烟标记只在哨兵支路出现。

plumbing 自检（零 QEMU；含错 payload / 无哨兵 / 错 canonical / 重复计数
/ 顺序倒置负样本）：

    python3 tools/gui8-smoke.py --self-test
    python3 tools/gui8-smoke.py --dry-run [--image out/berryos.img]
    python3 tools/gui8-smoke.py --dry-run --normal-boot

标记 canon：CLICK/LAYOUTW/LAYOUT 在本相按**精确次数**断言（Boot A
1/1/0，Boot B 0/0/1）；`CLICK ok n=1` 用负向前瞻防 `n=10` 子串假阳性。
QMP 点击前只认 click ready 状态谓词，不用固定 sleep 兜底。

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
MARK_APP_N9 = "APP ok n=9"
MARK_APP_DEMO_UP = "app APPDEMO up"
MARK_APP_DEMO_FS = "app APPDEMO fs ok 23"
MARK_APP_QUIET_UP = "app APPQUIET up"
MARK_LAUNCH = "LAUNCH ok"
MARK_PAGE = "PAGE ok n=2"
MARK_CLICK = "CLICK ok n=1"
MARK_CLICK_ANY = "CLICK ok n="
MARK_LAYOUTW = "LAYOUTW ok"
MARK_LAYOUT = "LAYOUT ok n="
MARK_CLICK_READY = "launcher: click ready"
MARK_WM_CLEAN = "WMDTY ok n=1"
MARK_WM_CONN1 = "WMCONN ok id=1"
MARK_WM_CONN2 = "WMCONN ok id=2"
MARK_WM_CONN3 = "WMCONN ok id=3"
MARK_WM_CONN4 = "WMCONN ok id=4"
MARK_WM_CONN5 = "WMCONN ok id=5"
MARK_WM_CONN6 = "WMCONN ok id=6"
MARK_WM_CONN7 = "WMCONN ok id=7"
MARK_WM_CONN8 = "WMCONN ok id=8"
MARK_WM_CONN9 = "WMCONN ok id=9"
MARK_WM_ROUTE_WINDOW_UP = "WMRTWIN ok id="
MARK_WM_DENIED = "WMDENY ok"
MARK_WM_SYSTEM_AUTH = "WMSYSAUTH ok"
MARK_WM_SYSTEM_CLIENT_DENIED = "WMSYSCLIENTDENY ok"
MARK_WM_SYSTEM_SEED_DENIED = "WMSYSSEEDDENY ok"
MARK_WM_SYSTEM_CONTROL_DENIED = "WMSYSCTRLDENY ok"
MARK_WM_ROLE_MASQUERADE_DENIED = "WMROLEMASQDENY ok"
MARK_WM_LAYER_DENIED = "WMLAYERDENY ok"
MARK_WM_PARENT_DENIED = "WMPARENTDENY ok"
MARK_WM_GLOBAL_DENIED = "WMGLOBALDENY ok"
MARK_WM_DAMAGE_BAD = "WMDAMAGEBAD ok"
MARK_WM_SYSTEM_OCCLUDER = "WMSYSOCC ok"
MARK_WM_AVOID_WINDOW = "WMAVOIDWIN ok"
MARK_WM_AVOID_HIDE = "WMAVOIDHIDE ok"
MARK_WM_AVOID_SHOW = "WMAVOIDSHOW ok"
MARK_WM_AVOID_GRANT = "WMAVOIDGRANT ok"
MARK_WM_AVOID_FRAME = "WMAVOIDFRAME ok"
MARK_WM_OCCLUDED_DAMAGE_BAD = "WMOCCDAMAGEBAD ok"
MARK_WM_DAMAGE_CLIP = "WMDAMAGECLIP ok"
MARK_WM_RAW_BUFFER_BAD = "WMRAWBUFBAD ok"
MARK_WM_CONFIG_SUBMIT_DENY = "WMCONFIGSUBMITDENY ok"
MARK_WM_CONFIG_SURFACE_DENY = "WMCONFIGSURFACEDENY ok"
MARK_WM_CONFIG_FOREIGN_ACK = "WMCONFIGFOREIGNACK ok"
MARK_WM_CONFIG_DESTROYED_ACK = "WMCONFIGDESTROYEDACK ok"
MARK_WM_CONFIG_NOOP_EMPTY = "WMCONFIGNOOPEMPTY ok"
MARK_WM_CONFIG_HIDE = "WMCONFIGHIDE ok"
MARK_WM_CONFIG_SHOW = "WMCONFIGSHOW ok"
MARK_WM_CONFIG_FOCUS_EMPTY = "WMCONFIGFOCUSEMPTY ok"
MARK_WM_CONFIG_ACK_DENIED = "WMCONFIGACKDENY ok"
MARK_WM_CONFIG_CREATED = "WMCONFIGCREATED ok"
MARK_WM_CONFIG_EMPTY = "WMCONFIGEMPTY ok"
MARK_WM_CONFIG_COALESCE = "WMCONFIGCOALESCE ok"
MARK_WM_CONFIG_STALE = "WMCONFIGSTALE ok"
MARK_WM_SURFACE_DENIED = "WMSDENY ok"
MARK_WM_ORPHAN = "WMORPHAN ok id="
MARK_WM_2FRAME = "WM2FRAME ok n=2"
MARK_WM_FOCUS_READY = "WMFOCUSREADY ok"
MARK_WM_KEY_FOCUS = "WMKEYFOCUS ok"
MARK_WM_FOCUS_KEY = "WMFOCUSKEY ok"
MARK_WM_FOCUS_NOOP = "WMFOCUSNOOP ok"
MARK_WM_FOCUS_SKIP = "WMFOCUSSKIP ok"
MARK_WM_FOCUS = "WMFOCUS ok"
MARK_WM_HIDE_FOCUS = "WMHIDEFOCUS ok"
MARK_WM_MISS_FOCUS = "WMMISSFOCUS ok"
MARK_WM_SHOW_FOCUS = "WMSHOWFOCUS ok"
MARK_WM_Z_FOCUS = "WMZFOCUS ok"
MARK_WM_LOCK_DOWN = "WMLOCKDOWN ok"
MARK_WM_CANCEL = "WMCANCEL ok"
MARK_WM_DEAD1 = "WMDEAD ok id=1 wins=0"
MARK_WM_DEAD2 = "WMDEAD ok id=2 wins=1"
MARK_WM_ROUTE_DESKTOP = "WMROUTE ok sid=3 window=0"
MARK_WM_ROUTE_WINDOW = "WMROUTEWIN ok phase="
MARK_BROWSER_UP = "BROWSER up"
MARK_BROWSER_WIN = "BROWSERWIN ok"
MARK_BROWSER_FRAME = "BROWSERFRAME ok"
MARK_BROWSER_BACK = "BROWSERBACK ok"
MARK_BROWSER_PROBE = "BROWSERPROBE ok"
MARK_BROWSER_DESTROY = "BROWSERDESTROY ok"
MARK_BROWSER_RENDER = "BROWSERRENDER ok seq="
MARK_BROWSER_EXIT = "BROWSEREXIT ok"
MARK_SETTINGS_UP = "SETTINGS up"
MARK_SETTINGS_WIN = "SETTINGSWIN ok"
MARK_SETTINGS_FRAME = "SETTINGSFRAME ok"
MARK_SETTINGS_GEOM = "SETTINGSGEOM ok"
MARK_SETTINGS_TOGGLE1 = "SETTINGSTOGGLE ok n=1"
MARK_SETTINGS_TOGGLE2 = "SETTINGSTOGGLE ok n=2"
MARK_SETTINGS_BACK = "SETTINGSBACK ok"
MARK_SETTINGS_PROBE = "SETTINGSPROBE ok"
MARK_SETTINGS_DESTROY = "SETTINGSDESTROY ok"
MARK_SETTINGS_EXIT = "SETTINGSEXIT ok"
MARK_FILES_UP = "FILES up"
MARK_FILES_WIN = "FILESWIN ok"
MARK_FILES_LIST = "FILESLIST ok n="
MARK_FILES_SEL = "FILESSEL ok i="
MARK_FILES_FRAME = "FILESFRAME ok"
MARK_FILES_OPEN = "FILESOPEN ok"
MARK_FILES_STAT = "FILESSTAT ok size="
MARK_FILES_READ = "FILESREAD ok n="
MARK_FILES_BACK = "FILESBACK ok"
MARK_FILES_PROBE = "FILESPROBE ok"
MARK_FILES_DESTROY = "FILESDESTROY ok"
MARK_FILES_EXIT = "FILESEXIT ok"
MARK_PHOTOS_UP = "PHOTOS up"
MARK_PHOTOS_WIN = "PHOTOSWIN ok"
MARK_PHOTOS_LIST = "PHOTOSLIST ok n="
MARK_PHOTOS_DATA = "PHOTOSDATA ok n=5000"
MARK_PHOTOS_FRAME = "PHOTOSFRAME ok"
MARK_PHOTOS_OPEN = "PHOTOSOPEN ok"
MARK_PHOTOS_VIEW = "PHOTOSVIEW ok"
MARK_PHOTOS_BACK = "PHOTOSBACK ok"
MARK_PHOTOS_RENDER = "PHOTOSRENDER ok seq="
MARK_PHOTOS_PROBE = "PHOTOSPROBE ok"
MARK_PHOTOS_DESTROY = "PHOTOSDESTROY ok"
MARK_PHOTOS_EXIT = "PHOTSEXIT ok"
MARK_CONTACTS_UP = "CONTACTS up"
MARK_CONTACTS_WIN = "CONTACTSWIN ok"
MARK_CONTACTS_LIST = "CONTACTSLIST ok n="
MARK_CONTACTS_READ = "CONTACTSREAD ok n=4"
MARK_CONTACTS_LOCAL = "CONTACTSLOCAL ok n=4"
MARK_CONTACTS_FRAME = "CONTACTSFRAME ok"
MARK_CONTACTS_DETAIL = "CONTACTSDETAIL ok i=1"
MARK_CONTACTS_FAV = "CONTACTSFAV ok n=1"
MARK_CONTACTS_BACK = "CONTACTSBACK ok"
MARK_CONTACTS_PROBE = "CONTACTSPROBE ok"
MARK_CONTACTS_DESTROY = "CONTACTSDESTROY ok"
MARK_CONTACTS_EXIT = "CONTACTSEXIT ok"
MARK_UITEST_UP = "UITEST up"
MARK_UITEST_ROOM = "UITESTROOM ok"
MARK_UITEST_WIN = "UITESTWIN ok w="
MARK_UITEST_FRAME = "UITESTFRAME ok"
MARK_UITEST_CLIP = "UITESTCLIP ok"
MARK_UITEST_SURFACE = "UITESTSURFACE ok"
MARK_UITEST_KEY_WAIT = "UITESTKEYWAIT ok"
MARK_UITEST_KEY = "UITESTKEY ok"
MARK_UITEST_PROBE = "UITESTPROBE ok"
MARK_UITEST_INPUT = "UITESTINPUT ok x="
MARK_UITEST_DESTROY = "UITESTDESTROY ok"
MARK_UITEST_EXIT = "UITESTEXIT ok"
MARK_WM_INPUT_DROP = "WMIDROP bad"
MARK_INPUT_ROUTE_BAD = "INPUTROUTE bad"
MARK_CJK_PROBE = "CJKPROBE ok n=2"
MARK_INPUT_DROPS_OK = "INPUTDROPS ok n=0"
MARK_INPUT_DROPS_BAD = "INPUTDROPS bad n="
MARK_SWIPE = "SWIPE ok n=1"
MARK_INPUT_ARMED = "launcher: input poll armed"
# 负向前瞻：`n=10` 不得命中 `n=1`（CLICK 恰一发的期望形态）。
RE_CLICK_N1 = r"CLICK ok n=1(?![0-9])"
# Boot B 读回标记：n 固定 2（canonical 双 cell）。
RE_LAYOUT_N2 = r"LAYOUT ok n=2(?![0-9])"

# —— 数据盘约定（与 tools/smoke.sh / just desktop 同源）——
DISK_SEAL = b"BERRYOSVBLK!"
SMK_NAME11 = b"GUI8    SMK"
# 哨兵只认 payload magic（文件名可被用户随手创建）；launcher 侧必须比对
# 同一串，harness 建盘也只写这一串。末尾换行是 magic 的一部分。
SMK_MAGIC = b"BERRYOS-GUI8-SMOKE-V1\n"
LAYOUT_NAME11 = b"LAYOUT  JSN"
RESERVED_ZONE = 16

# LAYOUT.JSN 只接受这一份 canonical 字节（109B = berry-layout emit 对
# 默认 APPDEMO+APPQUIET 双 app 注册表的确定性产物）。长度/字节/cells
# 三面任一不符即 FAIL——不再只看「格数」。
LAYOUT_CANONICAL = (
    b'{"schema":1,"cells":[{"app":"APPDEMO","page":0,"row":0,"col":0},'
    b'{"app":"APPQUIET","page":3,"row":0,"col":1}]}'
)
LAYOUT_CELLS_EXPECTED = (
    ("APPDEMO", 0, 0, 0),
    ("APPQUIET", 3, 0, 1),
)
if len(LAYOUT_CANONICAL) != 109:
    raise RuntimeError("LAYOUT canonical 字节数漂移（期望 109）")
if len(LAYOUT_CELLS_EXPECTED) != 2:
    raise RuntimeError("LAYOUT canonical cell 数漂移（期望 2）")

# —— 桌面几何（launcher desktop.rs 常量镜像；仅用于算 cell(0,0) 中心，
#    屏幕宽高从串口 LAUNCH/FB 行动态解析——裁决 D：不把 800x600 写死）——
STATUS_H = 44
# 小屏档无 Dock，只保留 SwipePager 16px 视觉点行（OH 96px 指示区仅 phone 档）。
DOT_H = 16
GRID_TOP = 8
BOTTOM_PAD = 8
GRID_COLS = 4
GRID_ROWS = 5
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

    def screendump(self, path: Path):
        """QEMU PPM 截图（native shell 像素反 mock 面）。"""
        return self.execute("screendump", {"filename": str(path)})

    def input_send(self, events):
        # 省 device 键 = 广播到 unbound handler 链（QEMU ui/input.c:
        # qemu_input_find_handler 第二圈）。device=tab0 会走
        # qemu_console_lookup_by_device_name（console 查找），在 -display none
        # 的 qemu-fixed-text-console 上触发 QEMU 11.0.3 的 QOM 崩溃——勿用。
        return self.execute("input-send-event", {"events": events})

    def key(self, code: int, down: bool) -> None:
        """注入 Linux evdev keycode（QMP number 形态，仍省 device 键）。"""
        return self.input_send([
            {
                "type": "key",
                "data": {
                    "down": down,
                    "key": {"type": "number", "data": code},
                },
            }
        ])

    def tap_key(self, code: int) -> None:
        """同码 down/up；QEMU 会各产 EV_KEY+SYN_REPORT 批。"""
        self.key(code, True)
        self.key(code, False)

    def pointer_down(self, x: float, y: float, w: int, h: int) -> None:
        """raw DOWN（不自动 UP）：P4 stale input-lock 相专用。"""
        ax = max(0, min(32767, int(round(x * 32768.0 / max(1, w)))))
        ay = max(0, min(32767, int(round(y * 32768.0 / max(1, h)))))
        self.input_send([
            {"type": "abs", "data": {"axis": "x", "value": ax}},
            {"type": "abs", "data": {"axis": "y", "value": ay}},
        ])
        time.sleep(0.1)
        self.input_send([
            {"type": "btn", "data": {"down": True, "button": "left"}},
            {"type": "abs", "data": {"axis": "x", "value": ax}},
            {"type": "abs", "data": {"axis": "y", "value": ay}},
        ])
        time.sleep(0.2)

    def pointer_up(self) -> None:
        """raw UP：须在 guest 已销毁锁定窗后注入。"""
        self.input_send([{"type": "btn", "data": {"down": False, "button": "left"}}])

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

    def abs_burst(self, x: float, y: float, w: int, h: int, count: int = 48) -> None:
        """无按键绝对坐标 burst：填 eventq 但不生成 pointer 手势。"""
        ax = max(0, min(32767, int(round(x * 32768.0 / max(1, w)))))
        ay = max(0, min(32767, int(round(y * 32768.0 / max(1, h)))))
        for i in range(count):
            # 小幅摆动避免设备端合并/去重；仍保持同一点附近，不触发翻页。
            value = ax + (1 if i % 2 == 0 else -1)
            self.input_send([
                {"type": "abs", "data": {"axis": "x", "value": value}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
            ])

    def drag(
        self,
        x0: float,
        x1: float,
        y: float,
        w: int,
        h: int,
        steps: int = 10,
    ) -> None:
        """注入真实 virtio-tablet 横滑（abs 定位 → down → move×N → up）。

        steps=10 加首尾共 12 个接触事件，低于 inputd eventq 16 深帽；
        40ms 间隔让时间戳跨过 resampler 的 2ms 最小步长。
        """
        def ax(x: float) -> int:
            return max(0, min(32767, int(round(x * 32768.0 / max(1, w)))))

        ay = max(0, min(32767, int(round(y * 32768.0 / max(1, h)))))
        a0, a1 = ax(x0), ax(x1)
        self.input_send([
            {"type": "abs", "data": {"axis": "x", "value": a0}},
            {"type": "abs", "data": {"axis": "y", "value": ay}},
        ])
        time.sleep(0.04)
        self.input_send([
            {"type": "btn", "data": {"down": True, "button": "left"}},
            {"type": "abs", "data": {"axis": "x", "value": a0}},
            {"type": "abs", "data": {"axis": "y", "value": ay}},
        ])
        for i in range(1, steps + 1):
            x = x0 + (x1 - x0) * i / steps
            self.input_send([
                {"type": "abs", "data": {"axis": "x", "value": ax(x)}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
            ])
            time.sleep(0.04)
        self.input_send([{"type": "btn", "data": {"down": False, "button": "left"}}])
        time.sleep(0.2)

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

    def write_existing(self, name11: bytes, content: bytes) -> None:
        """原目录项原簇链覆写（自愈负样本专用；content 不得超过现尺寸）。"""
        found = self._find_root(name11)
        if found is None:
            raise SmokeError(f"FAT12 文件不存在：{name11!r}（{self.path}）")
        _, _, first, size = found
        if len(content) > size:
            raise SmokeError(
                f"FAT12 覆写超尺寸：{name11!r} {len(content)}>{size}"
            )
        padded = content + b"\x00" * (size - len(content))
        clus = first
        out_off = 0
        while 2 <= clus < 0xFF8 and out_off < len(padded):
            off = self._cluster_off(clus)
            take = min(self.cluster_bytes, len(padded) - out_off)
            self.data[off : off + take] = padded[out_off : out_off + take]
            out_off += take
            nxt = self._fat_get(clus)
            if nxt >= 0xFF8:
                break
            clus = nxt
        if out_off != len(padded):
            raise SmokeError(f"FAT12 覆写链过短：{name11!r}（{self.path}）")
        self._write()

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


def create_data_disk(path: Path, *, with_smoke: bool = True) -> None:
    """mkfat 默认 FAT12 盘 + LBA0 印章；with_smoke 时根目录写精确哨兵。"""
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
    if with_smoke:
        Fat12Volume(path).add_file(SMK_NAME11, SMK_MAGIC)
    verify_data_disk(path, expect_smoke=with_smoke)


def verify_data_disk(path: Path, *, expect_smoke: bool = True) -> None:
    """校验印章 + 哨兵 payload；expect_smoke=False 用于常态隔离 boot。

    文件名存在不算数：payload 必须逐字节等于 ``SMK_MAGIC``。缺席/错
    payload 一律 fail closed（用户自造 GUI8.SMK 不得打开冒烟支路）。
    """
    path = Path(path)
    if not path.is_file():
        raise SmokeError(f"数据盘缺席：{path}")
    with path.open("rb") as f:
        seal = f.read(len(DISK_SEAL))
    if seal != DISK_SEAL:
        raise SmokeError(f"数据盘 LBA0 印章缺失/不符：{seal!r}（{path}）")
    vol = Fat12Volume(path)
    got = vol.read_file(SMK_NAME11)
    if expect_smoke:
        if got is None:
            raise SmokeError(
                f"GUI8.SMK 缺席（哨兵不存在，拒绝进冒烟支路）：{path}"
            )
        if got != SMK_MAGIC:
            raise SmokeError(
                f"GUI8.SMK payload 非精确 magic：{got!r} != {SMK_MAGIC!r}"
                f"（{path}）"
            )
    elif got is not None:
        raise SmokeError(
            f"常态隔离盘不应有 GUI8.SMK：{got!r}（{path}）"
        )


def read_layout_canonical(path: Path) -> list:
    """读回 LAYOUT.JSN：恰 109 canonical bytes + 精确双 cell。

    不再是「只数 cells」：长度/逐字节/语义三面全等才返回，任何偏差
    抛 SmokeError（fail closed）。
    """
    vol = Fat12Volume(path)
    blob = vol.read_file(LAYOUT_NAME11)
    if blob is None:
        raise SmokeError(f"LAYOUT.JSN 缺席（{path}）")
    if len(blob) != len(LAYOUT_CANONICAL):
        raise SmokeError(
            f"LAYOUT.JSN 长度 {len(blob)} != canonical {len(LAYOUT_CANONICAL)}"
            f"（{path}）"
        )
    if blob != LAYOUT_CANONICAL:
        raise SmokeError(
            f"LAYOUT.JSN 非 canonical 字节：{blob!r} != {LAYOUT_CANONICAL!r}"
            f"（{path}）"
        )
    try:
        obj = json.loads(blob.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise SmokeError(f"LAYOUT.JSN 非法 JSON：{e}（{path}）") from e
    cells = obj.get("cells") if isinstance(obj, dict) else None
    if not isinstance(cells, list):
        raise SmokeError(f"LAYOUT.JSN 缺 cells 数组（{path}）")
    got = []
    for c in cells:
        if not isinstance(c, dict) or set(c) != {"app", "page", "row", "col"}:
            raise SmokeError(f"LAYOUT.JSN cell 形态非法：{c!r}（{path}）")
        got.append((c["app"], c["page"], c["row"], c["col"]))
    if tuple(got) != LAYOUT_CELLS_EXPECTED:
        raise SmokeError(
            f"LAYOUT.JSN cells {got!r} != {LAYOUT_CELLS_EXPECTED!r}（{path}）"
        )
    return cells


def write_layout_for_selftest(
    path: Path, payload: bytes = LAYOUT_CANONICAL
) -> list:
    """自检用：向无 LAYOUT 的盘注入指定 payload 并走 canonical 读回。"""
    Fat12Volume(path).add_file(LAYOUT_NAME11, payload)
    return read_layout_canonical(path)


def corrupt_layout_for_selfheal(path: Path) -> bytes:
    """真实 FAT 自愈负样本：原尺寸覆写为非法 JSON，返回损坏 payload。"""
    bad = (b"{" + b"X" * (len(LAYOUT_CANONICAL) - 2) + b"}")
    Fat12Volume(path).write_existing(LAYOUT_NAME11, bad)
    return bad


# =====================================================================
# 几何 / QEMU 命令 / Boot 生命周期
# =====================================================================


def cell0_center(w: int, h: int) -> tuple[int, int]:
    """cell(0,0) 中心（status 栏之下，4×5 网格第一格；800×600 调试档）。"""
    grid_h = max(0, h - (STATUS_H + GRID_TOP + DOT_H + BOTTOM_PAD))
    cell_w = max(1, (w - (GRID_COLS - 1) * GRID_GAP) // GRID_COLS)
    cell_h = max(1, (grid_h - (GRID_ROWS - 1) * GRID_GAP) // GRID_ROWS)
    return cell_w // 2, STATUS_H + GRID_TOP + cell_h // 2


RE_LAUNCH_GEOM = re.compile(r"LAUNCH v=\d+ wmax=\d+ (\d+)x(\d+)")
RE_FB_GEOM = re.compile(r"FB ok (\d+)x(\d+)")


def geometry_from_log(watcher: LogWatcher) -> tuple[int, int, str]:
    watcher.poll()
    for rx, name in ((RE_LAUNCH_GEOM, "LAUNCH"), (RE_FB_GEOM, "FB")):
        m = rx.search(watcher.text)
        if m:
            return int(m.group(1)), int(m.group(2)), m.group(0)
    return 800, 600, "fallback 800x600"


# =====================================================================
# 标记精确计数 / 顺序断言（纯函数；--self-test 直接覆盖负样本）
# =====================================================================


def _require_exact_count(
    text: str,
    needle: str,
    expected: int,
    phase: str,
    log_path: Path | None = None,
) -> None:
    got = text.count(needle)
    if got != expected:
        raise SmokeError(
            f"{phase}: {needle!r} 出现 {got} 次 != 期望 {expected}",
            log_path,
        )


def _require_pos_after(
    text: str,
    needle: str,
    anchor: int,
    phase: str,
    log_path: Path | None = None,
) -> None:
    pos = text.find(needle)
    if pos < 0:
        raise SmokeError(f"{phase}: 缺 {needle!r}", log_path)
    if pos <= anchor:
        raise SmokeError(
            f"{phase}: {needle!r} 早于 click ready（pos={pos} <= {anchor}）",
            log_path,
        )


def _require_p4_absent(
    text: str, phase: str, log_path: Path | None = None
) -> None:
    """P4 矩阵只能出现在 P3 交互证明成功的 Boot A。"""
    for marker in (
        MARK_WM_HIDE_FOCUS, MARK_WM_MISS_FOCUS, MARK_WM_SHOW_FOCUS,
        MARK_WM_Z_FOCUS, MARK_WM_LOCK_DOWN, MARK_WM_CANCEL,
    ):
        _require_exact_count(text, marker, 0, f"{phase} P4 isolation", log_path)


def verify_boot_a_click(
    text: str,
    ready_pos: int,
    demo_before: int,
    quiet_before: int,
    fs_before: int = 0,
    log_path: Path | None = None,
) -> None:
    """Boot A 点击后精确验收：1/1/0 + 新 APPDEMO 恰一 + 零 APPQUIET。

    计数与顺序任一不符都 FAIL（重复点击/双 spawn/顺序倒置不得假绿）。
    """
    n_click = len(re.findall(RE_CLICK_N1, text))
    if n_click != 1:
        raise SmokeError(
            f"boot A click: `CLICK ok n=1` 出现 {n_click} 次 != 1",
            log_path,
        )
    _require_exact_count(text, MARK_CLICK_ANY, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_LAYOUT, 0, "boot A click", log_path)
    _require_exact_count(text, MARK_CLICK_READY, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_INPUT_DROPS_OK, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_INPUT_DROPS_BAD, 0, "boot A click", log_path)
    _require_exact_count(text, MARK_WM_ROUTE_DESKTOP, 2, "boot A click", log_path)
    _require_exact_count(text, MARK_WM_INPUT_DROP, 0, "boot A click", log_path)
    _require_exact_count(text, MARK_INPUT_ROUTE_BAD, 0, "boot A click", log_path)
    _require_exact_count(text, MARK_WM_FOCUS_READY, 1, "boot A focus ready", log_path)
    _require_exact_count(text, MARK_WM_FOCUS_SKIP, 0, "boot A focus skip", log_path)
    _require_exact_count(text, MARK_WM_KEY_FOCUS, 2, "boot A keyboard focus", log_path)
    _require_exact_count(text, MARK_WM_FOCUS_KEY, 1, "boot A focus proof", log_path)
    _require_exact_count(
        text, MARK_WM_FOCUS_NOOP, 1, "boot A focus no-op", log_path
    )
    _require_exact_count(text, "WMKEYROUTE ok ", 6, "boot A focus keys", log_path)
    _require_exact_count(text, "WMKEYDROP bad ", 0, "boot A focus keys", log_path)
    _require_exact_count(
        text, "WMSNAPFOCUS ok ", 1, "boot A focus snapshot", log_path
    )
    _require_exact_count(
        text, "WMKEYFOCUS ok side=left ", 1, "boot A left focus", log_path
    )
    _require_exact_count(
        text, "WMKEYFOCUS ok side=right ", 1, "boot A right focus", log_path
    )
    _require_exact_count(text, MARK_WM_FOCUS, 4, "boot A click focus", log_path)
    _require_exact_count(text, MARK_WM_HIDE_FOCUS, 1, "boot A hide focus", log_path)
    _require_exact_count(text, MARK_WM_MISS_FOCUS, 1, "boot A desktop miss", log_path)
    _require_exact_count(text, MARK_WM_SHOW_FOCUS, 1, "boot A show focus", log_path)
    _require_exact_count(text, MARK_WM_Z_FOCUS, 2, "boot A z focus", log_path)
    _require_exact_count(text, MARK_WM_LOCK_DOWN, 1, "boot A lock down", log_path)
    _require_exact_count(text, MARK_WM_CANCEL, 1, "boot A lock cancel", log_path)
    _require_exact_count(text, "WMMISSREADY ok ", 1, "boot A miss ready", log_path)
    _require_exact_count(text, "WMZREADY ok ", 1, "boot A z ready", log_path)
    _require_exact_count(text, "WMLOCKREADY ok ", 1, "boot A lock ready", log_path)
    cancel_matches = re.findall(
        r"WMCANCEL ok window=[0-9]+ phase=4 flags=([0-9]+) v=[0-9]+", text
    )
    if len(cancel_matches) != 1 or int(cancel_matches[0]) & 2 == 0:
        raise SmokeError("boot A lock cancel phase/synth 位/计数不符", log_path)
    # P4 版本算术 backstop：guest 已 fail-loud；此处防 fake corpus 伪造
    # 各段 marker 但版本链漂移。
    hide_v = re.search(r"WMHIDEFOCUS ok [^\n]*v=([0-9]+)", text)
    miss_v = re.search(r"WMMISSFOCUS ok [^\n]*v=([0-9]+)", text)
    show_v = re.search(r"WMSHOWFOCUS ok [^\n]*v=([0-9]+)", text)
    z_vs = [int(v) for v in re.findall(r"WMZFOCUS ok [^\n]*v=([0-9]+)", text)]
    lock_v = re.search(r"WMLOCKREADY ok [^\n]*v=([0-9]+)", text)
    cancel_v = re.search(r"WMCANCEL ok [^\n]*v=([0-9]+)", text)
    focus_vs = [
        int(v) for v in re.findall(r"WMFOCUS ok [^\n]*v=([0-9]+)", text)
    ]
    if hide_v is None or miss_v is None or show_v is None or lock_v is None or cancel_v is None or len(z_vs) != 2:
        raise SmokeError("boot A P4 版本 marker 缺失", log_path)
    if miss_v.group(1) != hide_v.group(1):
        raise SmokeError("boot A desktop miss 未保持 table version", log_path)
    if int(show_v.group(1)) <= int(miss_v.group(1)):
        raise SmokeError("boot A SHOW 未推进 table version", log_path)
    if z_vs[0] >= z_vs[1]:
        raise SmokeError("boot A z-order 版本未递增", log_path)
    if int(cancel_v.group(1)) <= int(lock_v.group(1)):
        raise SmokeError("boot A lock destroy/cancel 版本未递增", log_path)
    if any(b <= a for a, b in zip(focus_vs, focus_vs[1:])):
        raise SmokeError("boot A WMFOCUS 版本序列非严格递增", log_path)

    demo_after = text.count(MARK_APP_DEMO_UP)
    if demo_after != demo_before + 1:
        raise SmokeError(
            f"boot A click: 新增 {MARK_APP_DEMO_UP!r} "
            f"{demo_after - demo_before} 行 != 1（点击未 spawn APPDEMO？）",
            log_path,
        )
    quiet_after = text.count(MARK_APP_QUIET_UP)
    if quiet_after != quiet_before:
        raise SmokeError(
            f"boot A click: 新增 {MARK_APP_QUIET_UP!r} "
            f"{quiet_after - quiet_before} 行——点击误 spawn APPQUIET",
            log_path,
        )
    fs_after = text.count(MARK_APP_DEMO_FS)
    if fs_after != fs_before + 1:
        raise SmokeError(
            f"boot A click: 点击拉起的 APPDEMO fs RPC 标记新增 "
            f"{fs_after - fs_before} 行 != 1（真实进程未走文件系统链）",
            log_path,
        )
    if text.rfind(MARK_APP_DEMO_FS) <= ready_pos:
        raise SmokeError(
            "boot A click: APPDEMO fs 标记不在 click ready 之后",
            log_path,
        )
    _require_pos_after(text, MARK_CLICK_ANY, ready_pos, "boot A click", log_path)
    # LAYOUTW 由 layout 装载面在启动期落盘后打印（可早于 click ready），
    # 只做精确计数，不参与点击顺序断言。
    if text.rfind(MARK_APP_DEMO_UP) <= ready_pos:
        raise SmokeError(
            f"boot A click: 新增 {MARK_APP_DEMO_UP!r} 不在 click ready 之后",
            log_path,
        )


def verify_boot_a_real_swipe(
    text: str,
    click_pos: int,
    demo_after_click: int,
    fs_after_click: int = 0,
    log_path: Path | None = None,
) -> None:
    """Boot A 真实输入 swipe 精确验收：SWIPE 恰一且不产生新点击/spawn。"""
    _require_exact_count(text, MARK_SWIPE, 1, "boot A real swipe", log_path)
    _require_exact_count(text, MARK_CLICK_ANY, 1, "boot A real swipe", log_path)
    _require_exact_count(text, MARK_WM_ROUTE_DESKTOP, 2, "boot A real swipe", log_path)
    _require_exact_count(text, MARK_WM_ROUTE_WINDOW, 1, "boot A real swipe", log_path)
    _require_exact_count(text, MARK_WM_INPUT_DROP, 0, "boot A real swipe", log_path)
    _require_exact_count(text, MARK_INPUT_ROUTE_BAD, 0, "boot A real swipe", log_path)
    if text.count(MARK_APP_DEMO_UP) != demo_after_click:
        raise SmokeError(
            f"boot A real swipe: 新增 {MARK_APP_DEMO_UP!r} "
            f"{text.count(MARK_APP_DEMO_UP) - demo_after_click} 行（滑动误判点击）",
            log_path,
        )
    if text.count(MARK_APP_DEMO_FS) != fs_after_click:
        raise SmokeError(
            "boot A real swipe: 滑动产生了新 APPDEMO/fs 行",
            log_path,
        )
    _require_pos_after(text, MARK_SWIPE, click_pos, "boot A real swipe", log_path)


def verify_boot_b(text: str, log_path: Path | None = None) -> None:
    """Boot B 精确验收：LAYOUT==1、LAYOUTW==0、CLICK==0。"""
    _require_exact_count(text, MARK_WM_FOCUS_SKIP, 1, "boot B focus skip", log_path)
    _require_exact_count(text, MARK_WM_FOCUS + " ", 0, "boot B focus change", log_path)
    _require_exact_count(text, MARK_WM_KEY_FOCUS, 0, "boot B keyboard focus", log_path)
    _require_p4_absent(text, "boot B", log_path)
    n_layout = len(re.findall(RE_LAYOUT_N2, text))
    if n_layout != 1:
        raise SmokeError(
            f"boot B: `LAYOUT ok n=2` 出现 {n_layout} 次 != 1", log_path
        )
    _require_exact_count(text, MARK_LAYOUT, 1, "boot B", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 0, "boot B", log_path)
    _require_exact_count(text, MARK_CLICK_ANY, 0, "boot B", log_path)


def verify_layout_selfheal_boot(text: str, log_path: Path | None = None) -> None:
    """损坏 LAYOUT 自愈 boot：解析失败 → 默认布局 → LAYOUTW 恰一次。"""
    _require_exact_count(text, MARK_WM_FOCUS_SKIP, 1, "boot C focus skip", log_path)
    _require_exact_count(text, MARK_WM_FOCUS + " ", 0, "boot C focus change", log_path)
    _require_exact_count(text, MARK_WM_KEY_FOCUS, 0, "boot C keyboard focus", log_path)
    _require_p4_absent(text, "boot C", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 1, "layout self-heal", log_path)
    _require_exact_count(text, MARK_LAYOUT, 0, "layout self-heal", log_path)
    _require_exact_count(text, MARK_CLICK_ANY, 0, "layout self-heal", log_path)


def verify_normal_boot(
    text: str,
    snapshot_pos: int,
    browser_before: int,
    settings_before: int,
    files_before: int,
    photos_before: int,
    contacts_before: int,
    demo_before: int,
    log_path: Path | None = None,
) -> None:
    """常态隔离 + 六个 BerryUI app 验收。

    冒烟标记 CLICK/SWIPE/LAYOUTW/LAYOUT/click ready 仍全零；六个 app 都
    必须完成第二个真实进程生命周期。Settings 要求两次状态重绘；Files 要求
    READDIR/STAT/GET；Photos 要求 5000B 图像数据；Contacts 要求 4 条真实
    记录、详情/收藏/返回三次重绘；Browser/Photos 要求 layout-driven 点击。
    """
    _require_exact_count(text, MARK_WM_FOCUS_SKIP, 1, "normal focus skip", log_path)
    _require_exact_count(text, MARK_WM_FOCUS + " ", 0, "normal focus change", log_path)
    _require_exact_count(text, MARK_WM_KEY_FOCUS, 0, "normal keyboard focus", log_path)
    _require_p4_absent(text, "normal", log_path)
    _verify_r1_authority(text, "normal", log_path)
    for marker in (
        MARK_WM_SYSTEM_OCCLUDER, MARK_WM_AVOID_WINDOW, MARK_WM_AVOID_HIDE,
        MARK_WM_AVOID_SHOW, MARK_WM_AVOID_GRANT, MARK_WM_AVOID_FRAME,
        MARK_WM_OCCLUDED_DAMAGE_BAD,
    ):
        _require_exact_count(text, marker, 0, "normal R2 sentinel", log_path)
    _require_exact_count(text, MARK_WM_DAMAGE_CLIP, 1, "normal R2 damage clip", log_path)
    _require_exact_count(text, MARK_WM_RAW_BUFFER_BAD, 1, "normal R2 raw buffer gate", log_path)
    _require_exact_count(
        text, MARK_WM_CONFIG_SUBMIT_DENY, 1, "normal R3 unconfigured gate", log_path
    )
    _require_exact_count(
        text, MARK_WM_CONFIG_SURFACE_DENY, 1,
        "normal R3 unconfigured surface", log_path,
    )
    for marker in (
        MARK_WM_CONFIG_FOREIGN_ACK, MARK_WM_CONFIG_DESTROYED_ACK,
        MARK_WM_CONFIG_NOOP_EMPTY,
        MARK_WM_CONFIG_HIDE, MARK_WM_CONFIG_SHOW,
    ):
        _require_exact_count(text, marker, 1, "normal R3 common config", log_path)
    _require_exact_count(
        text, MARK_WM_CONFIG_FOCUS_EMPTY, 0, "normal R3 focus-only", log_path
    )
    _require_exact_count(text, "WMCONFIGWRAP", 0, "normal R3 wrap", log_path)
    for marker in (
        MARK_WM_CONFIG_ACK_DENIED, MARK_WM_CONFIG_CREATED, MARK_WM_CONFIG_EMPTY,
        MARK_WM_CONFIG_COALESCE, MARK_WM_CONFIG_STALE,
    ):
        _require_exact_count(text, marker, 0, "normal R3 config", log_path)
    _require_exact_count(text, MARK_CLICK_ANY, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_LAYOUT, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_CLICK_READY, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_SWIPE, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_APP_QUIET_UP, 0, "normal boot", log_path)

    _require_exact_count(text, MARK_BROWSER_UP, browser_before + 1, "normal Browser", log_path)
    _require_exact_count(text, MARK_BROWSER_WIN, browser_before + 1, "normal Browser", log_path)
    _require_exact_count(text, MARK_BROWSER_FRAME, browser_before + 1, "normal Browser", log_path)
    _require_exact_count(
        text, "BROWSERLAYOUT ok back=", browser_before + 1,
        "normal Browser layout", log_path,
    )
    _require_exact_count(
        text, MARK_BROWSER_RENDER, browser_before + 1,
        "normal Browser render", log_path,
    )
    _require_exact_count(text, "BROWSERHIT ok node=back", 1, "normal Browser back hit", log_path)
    _require_exact_count(text, MARK_BROWSER_BACK, 1, "normal Browser back", log_path)
    _require_exact_count(text, MARK_BROWSER_DESTROY, browser_before + 1, "normal Browser", log_path)
    _require_exact_count(text, MARK_BROWSER_EXIT, browser_before + 1, "normal Browser", log_path)

    _require_exact_count(text, MARK_SETTINGS_UP, settings_before + 1, "normal Settings", log_path)
    _require_exact_count(text, MARK_SETTINGS_WIN, settings_before + 1, "normal Settings", log_path)
    # runtime 初帧 + WLAN 重绘 + Bluetooth 重绘。
    _require_exact_count(text, MARK_SETTINGS_FRAME, settings_before + 3, "normal Settings", log_path)
    _require_exact_count(text, MARK_SETTINGS_GEOM, settings_before + 1, "normal Settings", log_path)
    _require_exact_count(text, "SETTINGSLAYOUT ok", settings_before + 1, "normal Settings layout", log_path)
    _require_exact_count(text, "SETTINGSRENDER ok seq=", settings_before + 3, "normal Settings render", log_path)
    _require_exact_count(
        text, "SETTINGSSTATE ok n=1 wlan=1 bluetooth=0", 1,
        "normal Settings WLAN state", log_path,
    )
    _require_exact_count(
        text, "SETTINGSSTATE ok n=2 wlan=1 bluetooth=1", 1,
        "normal Settings Bluetooth state", log_path,
    )
    _require_exact_count(text, "SETTINGSHIT ok node=back", 1, "normal Settings back hit", log_path)
    _require_exact_count(text, MARK_SETTINGS_TOGGLE1, 1, "normal Settings toggle", log_path)
    _require_exact_count(text, MARK_SETTINGS_TOGGLE2, 1, "normal Settings toggle", log_path)
    _require_exact_count(text, MARK_SETTINGS_BACK, 1, "normal Settings back", log_path)
    _require_exact_count(text, MARK_SETTINGS_DESTROY, settings_before + 1, "normal Settings", log_path)
    _require_exact_count(text, MARK_SETTINGS_EXIT, settings_before + 1, "normal Settings", log_path)
    _require_exact_count(text, MARK_WM_INPUT_DROP, 0, "normal GUI app", log_path)
    _require_exact_count(text, MARK_INPUT_ROUTE_BAD, 0, "normal GUI app", log_path)

    _require_exact_count(text, MARK_FILES_UP, files_before + 1, "normal Files", log_path)
    _require_exact_count(text, MARK_FILES_WIN, files_before + 1, "normal Files", log_path)
    _require_exact_count(text, MARK_FILES_SEL, files_before + 1, "normal Files", log_path)
    _require_exact_count(
        text, "FILESLAYOUT ok mode=0", files_before + 2,
        "normal Files list layout", log_path,
    )
    _require_exact_count(text, "FILESLAYOUT ok mode=1", 1, "normal Files detail layout", log_path)
    _require_exact_count(
        text, "FILESRENDER ok seq=", files_before + 3,
        "normal Files render", log_path,
    )
    _require_exact_count(text, "FILESHIT ok node=back", 1, "normal Files back hit", log_path)
    _require_exact_count(text, MARK_FILES_BACK, 1, "normal Files back", log_path)
    # runtime 初帧 + 详情帧 + 返回列表帧。
    _require_exact_count(text, MARK_FILES_FRAME, files_before + 3, "normal Files", log_path)
    _require_exact_count(text, MARK_FILES_LIST, files_before + 2, "normal Files", log_path)
    _require_exact_count(text, MARK_FILES_OPEN, 1, "normal Files detail", log_path)
    _require_exact_count(text, MARK_FILES_STAT, 1, "normal Files detail", log_path)
    _require_exact_count(text, MARK_FILES_READ, 1, "normal Files detail", log_path)
    _require_exact_count(text, MARK_FILES_DESTROY, files_before + 1, "normal Files", log_path)
    _require_exact_count(text, MARK_FILES_EXIT, files_before + 1, "normal Files", log_path)

    _require_exact_count(text, MARK_PHOTOS_UP, photos_before + 1, "normal Photos", log_path)
    _require_exact_count(text, MARK_PHOTOS_WIN, photos_before + 1, "normal Photos", log_path)
    _require_exact_count(text, MARK_PHOTOS_LIST, photos_before + 2, "normal Photos", log_path)
    _require_exact_count(text, MARK_PHOTOS_DATA, photos_before + 1, "normal Photos", log_path)
    _require_exact_count(
        text, "PHOTOSLAYOUT ok mode=0", photos_before + 2,
        "normal Photos grid layout", log_path,
    )
    _require_exact_count(text, "PHOTOSLAYOUT ok mode=1", 1, "normal Photos view layout", log_path)
    _require_exact_count(
        text, MARK_PHOTOS_RENDER, photos_before + 3,
        "normal Photos render", log_path,
    )
    _require_exact_count(text, "PHOTOSHIT ok node=back", 1, "normal Photos back hit", log_path)
    _require_exact_count(text, MARK_PHOTOS_BACK, 1, "normal Photos back", log_path)
    # runtime 缩略图初帧 + 查看大图 + 返回网格。
    _require_exact_count(text, MARK_PHOTOS_FRAME, photos_before + 3, "normal Photos", log_path)
    _require_exact_count(text, MARK_PHOTOS_OPEN, 1, "normal Photos view", log_path)
    _require_exact_count(text, MARK_PHOTOS_VIEW, 1, "normal Photos view", log_path)
    _require_exact_count(text, MARK_PHOTOS_DESTROY, photos_before + 1, "normal Photos", log_path)
    _require_exact_count(text, MARK_PHOTOS_EXIT, photos_before + 1, "normal Photos", log_path)

    _require_exact_count(text, MARK_CONTACTS_UP, contacts_before + 1, "normal Contacts", log_path)
    _require_exact_count(text, MARK_CONTACTS_WIN, contacts_before + 1, "normal Contacts", log_path)
    _require_exact_count(text, MARK_CONTACTS_LIST, contacts_before + 2, "normal Contacts", log_path)
    _require_exact_count(text, MARK_CONTACTS_READ, contacts_before + 1, "normal Contacts", log_path)
    _require_exact_count(
        text, "CONTACTSLAYOUT ok mode=0", contacts_before + 2,
        "normal Contacts list layout", log_path,
    )
    _require_exact_count(
        text, "CONTACTSLAYOUT ok mode=1", 2,
        "normal Contacts detail layout", log_path,
    )
    _require_exact_count(
        text, "CONTACTSRENDER ok seq=", contacts_before + 4,
        "normal Contacts render", log_path,
    )
    _require_exact_count(text, "CONTACTSHIT ok node=back", 1, "normal Contacts back hit", log_path)
    _require_exact_count(text, MARK_CONTACTS_BACK, 1, "normal Contacts back", log_path)
    # runtime 初帧 + 详情 + 收藏 + 返回列表。
    _require_exact_count(text, MARK_CONTACTS_FRAME, contacts_before + 4, "normal Contacts", log_path)
    _require_exact_count(text, "CONTACTSDETAIL ok i=1", 1, "normal Contacts detail", log_path)
    _require_exact_count(text, MARK_CONTACTS_FAV, 1, "normal Contacts favorite", log_path)
    _require_exact_count(text, MARK_CONTACTS_DESTROY, contacts_before + 1, "normal Contacts", log_path)
    _require_exact_count(text, MARK_CONTACTS_EXIT, contacts_before + 1, "normal Contacts", log_path)

    # 常态布局把 APPDEMO 放 page1；GUI app 点击不得误 spawn。
    if text.count(MARK_APP_DEMO_UP) != demo_before:
        raise SmokeError(
            "normal GUI app: APPDEMO 计数漂移（误命中/误 spawn）",
            log_path,
        )
    if text.find(MARK_BROWSER_BACK) < snapshot_pos:
        raise SmokeError("normal Browser: BACK 早于快照", log_path)
    if text.find(MARK_SETTINGS_TOGGLE1) < snapshot_pos:
        raise SmokeError("normal Settings: TOGGLE1 早于快照", log_path)
    if text.find(MARK_SETTINGS_BACK) < snapshot_pos:
        raise SmokeError("normal Settings: BACK 早于快照", log_path)
    if text.find(MARK_FILES_READ) < snapshot_pos:
        raise SmokeError("normal Files: READ 早于快照", log_path)
    if text.rfind(MARK_FILES_EXIT) < snapshot_pos:
        raise SmokeError("normal Files: EXIT 不在快照后", log_path)
    if text.find(MARK_PHOTOS_VIEW) < snapshot_pos:
        raise SmokeError("normal Photos: VIEW 早于快照", log_path)
    if text.rfind(MARK_PHOTOS_EXIT) < snapshot_pos:
        raise SmokeError("normal Photos: EXIT 不在快照后", log_path)
    if text.find(MARK_CONTACTS_DETAIL) < snapshot_pos:
        raise SmokeError("normal Contacts: DETAIL 早于快照", log_path)
    if text.rfind(MARK_CONTACTS_EXIT) < snapshot_pos:
        raise SmokeError("normal Contacts: EXIT 不在快照后", log_path)

def ppm_pixel(path: Path, x: int, y: int) -> tuple[int, int, int]:
    """读 QEMU P6 PPM 指定像素（GUI app 反 mock 像素锚）。"""
    data = path.read_bytes()
    fields: list[bytes] = []
    pos = 0
    while len(fields) < 4:
        while pos < len(data) and data[pos : pos + 1].isspace():
            pos += 1
        if pos < len(data) and data[pos] == ord(b"#"):
            pos = data.index(b"\n", pos) + 1
            continue
        end = pos
        while end < len(data) and not data[end : end + 1].isspace():
            end += 1
        fields.append(data[pos:end])
        pos = end
    if fields[0] != b"P6":
        raise SmokeError(f"非 P6 PPM：{path} magic={fields[0]!r}")
    w, h, maxv = (int(v) for v in fields[1:4])
    if maxv != 255 or x >= w or y >= h:
        raise SmokeError(f"PPM 域非法/越界：{path} {w}x{h} max={maxv} ({x},{y})")
    off = pos + 1 + y * w * 3 + x * 3
    raw = data[off : off + 3]
    if len(raw) != 3:
        raise SmokeError(f"PPM 像素越界：{path} ({x},{y})")
    return raw[0], raw[1], raw[2]


def assert_pixel_close(
    got: tuple[int, int, int], want: tuple[int, int, int], what: str,
    log_path: Path | None = None,
) -> None:
    if any(abs(a - b) > 8 for a, b in zip(got, want)):
        raise SmokeError(f"像素锚 {what}: {got} != {want}±8", log_path)


UITEST_LAYOUT_BUTTON_RE = re.compile(
    r"UITESTLAYOUT ok page=(\d+) appbar=(-?\d+,-?\d+,-?\d+,-?\d+) "
    r"body=(-?\d+,-?\d+,-?\d+,-?\d+) button=(-?\d+),(-?\d+),(\d+),(\d+)"
)


def latest_uitest_button(
    text: str, page: int, start: int = 0,
) -> tuple[int, int, int, int]:
    matches = []
    for m in UITEST_LAYOUT_BUTTON_RE.finditer(text):
        if m.start() >= start and int(m.group(1)) == page:
            matches.append(m)
    if not matches:
        raise SmokeError(f"UITEST layout page={page} 缺失", None)
    m = matches[-1]
    return tuple(int(m.group(i)) for i in range(4, 8))  # type: ignore[return-value]


SETTINGS_LAYOUT_RE = re.compile(
    r"SETTINGSLAYOUT ok back=(-?\d+),(-?\d+),(\d+),(\d+) "
    r"wlan=(-?\d+),(-?\d+),(\d+),(\d+) "
    r"bluetooth=(-?\d+),(-?\d+),(\d+),(\d+)"
)


def latest_settings_rect(
    text: str, name: str, start: int = 0,
) -> tuple[int, int, int, int]:
    groups = {"back": (1, 2, 3, 4), "wlan": (5, 6, 7, 8), "bluetooth": (9, 10, 11, 12)}
    matches = [
        match for match in SETTINGS_LAYOUT_RE.finditer(text) if match.start() >= start
    ]
    if not matches:
        raise SmokeError(f"Settings layout {name} 缺失", None)
    match = matches[-1]
    return tuple(int(match.group(i)) for i in groups[name])  # type: ignore[return-value]


def rect_center(rect: tuple[int, int, int, int]) -> tuple[int, int]:
    return rect[0] + rect[2] // 2, rect[1] + rect[3] // 2


def latest_named_rect(
    pattern: re.Pattern[str], mode: int, name: str, text: str, start: int = 0,
) -> tuple[int, int, int, int]:
    groups = {"back": (2, 3, 4, 5), "extra": (6, 7, 8, 9)}
    matches = [
        match for match in pattern.finditer(text)
        if match.start() >= start and int(match.group(1)) == mode
    ]
    if not matches:
        raise SmokeError(f"layout mode={mode} {name} 缺失", None)
    match = matches[-1]
    indexes = groups["back"] if name == "back" else groups["extra"]
    raw = [match.group(i) for i in indexes]
    if any(value is None for value in raw):
        raise SmokeError(f"layout mode={mode} {name} rect 缺失", None)
    values = [int(value) for value in raw]
    return values[0], values[1], values[2], values[3]


FILES_LAYOUT_RE = re.compile(
    r"FILESLAYOUT ok mode=(\d+) back=(-?\d+),(-?\d+),(\d+),(\d+)"
    r"(?: open=(-?\d+),(-?\d+),(\d+),(\d+))?"
)
CONTACTS_LAYOUT_RE = re.compile(
    r"CONTACTSLAYOUT ok mode=(\d+) back=(-?\d+),(-?\d+),(\d+),(\d+)"
    r"(?: (?:row|favorite)=(-?\d+),(-?\d+),(\d+),(\d+))?"
)
BROWSER_LAYOUT_RE = re.compile(
    r"BROWSERLAYOUT ok back=(-?\d+),(-?\d+),(\d+),(\d+)"
)
PHOTOS_LAYOUT_RE = re.compile(
    r"PHOTOSLAYOUT ok mode=(\d+) back=(-?\d+),(-?\d+),(\d+),(\d+) "
    r"image=(-?\d+),(-?\d+),(\d+),(\d+)"
    r"(?: open=(-?\d+),(-?\d+),(\d+),(\d+))?"
)


def latest_browser_back(text: str, start: int = 0) -> tuple[int, int, int, int]:
    matches = [
        match for match in BROWSER_LAYOUT_RE.finditer(text)
        if match.start() >= start
    ]
    if not matches:
        raise SmokeError("Browser layout back 缺失", None)
    match = matches[-1]
    return tuple(int(match.group(i)) for i in range(1, 5))  # type: ignore[return-value]


def latest_photos_rect(
    text: str, mode: int, name: str, start: int = 0,
) -> tuple[int, int, int, int]:
    groups = {
        "back": (2, 3, 4, 5),
        "image": (6, 7, 8, 9),
        "open": (10, 11, 12, 13),
    }
    matches = [
        match for match in PHOTOS_LAYOUT_RE.finditer(text)
        if match.start() >= start and int(match.group(1)) == mode
    ]
    if not matches:
        raise SmokeError(f"Photos layout mode={mode} {name} 缺失", None)
    match = matches[-1]
    raw = [match.group(i) for i in groups[name]]
    if any(value is None for value in raw):
        raise SmokeError(f"Photos layout mode={mode} {name} rect 缺失", None)
    return tuple(int(value) for value in raw)  # type: ignore[return-value]




def verify_uitest_normal(
    text: str, snapshot_pos: int, before: int, log_path: Path | None = None,
) -> None:
    """GUI F5：BerryUI + Berry Room 系统验收 app 的真链生命周期。"""
    _require_exact_count(text, MARK_UITEST_UP, before + 1, "normal UITEST", log_path)
    _require_exact_count(text, MARK_UITEST_ROOM, before + 1, "normal UITEST", log_path)
    _require_exact_count(text, MARK_UITEST_WIN, before + 1, "normal UITEST", log_path)
    _require_exact_count(text, MARK_UITEST_FRAME, before + 1, "normal UITEST", log_path)
    _require_exact_count(text, MARK_UITEST_PROBE, before, "normal UITEST boot-probe", log_path)
    _require_exact_count(
        text,
        "UITESTLAYOUT ok page=0 appbar=0,0,1080,60 body=0,60,1080,1860 "
        "button=12,126,1056,60 scroll=12,377,1056,24 clip=12,377,1056,24",
        2,
        "normal UITEST page0 layout",
        log_path,
    )
    _require_exact_count(
        text,
        "UITESTLAYOUT ok page=1 appbar=0,0,1080,60 body=0,60,1080,1860 "
        "button=12,255,1056,60 scroll=0,60,1080,1860 clip=0,60,1080,1860",
        1,
        "normal UITEST page1 layout",
        log_path,
    )
    _require_exact_count(text, "UITESTHIT ok node=button", 3, "normal UITEST hit dump", log_path)
    _require_exact_count(
        text, "UITESTAST ok bytes=712 a11y=8", 2, "normal UITEST AST", log_path
    )
    _require_exact_count(text, "UITESTRENDER ok seq=", 11, "normal UITEST render", log_path)
    _require_exact_count(
        text,
        "UITESTFOCUSMOVE ok step=1 from=255 to=0 state=3",
        1, "normal UITEST first focus move", log_path,
    )
    _require_exact_count(
        text,
        "UITESTFOCUSMOVE ok step=2 from=0 to=1 state=3",
        1, "normal UITEST second focus move", log_path,
    )
    _require_exact_count(
        text,
        "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0",
        1, "normal UITEST focus activate", log_path,
    )
    _require_exact_count(text, MARK_UITEST_CLIP, 2, "normal UITEST clip", log_path)
    _require_exact_count(
        text,
        "UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 safe=0,0,0,0",
        1,
        "normal UITEST surface",
        log_path,
    )
    _require_exact_count(
        text, "INPUTDEV ok tab=", 1, "normal input devices", log_path
    )
    _require_exact_count(text, MARK_UITEST_KEY_WAIT, 1, "normal UITEST keyboard wait", log_path)
    _require_exact_count(
        text, "INPUTKEYDROPS ok n=0", 1, "normal keyboard drops baseline", log_path
    )
    _require_exact_count(
        text, "INPUTKEYDROPS bad", 0, "normal keyboard drops nonzero", log_path
    )
    _require_exact_count(
        text, "WMKEYDROP bad ", 6, "normal unauthorized keyboard drops", log_path
    )
    route_down = re.findall(
        r"WMKEYROUTE ok sid=([0-9]+) window=([0-9]+) code=([0-9]+) action=1", text
    )
    route_up = re.findall(
        r"WMKEYROUTE ok sid=([0-9]+) window=([0-9]+) code=([0-9]+) action=0", text
    )
    if (
        len(route_down) != 4
        or len(route_up) != 4
        or route_down != route_up
        or sorted(entry[2] for entry in route_down) != ["15", "15", "28", "28"]
        or any(entry[0] == "0" or entry[1] == "0" for entry in route_down)
    ):
        raise SmokeError(
            f"normal keyboard route: down={route_down} up={route_up}",
            log_path,
        )
    _require_exact_count(
        text,
        "UITESTKEY ok code=28 tab=2 downs=2 ups=2",
        1,
        "normal UITEST keyboard",
        log_path,
    )
    _require_exact_count(text, "UITESTSTATE ok page=1", 2, "normal UITEST push", log_path)
    _require_exact_count(text, "UITESTBACK ok page=0", 2, "normal UITEST pop", log_path)
    if not re.search(r"UITESTWIN ok w=1080 h=1920", text):
        raise SmokeError("normal UITEST: runtime 窗口几何缺失/错误", log_path)
    if not re.search(r"UITESTSCROLL ok offset=24(?![0-9]) switch=1", text):
        raise SmokeError("normal UITEST: boot-probe 状态/滚动链缺失", log_path)
    clip_matches = list(
        re.finditer(
            r"UITESTCLIP ok port=(-?\d+),(-?\d+),(\d+),(\d+) "
            r"item=(-?\d+),(-?\d+),(\d+),(\d+) "
            r"probe=(\d+),(\d+) seq=(\d+)",
            text,
        )
    )
    if len(clip_matches) != 2:
        raise SmokeError("normal UITEST: Clip guest 几何缺失/重复", log_path)
    parsed_clips = []
    for match in clip_matches:
        port = tuple(int(match.group(i)) for i in range(1, 5))
        item = tuple(int(match.group(i)) for i in range(5, 9))
        probe = (int(match.group(9)), int(match.group(10)))
        if (
            port[3] >= item[3]
            or item[1] >= port[1] + port[3]
            or item[1] + item[3] <= port[1] + port[3]
            or probe != (540, port[1] + port[3] + 8)
        ):
            raise SmokeError(
                f"normal UITEST: Clip 几何不诚实 port={port} item={item} probe={probe}",
                log_path,
            )
        parsed_clips.append((port, item, probe))
    scrolled, reset = parsed_clips
    if scrolled[1][1] >= scrolled[0][1] or reset[1][1] != reset[0][1]:
        raise SmokeError(
            f"normal UITEST: Clip 两个形态缺失 scrolled={scrolled} reset={reset}",
            log_path,
        )
    input_match = None
    for m in re.finditer(r"UITESTINPUT ok x=(\d+) y=(\d+)", text):
        input_match = m
    if input_match is None:
        raise SmokeError("normal UITEST: owner-local PhaseUP 缺失", log_path)
    button = latest_uitest_button(text, 1, snapshot_pos)
    want = (
        button[0] + button[2] // 2 - 1,
        button[1] + button[3] // 2 - 1,
    )
    got = (int(input_match.group(1)), int(input_match.group(2)))
    if got != want:
        raise SmokeError(
            f"normal UITEST: owner-local PhaseUP 坐标错误 got={got} want={want}", log_path
        )
    _require_exact_count(text, MARK_UITEST_INPUT, 1, "normal UITEST input", log_path)
    _require_exact_count(text, MARK_UITEST_DESTROY, before + 1, "normal UITEST", log_path)
    _require_exact_count(text, MARK_UITEST_EXIT, before + 1, "normal UITEST", log_path)
    if text.find(MARK_UITEST_INPUT) < snapshot_pos:
        raise SmokeError("normal UITEST: INPUT 早于快照", log_path)
    if text.rfind(MARK_UITEST_EXIT) < snapshot_pos:
        raise SmokeError("normal UITEST: EXIT 不在快照后", log_path)


def ppm_crop_colors(path: Path, box: tuple[int, int, int, int]) -> set[tuple[int, int, int]]:
    """读 P6 PPM 裁剪区 RGB 集合（HD A8 反马赛克锚的客观面）。"""
    x0, y0, x1, y1 = box
    if x0 >= x1 or y0 >= y1:
        raise SmokeError(f"PPM 裁剪域非法：{path} {box}")
    data = path.read_bytes()
    fields: list[bytes] = []
    pos = 0
    while len(fields) < 4:
        while pos < len(data) and data[pos : pos + 1].isspace():
            pos += 1
        if pos < len(data) and data[pos] == ord(b"#"):
            pos = data.index(b"\n", pos) + 1
            continue
        end = pos
        while end < len(data) and not data[end : end + 1].isspace():
            end += 1
        fields.append(data[pos:end])
        pos = end
    if fields[0] != b"P6":
        raise SmokeError(f"非 P6 PPM：{path} magic={fields[0]!r}")
    w, h, maxv = (int(v) for v in fields[1:4])
    if maxv != 255 or x1 > w or y1 > h:
        raise SmokeError(f"PPM 裁剪越界：{path} {w}x{h} max={maxv} box={box}")
    base = pos + 1
    out: set[tuple[int, int, int]] = set()
    for y in range(y0, y1):
        row = base + y * w * 3
        for x in range(x0, x1):
            off = row + x * 3
            out.add((data[off], data[off + 1], data[off + 2]))
    return out


def assert_color_count(
    path: Path, box: tuple[int, int, int, int], minimum: int, what: str,
    log_path: Path | None = None,
) -> None:
    got = len(ppm_crop_colors(path, box))
    if got < minimum:
        raise SmokeError(
            f"反马赛克色数锚 {what}: {got} < {minimum}（疑似点阵整数放大）",
            log_path,
        )


def qemu_command(
    args, disk: Path, vars_path: Path, sock_path: Path, serial_path: Path,
    display: str = "800x600@60",
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
        # P2：keyboard 追加在 tablet 后——virtio-mmio 倒序建槽，tablet 槽位
        # 零漂移；QMP key 事件必须省 device 键广播注入。
        "-device",
        "virtio-keyboard-device,id=kbd0",
        "-device",
        "ramfb",
        "-fw_cfg",
        f"name=opt/berry/display,string={display}",
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
            "enable=input_event_key",
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

    def __init__(
        self, phase: str, args, run_dir: Path, disk: Path, suffix: str,
        display: str = "800x600@60",
    ):
        self.phase = phase
        self.args = args
        self.run_dir = run_dir
        self.disk = disk
        self.suffix = suffix
        self.display = display
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
            self.args, self.disk, self.vars_path, self.sock_path,
            self.log_path, self.display,
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
        # QMP 单客户端：P3 早期 gfxdemo 焦点相已连接时复用同一连接，
        # 不得叠开第二条 socket。
        if self.qmp is not None:
            return
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
        "SWIPE": (src / "frameloop.rs", "SWIPE ok n="),
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


def _boot_base_markers() -> list[str]:
    return [
        MARK_BERRYBOOT, MARK_ROOTFS, MARK_APP_N9,
        MARK_WM_CONN1, MARK_WM_CONN2, MARK_WM_DENIED, MARK_WM_SURFACE_DENIED, MARK_WM_ORPHAN, MARK_WM_2FRAME,
        MARK_WM_DAMAGE_CLIP, MARK_WM_RAW_BUFFER_BAD,
        MARK_WM_CONFIG_SUBMIT_DENY, MARK_WM_CONFIG_SURFACE_DENY,
        MARK_WM_CONFIG_FOREIGN_ACK, MARK_WM_CONFIG_NOOP_EMPTY,
        MARK_WM_CONFIG_HIDE, MARK_WM_CONFIG_SHOW,
        MARK_WM_DEAD1, MARK_WM_DEAD2, MARK_WM_CLEAN, MARK_WM_CONN3,
        MARK_WM_CONN4, MARK_BROWSER_UP, MARK_BROWSER_WIN, MARK_BROWSER_FRAME,
        MARK_BROWSER_PROBE, MARK_BROWSER_DESTROY, MARK_BROWSER_EXIT,
        MARK_WM_CONN5, MARK_SETTINGS_UP, MARK_SETTINGS_WIN,
        MARK_SETTINGS_FRAME, MARK_SETTINGS_GEOM, MARK_SETTINGS_PROBE,
        MARK_SETTINGS_DESTROY, MARK_SETTINGS_EXIT,
        MARK_WM_CONN6, MARK_FILES_UP, MARK_FILES_WIN, MARK_FILES_LIST,
        MARK_FILES_SEL, MARK_FILES_FRAME, MARK_FILES_PROBE,
        MARK_FILES_DESTROY, MARK_FILES_EXIT,
        MARK_WM_CONN7, MARK_PHOTOS_UP, MARK_PHOTOS_WIN,
        MARK_PHOTOS_LIST, MARK_PHOTOS_DATA, MARK_PHOTOS_FRAME,
        MARK_PHOTOS_PROBE, MARK_PHOTOS_DESTROY, MARK_PHOTOS_EXIT,
        MARK_WM_CONN8, MARK_CONTACTS_UP, MARK_CONTACTS_WIN,
        MARK_CONTACTS_LIST, MARK_CONTACTS_FRAME,
        MARK_CONTACTS_PROBE, MARK_CONTACTS_DESTROY, MARK_CONTACTS_EXIT,
        MARK_WM_CONN9, MARK_UITEST_UP, MARK_UITEST_ROOM,
        MARK_UITEST_WIN, MARK_UITEST_FRAME, MARK_UITEST_PROBE,
        MARK_UITEST_DESTROY, MARK_UITEST_EXIT,
        MARK_CJK_PROBE, MARK_INPUT_DROPS_OK, MARK_LAUNCH, MARK_PAGE,
    ]


def _wait_boot_batch(boot: Boot, timeout: float) -> None:
    """基础 5 + boot 批 app 证据：先把 boot 批 APPDEMO up 吃完再点击。

    Contacts 数据源分拓扑：GUI8 哨兵盘无 CONTACTS.TXT → LOCAL；
    normal 盘写入 CONTACTS.TXT → READ。二者必居其一，不能同时缺席。
    """
    boot.wait_markers(_boot_base_markers(), timeout)
    if not boot.wait_any(
        [MARK_CONTACTS_LOCAL, MARK_CONTACTS_READ], timeout
    ):
        raise MissingMarkers(
            "boot batch",
            f"{MARK_CONTACTS_LOCAL} 或 {MARK_CONTACTS_READ}",
            timeout, boot.log_path,
        )
    boot.wait_markers([MARK_APP_DEMO_UP], timeout)
    _verify_r1_authority(boot.log.text, f"boot batch {boot.phase}", boot.log_path)
    if boot.phase != "normal":
        _verify_r2_occlusion(boot.log.text, f"boot batch {boot.phase}", boot.log_path)
        _verify_r3_config(boot.log.text, f"boot batch {boot.phase}", boot.log_path)

def _verify_r2_occlusion(text: str, phase: str, log_path: Path | None = None) -> None:
    """R2 Occlusion Contract：真实 SYSTEM 遮挡 + raw/visible 双几何证据。"""
    checks = {
        MARK_WM_SYSTEM_OCCLUDER: 1,
        MARK_WM_AVOID_WINDOW: 1,
        MARK_WM_AVOID_HIDE: 1,
        MARK_WM_AVOID_SHOW: 1,
        MARK_WM_AVOID_GRANT: 1,
        MARK_WM_AVOID_FRAME: 1,
        MARK_WM_OCCLUDED_DAMAGE_BAD: 1,
        MARK_WM_DAMAGE_CLIP: 1,
        MARK_WM_RAW_BUFFER_BAD: 1,
    }
    for marker, expected in checks.items():
        _require_exact_count(text, marker, expected, f"{phase} R2 occlusion", log_path)

def _verify_r1_authority(text: str, phase: str, log_path: Path | None = None) -> None:
    """R1 Window/Surface Authority：正反例均恰一次。"""
    checks = {
        MARK_WM_SYSTEM_AUTH: 1,
        MARK_WM_SYSTEM_CLIENT_DENIED: 1,
        MARK_WM_SYSTEM_SEED_DENIED: 1,
        MARK_WM_SYSTEM_CONTROL_DENIED: 1,
        MARK_WM_ROLE_MASQUERADE_DENIED: 1,
        MARK_WM_LAYER_DENIED: 1,
        MARK_WM_PARENT_DENIED: 1,
        MARK_WM_GLOBAL_DENIED: 1,
        MARK_WM_DAMAGE_BAD: 1,
    }
    for marker, expected in checks.items():
        _require_exact_count(text, marker, expected, f"{phase} R1 authority", log_path)


def _verify_r3_config(text: str, phase: str, log_path: Path | None = None) -> None:
    """R3 Config Event：CREATED/ACK/coalescing/stale 全部恰一次。"""
    checks = {
        MARK_WM_CONFIG_SUBMIT_DENY: 1,
        MARK_WM_CONFIG_SURFACE_DENY: 1,
        MARK_WM_CONFIG_FOREIGN_ACK: 1,
        MARK_WM_CONFIG_DESTROYED_ACK: 1,
        MARK_WM_CONFIG_NOOP_EMPTY: 1,
        MARK_WM_CONFIG_HIDE: 1,
        MARK_WM_CONFIG_SHOW: 1,
        MARK_WM_CONFIG_ACK_DENIED: 1,
        MARK_WM_CONFIG_CREATED: 1,
        MARK_WM_CONFIG_EMPTY: 1,
        MARK_WM_CONFIG_COALESCE: 1,
        MARK_WM_CONFIG_STALE: 1,
        "WMCONFIGWRAP": 0,
    }
    for marker, expected in checks.items():
        _require_exact_count(text, marker, expected, f"{phase} R3 config", log_path)


def _warn_input_armed(boot: Boot) -> None:
    # launcher 已挂 INPUT_POLL（请求已入 inputd 队列）。这是状态谓词，
    # 不保证 inputd 已 harvest——点击后的 CLICK 等待窗兜住后者。
    if not boot.wait_any([MARK_INPUT_ARMED], 60.0):
        warn(
            f"未等到「{MARK_INPUT_ARMED}」——QMP 事件可能早于 guest "
            "挂缓冲而被设备丢弃；仍继续"
        )


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
            f"sha256={sha256_file(disk)[:16]}…，LBA0 印章 + 精确 GUI8.SMK）"
        )

    boot_a: Boot | None = None
    boot_b: Boot | None = None
    success = False
    try:
        # ---------------- Boot A ----------------
        boot_a = Boot("A", args, run_dir, disk, "a")
        boot_a.start()

        # —— P3 click-to-focus：gfxdemo 双半屏窗口先声明 Retained+Keyboard，
        #    GUI8 用真实 QMP 点击左窗+KEY_A、右窗+KEY_A。guest 只在收到
        #    对应 pointer DOWN 与同侧 key pair 后打印证明。
        boot_a.wait_markers([MARK_WM_FOCUS_READY], args.click_ready_timeout)
        focus_ready = re.search(
            r"WMFOCUSREADY ok left=([0-9]+) right=([0-9]+) "
            r"left_rect=0,0,([0-9]+),([0-9]+) "
            r"right_rect=([0-9]+),0,([0-9]+),([0-9]+)",
            boot_a.log.text,
        )
        if focus_ready is None:
            raise SmokeError("boot A focus-ready geometry absent", boot_a.log_path)
        focus_left_id, focus_right_id = map(int, focus_ready.group(1, 2))
        focus_lw, focus_h, focus_rx, focus_rw = map(
            int, focus_ready.group(3, 4, 5, 6)
        )
        boot_a.qmp_connect()
        log(
            "boot A: QMP 点击左窗 + KEY_A（click-to-focus 左侧）"
        )
        focus_dw = focus_lw + focus_rw
        boot_a.qmp.click(focus_lw // 2, focus_h // 2, focus_dw, focus_h)
        left_focus_re = rf"WMFOCUS ok sid=1 window={focus_left_id} v=([0-9]+)"
        boot_a.wait_regex(left_focus_re, args.click_timeout)
        boot_a.log.poll()
        left_focus = re.search(left_focus_re, boot_a.log.text)
        if left_focus is None:
            raise SmokeError("boot A left focus version absent", boot_a.log_path)
        boot_a.qmp.tap_key(30)
        boot_a.wait_markers(
            ["WMKEYFOCUS ok side=left "], args.click_timeout
        )
        log(
            "boot A: QMP 点击右窗 + KEY_A（click-to-focus 右侧）"
        )
        boot_a.qmp.click(
            focus_rx + focus_rw // 2, focus_h // 2, focus_dw, focus_h
        )
        right_focus_re = rf"WMFOCUS ok sid=2 window={focus_right_id} v=([0-9]+)"
        boot_a.wait_regex(right_focus_re, args.click_timeout)
        boot_a.log.poll()
        right_focus = re.search(right_focus_re, boot_a.log.text)
        if right_focus is None:
            raise SmokeError("boot A right focus version absent", boot_a.log_path)
        if int(right_focus.group(1)) <= int(left_focus.group(1)):
            raise SmokeError("boot A focus snapshot version 未递增", boot_a.log_path)
        boot_a.qmp.tap_key(30)
        boot_a.wait_markers(
            ["WMKEYFOCUS ok side=right ", MARK_WM_FOCUS_KEY],
            args.click_timeout,
        )
        log("boot A: QMP 重复点击右窗（已聚焦 no-op）")
        boot_a.qmp.click(
            focus_rx + focus_rw // 2, focus_h // 2, focus_dw, focus_h
        )
        boot_a.wait_markers(["WMFOCUSNOOP ok "], args.click_timeout)

        # —— P4 window policy matrix：HIDE 归还焦点 → desktop miss 不清焦
        #    → SHOW 接焦 → z-order 两次命中 → 锁定窗销毁后 CANCEL。
        boot_a.wait_markers([MARK_WM_HIDE_FOCUS], args.click_timeout)
        boot_a.wait_regex(r"WMMISSREADY ok x=([0-9]+) y=([0-9]+)", args.click_timeout)
        boot_a.log.poll()
        miss_ready = re.search(
            r"WMMISSREADY ok x=([0-9]+) y=([0-9]+)", boot_a.log.text
        )
        if miss_ready is None:
            raise SmokeError("boot A desktop-miss geometry absent", boot_a.log_path)
        miss_x, miss_y = map(int, miss_ready.groups())
        log("boot A: QMP 点击已隐藏右窗区域 + KEY_A（desktop miss）")
        boot_a.qmp.click(miss_x, miss_y, focus_dw, focus_h)
        boot_a.qmp.tap_key(30)
        boot_a.wait_markers(
            [MARK_WM_MISS_FOCUS, MARK_WM_SHOW_FOCUS], args.click_timeout
        )

        boot_a.wait_regex(r"WMZREADY ok x=([0-9]+) y=([0-9]+)", args.click_timeout)
        boot_a.log.poll()
        z_ready = re.search(r"WMZREADY ok x=([0-9]+) y=([0-9]+)", boot_a.log.text)
        if z_ready is None:
            raise SmokeError("boot A z-order geometry absent", boot_a.log_path)
        z_x, z_y = map(int, z_ready.groups())
        log("boot A: QMP 点击重叠带（z 平局表序左窗胜）")
        boot_a.qmp.click(z_x, z_y, focus_dw, focus_h)
        boot_a.wait_markers(
            [f"WMZFOCUS ok winner={focus_left_id} "], args.click_timeout
        )
        boot_a.wait_markers(["WMZRAISE ok "], args.click_timeout)
        log("boot A: QMP 重复点击重叠带（右窗 z=1 胜）")
        boot_a.qmp.click(z_x, z_y, focus_dw, focus_h)
        boot_a.wait_markers(
            [f"WMZFOCUS ok winner={focus_right_id} "], args.click_timeout
        )

        boot_a.wait_regex(
            r"WMLOCKREADY ok window=([0-9]+) x=([0-9]+) y=([0-9]+) "
            r"w=([0-9]+) h=([0-9]+)",
            args.click_timeout,
        )
        boot_a.log.poll()
        lock_ready = re.search(
            r"WMLOCKREADY ok window=([0-9]+) x=([0-9]+) y=([0-9]+) "
            r"w=([0-9]+) h=([0-9]+)",
            boot_a.log.text,
        )
        if lock_ready is None:
            raise SmokeError("boot A lock-window geometry absent", boot_a.log_path)
        lock_id, lock_x, lock_y, lock_w, lock_h = map(int, lock_ready.groups())
        log(f"boot A: QMP raw DOWN 锁定第三窗 id={lock_id}")
        boot_a.qmp.pointer_down(
            lock_x + lock_w // 2,
            lock_y + lock_h // 2,
            focus_dw,
            focus_h,
        )
        boot_a.wait_markers([MARK_WM_LOCK_DOWN], args.click_timeout)
        log("boot A: 锁定窗销毁后注入 raw UP（须合成 CANCEL）")
        boot_a.qmp.pointer_up()
        boot_a.wait_markers([MARK_WM_CANCEL], args.click_timeout)

        # —— R2 damage 像素反 mock：跨界 damage 只落窗内 8px，右邻 4px
        #    保持原 tile 底色。此时 guest 在 marker 后留 2s 呈现窗。
        boot_a.wait_markers([MARK_WM_DAMAGE_CLIP], args.click_timeout)
        boot_a.log.poll()
        damage_clip = re.search(
            r"WMDAMAGECLIP ok x=([0-9]+) y=([0-9]+) w=([0-9]+) h=1",
            boot_a.log.text,
        )
        if damage_clip is None:
            raise SmokeError("boot A R2 damage clip geometry absent", boot_a.log_path)
        dc_x, dc_y, dc_w = map(int, damage_clip.groups())
        damage_shot = run_dir / "r2-damage-a.ppm"
        damage_deadline = time.monotonic() + 5.0
        while True:
            boot_a.qmp.screendump(damage_shot)
            inside = ppm_pixel(damage_shot, dc_x + 4, dc_y)
            neighbor = ppm_pixel(damage_shot, dc_x + dc_w - 4, dc_y)
            if all(abs(a - b) <= 8 for a, b in zip(inside, (255, 0, 255))) and all(
                abs(a - b) <= 8 for a, b in zip(neighbor, (24, 16, 40))
            ):
                break
            if time.monotonic() >= damage_deadline:
                raise SmokeError(
                    f"boot A R2 damage 像素：inside={inside} neighbor={neighbor}",
                    boot_a.log_path,
                )
            time.sleep(0.2)

        # —— R2 occlusion 像素反 mock：raw row y 必须落在屏幕 y。旧 eff
        #    stride 合约会把 screen y=40 读成 raw row 8（R 通道差 32）。
        boot_a.wait_markers([MARK_WM_AVOID_FRAME], args.boot_a_timeout)
        boot_a.log.poll()
        avoid = re.search(
            r"WMAVOIDWIN ok id=[0-9]+ raw=([0-9]+),([0-9]+),([0-9]+),([0-9]+) "
            r"visible=([0-9]+),([0-9]+),([0-9]+),([0-9]+) "
            r"safe=([0-9]+),([0-9]+),([0-9]+),([0-9]+)",
            boot_a.log.text,
        )
        avoid_frame = re.search(
            r"WMAVOIDFRAME ok id=[0-9]+ x=([0-9]+) y=([0-9]+) w=([0-9]+) h=([0-9]+)",
            boot_a.log.text,
        )
        if avoid is None or avoid_frame is None:
            raise SmokeError("boot A R2 occlusion geometry absent", boot_a.log_path)
        ar_x, ar_y, ar_w, ar_h, av_x, av_y, av_w, av_h, *av_safe = map(
            int, avoid.groups()
        )
        af_x, af_y, af_w, af_h = map(int, avoid_frame.groups())
        want_visible = (ar_x, ar_y + 32, ar_w, ar_h - 32)
        if ar_y != 0 or (
            av_x, av_y, av_w, av_h
        ) != want_visible or (af_x, af_y, af_w, af_h) != want_visible or tuple(av_safe) != (0, 32, 0, 0):
            raise SmokeError(
                f"boot A R2 occlusion 几何 raw={ar_x},{ar_y},{ar_w},{ar_h} "
                f"visible={av_x},{av_y},{av_w},{av_h} frame={af_x},{af_y},{af_w},{af_h} "
                f"safe={av_safe}",
                boot_a.log_path,
            )
        avoid_shot = run_dir / "r2-avoid-a.ppm"
        probe_x = af_x + af_w // 2
        probe_y1 = af_y + 8
        probe_y2 = min(af_y + 100, af_y + af_h - 1)
        avoid_deadline = time.monotonic() + 5.0
        while True:
            boot_a.qmp.screendump(avoid_shot)
            got1 = ppm_pixel(avoid_shot, probe_x, probe_y1)
            got2 = ppm_pixel(avoid_shot, probe_x, probe_y2)
            want1 = (probe_y1 & 0xFF, 64, 128)
            want2 = (probe_y2 & 0xFF, 64, 128)
            if all(abs(a - b) <= 8 for a, b in zip(got1, want1)) and all(
                abs(a - b) <= 8 for a, b in zip(got2, want2)
            ):
                break
            if time.monotonic() >= avoid_deadline:
                raise SmokeError(
                    f"boot A R2 row mapping：({probe_x},{probe_y1})={got1}!={want1} "
                    f"({probe_x},{probe_y2})={got2}!={want2}",
                    boot_a.log_path,
                )
            time.sleep(0.2)

        _wait_boot_batch(boot_a, args.boot_a_timeout)
        _require_exact_count(
            boot_a.log.text, MARK_WM_CONFIG_FOCUS_EMPTY, 1,
            "boot A R3 focus-only", boot_a.log_path,
        )
        _warn_input_armed(boot_a)

        # click ready = snap-to-0 已收敛的状态谓词，**强制在席恰一次**；
        # 缺席即 FAIL，不再 sleep-and-click 回退。
        boot_a.wait_markers(
            [MARK_WM_CONN4, MARK_WM_ROUTE_WINDOW_UP, MARK_CLICK_READY],
            args.click_ready_timeout,
        )
        ready_pos = boot_a.log.text.find(MARK_CLICK_READY)
        demo_before = boot_a.log.text.count(MARK_APP_DEMO_UP)
        quiet_before = boot_a.log.text.count(MARK_APP_QUIET_UP)
        fs_before = boot_a.log.text.count(MARK_APP_DEMO_FS)
        log(
            f"boot A: click ready 在席（pos={ready_pos}，"
            f"boot 批 APPDEMO up×{demo_before}）"
        )

        w, h, src = geometry_from_log(boot_a.log)
        cx, cy = cell0_center(w, h)
        log(f"boot A: display {w}x{h}（{src}）→ cell(0,0) 中心 ({cx},{cy})")

        boot_a.qmp_connect()
        log(f"boot A: QMP 注入无按键 ABS burst×48（旧 eventq16 会尾丢）")
        boot_a.qmp.abs_burst(cx, cy, w, h, count=48)
        time.sleep(1.0)
        boot_a.log.poll()
        if MARK_INPUT_DROPS_BAD in boot_a.log.text:
            raise SmokeError(
                "boot A burst: gui-inputd 报 pump 丢事件",
                boot_a.log_path,
            )
        _require_exact_count(
            boot_a.log.text, MARK_INPUT_DROPS_OK, 1, "boot A burst", boot_a.log_path
        )

        log("boot A: QMP 注入单击（abs → btn down → btn up）")
        boot_a.qmp.click(cx, cy, w, h)

        # 点击后等待三项同时成立：CLICK 恰一、LAYOUTW、新增 APPDEMO up。
        try:
            boot_a.log.wait_until(
                boot_a.proc,
                lambda t: re.search(RE_CLICK_N1, t) is not None
                and MARK_LAYOUTW in t
                and t.count(MARK_APP_DEMO_UP) > demo_before
                and t.count(MARK_APP_DEMO_FS) > fs_before,
                f"{MARK_CLICK} + {MARK_LAYOUTW} + 新 {MARK_APP_DEMO_UP}/fs ok",
                args.click_timeout,
                "boot A click",
            )
        except MissingMarkers:
            text = boot_a.log.text
            miss = []
            if re.search(RE_CLICK_N1, text) is None:
                miss.append(MARK_CLICK)
            if MARK_LAYOUTW not in text:
                miss.append(MARK_LAYOUTW)
            if text.count(MARK_APP_DEMO_UP) <= demo_before:
                miss.append(f"新 {MARK_APP_DEMO_UP}")
            if text.count(MARK_APP_DEMO_FS) <= fs_before:
                miss.append("新 APPDEMO fs ok")
            raise MissingMarkers(
                "boot A click",
                ", ".join(miss) or f"{MARK_CLICK} + {MARK_LAYOUTW}",
                args.click_timeout,
                boot_a.log_path,
            ) from None

        # —— 真实输入 swipe 相：同一 virtio-tablet 链从 QMP 注入横滑。
        #    SWIPE 标记由 page0→1 裁定 + 后续呈现帧共同门控；此处再验
        #    不新增 CLICK/APPDEMO，防止“滑动被误判点击”的假绿。
        click_pos = boot_a.log.text.find(MARK_CLICK_ANY)
        demo_after_click = boot_a.log.text.count(MARK_APP_DEMO_UP)
        fs_after_click = boot_a.log.text.count(MARK_APP_DEMO_FS)
        log(
            "boot A: QMP 注入真实横滑 "
            f"({int(w * 0.78)},{int(h * 0.52)}) → ({int(w * 0.20)},{int(h * 0.52)})"
        )
        boot_a.qmp.drag(w * 0.78, w * 0.20, h * 0.52, w, h)
        boot_a.wait_markers([MARK_SWIPE], args.click_timeout)

        # Phase 3 真实 owner 路由：点入 launcher 内嵌 APP session 的右下
        # 角窗口。事件必须由 inputd→wmd→APP input channel，且坐标已被
        # wmd 转成 window-local；不得触发桌面 CLICK/spawn。
        rt = re.search(
            r"WMRTWIN ok id=([0-9]+) x=([0-9]+) y=([0-9]+) "
            r"w=([0-9]+) h=([0-9]+)",
            boot_a.log.text,
        )
        if rt is None:
            raise SmokeError("boot A route window geometry absent", boot_a.log_path)
        rw_id, rx, ry, rw, rh = map(int, rt.groups())
        rcx, rcy = rx + rw // 2, ry + rh // 2
        log(f"boot A: QMP 点入 APP session 窗口 id={rw_id} 中心 ({rcx},{rcy})")
        boot_a.qmp.click(rcx, rcy, w, h)
        boot_a.wait_markers([MARK_WM_ROUTE_WINDOW], args.click_timeout)
        boot_a.log.poll()
        local = re.search(
            r"WMROUTEWIN ok phase=1 x=([0-9]+) y=([0-9]+)",
            boot_a.log.text,
        )
        if local is None:
            raise SmokeError("boot A APP route 缺 phase=1/local 坐标", boot_a.log_path)
        lx, ly = map(int, local.groups())
        if lx >= rw or ly >= rh:
            raise SmokeError(
                f"boot A APP route 坐标 ({lx},{ly}) 未local化 "
                f"(window {rw}x{rh})",
                boot_a.log_path,
            )
        # R2：输入坐标必须与 raw buffer 坐标同源。AVOID_SYS 顶遮挡下，
        # visible 顶部的点击 y 保留 32px raw 偏移，而不是从 0 重排。
        if abs(lx - (rcx - rx)) > 2 or abs(ly - (rcy - ry)) > 2:
            raise SmokeError(
                f"boot A APP route raw 坐标偏差 local=({lx},{ly}) "
                f"want=({rcx - rx},{rcy - ry})",
                boot_a.log_path,
            )
        owner_marker = f"WMROUTE ok sid=10 window={rw_id}"
        if owner_marker not in boot_a.log.text:
            raise SmokeError(f"boot A APP route 缺 {owner_marker!r}", boot_a.log_path)

        # 先停 QEMU 再对最终串口日志做精确计数（重复/顺序假绿 fail loud）。
        watcher_a = boot_a.log
        log_path_a = boot_a.log_path
        boot_a.stop()
        boot_a = None
        watcher_a.poll()
        verify_boot_a_click(
            watcher_a.text, ready_pos, demo_before, quiet_before, fs_before, log_path_a
        )
        verify_boot_a_real_swipe(
            watcher_a.text, click_pos, demo_after_click, fs_after_click, log_path_a
        )
        log(
            "boot A: 精确计数 CLICK×1 SWIPE×1 LAYOUTW×1 LAYOUT×0；"
            f"新增 {MARK_APP_DEMO_UP}×1 + fs RPC×1、APPQUIET×0"
        )

        verify_data_disk(disk)
        cells_a = read_layout_canonical(disk)
        log(
            f"boot A: LAYOUT.JSN 宿主读回 canonical "
            f"{len(LAYOUT_CANONICAL)}B / {len(cells_a)} cells "
            f"（{LAYOUT_CELLS_EXPECTED!r}）"
        )

        if args.layout_selfheal and args.only_a:
            raise SmokeError("--layout-selfheal 需要 dual Boot 全跑（不能与 --only-a 同用）")

        if args.only_a:
            success = True
            log(
                "PASS（--only-a）：Boot A 单击 + 真实横滑 + LAYOUTW + canonical 落盘读回；"
                "Boot B 未跑"
            )
            return 0

        # ---------------- Boot B ----------------
        hash_before_b = sha256_file(disk)
        boot_b = Boot("B", args, run_dir, disk, "b")
        boot_b.start()
        boot_b.wait_markers(_boot_base_markers(), args.boot_b_timeout)
        boot_b.wait_markers(
            [MARK_WM_CONN4, MARK_WM_ROUTE_WINDOW_UP], args.boot_b_timeout
        )
        boot_b.wait_regex(RE_LAYOUT_N2, args.boot_b_timeout)
        boot_b.log.poll()
        _verify_r2_occlusion(boot_b.log.text, "boot B", boot_b.log_path)
        _verify_r3_config(boot_b.log.text, "boot B", boot_b.log_path)

        watcher_b = boot_b.log
        log_path_b = boot_b.log_path
        boot_b.stop()
        boot_b = None
        watcher_b.poll()
        verify_boot_b(watcher_b.text, log_path_b)
        log("boot B: 精确计数 LAYOUT×1 LAYOUTW×0 CLICK×0")

        hash_after_b = sha256_file(disk)
        if hash_before_b != hash_after_b:
            raise SmokeError(
                "boot B: 数据盘字节发生变化——跨 boot 读路径不应重写 "
                f"(before={hash_before_b[:16]}… after={hash_after_b[:16]}…)"
            )
        verify_data_disk(disk)
        cells_b = read_layout_canonical(disk)
        log(
            f"boot B: 数据盘 sha256 前后一致（{hash_after_b[:16]}…），"
            f"LAYOUT.JSN canonical {len(LAYOUT_CANONICAL)}B / "
            f"{len(cells_b)} cells 跨 boot 存活"
        )

        if args.layout_selfheal:
            bad = corrupt_layout_for_selfheal(disk)
            try:
                read_layout_canonical(disk)
            except SmokeError:
                pass
            else:
                raise SmokeError("layout self-heal: 宿主损坏 LAYOUT 后仍被读成合法")
            log(
                "layout self-heal: 宿主已在真实 FAT 簇上覆写 "
                f"{len(bad)}B 非法 JSON，启动 Boot C"
            )

            boot_c = Boot("C", args, run_dir, disk, "c")
            boot_c.start()
            boot_c.wait_markers(
                _boot_base_markers()
                + [MARK_APP_DEMO_UP, MARK_WM_CONN4, MARK_WM_ROUTE_WINDOW_UP, MARK_LAYOUTW],
                args.boot_b_timeout,
            )
            boot_c.log.poll()
            _verify_r2_occlusion(boot_c.log.text, "boot C", boot_c.log_path)
            _verify_r3_config(boot_c.log.text, "boot C", boot_c.log_path)
            watcher_c = boot_c.log
            log_path_c = boot_c.log_path
            boot_c.stop()
            boot_c = None
            watcher_c.poll()
            verify_layout_selfheal_boot(watcher_c.text, log_path_c)
            cells_c = read_layout_canonical(disk)
            log(
                "layout self-heal: Boot C LAYOUTW×1，"
                f"canonical {len(LAYOUT_CANONICAL)}B / {len(cells_c)} cells 已重写"
            )

        success = True
        log(
            "PASS：EFI GUI 交互相双 boot（A: ABS burst 零丢 + CLICK ok n=1 + 真实 SWIPE ok n=1 + "
            "LAYOUTW ok + APPDEMO up；B: LAYOUT ok n=2 + 零重写 + canonical 109B）"
            + ("；C: 损坏 LAYOUT 真路径自愈" if args.layout_selfheal else "")
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
# --normal-boot：哨兵缺席的常态隔离（spawn 仍发生，标记零打印）
# =====================================================================


def run_normal_boot(args) -> int:
    preflight(args)
    print_marker_status()
    if args.only_a:
        raise SmokeError("--normal-boot 与 --only-a 互斥")

    run_dir = Path(args.work_dir) if args.work_dir else Path(
        tempfile.mkdtemp(prefix="gui8-normal.", dir=args.tmp_dir)
    )
    run_dir.mkdir(parents=True, exist_ok=True)
    log(f"normal-boot run dir：{run_dir}")

    disk = Path(args.data_disk) if args.data_disk else run_dir / "normal.img"
    if disk.exists():
        if not args.reuse_data_disk:
            raise SmokeError(f"数据盘已存在：{disk}（覆盖请删或 --reuse-data-disk）")
        verify_data_disk(disk, expect_smoke=False)
        log(f"复用常态盘：{disk}（无 GUI8.SMK，sha256={sha256_file(disk)[:16]}…）")
    else:
        disk.parent.mkdir(parents=True, exist_ok=True)
        create_data_disk(disk, with_smoke=False)
        # 常态桌面默认隐藏 APPDEMO/APPQUIET（官方系统 app 在席时）；
        # 注入 canonical 双 app 布局保持常态隔离点击面，不造 GUI8.SMK。
        native_layout = (
            b'{"schema":1,"cells":[{"app":"BROWSER","page":0,"row":0,"col":0},'
            b'{"app":"FILES","page":0,"row":0,"col":1},'
            b'{"app":"UITEST","page":0,"row":0,"col":2},'
            b'{"app":"SETTINGS","page":0,"row":0,"col":3},'
            b'{"app":"APPDEMO","page":1,"row":0,"col":0}]}'
        )
        Fat12Volume(disk).add_file(LAYOUT_NAME11, native_layout)
        contacts_payload = (
            b"Alice Chen,13800000001\nBerry Wang,13800000002\n"
            b"Cedar Li,13800000003\nDawn Zhou,13800000004\n"
        )
        Fat12Volume(disk).add_file(b"CONTACTSTXT", contacts_payload)
        log(
            f"新常态盘：{disk}（{disk.stat().st_size}B，"
            f"sha256={sha256_file(disk)[:16]}…，LBA0 印章，零 GUI8.SMK，"
            "BROWSER 首格）"
        )

    boot: Boot | None = None
    success = False
    try:
        boot = Boot(
            "normal", args, run_dir, disk, "normal", display="1080x1920@60"
        )
        boot.start()
        _wait_boot_batch(boot, args.boot_a_timeout)
        _warn_input_armed(boot)

        # 快照点 = 注入前（哨兵缺席时 click ready 结构性不打印，不能拿它
        # 当门）。boot 批 APPDEMO up 已吃完，此后的新增只能来自本次点击。
        boot.log.poll()
        snapshot_pos = len(boot.log.text)
        demo_before = boot.log.text.count(MARK_APP_DEMO_UP)
        quiet_before = boot.log.text.count(MARK_APP_QUIET_UP)
        browser_before = boot.log.text.count(MARK_BROWSER_UP)
        settings_before = boot.log.text.count(MARK_SETTINGS_UP)
        files_before = boot.log.text.count(MARK_FILES_UP)
        photos_before = boot.log.text.count(MARK_PHOTOS_UP)
        contacts_before = boot.log.text.count(MARK_CONTACTS_UP)
        uitest_before = boot.log.text.count(MARK_UITEST_UP)
        if browser_before < 1 or settings_before < 1 or files_before < 1 or photos_before < 1 or contacts_before < 1 or uitest_before < 1:
            raise MissingMarkers(
                "normal boot", "Browser boot-probe BROWSER up", 1, boot.log_path
            )
        if MARK_CLICK_READY in boot.log.text:
            raise SmokeError(
                f"normal boot: 哨兵缺席却打印 {MARK_CLICK_READY!r}——"
                "click ready 未按 GUI8.SMK 门控",
                boot.log_path,
            )

        w, h, src = geometry_from_log(boot.log)
        if (w, h) != (1080, 1920):
            raise SmokeError(
                f"normal GUI app 须 product portrait 1080x1920，got {w}x{h}（{src}）",
                boot.log_path,
            )
        log(
            "normal-boot: display 1080x1920 product portrait；"
            "BROWSER/FILES/UITEST 首行 (156/412/668,282)，"
            "CONTACTS/PHOTOS/SETTINGS dock (164/415/665,1668)"
        )

        boot.qmp_connect()

        # —— P2 键盘 fail-closed：启动态无 Keyboard 合约，KEY_A 必须被
        # wmd 拒收，且 down/up 各留下带 code 的证据行。
        log("normal-keyboard: QMP 注入未授权 KEY_A down/up")
        boot.qmp.tap_key(30)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: len(re.findall(
                    r"WMKEYDROP bad [^\n]*code=30", t
                )) >= 2 and "WMKEYROUTE ok" not in t,
                "未授权 KEY_A down/up 各一次 WMKEYDROP",
                args.click_timeout,
                "normal keyboard unauthorized",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal keyboard unauthorized",
                "WMKEYDROP bad code=30 ×2 / zero WMKEYROUTE",
                args.click_timeout,
                boot.log_path,
            ) from None

        # —— Browser BerryUI app：首格真实 APP_SPAWN → 窗口/帧 → back。 ——
        log("normal-browser: QMP 点击 BROWSER 首格（真实 APP_SPAWN）")
        boot.qmp.click(156, 282, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_BROWSER_UP) > browser_before
                and t.count(MARK_BROWSER_FRAME) > browser_before
                and "BROWSERLAYOUT ok back=" in t,
                f"runtime {MARK_BROWSER_UP} + {MARK_BROWSER_FRAME} + layout",
                args.click_timeout,
                "normal Browser launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Browser launch",
                f"新增 {MARK_BROWSER_UP} + {MARK_BROWSER_FRAME}",
                args.click_timeout,
                boot.log_path,
            ) from None
        # TCG 下 screendump 偶发早于呈现链完成：按 AppBar+surface 双锚
        # 重拍到稳定帧，不做固定 sleep 假设。
        browser_shot = run_dir / "browser.ppm"
        stable_deadline = time.monotonic() + 15.0
        while True:
            boot.qmp.screendump(browser_shot)
            appbar = ppm_pixel(browser_shot, 540, 20)
            surface = ppm_pixel(browser_shot, 540, 600)
            stable = all(
                abs(a - b) <= 8
                for a, b in zip(appbar, (0, 116, 224))
            ) and all(
                abs(a - b) <= 8
                for a, b in zip(surface, (255, 255, 255))
            )
            if stable:
                break
            if time.monotonic() >= stable_deadline:
                raise SmokeError(
                    f"Browser 稳定截图超时 AppBar={appbar} surface={surface}",
                    boot.log_path,
                )
            time.sleep(0.5)
        assert_pixel_close(
            ppm_pixel(browser_shot, 540, 20), (0, 116, 224),
            "Browser AppBar", boot.log_path,
        )
        assert_pixel_close(
            ppm_pixel(browser_shot, 540, 600), (255, 255, 255),
            "Browser surface", boot.log_path,
        )
        assert_color_count(
            browser_shot, (20, 70, 900, 250), 128,
            "Browser 系统 A8 文本", boot.log_path,
        )
        log("normal-browser: BerryUI 像素/色数锚 ok（AppBar/surface/A8）")

        # Browser 已是真实 focused window，但其 Retained surface 未声明
        # Keyboard：同样 KEY_A 必须走 reason=class 拒收。
        log("normal-keyboard: QMP 向 focused Browser 注入无 Keyboard 合约 KEY_A")
        boot.qmp.tap_key(30)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: len(re.findall(
                    r"WMKEYDROP bad [^\n]*reason=class code=30", t
                )) >= 2 and "WMKEYROUTE ok" not in t,
                "focused Browser class drop ×2",
                args.click_timeout,
                "normal keyboard Browser class",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal keyboard Browser class",
                "WMKEYDROP reason=class code=30 ×2",
                args.click_timeout,
                boot.log_path,
            ) from None

        browser_back = latest_browser_back(boot.log.text, snapshot_pos)
        browser_click = rect_center(browser_back)
        log(f"normal-browser: QMP 点击 Back Button ({browser_click[0]},{browser_click[1]})")
        boot.qmp.click(browser_click[0], browser_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_BROWSER_BACK) >= 1
                and t.count(MARK_BROWSER_EXIT) > browser_before,
                f"{MARK_BROWSER_BACK} + 新 {MARK_BROWSER_EXIT}",
                args.click_timeout,
                "normal Browser back",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Browser back",
                f"{MARK_BROWSER_BACK} + 新 {MARK_BROWSER_EXIT}",
                args.click_timeout,
                boot.log_path,
            ) from None

        # —— Settings BerryUI app：dock 点击 → 两项状态切换 → back。 ——
        time.sleep(1.0)
        log("normal-settings: QMP 点击 SmartDock SETTINGS（真实 APP_SPAWN）")
        boot.qmp.click(665, 1668, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_SETTINGS_UP) > settings_before
                and t.count(MARK_SETTINGS_FRAME) > settings_before,
                f"runtime {MARK_SETTINGS_UP} + {MARK_SETTINGS_FRAME}",
                args.click_timeout,
                "normal Settings launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Settings launch",
                f"新增 {MARK_SETTINGS_UP} + {MARK_SETTINGS_FRAME}",
                args.click_timeout,
                boot.log_path,
            ) from None

        wlan_rect = latest_settings_rect(boot.log.text, "wlan", snapshot_pos)
        wlan_click = rect_center(wlan_rect)
        log(f"normal-settings: QMP 点击 WLAN Switch ({wlan_click[0]},{wlan_click[1]})")
        boot.qmp.click(wlan_click[0], wlan_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_SETTINGS_TOGGLE1) >= 1
                and t.count(MARK_SETTINGS_FRAME) > settings_before + 1,
                f"{MARK_SETTINGS_TOGGLE1} + 重绘帧",
                args.click_timeout,
                "normal Settings WLAN",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Settings WLAN",
                f"{MARK_SETTINGS_TOGGLE1} + 重绘帧",
                args.click_timeout,
                boot.log_path,
            ) from None

        bluetooth_rect = latest_settings_rect(boot.log.text, "bluetooth", snapshot_pos)
        bluetooth_click = rect_center(bluetooth_rect)
        log(
            f"normal-settings: QMP 点击 Bluetooth Switch "
            f"({bluetooth_click[0]},{bluetooth_click[1]})"
        )
        boot.qmp.click(bluetooth_click[0], bluetooth_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_SETTINGS_TOGGLE2) >= 1
                and t.count(MARK_SETTINGS_FRAME) > settings_before + 2,
                f"{MARK_SETTINGS_TOGGLE2} + 第二重绘帧",
                args.click_timeout,
                "normal Settings Bluetooth",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Settings Bluetooth",
                f"{MARK_SETTINGS_TOGGLE2} + 第二重绘帧",
                args.click_timeout,
                boot.log_path,
            ) from None

        # 像素反 mock：两个开关必须变 Berry Theme Selected 蓝，主题面同步对锚。
        shot = run_dir / "settings.ppm"
        boot.qmp.screendump(shot)
        assert_pixel_close(ppm_pixel(shot, 540, 20), (0, 116, 224), "Settings AppBar", boot.log_path)
        assert_pixel_close(ppm_pixel(shot, 540, 600), (255, 255, 255), "Settings surface", boot.log_path)
        assert_pixel_close(
            ppm_pixel(shot, wlan_click[0], wlan_click[1]),
            (214, 228, 255),
            "Settings WLAN Selected",
            boot.log_path,
        )
        assert_pixel_close(
            ppm_pixel(shot, bluetooth_click[0], bluetooth_click[1]),
            (214, 228, 255),
            "Settings Bluetooth Selected",
            boot.log_path,
        )
        assert_color_count(
            shot, (20, 90, 700, 330), 128, "Settings 系统 A8 文本", boot.log_path,
        )
        log("normal-settings: BerryUI 像素/色数锚 ok（AppBar/surface/双Switch/A8）")

        back_rect = latest_settings_rect(boot.log.text, "back", snapshot_pos)
        back_click = rect_center(back_rect)
        log(f"normal-settings: QMP 点击 Back Button ({back_click[0]},{back_click[1]})")
        boot.qmp.click(back_click[0], back_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_SETTINGS_BACK) >= 1
                and t.count(MARK_SETTINGS_EXIT) > settings_before,
                f"{MARK_SETTINGS_BACK} + 新 {MARK_SETTINGS_EXIT}",
                args.click_timeout,
                "normal Settings back",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Settings back",
                f"{MARK_SETTINGS_BACK} + 新 {MARK_SETTINGS_EXIT}",
                args.click_timeout,
                boot.log_path,
            ) from None

        # —— Files BerryUI app：列表点击 → 真实 READDIR/STAT/GET。 ——
        time.sleep(1.0)
        log("normal-files: QMP 点击 FILES 首行第二格（真实 APP_SPAWN）")
        boot.qmp.click(412, 282, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_FILES_UP) > files_before
                and t.count(MARK_FILES_FRAME) > files_before
                and "FILESLAYOUT ok mode=0" in t,
                f"runtime {MARK_FILES_UP} + {MARK_FILES_FRAME} + layout",
                args.click_timeout,
                "normal Files launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Files launch",
                f"新增 {MARK_FILES_UP} + {MARK_FILES_FRAME}",
                args.click_timeout,
                boot.log_path,
            ) from None
        boot.log.poll()
        sel_matches = re.findall(r"FILESSEL ok i=([0-9])", boot.log.text)
        if not sel_matches:
            raise SmokeError("normal Files: 缺 FILESSEL 索引", boot.log_path)
        file_row = int(sel_matches[-1])
        log(f"normal-files: HELLO.TXT 列表行 index={file_row}")
        file_open = latest_named_rect(
            FILES_LAYOUT_RE, 0, "extra", boot.log.text, snapshot_pos
        )
        file_click = rect_center(file_open)
        log(f"normal-files: QMP 点击 HELLO.TXT Button ({file_click[0]},{file_click[1]})")
        boot.qmp.click(file_click[0], file_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_FILES_READ) >= 1
                and t.count(MARK_FILES_FRAME) > files_before + 1
                and "FILESLAYOUT ok mode=1" in t,
                f"{MARK_FILES_READ} + 详情重绘帧 + layout",
                args.click_timeout,
                "normal Files detail",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Files detail",
                f"{MARK_FILES_READ} + 详情重绘帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        if not re.search(r"FILESSTAT ok size=28(?![0-9])", boot.log.text):
            raise SmokeError("normal Files: HELLO.TXT size != 28", boot.log_path)
        if not re.search(r"FILESREAD ok n=28(?![0-9])", boot.log.text):
            raise SmokeError("normal Files: HELLO.TXT GET != 28B", boot.log_path)

        files_shot = run_dir / "files.ppm"
        boot.qmp.screendump(files_shot)
        assert_pixel_close(ppm_pixel(files_shot, 540, 20), (0, 116, 224), "Files AppBar", boot.log_path)
        assert_pixel_close(ppm_pixel(files_shot, 540, 600), (255, 255, 255), "Files surface", boot.log_path)
        assert_color_count(
            files_shot, (20, 90, 900, 420), 128, "Files 系统 A8 文本", boot.log_path,
        )
        log("normal-files: BerryUI 像素/色数锚 ok（AppBar/surface/A8）")

        files_detail_back = latest_named_rect(
            FILES_LAYOUT_RE, 1, "back", boot.log.text, snapshot_pos
        )
        files_back_click = rect_center(files_detail_back)
        log(f"normal-files: QMP 点击详情 Back ({files_back_click[0]},{files_back_click[1]})")
        boot.qmp.click(files_back_click[0], files_back_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_FILES_LIST) > files_before + 1
                and t.count(MARK_FILES_FRAME) > files_before + 2
                and t.count("FILESLAYOUT ok mode=0") >= 2,
                f"返回列表 {MARK_FILES_LIST} + 第三帧 + layout",
                args.click_timeout,
                "normal Files back-to-list",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Files back-to-list",
                f"返回列表 {MARK_FILES_LIST} + 第三帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        files_list_back = latest_named_rect(
            FILES_LAYOUT_RE, 0, "back", boot.log.text, snapshot_pos
        )
        files_exit_click = rect_center(files_list_back)
        log(f"normal-files: QMP 点击列表 Back ({files_exit_click[0]},{files_exit_click[1]})")
        boot.qmp.click(files_exit_click[0], files_exit_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_FILES_EXIT) > files_before,
                f"新 {MARK_FILES_EXIT}",
                args.click_timeout,
                "normal Files exit",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Files exit", f"新 {MARK_FILES_EXIT}",
                args.click_timeout, boot.log_path,
            ) from None

        # —— Photos BerryUI app：SmartDock 第二格 → 5000B 真图 → 查看。 ——
        time.sleep(1.0)
        log("normal-photos: QMP 点击 SmartDock PHOTOS（真实 APP_SPAWN）")
        # Photos runtime 先完整走 READDIR/STAT/GET 5000B FAT 链再首帧；
        # TCG 慢链可超过普通点击帽。240s 仍有界 fail closed，不降数据链。
        photos_launch_timeout = max(args.click_timeout, 240.0)
        boot.qmp.click(415, 1668, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_PHOTOS_UP) > photos_before
                and t.count(MARK_PHOTOS_FRAME) > photos_before
                and "PHOTOSLAYOUT ok mode=0" in t,
                f"runtime {MARK_PHOTOS_UP} + {MARK_PHOTOS_FRAME} + layout",
                photos_launch_timeout,
                "normal Photos launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Photos launch",
                f"新增 {MARK_PHOTOS_UP} + {MARK_PHOTOS_FRAME}",
                photos_launch_timeout,
                boot.log_path,
            ) from None
        if boot.log.count(MARK_PHOTOS_DATA) <= photos_before:
            raise MissingMarkers(
                "normal Photos remote image",
                MARK_PHOTOS_DATA, photos_launch_timeout, boot.log_path,
            )

        photos_open = latest_photos_rect(
            boot.log.text, 0, "open", snapshot_pos
        )
        photos_click = rect_center(photos_open)
        log(f"normal-photos: QMP 点击 View Button ({photos_click[0]},{photos_click[1]})")
        boot.qmp.click(photos_click[0], photos_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_PHOTOS_VIEW) >= 1
                and t.count(MARK_PHOTOS_FRAME) > photos_before + 1
                and "PHOTOSLAYOUT ok mode=1" in t,
                f"{MARK_PHOTOS_VIEW} + 查看大图帧 + layout",
                args.click_timeout,
                "normal Photos view",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Photos view",
                f"{MARK_PHOTOS_VIEW} + 查看大图帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        photos_shot = run_dir / "photos.ppm"
        boot.qmp.screendump(photos_shot)
        assert_pixel_close(
            ppm_pixel(photos_shot, 540, 20), (0, 116, 224),
            "Photos AppBar", boot.log_path,
        )
        # PATTERN.BIN 5000B 公式字节经 IconSource SmoothArgb 上屏。
        assert_pixel_close(
            ppm_pixel(photos_shot, 540, 800), (148, 148, 148),
            "Photos PATTERN 像素", boot.log_path,
        )
        assert_color_count(
            photos_shot, (20, 180, 900, 1300), 128,
            "Photos PATTERN 灰度层", boot.log_path,
        )
        log("normal-photos: BerryUI 像素/色数锚 ok（AppBar/PATTERN 真字节）")

        photos_detail_back = latest_photos_rect(
            boot.log.text, 1, "back", snapshot_pos
        )
        photos_back_click = rect_center(photos_detail_back)
        log(f"normal-photos: QMP 点击查看页 Back ({photos_back_click[0]},{photos_back_click[1]})")
        boot.qmp.click(photos_back_click[0], photos_back_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_PHOTOS_LIST) > photos_before + 1
                and t.count(MARK_PHOTOS_FRAME) > photos_before + 2
                and t.count("PHOTOSLAYOUT ok mode=0") >= 2,
                f"返回网格 {MARK_PHOTOS_LIST} + 第三帧 + layout",
                args.click_timeout,
                "normal Photos back-to-grid",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Photos back-to-grid",
                f"返回网格 {MARK_PHOTOS_LIST} + 第三帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        photos_grid_back = latest_photos_rect(
            boot.log.text, 0, "back", snapshot_pos
        )
        photos_exit_click = rect_center(photos_grid_back)
        log(f"normal-photos: QMP 点击网格 Back ({photos_exit_click[0]},{photos_exit_click[1]})")
        boot.qmp.click(photos_exit_click[0], photos_exit_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_PHOTOS_EXIT) > photos_before,
                f"新 {MARK_PHOTOS_EXIT}",
                args.click_timeout,
                "normal Photos exit",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Photos exit", f"新 {MARK_PHOTOS_EXIT}",
                args.click_timeout, boot.log_path,
            ) from None

        # —— Contacts BerryUI app：dock 首格 → 4 条真实记录 → 收藏。 ——
        time.sleep(1.0)
        log("normal-contacts: QMP 点击 SmartDock CONTACTS（真实 APP_SPAWN）")
        boot.qmp.click(164, 1668, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_CONTACTS_UP) > contacts_before
                and t.count(MARK_CONTACTS_FRAME) > contacts_before
                and "CONTACTSLAYOUT ok mode=0" in t,
                f"runtime {MARK_CONTACTS_UP} + {MARK_CONTACTS_FRAME} + layout",
                args.click_timeout,
                "normal Contacts launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Contacts launch",
                f"新增 {MARK_CONTACTS_UP} + {MARK_CONTACTS_FRAME}",
                args.click_timeout,
                boot.log_path,
            ) from None
        if boot.log.count(MARK_CONTACTS_READ) <= contacts_before:
            raise MissingMarkers(
                "normal Contacts remote records",
                MARK_CONTACTS_READ, args.click_timeout, boot.log_path,
            )

        contact_row = latest_named_rect(
            CONTACTS_LAYOUT_RE, 0, "extra", boot.log.text, snapshot_pos
        )
        contact_click = rect_center(contact_row)
        log(f"normal-contacts: QMP 点击 Berry Wang Button ({contact_click[0]},{contact_click[1]})")
        boot.qmp.click(contact_click[0], contact_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_CONTACTS_DETAIL) >= 1
                and t.count(MARK_CONTACTS_FRAME) > contacts_before + 1
                and "CONTACTSLAYOUT ok mode=1" in t,
                f"{MARK_CONTACTS_DETAIL} + 详情帧 + layout",
                args.click_timeout,
                "normal Contacts detail",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Contacts detail",
                f"{MARK_CONTACTS_DETAIL} + 详情帧",
                args.click_timeout,
                boot.log_path,
            ) from None

        favorite_rect = latest_named_rect(
            CONTACTS_LAYOUT_RE, 1, "extra", boot.log.text, snapshot_pos
        )
        favorite_click = rect_center(favorite_rect)
        log(f"normal-contacts: QMP 点击 Favorite Button ({favorite_click[0]},{favorite_click[1]})")
        boot.qmp.click(favorite_click[0], favorite_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_CONTACTS_FAV) >= 1
                and t.count(MARK_CONTACTS_FRAME) > contacts_before + 2,
                f"{MARK_CONTACTS_FAV} + 收藏帧",
                args.click_timeout,
                "normal Contacts favorite",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Contacts favorite",
                f"{MARK_CONTACTS_FAV} + 收藏帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        contacts_shot = run_dir / "contacts.ppm"
        boot.qmp.screendump(contacts_shot)
        assert_pixel_close(
            ppm_pixel(contacts_shot, 540, 20), (0, 116, 224),
            "Contacts AppBar", boot.log_path,
        )
        assert_pixel_close(
            ppm_pixel(contacts_shot, favorite_click[0], favorite_click[1]),
            (214, 228, 255),
            "Contacts Favorite Selected", boot.log_path,
        )
        assert_color_count(
            contacts_shot, (20, 90, 900, 420), 128,
            "Contacts 系统 A8 文本", boot.log_path,
        )
        log("normal-contacts: BerryUI 像素/色数锚 ok（AppBar/Favorite/A8）")

        contacts_detail_back = latest_named_rect(
            CONTACTS_LAYOUT_RE, 1, "back", boot.log.text, snapshot_pos
        )
        contacts_back_click = rect_center(contacts_detail_back)
        log(f"normal-contacts: QMP 点击详情 Back ({contacts_back_click[0]},{contacts_back_click[1]})")
        boot.qmp.click(contacts_back_click[0], contacts_back_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_CONTACTS_LIST) > contacts_before + 1
                and t.count(MARK_CONTACTS_FRAME) > contacts_before + 3
                and t.count("CONTACTSLAYOUT ok mode=0") >= 2,
                f"返回列表 {MARK_CONTACTS_LIST} + 第四帧 + layout",
                args.click_timeout,
                "normal Contacts back-to-list",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Contacts back-to-list",
                f"返回列表 {MARK_CONTACTS_LIST} + 第四帧",
                args.click_timeout,
                boot.log_path,
            ) from None
        contacts_list_back = latest_named_rect(
            CONTACTS_LAYOUT_RE, 0, "back", boot.log.text, snapshot_pos
        )
        contacts_exit_click = rect_center(contacts_list_back)
        log(f"normal-contacts: QMP 点击列表 Back ({contacts_exit_click[0]},{contacts_exit_click[1]})")
        boot.qmp.click(contacts_exit_click[0], contacts_exit_click[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_CONTACTS_EXIT) > contacts_before,
                f"新 {MARK_CONTACTS_EXIT}",
                args.click_timeout,
                "normal Contacts exit",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal Contacts exit", f"新 {MARK_CONTACTS_EXIT}",
                args.click_timeout, boot.log_path,
            ) from None

        # —— UITEST F5：BerryUI 组件 + Berry Room 真实帧/输入/返回栈。 ——
        time.sleep(1.0)
        log("normal-uitest: QMP 点击 UITEST 第三格（真实 APP_SPAWN）")
        boot.qmp.click(668, 282, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_UITEST_UP) > uitest_before
                and t.count(MARK_UITEST_FRAME) > uitest_before
                and "UITESTHIT ok node=button" in t,
                f"runtime {MARK_UITEST_UP} + {MARK_UITEST_FRAME} + layout/hit",
                args.click_timeout,
                "normal UITEST launch",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal UITEST launch",
                f"新增 {MARK_UITEST_UP} + {MARK_UITEST_FRAME} + dump",
                args.click_timeout,
                boot.log_path,
            ) from None
        if not re.search(r"UITESTWIN ok w=1080 h=1920", boot.log.text):
            raise SmokeError("normal UITEST: 窗口几何 != 1080x1920", boot.log_path)
        button0 = latest_uitest_button(boot.log.text, 0, snapshot_pos)
        click0 = (
            button0[0] + button0[2] // 2,
            button0[1] + button0[3] // 2,
        )
        uitest_shot = run_dir / "uitest-page0.ppm"
        boot.qmp.screendump(uitest_shot)
        assert_pixel_close(ppm_pixel(uitest_shot, 540, 20), (0, 116, 224), "UITEST AppBar", boot.log_path)
        assert_pixel_close(ppm_pixel(uitest_shot, 540, 60), (255, 255, 255), "UITEST surface", boot.log_path)
        assert_pixel_close(ppm_pixel(uitest_shot, 540, 100), (27, 37, 64), "UITEST title text", boot.log_path)
        assert_pixel_close(ppm_pixel(uitest_shot, 540, 600), (245, 247, 250), "UITEST empty background", boot.log_path)
        log("normal-uitest: BerryUI 首帧像素锚 ok（AppBar/surface/text/EmptyState）")

        log(f"normal-uitest: QMP 点击 Details ({click0[0]},{click0[1]})")
        boot.qmp.click(click0[0], click0[1], w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: "UITESTLAYOUT ok page=1" in t
                and "UITESTSTATE ok page=1" in t,
                "page1 layout + push state",
                args.click_timeout,
                "normal UITEST push",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal UITEST push", "page1 layout/state", args.click_timeout, boot.log_path
            ) from None
        button1 = latest_uitest_button(boot.log.text, 1, snapshot_pos)
        click1 = (
            button1[0] + button1[2] // 2,
            button1[1] + button1[3] // 2,
        )
        page1_shot = run_dir / "uitest-page1.ppm"
        boot.qmp.screendump(page1_shot)
        assert_pixel_close(ppm_pixel(page1_shot, 540, 20), (0, 116, 224), "UITEST page1 AppBar", boot.log_path)
        assert_pixel_close(ppm_pixel(page1_shot, 540, 100), (27, 37, 64), "UITEST page1 title", boot.log_path)
        log("normal-uitest: page1 布局/状态/像素锚 ok")

        log(f"normal-uitest: QMP 点击 Back ({click1[0]},{click1[1]})")
        boot.qmp.click(click1[0], click1[1], w, h)
        want_input = (
            click1[0] - 1,
            click1[1] - 1,
        )
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: MARK_UITEST_KEY_WAIT in t,
                MARK_UITEST_KEY_WAIT,
                args.click_timeout,
                "normal UITEST keyboard wait",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal UITEST keyboard wait", MARK_UITEST_KEY_WAIT,
                args.click_timeout, boot.log_path,
            ) from None
        wm_focus_before_focus = boot.log.text.count("WMFOCUS ok ")
        log("normal-uitest: QMP 注入 Tab×2（component focus 前进）")
        boot.qmp.tap_key(15)
        boot.qmp.tap_key(15)
        log("normal-uitest: QMP 注入 Enter×2（激活 Switch 两次，状态回 false）")
        boot.qmp.tap_key(28)
        boot.qmp.tap_key(28)
        input_re = rf"UITESTINPUT ok x={want_input[0]} y={want_input[1]}"
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: re.search(input_re, t) is not None
                and "UITESTBACK ok page=0 switch=0" in t
                and "UITESTFOCUSMOVE ok step=1 from=255 to=0" in t
                and "UITESTFOCUSMOVE ok step=2 from=0 to=1" in t
                and "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0" in t
                and "UITESTKEY ok code=28 tab=2 downs=2 ups=2" in t
                and t.count("WMFOCUS ok ") == wm_focus_before_focus
                and t.count(MARK_UITEST_EXIT) > uitest_before,
                f"{input_re} + BACK + FOCUS + KEY + EXIT",
                args.click_timeout,
                "normal UITEST pop/keyboard/exit",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal UITEST pop/keyboard/exit",
                "input + back + focus + key + exit",
                args.click_timeout,
                boot.log_path,
            ) from None

        # UITEST 已真实 WM_DESTROY / WMDEAD：再注入 KEY_A，必须新增两次
        # 拒收且不再出现键盘路由，证明 surface 授权与队列不残留。
        key_route_before = boot.log.text.count("WMKEYROUTE ok")
        key_drop_before = len(re.findall(r"WMKEYDROP bad [^\n]*code=30", boot.log.text))
        log("normal-keyboard: UITEST 销毁后注入 KEY_A down/up")
        boot.qmp.tap_key(30)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: len(re.findall(
                    r"WMKEYDROP bad [^\n]*code=30", t
                )) >= key_drop_before + 2
                and t.count("WMKEYROUTE ok") == key_route_before,
                "销毁后 KEY_A drop ×2 / 零新增路由",
                args.click_timeout,
                "normal keyboard after destroy",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal keyboard after destroy",
                "post-destroy WMKEYDROP ×2 / no new WMKEYROUTE",
                args.click_timeout,
                boot.log_path,
            ) from None

        watcher = boot.log
        log_path = boot.log_path
        boot.stop()
        boot = None
        watcher.poll()
        verify_normal_boot(
            watcher.text, snapshot_pos, browser_before, settings_before,
            files_before, photos_before, contacts_before, demo_before,
            log_path,
        )
        verify_uitest_normal(watcher.text, snapshot_pos, uitest_before, log_path)
        verify_data_disk(disk, expect_smoke=False)
        log(
            "normal-gui: Settings/Files/Contacts/Browser/Photos/UITEST 六个"
            " BerryUI app 均以独立进程完成真实 WM 窗口/帧、状态/fs/图像/联系人记录"
            "与销毁全链；"
            "CLICK/LAYOUTW/LAYOUT/click ready 全零"
        )

        success = True
        log(
            "PASS（--normal-boot）：六个 BerryUI app 真链 + "
            "哨兵缺席冒烟标记隔离成立"
        )
        return 0
    finally:
        if boot is not None:
            boot.stop()
        if success and not args.keep:
            shutil.rmtree(run_dir, ignore_errors=True)
        else:
            log(f"artifacts 保留：{run_dir}")
            log(f"  串口：{run_dir}/boot-normal.log")
            log(f"  数据盘：{disk}")
            log(f"  命令：{run_dir}/boot-normal.cmd.txt")


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
    normal = bool(args.normal_boot)
    try:
        disk = Path(args.data_disk) if args.data_disk else (
            run_dir / ("normal.img" if normal else "layout.img")
        )
        if disk.exists():
            verify_data_disk(disk, expect_smoke=not normal)
        else:
            create_data_disk(disk, with_smoke=not normal)
        log(f"dry-run: mode  = {'normal-boot（无 GUI8.SMK）' if normal else 'dual-boot（精确 GUI8.SMK）'}")
        log(f"dry-run: image = {args.image}")
        log(f"dry-run: code  = {args.code}")
        log(f"dry-run: vars  = {args.vars or '(未给；每 boot 仍产 fresh blank)'}")
        log(f"dry-run: disk  = {disk}（sha256={sha256_file(disk)}）")
        log(f"dry-run: disk 根目录 = {Fat12Volume(disk).list_root()}")
        if normal:
            markers = [
                MARK_BERRYBOOT,
                MARK_ROOTFS,
                MARK_APP_N9,
                MARK_LAUNCH,
                MARK_PAGE,
                MARK_APP_DEMO_UP,
                MARK_BROWSER_UP,
                MARK_BROWSER_WIN,
                MARK_BROWSER_FRAME,
                MARK_BROWSER_BACK,
                MARK_BROWSER_DESTROY,
                MARK_BROWSER_EXIT,
                f"ZERO {MARK_CLICK_ANY} / {MARK_LAYOUTW} / {MARK_LAYOUT} / "
                f"{MARK_CLICK_READY}",
            ]
            cmd = qemu_command(
                args,
                disk,
                run_dir / "vars-normal.fd",
                run_dir / "qmp-normal.sock",
                run_dir / "boot-normal.log",
            )
            log(f"dry-run: boot NORMAL QEMU = {shlex.join(cmd)}")
            log(f"dry-run: boot NORMAL 期望标记 = {markers}")
        else:
            for suffix, markers, zero in (
                (
                    "a",
                    [
                        MARK_BERRYBOOT,
                        MARK_ROOTFS,
                        MARK_APP_N9,
                        MARK_LAUNCH,
                        MARK_PAGE,
                        MARK_APP_DEMO_UP,
                        MARK_CLICK_READY,
                        MARK_CLICK,
                        MARK_LAYOUTW,
                    ],
                    [MARK_LAYOUT],
                ),
                (
                    "b",
                    [
                        MARK_BERRYBOOT,
                        MARK_ROOTFS,
                        MARK_APP_N9,
                        MARK_LAUNCH,
                        MARK_PAGE,
                        "LAYOUT ok n=2",
                    ],
                    [MARK_LAYOUTW, MARK_CLICK_ANY],
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
                log(
                    f"dry-run: boot {suffix.upper()} 期望标记 = {markers}；"
                    f"零标记 = {zero}"
                )
            log(
                f"dry-run: LAYOUT.JSN 验收 = 恰 {len(LAYOUT_CANONICAL)} canonical "
                f"bytes + {LAYOUT_CELLS_EXPECTED!r}"
            )
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

    def expect_error(label: str, fn) -> None:
        try:
            fn()
        except SmokeError:
            return
        raise SmokeError(f"自检负样本未失败：{label}")

    expect_error(
        "Browser layout 截断 fail closed",
        lambda: latest_browser_back("BROWSERLAYOUT ok back=1,2,3\n"),
    )
    expect_error(
        "Photos layout 缺 open fail closed",
        lambda: latest_photos_rect(
            "PHOTOSLAYOUT ok mode=0 back=1,2,3,4 image=5,6,7,8\n",
            0, "open",
        ),
    )
    try:
        # 1) FAT12 + 印章 + 精确 GUI8.SMK payload
        disk = tmp / "layout.img"
        create_data_disk(disk)
        verify_data_disk(disk)
        if Fat12Volume(disk).read_file(SMK_NAME11) != SMK_MAGIC:
            raise SmokeError("GUI8.SMK 读回 != 精确 magic")
        roots = Fat12Volume(disk).list_root()
        if "GUI8    SMK" not in roots:
            raise SmokeError(f"自检 FAT12 根目录异常：{roots}")
        log(f"self-test: FAT12 ok（GUI8.SMK 精确 magic，根目录 {roots}）")

        # 1b) 无哨兵盘：常态校验通过，冒烟校验 fail closed
        no_smk = tmp / "no-smk.img"
        create_data_disk(no_smk, with_smoke=False)
        verify_data_disk(no_smk, expect_smoke=False)
        expect_error(
            "缺 GUI8.SMK 仍进冒烟支路", lambda: verify_data_disk(no_smk)
        )
        expect_error(
            "哨兵盘冒充常态盘",
            lambda: verify_data_disk(disk, expect_smoke=False),
        )
        log("self-test: 无哨兵盘 ok（常态接受 / 冒烟 fail closed）")

        # 1c) 用户自造同名文件但 payload 错：必须 fail closed
        bad_smk = tmp / "bad-smk.img"
        create_data_disk(bad_smk, with_smoke=False)
        Fat12Volume(bad_smk).add_file(SMK_NAME11, SMK_MAGIC.rstrip(b"\n"))
        expect_error(
            "同名 GUI8.SMK 错 payload", lambda: verify_data_disk(bad_smk)
        )
        bad_smk2 = tmp / "bad-smk2.img"
        create_data_disk(bad_smk2, with_smoke=False)
        Fat12Volume(bad_smk2).add_file(SMK_NAME11, SMK_MAGIC + b"x")
        expect_error(
            "GUI8.SMK magic 带尾随字节", lambda: verify_data_disk(bad_smk2)
        )
        log("self-test: 错 payload ok（缺 LF / 尾随字节均 fail closed）")

        # 1d) LAYOUT.JSN 恰 109 canonical bytes + 精确双 cell
        write_layout_for_selftest(disk)
        blob = Fat12Volume(disk).read_file(LAYOUT_NAME11)
        if blob != LAYOUT_CANONICAL or len(blob) != 109:
            raise SmokeError(
                f"canonical LAYOUT 读回漂移：len={len(blob or b'')}"
            )
        bad_len = tmp / "bad-len.img"
        create_data_disk(bad_len)
        Fat12Volume(bad_len).add_file(LAYOUT_NAME11, LAYOUT_CANONICAL + b"\n")
        expect_error("LAYOUT 多一字节", lambda: read_layout_canonical(bad_len))
        bad_cell = tmp / "bad-cell.img"
        create_data_disk(bad_cell)
        bad_payload = LAYOUT_CANONICAL.replace(b"APPDEMO", b"APPDEMX")
        if len(bad_payload) != 109:
            raise SmokeError("自检坏 cell payload 长度漂移")
        Fat12Volume(bad_cell).add_file(LAYOUT_NAME11, bad_payload)
        expect_error("LAYOUT 错 app id", lambda: read_layout_canonical(bad_cell))
        bad_page = tmp / "bad-page.img"
        create_data_disk(bad_page)
        Fat12Volume(bad_page).add_file(
            LAYOUT_NAME11, LAYOUT_CANONICAL.replace(b'"page":3', b'"page":2')
        )
        expect_error("LAYOUT 错 page", lambda: read_layout_canonical(bad_page))
        heal = tmp / "self-heal.img"
        create_data_disk(heal)
        write_layout_for_selftest(heal)
        corrupt_layout_for_selfheal(heal)
        expect_error("损坏 LAYOUT 未 fail", lambda: read_layout_canonical(heal))
        Fat12Volume(heal).write_existing(LAYOUT_NAME11, LAYOUT_CANONICAL)
        heal_cells = read_layout_canonical(heal)
        if len(heal_cells) != len(LAYOUT_CELLS_EXPECTED):
            raise SmokeError("FAT 原簇覆写后 canonical cell 数漂移")
        verify_layout_selfheal_boot(f"{MARK_WM_FOCUS_SKIP} reason=no-pointer\n{MARK_LAYOUTW}\n")
        log(
            "self-test: LAYOUT canonical ok（109B 双 cell / 原簇损坏覆写 / "
            "错 id、错 page、多字节均 FAIL）"
        )

        # 2) 几何
        if cell0_center(800, 600) != (95, 99):
            raise SmokeError(f"cell0_center 异常：{cell0_center(800, 600)}")
        if cell0_center(480, 1000) != (55, 139):
            raise SmokeError(f"cell0_center 480x1000 异常：{cell0_center(480, 1000)}")
        log("self-test: geometry ok（800x600→(95,99)，480x1000→(55,139)）")

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

        # 3b) 精确计数 / 顺序：正向通过 + 每个重复/顺序假绿负样本
        if re.search(RE_LAYOUT_N2, "LAYOUT ok n=2"):
            pass
        else:
            raise SmokeError("LAYOUT n=2 正则未命中正常形态")
        if re.search(RE_LAYOUT_N2, "LAYOUT ok n=20"):
            raise SmokeError("LAYOUT 正则误吞 n=20")
        if re.search(RE_CLICK_N1, "CLICK ok n=10"):
            raise SmokeError("CLICK 正则误吞 n=10")

        fake_a = (
            "WMFOCUSREADY ok left=5 right=6 left_rect=0,0,400,600 "
            "right_rect=400,0,400,600\n"
            "WMFOCUS ok sid=1 window=5 v=8\n"
            "WMKEYROUTE ok sid=1 window=5 code=30 action=1\n"
            "WMKEYROUTE ok sid=1 window=5 code=30 action=0\n"
            "WMKEYFOCUS ok side=left window=5 pointer=1 keys=2\n"
            "WMFOCUS ok sid=2 window=6 v=9\n"
            "WMKEYROUTE ok sid=2 window=6 code=30 action=1\n"
            "WMKEYROUTE ok sid=2 window=6 code=30 action=0\n"
            "WMKEYFOCUS ok side=right window=6 pointer=1 keys=2\n"
            f"{MARK_WM_FOCUS_KEY}\n"
            "WMSNAPFOCUS ok v=9 right=1\n"
            f"{MARK_WM_FOCUS_NOOP} window=6 v=9\n"
            "WMFOCUS ok sid=1 window=5 v=12\n"
            "WMFOCUS ok sid=2 window=6 v=13\n"
            "WMMISSREADY ok x=600 y=300\n"
            f"{MARK_WM_HIDE_FOCUS} hidden=6 focused=5 v=10\n"
            "WMKEYROUTE ok sid=1 window=5 code=30 action=1\n"
            "WMKEYROUTE ok sid=1 window=5 code=30 action=0\n"
            f"{MARK_WM_MISS_FOCUS} focused=5 keys=2 v=10\n"
            f"{MARK_WM_SHOW_FOCUS} shown=6 v=11\n"
            "WMZREADY ok x=300 y=300 z=0\n"
            f"WMZFOCUS ok winner=5 z=0 v=12\n"
            "WMZRAISE ok window=6 z=1\n"
            f"WMZFOCUS ok winner=6 z=1 v=13\n"
            "WMLOCKREADY ok window=7 x=24 y=96 w=160 h=96 v=14\n"
            f"{MARK_WM_LOCK_DOWN} window=7 destroyed=1\n"
            f"{MARK_WM_CANCEL} window=7 phase=4 flags=2 v=15\n"
            f"{MARK_APP_DEMO_UP}\n"
            f"{MARK_LAYOUTW}\n"
            f"{MARK_INPUT_DROPS_OK}\n"
            f"{MARK_CLICK_READY}\n"
            f"{MARK_CLICK}\n"
            f"{MARK_WM_ROUTE_DESKTOP}\n"
            f"{MARK_WM_ROUTE_DESKTOP}\n"
            f"{MARK_APP_DEMO_UP}\n"
            f"{MARK_APP_DEMO_FS}\n"
        )
        pos_a = fake_a.index(MARK_CLICK_READY)
        verify_boot_a_click(fake_a, pos_a, 1, 0)
        expect_error(
            "Boot A 缺焦点 ready",
            lambda: verify_boot_a_click(
                fake_a.replace("WMFOCUSREADY ok", "WMFOCUSX ok", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 缺焦点快照",
            lambda: verify_boot_a_click(
                fake_a.replace("WMSNAPFOCUS ok ", "WMSNAPFOCUSX ok ", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 键盘路由数漂移",
            lambda: verify_boot_a_click(
                fake_a + "WMKEYROUTE ok sid=2 window=6 code=30 action=1\n",
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 缺锁定窗 CANCEL",
            lambda: verify_boot_a_click(
                fake_a.replace(f"{MARK_WM_CANCEL} window=7", "WMCANCELX window=7", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A P4 版本链漂移",
            lambda: verify_boot_a_click(
                fake_a.replace("WMSHOWFOCUS ok shown=6 v=11", "WMSHOWFOCUS ok shown=6 v=10", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A z 焦点证明缺一层",
            lambda: verify_boot_a_click(
                fake_a.replace("WMZFOCUS ok winner=6 ", "", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 缺已聚焦 no-op 证明",
            lambda: verify_boot_a_click(
                fake_a.replace(f"{MARK_WM_FOCUS_NOOP} window=6 v=9\n", "", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 焦点证明缺一侧",
            lambda: verify_boot_a_click(
                fake_a.replace("WMKEYFOCUS ok side=left ", "", 1),
                pos_a, 1, 0,
            ),
        )
        expect_error(
            "Boot A 重复 CLICK",
            lambda: verify_boot_a_click(
                fake_a + MARK_CLICK + "\n", pos_a, 1, 0
            ),
        )
        expect_error(
            "Boot A 重复 LAYOUTW",
            lambda: verify_boot_a_click(
                fake_a + MARK_LAYOUTW + "\n", pos_a, 1, 0
            ),
        )
        expect_error(
            "Boot A 出现 LAYOUT",
            lambda: verify_boot_a_click(
                fake_a + "LAYOUT ok n=2\n", pos_a, 1, 0
            ),
        )
        expect_error(
            "Boot A 缺新 APPDEMO",
            lambda: verify_boot_a_click(
                f"{MARK_APP_DEMO_UP}\n{MARK_CLICK_READY}\n"
                f"{MARK_CLICK}\n{MARK_LAYOUTW}\n",
                fake_a.index(MARK_CLICK_READY),
                1,
                0,
            ),
        )
        expect_error(
            "Boot A 误 spawn APPQUIET",
            lambda: verify_boot_a_click(
                fake_a + MARK_APP_QUIET_UP + "\n", pos_a, 1, 0
            ),
        )
        expect_error(
            "Boot A 缺 APPDEMO fs RPC",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_APP_DEMO_FS + "\n", ""), pos_a, 1, 0
            ),
        )
        order_bad = fake_a.replace(MARK_CLICK_READY + "\n", "").replace(
            MARK_CLICK + "\n", MARK_CLICK + "\n" + MARK_CLICK_READY + "\n"
        )
        expect_error(
            "Boot A CLICK 早于 click ready",
            lambda: verify_boot_a_click(
                order_bad, order_bad.index(MARK_CLICK_READY), 1, 0
            ),
        )
        expect_error(
            "Boot A 缺 click ready",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_CLICK_READY + "\n", ""),
                fake_a.index(MARK_CLICK_READY),
                1,
                0,
            ),
        )
        fake_swipe = fake_a + MARK_SWIPE + "\n" + MARK_WM_ROUTE_WINDOW + "1 x=50 y=32\n"
        verify_boot_a_real_swipe(
            fake_swipe,
            fake_swipe.index(MARK_CLICK_ANY),
            2,
            1,
        )
        expect_error(
            "Boot A 真实 swipe 缺标记",
            lambda: verify_boot_a_real_swipe(
                fake_a, fake_a.index(MARK_CLICK_ANY), 2, 1
            ),
        )
        expect_error(
            "Boot A 真实 swipe 误点击",
            lambda: verify_boot_a_real_swipe(
                fake_swipe + MARK_CLICK + "\n",
                fake_swipe.index(MARK_CLICK_ANY),
                2,
                1,
            ),
        )
        log(
            "self-test: Boot A 精确断言 ok（1/1/0 + 新 APPDEMO + fs RPC；"
            "重复/错序/缺 ready/APPQUIET 均 FAIL）"
        )

        fake_b = f"{MARK_WM_FOCUS_SKIP} reason=no-pointer\nLAUNCH ok\nLAYOUT ok n=2\n"
        verify_boot_b(fake_b)
        expect_error(
            "Boot B 泄漏 P4 矩阵",
            lambda: verify_boot_b(fake_b + "WMZFOCUS ok winner=6 z=1 v=13\n"),
        )
        expect_error(
            "Boot B 重复 LAYOUT",
            lambda: verify_boot_b(fake_b + "LAYOUT ok n=2\n"),
        )
        expect_error(
            "Boot B 出现 LAYOUTW",
            lambda: verify_boot_b(fake_b + MARK_LAYOUTW + "\n"),
        )
        expect_error(
            "Boot B 出现 CLICK",
            lambda: verify_boot_b(fake_b + MARK_CLICK_ANY + "1\n"),
        )
        expect_error(
            "Boot B LAYOUT n 漂移",
            lambda: verify_boot_b("LAUNCH ok\nLAYOUT ok n=3\n"),
        )
        log(
            "self-test: Boot B 精确断言 ok（LAYOUT×1 / LAYOUTW×0 / CLICK×0）"
        )

        browser_boot = (
            f"{MARK_BROWSER_UP}\n{MARK_BROWSER_WIN}\n"
            "BROWSERLAYOUT ok back=12,72,1056,60\n"
            "BROWSERRENDER ok seq=1 cmds=10\n"
            f"{MARK_BROWSER_FRAME}\n{MARK_BROWSER_PROBE}\n"
            f"{MARK_BROWSER_DESTROY}\n{MARK_BROWSER_EXIT}\n"
        )
        browser_runtime = (
            f"{MARK_BROWSER_UP}\n{MARK_BROWSER_WIN}\n"
            "BROWSERLAYOUT ok back=12,72,1056,60\n"
            "BROWSERRENDER ok seq=1 cmds=10\n"
            f"{MARK_BROWSER_FRAME}\n"
            "BROWSERHIT ok node=back x=539 y=101\n"
            f"{MARK_BROWSER_BACK}\n{MARK_BROWSER_DESTROY}\n"
            f"{MARK_BROWSER_EXIT}\n"
        )

        settings_boot = (
            f"{MARK_SETTINGS_UP}\n{MARK_SETTINGS_WIN}\n"
            "SETTINGSLAYOUT ok back=12,72,1056,60 wlan=146,141,78,42 "
            "bluetooth=306,203,78,42\n"
            f"{MARK_SETTINGS_GEOM}\n"
            "SETTINGSRENDER ok seq=1 cmds=22\n"
            f"{MARK_SETTINGS_FRAME}\n"
            f"{MARK_SETTINGS_PROBE}\n{MARK_SETTINGS_DESTROY}\n"
            f"{MARK_SETTINGS_EXIT}\n"
        )
        settings_runtime = (
            f"{MARK_SETTINGS_UP}\n{MARK_SETTINGS_WIN}\n"
            "SETTINGSLAYOUT ok back=12,72,1056,60 wlan=146,141,78,42 "
            "bluetooth=306,203,78,42\n"
            f"{MARK_SETTINGS_GEOM}\n"
            "SETTINGSRENDER ok seq=1 cmds=22\n"
            f"{MARK_SETTINGS_FRAME}\n"
            "SETTINGSSTATE ok n=1 wlan=1 bluetooth=0\n"
            f"{MARK_SETTINGS_TOGGLE1}\n{MARK_SETTINGS_FRAME}\n"
            "SETTINGSRENDER ok seq=2 cmds=22\n"
            "SETTINGSSTATE ok n=2 wlan=1 bluetooth=1\n"
            f"{MARK_SETTINGS_TOGGLE2}\n{MARK_SETTINGS_FRAME}\n"
            "SETTINGSRENDER ok seq=3 cmds=22\n"
            "SETTINGSHIT ok node=back x=539 y=101\n"
            f"{MARK_SETTINGS_BACK}\n{MARK_SETTINGS_DESTROY}\n"
            f"{MARK_SETTINGS_EXIT}\n"
        )

        files_boot = (
            f"{MARK_FILES_UP}\n{MARK_FILES_WIN}\n{MARK_FILES_LIST}\n"
            f"{MARK_FILES_SEL}1\n"
            "FILESLAYOUT ok mode=0 back=12,72,1056,60 open=12,204,1056,60\n"
            "FILESRENDER ok seq=1 cmds=20\n"
            f"{MARK_FILES_FRAME}\n{MARK_FILES_PROBE}\n"
            f"{MARK_FILES_DESTROY}\n{MARK_FILES_EXIT}\n"
        )
        files_runtime = (
            f"{MARK_FILES_UP}\n{MARK_FILES_WIN}\n{MARK_FILES_LIST}\n"
            f"{MARK_FILES_SEL}1\n"
            "FILESLAYOUT ok mode=0 back=12,72,1056,60 open=12,204,1056,60\n"
            "FILESRENDER ok seq=1 cmds=20\n"
            f"{MARK_FILES_FRAME}\n{MARK_FILES_OPEN}\n"
            f"{MARK_FILES_STAT}28\n{MARK_FILES_READ}28\n"
            "FILESLAYOUT ok mode=1 back=12,234,1056,60\n"
            "FILESRENDER ok seq=2 cmds=18\n"
            f"{MARK_FILES_FRAME}\n"
            "FILESLAYOUT ok mode=0 back=12,72,1056,60 open=12,204,1056,60\n"
            "FILESRENDER ok seq=3 cmds=20\n"
            f"{MARK_FILES_LIST}\n{MARK_FILES_FRAME}\n"
            "FILESHIT ok node=back x=539 y=101\n"
            f"{MARK_FILES_BACK}\n{MARK_FILES_DESTROY}\n{MARK_FILES_EXIT}\n"
        )

        photos_boot = (
            f"{MARK_PHOTOS_UP}\n{MARK_PHOTOS_WIN}\n{MARK_PHOTOS_LIST}1\n"
            f"{MARK_PHOTOS_DATA}\n"
            "PHOTOSLAYOUT ok mode=0 back=12,72,1056,60 "
            "image=12,192,360,720 open=12,918,1056,60\n"
            "PHOTOSRENDER ok seq=1 cmds=11\n"
            f"{MARK_PHOTOS_FRAME}\n{MARK_PHOTOS_PROBE}\n"
            f"{MARK_PHOTOS_DESTROY}\n{MARK_PHOTOS_EXIT}\n"
        )
        photos_runtime = (
            f"{MARK_PHOTOS_UP}\n{MARK_PHOTOS_WIN}\n{MARK_PHOTOS_LIST}1\n"
            f"{MARK_PHOTOS_DATA}\n"
            "PHOTOSLAYOUT ok mode=0 back=12,72,1056,60 "
            "image=12,192,360,720 open=12,918,1056,60\n"
            "PHOTOSRENDER ok seq=1 cmds=11\n"
            f"{MARK_PHOTOS_FRAME}\n{MARK_PHOTOS_OPEN}\n"
            "PHOTOSLAYOUT ok mode=1 back=12,72,1056,60 image=12,192,540,1080\n"
            "PHOTOSRENDER ok seq=2 cmds=8\n"
            f"{MARK_PHOTOS_VIEW}\n{MARK_PHOTOS_FRAME}\n"
            "PHOTOSLAYOUT ok mode=0 back=12,72,1056,60 "
            "image=12,192,360,720 open=12,918,1056,60\n"
            "PHOTOSRENDER ok seq=3 cmds=11\n"
            f"{MARK_PHOTOS_LIST}1\n{MARK_PHOTOS_FRAME}\n"
            "PHOTOSHIT ok node=back x=539 y=101\n"
            f"{MARK_PHOTOS_BACK}\n{MARK_PHOTOS_DESTROY}\n"
            f"{MARK_PHOTOS_EXIT}\n"
        )

        contacts_boot = (
            f"{MARK_CONTACTS_UP}\n{MARK_CONTACTS_WIN}\n{MARK_CONTACTS_LIST}\n"
            f"{MARK_CONTACTS_READ}\n"
            "CONTACTSLAYOUT ok mode=0 back=12,72,1056,60 row=12,204,1056,60\n"
            "CONTACTSRENDER ok seq=1 cmds=15\n"
            f"{MARK_CONTACTS_FRAME}\n"
            f"{MARK_CONTACTS_PROBE}\n{MARK_CONTACTS_DESTROY}\n"
            f"{MARK_CONTACTS_EXIT}\n"
        )
        contacts_runtime = (
            f"{MARK_CONTACTS_UP}\n{MARK_CONTACTS_WIN}\n{MARK_CONTACTS_LIST}\n"
            f"{MARK_CONTACTS_READ}\n"
            "CONTACTSLAYOUT ok mode=0 back=12,72,1056,60 row=12,204,1056,60\n"
            "CONTACTSRENDER ok seq=1 cmds=15\n"
            f"{MARK_CONTACTS_FRAME}\n"
            "CONTACTSLAYOUT ok mode=1 back=12,246,1056,60 favorite=12,180,1056,60\n"
            "CONTACTSRENDER ok seq=2 cmds=18\n"
            "CONTACTSDETAIL ok i=1\n"
            f"{MARK_CONTACTS_FRAME}\n"
            "CONTACTSLAYOUT ok mode=1 back=12,246,1056,60 favorite=12,180,1056,60\n"
            "CONTACTSRENDER ok seq=3 cmds=18\n"
            f"{MARK_CONTACTS_FAV}\n{MARK_CONTACTS_FRAME}\n"
            "CONTACTSLAYOUT ok mode=0 back=12,72,1056,60 row=12,204,1056,60\n"
            "CONTACTSRENDER ok seq=4 cmds=15\n"
            f"{MARK_CONTACTS_LIST}\n{MARK_CONTACTS_FRAME}\n"
            "CONTACTSHIT ok node=back x=539 y=101\n"
            "CONTACTSBACK ok\n"
            f"{MARK_CONTACTS_DESTROY}\n{MARK_CONTACTS_EXIT}\n"
        )

        fake_n = (
            f"{MARK_WM_SYSTEM_AUTH} id=3\n"
            f"{MARK_WM_SYSTEM_CLIENT_DENIED}\n"
            f"{MARK_WM_SYSTEM_SEED_DENIED}\n"
            f"{MARK_WM_SYSTEM_CONTROL_DENIED}\n"
            f"{MARK_WM_ROLE_MASQUERADE_DENIED}\n"
            f"{MARK_WM_LAYER_DENIED}\n"
            f"{MARK_WM_PARENT_DENIED}\n"
            f"{MARK_WM_GLOBAL_DENIED}\n"
            f"{MARK_WM_DAMAGE_BAD}\n"
            f"{MARK_WM_DAMAGE_CLIP}\n"
            f"{MARK_WM_RAW_BUFFER_BAD}\n"
            f"{MARK_WM_CONFIG_SUBMIT_DENY}\n"
            f"{MARK_WM_CONFIG_SURFACE_DENY}\n"
            f"{MARK_WM_CONFIG_FOREIGN_ACK}\n"
            f"{MARK_WM_CONFIG_DESTROYED_ACK}\n"
            f"{MARK_WM_CONFIG_NOOP_EMPTY}\n"
            f"{MARK_WM_CONFIG_HIDE}\n"
            f"{MARK_WM_CONFIG_SHOW}\n"
            f"{MARK_WM_FOCUS_SKIP} reason=no-pointer\n"
            f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
            f"{photos_boot}{contacts_boot}"
        )
        snap_n = len(fake_n)
        fake_n += (
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=4 window=1 reason=class code=30\n"
            "WMKEYDROP bad sid=4 window=1 reason=class code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "INPUTKEYDROPS ok n=0\n"
            f"{browser_runtime}{MARK_WM_ROUTE_DESKTOP}\n"
            f"{settings_runtime}{files_runtime}{photos_runtime}"
            f"{contacts_runtime}"
        )
        verify_normal_boot(fake_n, snap_n, 1, 1, 1, 1, 1, 1)
        expect_error(
            "常态缺 R1 SYSTEM 身份正例",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_WM_SYSTEM_AUTH} id=3\n", "", 1),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态缺 R1 错 seed 拒绝",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_WM_SYSTEM_SEED_DENIED}\n", "", 1),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态缺 R1 role 伪装拒绝",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_WM_ROLE_MASQUERADE_DENIED}\n", "", 1),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态缺 R1 全局 surface 拒绝",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_WM_GLOBAL_DENIED}\n", "", 1),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态缺 R2 damage clip",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_WM_DAMAGE_CLIP}\n", "", 1),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        r2_fake = "\n".join([
            MARK_WM_SYSTEM_OCCLUDER,
            MARK_WM_AVOID_WINDOW,
            MARK_WM_AVOID_HIDE,
            MARK_WM_AVOID_SHOW,
            MARK_WM_AVOID_GRANT,
            MARK_WM_AVOID_FRAME,
            MARK_WM_OCCLUDED_DAMAGE_BAD,
            MARK_WM_DAMAGE_CLIP,
            MARK_WM_RAW_BUFFER_BAD,
        ]) + "\n"
        _verify_r2_occlusion(r2_fake, "self-test")
        expect_error(
            "R2 缺遮挡帧证据",
            lambda: _verify_r2_occlusion(
                r2_fake.replace(f"{MARK_WM_AVOID_FRAME}\n", "", 1), "self-test"
            ),
        )
        expect_error(
            "R2 damage clip 重复",
            lambda: _verify_r2_occlusion(r2_fake + MARK_WM_DAMAGE_CLIP + "\n", "self-test"),
        )
        r3_fake = "\n".join([
            MARK_WM_CONFIG_SUBMIT_DENY,
            MARK_WM_CONFIG_SURFACE_DENY,
            MARK_WM_CONFIG_FOREIGN_ACK,
            MARK_WM_CONFIG_DESTROYED_ACK,
            MARK_WM_CONFIG_NOOP_EMPTY,
            MARK_WM_CONFIG_HIDE,
            MARK_WM_CONFIG_SHOW,
            MARK_WM_CONFIG_ACK_DENIED,
            MARK_WM_CONFIG_CREATED,
            MARK_WM_CONFIG_EMPTY,
            MARK_WM_CONFIG_COALESCE,
            MARK_WM_CONFIG_STALE,
        ]) + "\n"
        _verify_r3_config(r3_fake, "self-test")
        expect_error(
            "R3 缺 config coalescing",
            lambda: _verify_r3_config(
                r3_fake.replace(f"{MARK_WM_CONFIG_COALESCE}\n", "", 1), "self-test"
            ),
        )
        expect_error(
            "R3 stale ack 重复",
            lambda: _verify_r3_config(r3_fake + MARK_WM_CONFIG_STALE + "\n", "self-test"),
        )
        verify_normal_boot(fake_n, snap_n, 1, 1, 1, 1, 1, 1)
        for needle, label in (
            (MARK_CLICK_ANY + "1\n", "CLICK"),
            (MARK_LAYOUTW + "\n", "LAYOUTW"),
            ("LAYOUT ok n=2\n", "LAYOUT"),
            (MARK_CLICK_READY + "\n", "click ready"),
            (MARK_APP_QUIET_UP + "\n", "APPQUIET"),
        ):
            expect_error(
                f"常态隔离出现 {label}",
                lambda n=needle: verify_normal_boot(
                    fake_n + n, snap_n, 1, 1, 1, 1, 1, 1
                ),
            )
        expect_error(
            "常态 Browser 无第二进程",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
                f"{photos_boot}{contacts_boot}{settings_runtime}"
                f"{files_runtime}{photos_runtime}{contacts_runtime}",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Settings 无第二进程",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
                f"{photos_boot}{contacts_boot}{browser_boot}"
                f"{MARK_BROWSER_BACK}\n{files_runtime}{photos_runtime}"
                f"{contacts_runtime}",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Settings 缺第二重绘帧",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_SETTINGS_FRAME}\n", "", 2),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Files 无第二进程",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
                f"{photos_boot}{contacts_boot}{browser_boot}"
                f"{MARK_BROWSER_BACK}\n{settings_runtime}{photos_runtime}"
                f"{contacts_runtime}",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Files 缺真实 GET",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_FILES_READ}28\n", ""),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Files 缺详情重绘",
            lambda: verify_normal_boot(
                fake_n.replace(
                    "FILESLAYOUT ok mode=1 back=12,234,1056,60\n"
                    "FILESRENDER ok seq=2 cmds=18\n"
                    f"{MARK_FILES_FRAME}\n",
                    "FILESLAYOUT ok mode=1 back=12,234,1056,60\n"
                    "FILESRENDER ok seq=2 cmds=18\n",
                    1,
                ),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Photos 无第二进程",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
                f"{photos_boot}{contacts_boot}{browser_boot}"
                f"{MARK_BROWSER_BACK}\n{settings_runtime}{files_runtime}"
                f"{contacts_runtime}",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Photos 缺真实 5000B 图",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_PHOTOS_DATA}\n", "", 2),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Photos 缺查看帧",
            lambda: verify_normal_boot(
                fake_n.replace(
                    f"{MARK_PHOTOS_VIEW}\n{MARK_PHOTOS_FRAME}\n",
                    f"{MARK_PHOTOS_VIEW}\n", 1,
                ),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Contacts 无第二进程",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_UP}\n{browser_boot}{settings_boot}{files_boot}"
                f"{photos_boot}{contacts_boot}{browser_boot}"
                f"{MARK_BROWSER_BACK}\n{settings_runtime}{files_runtime}"
                f"{photos_runtime}",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Contacts 缺真实记录",
            lambda: verify_normal_boot(
                fake_n.replace(f"{MARK_CONTACTS_READ}\n", ""),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 Contacts 缺收藏帧",
            lambda: verify_normal_boot(
                fake_n.replace(
                    f"{MARK_CONTACTS_FAV}\n{MARK_CONTACTS_FRAME}\n",
                    f"{MARK_CONTACTS_FAV}\n", 1,
                ),
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        expect_error(
            "常态 GUI app 误 spawn APPDEMO",
            lambda: verify_normal_boot(
                fake_n + MARK_APP_DEMO_UP + "\n",
                snap_n, 1, 1, 1, 1, 1, 1,
            ),
        )
        log(
            "self-test: 常态 GUI app 断言 ok（六 BerryUI app 状态与数据链、"
            "layout/render/hit、重绘与销毁；冒烟标记全零）"
        )

        uitest_boot = (
            "INPUTDEV ok tab=3 kb=2 q=64x8B cal=x[0,32768) y[0,32768)\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=4 window=1 reason=class code=30\n"
            "WMKEYDROP bad sid=4 window=1 reason=class code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "WMKEYDROP bad sid=0 window=0 reason=focus code=30\n"
            "INPUTKEYDROPS ok n=0\n"
            f"{MARK_UITEST_UP}\n{MARK_UITEST_ROOM}\n"
            f"{MARK_UITEST_WIN}1080 h=1920\n"
            "UITESTLAYOUT ok page=0 appbar=0,0,1080,60 body=0,60,1080,1860 "
            "button=12,126,1056,60 scroll=12,377,1056,24 clip=12,377,1056,24\n"
            "UITESTHIT ok node=button x=540 y=156\n"
            "UITESTAST ok bytes=712 a11y=8\n"
            "UITESTRENDER ok seq=1 cmds=25\n"
            f"{MARK_UITEST_FRAME}\n"
            "UITESTRENDER ok seq=2 cmds=49\n"
            "UITESTSCROLL ok offset=24 switch=1\n"
            "UITESTRENDER ok seq=3 cmds=26\n"
            "UITESTCLIP ok port=12,377,1056,24 item=12,353,1056,72 "
            "probe=540,409 seq=3\n"
            "UITESTSTATE ok page=1 switch=1\n"
            "UITESTRENDER ok seq=4 cmds=17\n"
            "UITESTBACK ok page=0 switch=1\n"
            "UITESTRENDER ok seq=5 cmds=26\n"
            "UITESTRENDER ok seq=6 cmds=25\n"
            "UITESTRENDER ok seq=7 cmds=25\n"
            "UITESTCLIP ok port=12,377,1056,24 item=12,377,1056,72 "
            "probe=540,409 seq=7\n"
            "UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 "
            "safe=0,0,0,0\n"
            f"{MARK_UITEST_PROBE}\n{MARK_UITEST_DESTROY}\n"
            f"{MARK_UITEST_EXIT}\n"
        )
        uitest_runtime = (
            f"{MARK_UITEST_UP}\n{MARK_UITEST_ROOM}\n"
            f"{MARK_UITEST_WIN}1080 h=1920\n"
            "UITESTLAYOUT ok page=0 appbar=0,0,1080,60 body=0,60,1080,1860 "
            "button=12,126,1056,60 scroll=12,377,1056,24 clip=12,377,1056,24\n"
            "UITESTHIT ok node=button x=540 y=156\n"
            "UITESTAST ok bytes=712 a11y=8\n"
            "UITESTRENDER ok seq=1 cmds=25\n"
            f"{MARK_UITEST_FRAME}\n"
            "UITESTLAYOUT ok page=1 appbar=0,0,1080,60 body=0,60,1080,1860 "
            "button=12,255,1056,60 scroll=0,60,1080,1860 clip=0,60,1080,1860\n"
            "UITESTHIT ok node=button x=540 y=285\n"
            "UITESTSTATE ok page=1 switch=0\n"
            "UITESTRENDER ok seq=2 cmds=18\n"
            "UITESTINPUT ok x=539 y=284\n"
            "UITESTRENDER ok seq=3 cmds=18\n"
            "UITESTBACK ok page=0 switch=0\n"
            "WMKEYROUTE ok sid=15 window=15 code=15 action=1\n"
            "WMKEYROUTE ok sid=15 window=15 code=15 action=0\n"
            "WMKEYROUTE ok sid=15 window=15 code=15 action=1\n"
            "WMKEYROUTE ok sid=15 window=15 code=15 action=0\n"
            "WMKEYROUTE ok sid=15 window=15 code=28 action=1\n"
            "WMKEYROUTE ok sid=15 window=15 code=28 action=0\n"
            "WMKEYROUTE ok sid=15 window=15 code=28 action=1\n"
            "WMKEYROUTE ok sid=15 window=15 code=28 action=0\n"
            "UITESTKEYWAIT ok input=9\n"
            "UITESTFOCUSMOVE ok step=1 from=255 to=0 state=3\n"
            "UITESTFOCUSMOVE ok step=2 from=0 to=1 state=3\n"
            "UITESTRENDER ok seq=4 cmds=18\n"
            "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0\n"
            "UITESTKEY ok code=28 tab=2 downs=2 ups=2\n"
            f"{MARK_UITEST_DESTROY}\n{MARK_UITEST_EXIT}\n"
        )
        fake_u = uitest_boot + uitest_runtime
        snap_u = len(uitest_boot)
        verify_uitest_normal(fake_u, snap_u, 1)
        expect_error(
            "UITEST Surface 合约位造假",
            lambda: verify_uitest_normal(
                fake_u.replace("flags=31 scale=1000", "flags=30 scale=1000", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺键盘等待",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTKEYWAIT ok input=9\n", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺输入设备标记",
            lambda: verify_uitest_normal(
                fake_u.replace("INPUTDEV ok tab=3 kb=2", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺键盘零丢基线",
            lambda: verify_uitest_normal(
                fake_u.replace("INPUTKEYDROPS ok n=0\n", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键盘拒收数不足",
            lambda: verify_uitest_normal(
                fake_u.replace("WMKEYDROP bad ", "WMKEYDROPX bad ", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键码造假",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTKEY ok code=28", "UITESTKEY ok code=29", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺 component focus 前进",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTFOCUSMOVE ok step=2 from=0 to=1 state=3\n", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 焦点第一步 payload 造假",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTFOCUSMOVE ok step=1 from=255 to=0 state=3\n",
                    "UITESTFOCUSMOVE ok step=1 from=254 to=0 state=3\n", 1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 焦点激活状态链造假",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0\n",
                    "UITESTFOCUSACT ok node=1 toggles=2 states=0,0,0 final=0\n", 1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键路由 sid 造假",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "WMKEYROUTE ok sid=15 window=15 code=15 action=1\n",
                    "WMKEYROUTE ok sid=0 window=15 code=15 action=1\n", 1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键路由 code 集造假",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "WMKEYROUTE ok sid=15 window=15 code=28 action=1\n",
                    "WMKEYROUTE ok sid=15 window=15 code=29 action=1\n", 1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键路由 down/up 错配",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "WMKEYROUTE ok sid=15 window=15 code=15 action=1\n",
                    "WMKEYROUTE ok sid=15 window=15 code=15 action=0\n", 1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST focus render 计数不足",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTRENDER ok seq=4 cmds=18\n", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST component focus 激活重复",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0\n",
                    "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0\n"
                    "UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0\n",
                    1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 键盘 marker 重复",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTKEY ok code=28 tab=2 downs=2 ups=2\n",
                    "UITESTKEY ok code=28 tab=2 downs=2 ups=2\n"
                    "UITESTKEY ok code=28 tab=2 downs=2 ups=2\n",
                    1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST Surface marker 重复",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 "
                    "safe=0,0,0,0\n",
                    "UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 "
                    "safe=0,0,0,0\n"
                    "UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 "
                    "safe=0,0,0,0\n",
                    1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺 Surface 真链",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTSURFACE ok modes=1,2 frames=8,9", "", 1),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺 reset Clip proof",
            lambda: verify_uitest_normal(
                fake_u.replace(
                    "UITESTCLIP ok port=12,377,1056,24 item=12,377,1056,72 "
                    "probe=540,409 seq=7\n",
                    "",
                    1,
                ),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺 owner-local 输入",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTINPUT ok x=539 y=284\n", ""),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺真实帧",
            lambda: verify_uitest_normal(
                fake_u.replace(f"{MARK_UITEST_FRAME}\n", "", 2),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺真实滚动 marker",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTSCROLL ok offset=24 switch=1\n", ""),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 缺 hit dump",
            lambda: verify_uitest_normal(
                fake_u.replace("UITESTHIT ok node=button x=540 y=285\n", ""),
                snap_u, 1,
            ),
        )
        expect_error(
            "UITEST 输入早于快照",
            lambda: verify_uitest_normal(
                uitest_boot.replace(
                    f"{MARK_UITEST_PROBE}\n",
                    f"{MARK_UITEST_PROBE}\nUITESTINPUT ok x=539 y=284\n",
                ) + uitest_runtime,
                snap_u, 1,
            ),
        )
        log("self-test: F5 UITEST 常态断言 ok（像素帧/布局/命中/状态/返回栈）")

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

        log(
            "SELFTEST PASS（精确哨兵/canonical 109B/精确计数与顺序/"
            "常态隔离/geometry/log/QMP；零 QEMU）"
        )
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
            "  python3 tools/gui8-smoke.py --dry-run --normal-boot\n"
            "  python3 tools/gui8-smoke.py --image out/berryos.img\n"
            "  python3 tools/gui8-smoke.py --normal-boot\n"
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
        help="允许复用已存在数据盘（dual 必须精确 GUI8.SMK；normal 必须无哨兵）",
    )
    ap.add_argument("--work-dir", help="指定 run 目录（默认 mktemp）")
    ap.add_argument(
        "--tmp-dir",
        default="/tmp" if Path("/tmp").is_dir() else tempfile.gettempdir(),
        help="临时目录（默认 /tmp；QMP sock 路径帽）",
    )
    ap.add_argument("--boot-a-timeout", type=float, default=420.0)
    ap.add_argument("--boot-b-timeout", type=float, default=420.0)
    ap.add_argument("--click-timeout", type=float, default=120.0)
    ap.add_argument(
        "--click-ready-timeout",
        type=float,
        default=60.0,
        help="dual Boot A 等 click ready 的秒数（缺席即 FAIL，无 sleep 回退）",
    )
    ap.add_argument(
        "--normal-boot",
        action="store_true",
        help="常态隔离：不造 GUI8.SMK，点击后断言 spawn 仍发生且零标记",
    )
    ap.add_argument("--only-a", action="store_true", help="只跑 Boot A（plumbing）")
    ap.add_argument(
        "--layout-selfheal",
        action="store_true",
        help="dual Boot 后追加 Boot C：宿主损坏 FAT 上 LAYOUT.JSN，验真路径自愈重写",
    )
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
        if args.normal_boot:
            return run_normal_boot(args)
        return run_dual_boot(args)
    except MissingMarkers as e:
        print(f"gui8-smoke: FAIL: {e}", file=sys.stderr, flush=True)
        print(
            "  hint: dual 相要求 launcher 段④ 标记在哨兵支路出现"
            "（click ready / CLICK / LAYOUTW / LAYOUT）；先跑 "
            "--self-test/--dry-run 验 plumbing。",
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
