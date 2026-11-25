#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gui8-smoke —— GUI-8 段④「EFI GUI 交互相」双 boot 冒烟 rig。

为什么单独立脚本（不并回 smoke-image.sh）
----------------------------------------
冻结相 smoke-image 的断言面是「EFI 十段恰一次 + 裸相 32 + 生态 52（历史注释值曾 36/39 随刀漂移；CLI/TERM 链位 fat park 前照打，不属缺席族）+ 字节
对拍 + 金丝雀」，其 QEMU 拓扑**结构性无 virtio**（不挂 blk/tablet/QMP）。
交互相需要另一套拓扑（FAT12 数据盘 + virtio-tablet + QMP unix sock）和
另一套标记族（fs 端点让给 launcher 后 bsh 在 fs_selftest park，ECO 缺席族里
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
  - 等基础 5 + boot 批 `app APPDEMO gui deferred`（裁决 B：boot 批对
    [ui] app 入册不拉起——桌面就绪锚 = LAUNCH/PAGE 基础 5 本体）；
  - guifbd 标记三枚恰一：`BANDC ok` / `BANDR ok n=`（批B 带合成——首
    带缓存 seed 同拍落印，BANDC 打印后随批执行幂等自验证重放触发
    BANDR，远早于点击）+ `WMSUB ok`（wm 订阅私有通道刀——guifbd 首次
    快照真值接入 one-shot latch）；门在停机后全量日志上计数，非 boot
    期等待，均零序锚（WMSUB 跨通道无协议序，纯恰一计数）；
  - `launcher: click ready` **强制在席恰一次**（缺席即 FAIL，不 sleep
    回退——这是注入前的状态谓词门）；
  - 单击注入通路 --input-path：qmp（默认——单条 input-send-event：
    abs 定位 + btn down + btn up 同批投递，无固定睡）或 vnc（RFB
    PointerEvent 直通——QEMU 另挂 -vnc 127.0.0.1:0
    自动分配口，内建最小 RFB 客户端带 SetEncodings；gui8 点击历来走
    QMP、用户手走的 VNC 人面路从未被测，正是「QEMU 11 VNC 丢绝对坐标」
    旧判例能长期立足的制度性原因，本臂给人面路立法测试面）；vnc 形
    断言面不变——点击同一格中心、VNC 表面域坐标直给勿缩放（ServerInit
    实拍表面几何必须与串口几何全等，不等即 fail closed）；
    `--aged-minutes` 与两通路正交组合可用；
  - 精确计数：`CLICK ok n=1` == 1、`LAYOUTW ok` == 1、`LAYOUT ok n=`
    == 0（重复/顺序假绿一律 FAIL，不 warn）；
  - 点击后 `app APPDEMO up` 恰 1 行（boot 批 0 行 = 裁决 B 面，
    点击真 spawn 且全日志唯一），
    `app APPQUIET up` 零新增（未误点/误 spawn）；
  - 宿主侧再经 FAT12 读回 `LAYOUT.JSN`：必须恰 **109 canonical bytes**
    且 cells 逐项 == `APPDEMO(0,0,0)` + `APPQUIET(page=3,row=0,col=1)`。
- Boot B：同一镜像、同一数据盘（不重造），fresh vars/QMP。
  - 精确计数：`LAYOUT ok n=2` == 1、`LAYOUTW ok` == 0、
    `CLICK ok n=` == 0（写路径一次性状态门）；
  - 宿主侧 sha256 前后相等 = 数据盘零重写；LAYOUT.JSN 仍为 canonical。
- `--normal-boot`（常态隔离，可选）：同一镜像/tablet/数据盘拓扑但
  **不造 `GUI8.SMK`**；boot 一次，点击 APPDEMO，断言 spawn 仍发生
  （新增 `app APPDEMO up` == 1）、而 `CLICK`/`LAYOUTW`/`LAYOUT` 以及
  `launcher: click ready` 零打印——证明标记只在哨兵支路出现，常态点击
  行为不变且不新增任何标记。

plumbing 自检（零 QEMU；含错 payload / 无哨兵 / 错 canonical / 重复计数
/ 顺序倒置负样本）：

    python3 tools/gui8-smoke.py --self-test
    python3 tools/gui8-smoke.py --dry-run [--image out/berryos.img]
    python3 tools/gui8-smoke.py --dry-run --normal-boot

标记 canon：CLICK/LAYOUTW/LAYOUT 在本相按**精确次数**断言（Boot A
1/1/0，Boot B 0/0/1）；`CLICK ok n=1` 用负向前瞻防 `n=10` 子串假阳性。
批B guifbd 两标记 Boot A 全日志恰一次（one-shot latch 对拍）。QMP 点击前
只认 click ready 状态谓词，不用固定 sleep 兜底。

macOS 纪律：serial 落文件（不依赖 terminal）、QMP 只用 unix socket
（默认零 ambient TCP 端口；--input-path vnc 例外——QEMU 另开 127.0.0.1
VNC 口自动避让分配，走 TCP 形态是判例驳斥快照实验陷阱段结论：unix
vnc.sock 异常关闭而 TCP 口正常）、每 boot fresh blank vars.fd、
SIGTERM/QMP quit 双路收尾、无 Docker/UTM 依赖。
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
# TERMINAL 刀滚新：槽 9→10（n=10 两位形——svcmgr 写面宽度参数化随动）。
MARK_APP_N10 = "APP ok n=10"
MARK_APP_DEMO_UP = "app APPDEMO up"
MARK_APP_DEMO_DEFERRED = "app APPDEMO gui deferred"
MARK_APP_QUIET_UP = "app APPQUIET up"
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
# Boot B 读回标记：n 固定 2（canonical 双 cell）。
RE_LAYOUT_N2 = r"LAYOUT ok n=2(?![0-9])"

# —— guifbd one-shot latch 标记（guest.rs 首现打印恰一次）——
# BANDC/BANDR 是批B 带合成打印（带缓存首 seed / 首重放 ≥1 像素）。只进
# Boot A 腿，Boot B / 常态隔离腿不设门（SPEC 裁决：对照腿不要求）。
# WMSUB 在册（wm 订阅私有通道刀复活——批B 曾因共享 wm 队列双消费者应答
# 互截摘牌，私有通道正拓扑落成后回归）：guifbd 经 svcmgr 静默窗代发订阅
# 的私有通道收到首个快照真值即打恰一次（count=0 空表也算面活）。门 = 纯
# 恰一计数零序锚（S4 证 FBSERVE 序锚从未存在且跨通道无协议序）。
MARK_BANDC = "BANDC ok"
# BANDR 打印形带写数后缀（`BANDR ok n=<writes>`），断言取 n= 前缀形。
MARK_BANDR = "BANDR ok n="
MARK_WMSUB = "WMSUB ok"

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

# —— 活体 app 回收时延回归门（2026-11-21 冻结案测试面补缺，--recycle-gate）——
# gui8 全臂以 APPDEMO（console=true，out_s==0 结构性跳过 collect_eof）为
# 载荷 = 回收烧帽类缺陷 CI 恒盲（真机场 SETTINGS/CONTACTS 两案全靠用户
# 撞出）。本门换活体 console=false app：BROWSER（wants_fs，wm_present_h
# 正轨 = 真开窗活泵实例；非 dock/test app = 紧凑档可入网格——CONTACTS/
# PHOTOS/SETTINGS/TERMINAL 是 dock app，800x600 无 dock 不可点）。
# 注入链 = 图标首击（spawn）→ 多任务键二击（recents 重拉 = 同 id 回收
# 〔svcmgr app_spawn_recycle 外部击杀形〕）。判据三面：① eco 行序
# it → recycle → killed → it（killed 短行 = 主修在位锚——修回退即缺行
# FAIL）；② 全程零 no EOF/exit code 降级行（烧帽/真异常退场分诊面）；
# ③ 二击时延帽 = 首击时延 + headroom（默认 40s < 60s 烧帽 = 回归必爆
# 帽，正常毫秒级回收恒过；随机器自适应免 TCG 慢机假红）。
GATE_APP = "BROWSER"
LAYOUT_GATE = (
    b'{"schema":1,"cells":[{"app":"BROWSER","page":0,"row":0,"col":0}]}'
)
LAYOUT_GATE_CELLS = (
    ("BROWSER", 0, 0, 0),
)
if len(LAYOUT_GATE) != 65:
    raise RuntimeError("LAYOUT_GATE 字节数漂移（期望 65）")
MARK_GATE_DEFERRED = "app BROWSER gui deferred"
ECO_GATE_ALIVE = "eco: app BROWSER alive"
ECO_GATE_RECYCLE = "eco: app BROWSER recycle"
ECO_GATE_KILLED = "eco: app BROWSER killed"
ECO_GATE_NO_EOF = "eco: app BROWSER no EOF"
ECO_GATE_EXIT_CODE = "eco: app BROWSER exit code"
ECO_GATE_KILLED_NOTHING = "eco: app BROWSER killed for nothing"
RE_GATE_IT = re.compile(r"eco: app BROWSER it [0-9A-Fa-f]{2}")
# launcher 侧失败诊断（spawn 拒/重拉败——门内出现即 FAIL loud）。
GATE_LAUNCHER_FAILS = (
    "launcher: recents respawn failed",
    "launcher: spawn BROWSER",
)
if len(LAYOUT_CELLS_EXPECTED) != 2:
    raise RuntimeError("LAYOUT canonical cell 数漂移（期望 2）")

# —— 启动器前台态握手行（frameloop.rs 常驻诊断，非冒烟态专属）——
# `fg+ spawned`：flush_pending_spawn 的阻塞 APP_SPAWN 已返回且真拉起、
# 前台态在席（每次真拉起恰一行）；`fg- <reason>`：leave_fg 清前台态
#（每次收口恰一行，reason ∈ table/back/recents/home）。回收门拿它们
# 做交接握手：等 `fg+` = 知道启动器已出阻塞 RPC；等 `fg- back` = 知道
# 返回键真被消费、前台态已清，此后点图标才不被整击吞掉——替代原固定
# sleep(2.0)。与 GATE_LAUNCHER_FAILS 两条、「launcher: click ready /
# input poll armed」互不为子串（tools/ 与 justfile 内此前零命中）。
MARK_FG_PLUS = "launcher: fg+ spawned"
MARK_FG_MINUS_PREFIX = "launcher: fg- "
MARK_FG_MINUS_BACK = MARK_FG_MINUS_PREFIX + "back"
MARK_FG_MINUS_RECENTS = MARK_FG_MINUS_PREFIX + "recents"
# 回收门每击「段一：等消费证据」的上限秒：零证据 = 该击未达启动器（零动作
# 或丢失），重注一击（至多 GATE_MAX_RETRIES 次）；证据（新 it / 新 fg-）
# 一出即转段二在途续等、不再重注（见 _gate_click_wait_spawn 头注）。实测
# 绿轮首击/二击 3 到 12s 出 it，TCG 单核重载下阻塞 spawn 0.4 到 4.1s、
# 单拍最长 5.8s——30s 留 5 倍余量；每击总帽另由 click_timeout 钉死。
GATE_CLICK_STEP_TIMEOUT = 30.0
# 返回键交接上限秒：点返回键后等 `launcher: fg- back` 的帽。二击的
# `fg+` 已等过（启动器不在阻塞 RPC 内），返回键只需排进下一拍——TCG
# 重载下单拍最长约 6s，10s 足够；超时重注，至多 GATE_MAX_RETRIES 次。
GATE_BACK_STEP_TIMEOUT = 10.0
GATE_MAX_RETRIES = 2

# —— 桌面几何（launcher desktop.rs 常量镜像；仅用于算 cell(0,0) 中心，
#    屏幕宽高从串口 LAUNCH/FB 行动态解析——裁决 D：不把 800x600 写死）——
STATUS_H = 44
# 小屏档无 Dock，只保留 SwipePager 16px 视觉点行（OH 96px 指示区仅 phone 档）。
DOT_H = 16
GRID_TOP = 8
# 底部保留 = 8px 底垫 + 导航条 44px（刀 D 三键导航——desktop.rs 紧凑档
# grid_h 公式镜像；导航条本身不是点击断言面，只参与格高推导）。
BOTTOM_PAD = 52
NAV_KEY_PX = 44
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

    def input_send(self, events):
        # 省 device 键 = 广播到 unbound handler 链（QEMU ui/input.c:
        # qemu_input_find_handler 第二圈）。device=tab0 会走
        # qemu_console_lookup_by_device_name（console 查找），在 -display none
        # 的 qemu-fixed-text-console 上触发 QEMU 11.0.3 的 QOM 崩溃——勿用。
        return self.execute("input-send-event", {"events": events})

    def click(self, x: float, y: float, w: int, h: int) -> None:
        """单击：abs 定位 + btn down + btn up 同一条 input-send-event（单批）。

        原三批形（①abs ②btn down+abs ③btn up，每批后 sleep 0.25s 让
        guest 逐批消费）在 TCG 单核重载下有真实缺陷：gui-inputd 的 wire
        时戳是**出队时刻**（main.rs harvest 逐事件 `ts = now()`），DOWN
        取批 ② 的 ABS_Y 样本时戳、UP 取批 ③ 的 BTN_LEFT 时戳，两批各自
        出队的调度抖动叠加宿主 0.25s 固定睡，把引擎看到的按压时长（UP.ts
        − DOWN.ts）抖到 berry-ui hit.rs 的 450ms 长按阈之上——实抓
        461/484ms 两例：referee 只产 Release+LongPress 不产 Click，启动器
        零动作零诊断（r11-diag 报告 §3）。单批形下 QEMU 缓冲到一次
        SYN_REPORT 投递，inputd close_batch 给同批 DOWN/UP 同一个
        synth_ts（lib.rs「QMP 单命令 down+up 即 press+release 同批的标准
        注入形态」+ 单测 pump_same_batch_tap_pairs_down_up），UP−DOWN 恒
        0，与负载无关。宿主 berry CLI 的 tap_abs（tools/berry/src/qmp.rs）
        即此形。批间不再有固定睡。
        """
        ax = max(0, min(32767, int(round(x * 32768.0 / max(1, w)))))
        ay = max(0, min(32767, int(round(y * 32768.0 / max(1, h)))))
        self.input_send(
            [
                {"type": "abs", "data": {"axis": "x", "value": ax}},
                {"type": "abs", "data": {"axis": "y", "value": ay}},
                {"type": "btn", "data": {"down": True, "button": "left"}},
                {"type": "btn", "data": {"down": False, "button": "left"}},
            ]
        )

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
# 最小 RFB 客户端（--input-path vnc：VNC 直通注入臂）
# =====================================================================
# 判例驳斥锚（归档考古入口）：
# .agents/research/证据快照/2026-11-20-VNC直通绝对坐标判例驳斥.md
# （提交 9be8d6158）。定谳：well-formed RFB 客户端（发 SetEncodings——
# 浏览器 noVNC 恒发）经 QEMU VNC 直通的 PointerEvent 绝对坐标直达客机；
# QEMU vs->absolute 由 set_encodings 触发 check_pointer_type_change 依
# 设备侧 handler 图置位（virtio-tablet 在席即真）——不发 SetEncodings 的
# 裸客户端走 REL 分支被静默丢弃，正是旧判例观测形态的根源。


class RfbError(SmokeError):
    pass


class RfbClient:
    """input-only 最小 RFB 3.8 客户端——只服务单击注入，不收帧。

    三铁律照抄判例驳斥快照实验陷阱段：①PointerEvent 恰 6B（type+mask+
    u16 x+u16 y）无 pad，多写一字节推歪协议流即被踢；②读侧 leftover 粘包
    缓冲（TCP 合流与分段都不许读歪）；③input-only 不发
    FramebufferUpdateRequest——零推送零排水（快照实验里帧更新不排水即
    输出缓冲超帽被踢；不发请求即结构性免排水）。
    """

    def __init__(self, host: str, port: int, timeout: float = 15.0):
        self.host = host
        self.port = int(port)
        self.sock = socket.create_connection((host, self.port), timeout=timeout)
        self.sock.settimeout(timeout)
        # leftover 粘包缓冲（铁律二）：_read_exact 只从这里割字节。
        self.buf = b""
        self.fb_w = 0
        self.fb_h = 0
        self._handshake()

    def _read_exact(self, n: int) -> bytes:
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise RfbError(
                    f"RFB EOF（还差 {n - len(self.buf)}B——QEMU 已退出或被踢）"
                )
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _send(self, data: bytes) -> None:
        self.sock.sendall(data)

    def _handshake(self) -> None:
        ver = self._read_exact(12)
        if not ver.startswith(b"RFB 003."):
            raise RfbError(f"RFB 版本行异常：{ver!r}")
        # 客户端回 3.8（QEMU 恒发 003.008；3.3 服务器不在本 rig 面内）。
        self._send(b"RFB 003.008\n")
        n_types = self._read_exact(1)[0]
        if n_types == 0:
            rlen = struct.unpack(">I", self._read_exact(4))[0]
            reason = self._read_exact(min(rlen, 512))
            raise RfbError(f"RFB 服务器拒绝（无安全类型）：{reason!r}")
        types = self._read_exact(n_types)
        if 1 not in types:
            raise RfbError(
                f"RFB 安全类型不含 None(1)（得 {list(types)}）——"
                "只支持无鉴权 VNC（-vnc 勿带 password）"
            )
        self._send(b"\x01")
        result = struct.unpack(">I", self._read_exact(4))[0]
        if result != 0:
            # 3.8 失败面还带 u32 长度 + reason——尽力读出再抛。
            try:
                rlen = struct.unpack(">I", self._read_exact(4))[0]
                reason = self._read_exact(min(rlen, 512))
            except RfbError:
                reason = b""
            raise RfbError(f"RFB 安全握手失败 result={result}：{reason!r}")
        # ClientInit shared=1（不独占——与生产 berry 客户端同形）。
        self._send(b"\x01")
        # ServerInit：u16 宽高 + 16B 像素格式（input-only 不消费）+ 名字。
        self.fb_w, self.fb_h = struct.unpack(">HH", self._read_exact(4))
        self._read_exact(16)
        name_len = struct.unpack(">I", self._read_exact(4))[0]
        if name_len > 4096:
            raise RfbError(f"RFB ServerInit 名字超长 {name_len}")
        self._read_exact(name_len)
        # SetEncodings ×1（RAW）——判例驳斥的实验本体：这一发触发 QEMU
        # check_pointer_type_change 把 vs->absolute 置真，后续 PointerEvent
        # 才走绝对坐标直通 virtio-tablet。注意本消息头自带 1B pad（type+
        # pad+u16 数量+i32 编码），与 PointerEvent 的「恰 6B 无 pad」恰成
        # 对照——两处头格式别写串。
        self._send(b"\x02\x00" + struct.pack(">Hi", 1, 0))

    def pointer(self, mask: int, x: int, y: int) -> None:
        # 铁律一：PointerEvent 恰 6B（type=5+mask+u16 x+u16 y 大端）无 pad
        # ；坐标 = VNC 表面域 u16 直给勿缩放（宿主不做任何客机几何换算）。
        if not (0 <= x <= 65535 and 0 <= y <= 65535):
            raise RfbError(f"RFB PointerEvent 坐标越界：({x},{y})")
        self._send(struct.pack("!BBHH", 5, mask & 0xFF, x, y))

    def click(self, x: int, y: int) -> None:
        # 三发单击：①move 定位 ②左键 down ③up——RFB 人面路形（每发一次
        # PointerEvent，QEMU 各自 sync）。0.25s 间隔让 guest 输入泵逐发
        # 消费；QMP 臂已改单批同投（QmpClient.click 头注——慢点击在 TCG
        # 重载下会被引擎判长按），本臂保留三发形不在此刀范围。
        self.pointer(0, x, y)
        time.sleep(0.25)
        self.pointer(1, x, y)
        time.sleep(0.25)
        self.pointer(0, x, y)

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass


def vnc_port_from_qmp(qmp: QmpClient) -> int:
    """query-vnc 探 -vnc 127.0.0.1:N 自动避让后的实口（端口发现不走猜）。"""
    info = qmp.execute("query-vnc") or {}
    if not info.get("enabled"):
        raise RfbError(
            f"query-vnc：VNC 未启用（{info!r}）——vnc 注入路需要 QEMU 挂 -vnc"
        )
    # QEMU 11 端口在 service 键（str 形）；老树 port int 键一并兼容。
    raw = info.get("service") or info.get("port")
    if raw is None:
        raise RfbError(f"query-vnc：缺 service/port 键（{info!r}）")
    try:
        port = int(raw)
    except (TypeError, ValueError) as e:
        raise RfbError(f"query-vnc 端口非法：{raw!r}（{info!r}）") from e
    if not (1 <= port <= 65535):
        raise RfbError(f"query-vnc 端口越界：{port}")
    return port


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
    return _read_layout_exact(
        path, LAYOUT_CANONICAL, LAYOUT_CELLS_EXPECTED, "canonical"
    )


def read_layout_gate(path: Path) -> list:
    """读回回收门盘 LAYOUT.JSN：恰 LAYOUT_GATE 65 bytes + BROWSER(0,0,0)。

    与 canonical 同一 fail-closed 三面（长度/逐字节/cells）；官方 app
    在席 → APPDEMO/APPQUIET 按桌面建树规则隐藏（本门零依赖）。
    """
    return _read_layout_exact(path, LAYOUT_GATE, LAYOUT_GATE_CELLS, "gate")


def _read_layout_exact(
    path: Path, payload: bytes, cells_expected: tuple, label: str
) -> list:
    vol = Fat12Volume(path)
    blob = vol.read_file(LAYOUT_NAME11)
    if blob is None:
        raise SmokeError(f"LAYOUT.JSN 缺席（{path}）")
    if len(blob) != len(payload):
        raise SmokeError(
            f"LAYOUT.JSN[{label}] 长度 {len(blob)} != 期望 {len(payload)}"
        )
    if blob != payload:
        raise SmokeError(
            f"LAYOUT.JSN[{label}] 非精确字节：{blob!r} != {payload!r}"
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
    if tuple(got) != cells_expected:
        raise SmokeError(
            f"LAYOUT.JSN[{label}] cells {got!r} != {cells_expected!r}"
        )
    return cells


def write_layout_for_selftest(
    path: Path, payload: bytes = LAYOUT_CANONICAL
) -> list:
    """自检用：向无 LAYOUT 的盘注入指定 payload 并走 canonical 读回。"""
    Fat12Volume(path).add_file(LAYOUT_NAME11, payload)
    return read_layout_canonical(path)


def write_layout_bytes(path: Path, payload: bytes) -> None:
    """注入指定 LAYOUT payload（无读回——读回归各 verify 面）。"""
    Fat12Volume(path).add_file(LAYOUT_NAME11, payload)


# =====================================================================
# 几何 / QEMU 命令 / Boot 生命周期
# =====================================================================


def cell0_center(w: int, h: int) -> tuple[int, int]:
    """cell(0,0) 中心（status 栏之下，4×5 网格第一格；800×600 调试档）。"""
    grid_h = max(0, h - (STATUS_H + GRID_TOP + DOT_H + BOTTOM_PAD))
    cell_w = max(1, (w - (GRID_COLS - 1) * GRID_GAP) // GRID_COLS)
    cell_h = max(1, (grid_h - (GRID_ROWS - 1) * GRID_GAP) // GRID_ROWS)
    return cell_w // 2, STATUS_H + GRID_TOP + cell_h // 2

# 回收门多任务键几何（desktop.rs 五键导航条镜像——紧凑档专用）：
# 条 = content 列尾行（Row：back/home/recents 三键 44px + gap 76，总宽
# 44*3+76*2=284，cross=Center 水平居中）；纵向 = status(44)+grid_top(8)
# +pages_host(host_h)+nav(44)，条下 8px 底垫（host_h = h-44-8-0-44-8）。
# recents = 第三键：中心 x = x0 + 2*(44+76) + 22 = x0+262；y = h-8-22。
# big 档（OH 1080x1920 族）几何不同——本门 fw_cfg 恒 800x600 紧凑档，
# big 档挂账不测。
NAV_KEY_PX = 44
NAV_GAP = 76
NAV_BAR_W = NAV_KEY_PX * 3 + NAV_GAP * 2


def recents_center(w: int, h: int) -> tuple[int, int]:
    """多任务键（nav.recents）中心——回收门注入点之一（紧凑档）。"""
    x0 = (w - NAV_BAR_W) // 2
    return x0 + 2 * (NAV_KEY_PX + NAV_GAP) + NAV_KEY_PX // 2, h - 8 - NAV_KEY_PX // 2


def back_center(w: int, h: int) -> tuple[int, int]:
    """back 键（nav.back）中心——销窗不杀进程（真泄漏形载具）。"""
    x0 = (w - NAV_BAR_W) // 2
    return x0 + NAV_KEY_PX // 2, h - 8 - NAV_KEY_PX // 2


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


def verify_boot_a_click(
    text: str,
    ready_pos: int,
    demo_before: int,
    quiet_before: int,
    log_path: Path | None = None,
) -> None:
    """Boot A 点击后精确验收：1/1/0 + 新 APPDEMO 恰一 + 零 APPQUIET。

    计数与顺序任一不符都 FAIL（重复点击/双 spawn/顺序倒置不得假绿）。
    guifbd 面：三标记（BANDC/BANDR/WMSUB）全日志恰一。
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
    # 裁决 B 面：deferral 行恰一（boot 批入册不拉起的记账——多次/零次
    # 都 FAIL）。
    _require_exact_count(text, MARK_APP_DEMO_DEFERRED, 1, "boot A click", log_path)
    demo_after = text.count(MARK_APP_DEMO_UP)
    if demo_after != demo_before + 1:
        raise SmokeError(
            f"boot A click: 新增 {MARK_APP_DEMO_UP!r} "
            f"{demo_after - demo_before} 行 != 1（点击未 spawn APPDEMO？）",
            log_path,
        )
    if demo_after != 1:
        raise SmokeError(
            f"boot A click: {MARK_APP_DEMO_UP!r} 全日志 {demo_after} 行"
            " != 1（裁决 B「恰 1 行」——boot 批/点击外多 spawn）",
            log_path,
        )
    quiet_after = text.count(MARK_APP_QUIET_UP)
    if quiet_after != quiet_before:
        raise SmokeError(
            f"boot A click: 新增 {MARK_APP_QUIET_UP!r} "
            f"{quiet_after - quiet_before} 行——点击误 spawn APPQUIET",
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
    # guifbd 三标记（guest.rs one-shot latch 对拍）：全日志各恰一次。
    # BANDC/BANDR boot 期 seed 自验证拍同批落印（见 :25 注）；WMSUB = wm
    # 私有通道首快照接入拍——纯恰一计数零序锚（跨通道无协议序，不设顺
    # 序断言）。门在停机后全量日志上计数（非 boot 期等待）。
    _require_exact_count(text, MARK_BANDC, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_BANDR, 1, "boot A click", log_path)
    _require_exact_count(text, MARK_WMSUB, 1, "boot A click", log_path)


def verify_boot_b(text: str, log_path: Path | None = None) -> None:
    """Boot B 精确验收：LAYOUT==1、LAYOUTW==0、CLICK==0。"""
    n_layout = len(re.findall(RE_LAYOUT_N2, text))
    if n_layout != 1:
        raise SmokeError(
            f"boot B: `LAYOUT ok n=2` 出现 {n_layout} 次 != 1", log_path
        )
    _require_exact_count(text, MARK_LAYOUT, 1, "boot B", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 0, "boot B", log_path)
    _require_exact_count(text, MARK_CLICK_ANY, 0, "boot B", log_path)
    # 裁决 B 面：无点击 boot 的 deferral 记账恰一；APPDEMO up 零行
    #（Boot B 不点击 = 不 spawn）。
    _require_exact_count(text, MARK_APP_DEMO_DEFERRED, 1, "boot B", log_path)
    _require_exact_count(text, MARK_APP_DEMO_UP, 0, "boot B", log_path)


def verify_normal_boot(
    text: str,
    snapshot_pos: int,
    demo_before: int,
    quiet_before: int,
    log_path: Path | None = None,
) -> None:
    """常态隔离验收：spawn 仍发生，但零 CLICK/LAYOUTW/LAYOUT/click ready。"""
    _require_exact_count(text, MARK_CLICK_ANY, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_LAYOUTW, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_LAYOUT, 0, "normal boot", log_path)
    _require_exact_count(text, MARK_CLICK_READY, 0, "normal boot", log_path)
    # 裁决 B 面：常态 boot 同走 deferral 记账（非哨兵门控行）。
    _require_exact_count(text, MARK_APP_DEMO_DEFERRED, 1, "normal boot", log_path)
    demo_after = text.count(MARK_APP_DEMO_UP)
    if demo_after != demo_before + 1:
        raise SmokeError(
            f"normal boot: 新增 {MARK_APP_DEMO_UP!r} "
            f"{demo_after - demo_before} 行 != 1（常态点击未 spawn APPDEMO）",
            log_path,
        )
    quiet_after = text.count(MARK_APP_QUIET_UP)
    if quiet_after != quiet_before:
        raise SmokeError(
            f"normal boot: 新增 {MARK_APP_QUIET_UP!r} "
            f"{quiet_after - quiet_before} 行——常态点击误 spawn APPQUIET",
            log_path,
        )
    if text.rfind(MARK_APP_DEMO_UP) < snapshot_pos:
        raise SmokeError(
            f"normal boot: 新增 {MARK_APP_DEMO_UP!r} 不在注入前快照之后",
            log_path,
        )

def verify_recycle_gate(text: str, log_path: Path | None = None) -> None:
    """回收门精确断言（--recycle-gate；纯函数，--self-test 覆盖正负样本）。

    期望全日志形态：boot 批 `app BROWSER gui deferred` 恰一 → 首击 it 行
    → 二击（多任务键）recycle → killed → it 行 → back 键 → 三击 recycle →
    killed → it 行；it 恰三（三 spawn）、recycle/killed 各恰二（两轮同 id
    回收再开）、alive 零（BROWSER 走 surface-0 兼容档不建 wmd 窗，svcmgr
    already-alive 判据恒假——同 id 重拉恒回收再开）、no EOF/exit code/
    killed for nothing 全零（烧帽回归 = no EOF 行重现 + it 迟到；修回退 =
    killed 行缺席）。
    行序锚：it[0] < recycle < killed < it[1] < recycle < killed < it[2]。
    握手行核数（r11）：`launcher: fg+ spawned` 恰三——每次真拉起进前台态
    一行，与 it 恰三互证；`launcher: fg- recents` 恰一——多任务键被消费
    恰一次（导航键消费两次 = 第二次会在前台态置位后再清一次，复审 M1
    的假红形，不得放过）；`launcher: fg- back` 恰一——返回键交接握手行。
    重注只在零消费证据时发生，故三条核数不受合法重注扰动。
    """
    _require_exact_count(
        text, MARK_GATE_DEFERRED, 1, "recycle gate", log_path
    )
    _require_exact_count(text, ECO_GATE_RECYCLE, 2, "recycle gate", log_path)
    _require_exact_count(text, ECO_GATE_KILLED, 2, "recycle gate", log_path)
    _require_exact_count(
        text, MARK_FG_PLUS + "\n", 3, "recycle gate", log_path
    )
    _require_exact_count(
        text, MARK_FG_MINUS_RECENTS + "\n", 1, "recycle gate", log_path
    )
    _require_exact_count(
        text, MARK_FG_MINUS_BACK + "\n", 1, "recycle gate", log_path
    )
    _require_exact_count(text, ECO_GATE_NO_EOF, 0, "recycle gate", log_path)
    _require_exact_count(
        text, ECO_GATE_EXIT_CODE, 0, "recycle gate", log_path
    )
    _require_exact_count(
        text, ECO_GATE_KILLED_NOTHING, 0, "recycle gate", log_path
    )
    for bad in GATE_LAUNCHER_FAILS:
        _require_exact_count(text, bad, 0, "recycle gate", log_path)
    _require_exact_count(text, ECO_GATE_ALIVE, 0, "recycle gate", log_path)
    its = list(RE_GATE_IT.finditer(text))
    recs = list(re.finditer(re.escape(ECO_GATE_RECYCLE) + r"\n", text))
    kills = list(re.finditer(re.escape(ECO_GATE_KILLED) + r"\n", text))
    if (len(its), len(recs), len(kills)) != (3, 2, 2):
        raise SmokeError(
            f"recycle gate: BROWSER it/recycle/killed 行 "
            f"{len(its)}/{len(recs)}/{len(kills)} != 3/2/2"
            "（首击 + 二击回收再拉 + 三击回收再拉；多于期望 = 误多点/连环 spawn）",
            log_path,
        )
    chain = [
        its[0].end(), recs[0].start(), kills[0].start(), its[1].start(),
        recs[1].start(), kills[1].start(), its[2].start(),
    ]
    if any(a >= b for a, b in zip(chain, chain[1:])):
        raise SmokeError(
            "recycle gate: 行序破缺（期望 it → recycle → killed → it → "
            "recycle → killed → it；got "
            + " < ".join(str(p) for p in chain) + "）",
            log_path,
        )


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
    # VNC 直通注入臂（--input-path vnc）：QEMU 另挂 -vnc 127.0.0.1:0——
    # 从 5900 起自动避让在用口，实口经 QMP query-vnc 探明（不猜）。TCP
    # 形态是判例驳斥快照实验陷阱段结论（unix vnc.sock 异常关闭，TCP 口
    # 正常）。默认 qmp 路不加此旗（CI 零扰动）。判例锚：
    # .agents/research/证据快照/2026-11-20-VNC直通绝对坐标判例驳斥.md
    if getattr(args, "input_path", "qmp") == "vnc":
        cmd += ["-vnc", "127.0.0.1:0"]
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
        vnc_note = (
            "，VNC 127.0.0.1 自动口" if self.args.input_path == "vnc" else ""
        )
        log(
            f"boot {self.phase}: QEMU 启动（serial→{self.log_path.name}，"
            f"QMP→{self.sock_path.name}，m=256M，display none{vnc_note}）"
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


def _boot_base_markers() -> list[str]:
    return [MARK_BERRYBOOT, MARK_ROOTFS, MARK_APP_N10, MARK_LAUNCH, MARK_PAGE]


def _wait_boot_batch(boot: Boot, timeout: float) -> None:
    """基础 5 + 裁决 B deferral 面：[ui] app 入册不拉起的 boot 批记账。

    波 4 裁决 B（W4-RECON §1.7）后 boot 批不再拉起 [ui] app——原
    「boot 批 APPDEMO up」锚退役，桌面就绪锚 = 基础 5 本体（LAUNCH/PAGE）
    ；boot 批证据改验 deferral 行恰在席（点击 spawn 前置面）。
    """
    boot.wait_markers(_boot_base_markers(), timeout)
    boot.wait_markers([MARK_APP_DEMO_DEFERRED], timeout)


def _warn_input_armed(boot: Boot) -> None:
    # launcher 已挂 INPUT_POLL（请求已入 inputd 队列）。这是状态谓词，
    # 不保证 inputd 已 harvest——点击后的 CLICK 等待窗兜住后者。
    if not boot.wait_any([MARK_INPUT_ARMED], 60.0):
        warn(
            f"未等到「{MARK_INPUT_ARMED}」——QMP 事件可能早于 guest "
            "挂缓冲而被设备丢弃；仍继续"
        )


def _aged_wait(label: str, minutes: float) -> None:
    # aged 注入用例（rejected 判例盲区补口）：armed 即测即退永远测不到
    # 「放久楔死」（输入链新靴活/放久死的时变面）——click ready 门通过
    # 后、注入前静置 N 分钟再点。默认 0 = 关闭，行为与现状等价（CI 零
    # 扰动）。只给注入腿用；纯隔离验证腿（boot B）不加。
    if minutes <= 0:
        return
    start = time.strftime("%H:%M:%S")
    log(f"{label}: aged 等待起——静置 {minutes:g} 分钟（{start}）")
    time.sleep(minutes * 60.0)
    end = time.strftime("%H:%M:%S")
    log(f"{label}: aged 等待止（{start} → {end}），续注入")


def _inject_click(boot: "Boot", cx: int, cy: int, w: int, h: int) -> None:
    """boot A 单击注入分派：--input-path qmp=QMP 单批（abs+down+up 同投）；vnc=RFB PointerEvent 直通。"""
    if boot.args.input_path == "vnc":
        assert boot.qmp is not None
        port = vnc_port_from_qmp(boot.qmp)
        rfb = RfbClient("127.0.0.1", port)
        try:
            # 坐标域门：VNC 表面几何（ServerInit 实拍）必须与串口解析几何
            # 全等——本 rig fw_cfg 800x600 宽度已 32 对齐，两域 1:1；不等
            # 即几何单源链（QEMU 表面几何→设备 ABS_INFO→inputd 归一）漂移
            # ，fail closed 勿缩放硬点（宿主做缩放=私藏客机几何，正是判例
            # 驳斥后要退役的形态）。
            if (rfb.fb_w, rfb.fb_h) != (w, h):
                raise SmokeError(
                    f"boot {boot.phase}: VNC 表面 {rfb.fb_w}x{rfb.fb_h} != "
                    f"串口几何 {w}x{h}——两域不一致，fail closed（勿缩放）",
                    boot.log_path,
                )
            log(
                f"boot {boot.phase}: VNC 直通注入单击（127.0.0.1:{port}，"
                "RFB PointerEvent move → left down → up，带 SetEncodings）"
            )
            rfb.click(cx, cy)
        finally:
            rfb.close()
    else:
        log(f"boot {boot.phase}: QMP 注入单击（abs → btn down → btn up）")
        boot.qmp.click(cx, cy, w, h)


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
        _wait_boot_batch(boot_a, args.boot_a_timeout)
        _warn_input_armed(boot_a)

        # click ready = snap-to-0 已收敛的状态谓词，**强制在席恰一次**；
        # 缺席即 FAIL，不再 sleep-and-click 回退。
        boot_a.wait_markers([MARK_CLICK_READY], args.click_ready_timeout)
        ready_pos = boot_a.log.text.find(MARK_CLICK_READY)
        demo_before = boot_a.log.text.count(MARK_APP_DEMO_UP)
        quiet_before = boot_a.log.text.count(MARK_APP_QUIET_UP)
        # 裁决 B 面（波 4 两处重裁之二）：boot 批 0 拉起——点击前
        # APPDEMO up 必须零行，点击后的全日志恰 1 行才成立。
        if demo_before != 0:
            raise SmokeError(
                f"boot A: 点击前 {MARK_APP_DEMO_UP!r} 已有 {demo_before} 行"
                "——boot 批仍拉起 [ui] app（裁决 B 未生效？）",
                boot_a.log_path,
            )
        log(
            f"boot A: click ready 在席（pos={ready_pos}，"
            f"boot 批 APPDEMO up×{demo_before}，deferral 生效）"
        )

        w, h, src = geometry_from_log(boot_a.log)
        cx, cy = cell0_center(w, h)
        log(f"boot A: display {w}x{h}（{src}）→ cell(0,0) 中心 ({cx},{cy})")

        boot_a.qmp_connect()
        # aged 用例锚点：click ready 门已过、demo_before/quiet_before 快照
        # 已取——静置窗后原快照续用（aged 期冒出的 up 行会被精确计数
        # 验证面 FAIL 兜住，不静默吞）。qmp 连接在 vnc 形下同样在席
        # （quit 收尾 + query-vnc 端口发现都走它）。
        _aged_wait("boot A", args.aged_minutes)
        _inject_click(boot_a, cx, cy, w, h)

        # 点击后等待三项同时成立：CLICK 恰一、LAYOUTW、新增 APPDEMO up。
        # （批B BANDR 不入等待集——首触发 = guifbd seed 自验证拍，boot 期
        # BANDC 同拍已落，远早于点击。）
        try:
            boot_a.log.wait_until(
                boot_a.proc,
                lambda t: re.search(RE_CLICK_N1, t) is not None
                and MARK_LAYOUTW in t
                and t.count(MARK_APP_DEMO_UP) > demo_before,
                f"{MARK_CLICK} + {MARK_LAYOUTW} + 新 {MARK_APP_DEMO_UP}",
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
            raise MissingMarkers(
                "boot A click",
                ", ".join(miss) or f"{MARK_CLICK} + {MARK_LAYOUTW}",
                args.click_timeout,
                boot_a.log_path,
            ) from None

        # 先停 QEMU 再对最终串口日志做精确计数（重复/顺序假绿 fail loud）。
        watcher_a = boot_a.log
        log_path_a = boot_a.log_path
        boot_a.stop()
        boot_a = None
        watcher_a.poll()
        verify_boot_a_click(
            watcher_a.text, ready_pos, demo_before, quiet_before, log_path_a
        )
        log(
            "boot A: 精确计数 CLICK×1 LAYOUTW×1 LAYOUT×0；"
            f"新增 {MARK_APP_DEMO_UP}×1、APPQUIET×0；"
            "guifbd BANDC/BANDR/WMSUB×1"
        )

        verify_data_disk(disk)
        cells_a = read_layout_canonical(disk)
        log(
            f"boot A: LAYOUT.JSN 宿主读回 canonical "
            f"{len(LAYOUT_CANONICAL)}B / {len(cells_a)} cells "
            f"（{LAYOUT_CELLS_EXPECTED!r}）"
        )

        if args.only_a:
            success = True
            log(
                "PASS（--only-a）：Boot A 单击 + LAYOUTW + canonical 落盘读回；"
                "Boot B 未跑"
            )
            return 0

        # ---------------- Boot B ----------------
        hash_before_b = sha256_file(disk)
        boot_b = Boot("B", args, run_dir, disk, "b")
        boot_b.start()
        boot_b.wait_markers(_boot_base_markers(), args.boot_b_timeout)
        boot_b.wait_regex(RE_LAYOUT_N2, args.boot_b_timeout)

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

        success = True
        log(
            "PASS：EFI GUI 交互相双 boot（A: CLICK ok n=1 + LAYOUTW ok + "
            f"APPDEMO up；B: LAYOUT ok n=2 + 零重写 + canonical 109B）"
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
        write_layout_for_selftest(disk)
        log(
            f"新常态盘：{disk}（{disk.stat().st_size}B，"
            f"sha256={sha256_file(disk)[:16]}…，LBA0 印章，零 GUI8.SMK）"
        )

    boot: Boot | None = None
    success = False
    try:
        boot = Boot("normal", args, run_dir, disk, "normal")
        boot.start()
        _wait_boot_batch(boot, args.boot_a_timeout)
        _warn_input_armed(boot)

        # 快照点 = 注入前（哨兵缺席时 click ready 结构性不打印，不能拿它
        # 当门）。boot 批 deferral 行已吃完（裁决 B：up 行零基线），此后
        # 的新增只能来自本次点击。
        boot.log.poll()
        snapshot_pos = len(boot.log.text)
        demo_before = boot.log.text.count(MARK_APP_DEMO_UP)
        quiet_before = boot.log.text.count(MARK_APP_QUIET_UP)
        if MARK_CLICK_READY in boot.log.text:
            raise SmokeError(
                f"normal boot: 哨兵缺席却打印 {MARK_CLICK_READY!r}——"
                "click ready 未按 GUI8.SMK 门控",
                boot.log_path,
            )

        w, h, src = geometry_from_log(boot.log)
        cx, cy = cell0_center(w, h)
        log(f"normal-boot: display {w}x{h}（{src}）→ cell(0,0) 中心 ({cx},{cy})")

        boot.qmp_connect()
        log("normal-boot: QMP 注入单击（abs → btn down → btn up）")
        boot.qmp.click(cx, cy, w, h)

        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_APP_DEMO_UP) > demo_before,
                f"新 {MARK_APP_DEMO_UP}（常态 spawn 活证）",
                args.click_timeout,
                "normal boot click",
            )
        except MissingMarkers:
            raise MissingMarkers(
                "normal boot click",
                f"新 {MARK_APP_DEMO_UP}（常态 spawn 活证）",
                args.click_timeout,
                boot.log_path,
            ) from None

        watcher = boot.log
        log_path = boot.log_path
        boot.stop()
        boot = None
        watcher.poll()
        verify_normal_boot(
            watcher.text, snapshot_pos, demo_before, quiet_before, log_path
        )
        verify_data_disk(disk, expect_smoke=False)
        log(
            "normal-boot: spawn 仍发生（新增 APPDEMO up×1），"
            "且 CLICK/LAYOUTW/LAYOUT/click ready 全零"
        )

        success = True
        log("PASS（--normal-boot）：哨兵缺席的常态点击隔离成立")
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
# 活体 app 回收时延回归门（2026-11-21 冻结案测试面补缺）
# =====================================================================


def _gate_click_wait_spawn(
    boot: Boot,
    x: int,
    y: int,
    w: int,
    h: int,
    label: str,
    fail_fast: str | None = None,
    consumed_mark: str | None = None,
    step_timeout: float = GATE_CLICK_STEP_TIMEOUT,
    total_timeout: float = 120.0,
) -> tuple[float, int]:
    """回收门「点击 → 等拉起握手」单元，两段式。返回（自首击起的墙钟
    秒，重试次数）。

    段一（消费证据）：发一击后等任一「该击已被启动器消费」的证据——新
    `eco: app BROWSER it`（svcmgr 在阻塞 APP_SPAWN 内打 = RPC 在途）、新
    `consumed_mark`（二击臂传 `launcher: fg- recents`：多任务键消费拍**先**
    清前台态再入队，该行早于 RPC）、或新 `launcher: fg+ spawned`（已完成）。
    step_timeout 内零证据 = 该击未达启动器（零动作 / 丢失）→ 重注一击，
    至多 GATE_MAX_RETRIES 次。
    段二（在途续等）：证据已出即该击已消费——只续等「新 it ∧ 新 fg+」，
    **绝不重注**；帽 = total_timeout − 自首击已耗时（每击总帽有限）。

    为什么证据后绝不重注（复审 M1）：多任务键是导航键，frameloop.rs
    on_nav_click 的 recents 分支在 `app_fg.is_some()` 时先 leave_fg 再
    take `recents_app`——RPC 在途时重注的那一击会在 RPC 归来、前台态置位
    后的下一拍再清一次前台态（`recents_app` 已 take 取到 None 也不免），
    返回键步此后永远等不到 `fg- back`、必红且文案错指返回键。图标步的
    重注虽被前台态整击吞（无多拉起），但同样无必要——证据已出就是 RPC
    在途，等就是；在途超总帽以明确文案红。

    `fail_fast`（多任务键臂传 alive 行）：该行一出现立即 SmokeError，不等
    满帽、不重试（已活路是判据性失败，重击无意义）。

    零证据重注的安全性：证据缺席 = 启动器尚未处理该击（或已丢）。图标步
    重击与首击同格——落同一 `pending_spawn` 单槽或被前台态吞，至多一次
    拉起；二击/返回键若首击只是迟到（仍在队列里）会被消费两次——二击的
    `fg- recents` 恰一核数把它判红并指向多任务键（不假绿），返回键的
    leave_fg 以 `app_fg.is_some()` 为门、重击零动作。实测出队到处理最长
    5.8s，step_timeout=30s 余量 5 倍。
    """
    boot.log.poll()
    it0 = len(RE_GATE_IT.findall(boot.log.text))
    fg0 = boot.log.text.count(MARK_FG_PLUS)
    cm0 = boot.log.text.count(consumed_mark) if consumed_mark else 0

    def _new_it(t: str) -> bool:
        return len(RE_GATE_IT.findall(t)) > it0

    def _done(t: str) -> bool:
        return _new_it(t) and t.count(MARK_FG_PLUS) > fg0

    def _evidence(t: str) -> bool:
        if _new_it(t):
            return True
        return consumed_mark is not None and t.count(consumed_mark) > cm0

    def _ff(t: str) -> bool:
        return fail_fast is not None and fail_fast in t

    evidence_desc = (
        f"{label}已被消费的证据（新 eco: app BROWSER it 行"
        + (f" / 新 {consumed_mark}" if consumed_mark else "")
        + f" / 新 {MARK_FG_PLUS}）"
    )
    done_desc = (
        f"{label}后新 eco: app BROWSER it 行 + 新 {MARK_FG_PLUS}（在途续等，不重注）"
    )
    t0 = time.monotonic()
    retries = 0
    while True:
        boot.qmp.click(x, y, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: _evidence(t) or _ff(t),
                evidence_desc,
                step_timeout,
                "recycle gate",
            )
            break
        except MissingMarkers:
            if retries >= GATE_MAX_RETRIES:
                raise
            retries += 1
            log(
                f"recycle-gate: {label} {step_timeout:.0f}s 内未见消费证据"
                f"——重注（第 {retries}/{GATE_MAX_RETRIES} 次重试）"
            )
    if _ff(boot.log.text):
        raise SmokeError(
            f"recycle gate: {label}走了已活路——{fail_fast} 先于新 it"
            "（BROWSER 无 wmd 窗应走回收路：载具换型或 alive 判据变化）",
            boot.log_path,
        )
    remaining = total_timeout - (time.monotonic() - t0)
    if remaining <= 0:
        raise MissingMarkers(
            "recycle gate", done_desc, total_timeout, boot.log_path
        )
    boot.log.wait_until(
        boot.proc,
        lambda t: _done(t) or _ff(t),
        done_desc,
        remaining,
        "recycle gate",
    )
    if _ff(boot.log.text):
        raise SmokeError(
            f"recycle gate: {label}走了已活路——{fail_fast} 先于新 it"
            "（BROWSER 无 wmd 窗应走回收路：载具换型或 alive 判据变化）",
            boot.log_path,
        )
    return time.monotonic() - t0, retries


def _gate_click_wait_back(
    boot: Boot, x: int, y: int, w: int, h: int,
    step_timeout: float = GATE_BACK_STEP_TIMEOUT,
) -> int:
    """回收门返回键交接：点返回键，等启动器打出 `launcher: fg- back`
    （leave_fg 清前台态时随打的握手行）；GATE_BACK_STEP_TIMEOUT 内未到即
    重注，至多 GATE_MAX_RETRIES 次。返回重试次数。

    原动作为点返回键后固定 sleep(2.0) 再点图标：返回键点击若未被消费，
    门不知道，随后的图标点击被前台态整击吞掉，只能等满帽才红。现以
    握手行替代——`fg- back` 在席才点图标。

    重注安全性（与二击不同，这里重注恒安全）：back 分支的 leave_fg 以
    `app_fg.is_some()` 为门——首次已消费 → 前台态已清，重击走次级语义
    （查表无焦点应用窗 / tab=0 / page=0 → 零动作、不打行）；首次未消费
    → 重击即首击；首次只是迟到（仍在队列里）→ 两击先后消费、第二击零
    动作。三形下 `fg- back` 恒恰一行（verify_recycle_gate 核数）。
    """
    boot.log.poll()
    base = boot.log.text.count(MARK_FG_MINUS_BACK)
    retries = 0
    while True:
        boot.qmp.click(x, y, w, h)
        try:
            boot.log.wait_until(
                boot.proc,
                lambda t: t.count(MARK_FG_MINUS_BACK) > base,
                f"{MARK_FG_MINUS_BACK}（返回键已清前台态）",
                step_timeout,
                "recycle gate back",
            )
            break
        except MissingMarkers:
            if retries >= GATE_MAX_RETRIES:
                raise
            retries += 1
            log(
                f"recycle-gate: 返回键 {step_timeout:.0f}s 内未等到"
                f"{MARK_FG_MINUS_BACK}——重注（第 {retries}/{GATE_MAX_RETRIES} "
                "次重试）"
            )
    return retries


def run_recycle_gate(args) -> int:
    """单 boot 回收门：BROWSER 图标首击（spawn）→ 多任务键二击（同 id
    回收）——时延帽 + eco 行序 + 零降级行三面断言。

    选 BROWSER 不选 CONTACTS：CONTACTS/PHOTOS/SETTINGS/TERMINAL 是 dock
    app（desktop.rs DOCK_IDS），紧凑档（800x600）无 dock 渲染不可点；
    BROWSER 非 dock 非 test = 网格可点。二击走 nav.recents 重拉（导航键
    覆盖态恒最先——app 窗在盖也不挡）而非再点图标（覆盖态点击门会先收
    盖消费点击，窗起时序不可观测 = 不确定）。

    三段制（首击 it → 二击 recycle/killed/it → back 键 → 三击 recycle/
    killed/it）。二击 = 多任务键：启动器 R8 调查刀修复后，自真拉起成功起
    进前台态——非导航键点击整击吞掉（同图标再击到不了 spawn 面），返回/
    主页/多任务三键照常；多任务键 = 回桌面 + 重拉上次应用，是前台态下重
    拉应用的入口（走同一 spawn_by_id_inner → svcmgr 同 id 命中）。BROWSER
    以 surface-0 兼容档 SUBMIT、不建 wmd 窗，svcmgr already-alive 判据
    （可见 APP 窗在场）恒假——同 id 重拉恒走回收再开，故本门断言两轮
    recycle → killed → it、alive 零（alive 路需建窗 app 作载具，不在本门）。

    交接全走握手行（r11 修：原固定 sleep 与慢点击是红轮直接诱因）：
    三次拉起各等「新 it ∧ 新 `launcher: fg+ spawned`」——it 是 svcmgr 在
    阻塞 RPC 内打的，fg+ 才表示启动器已出 RPC；返回键等 `launcher: fg-
    back`。每击只在「零消费证据」时重注（至多两次）：证据（新 it / 二击
    的 `fg- recents`）一出即 RPC 在途、只续等不重注，每击总帽
    click_timeout（安全性见 `_gate_click_wait_spawn` 头注）。
    """
    preflight(args)
    print_marker_status()
    if args.only_a or args.normal_boot:
        raise SmokeError("--recycle-gate 与 --only-a/--normal-boot 互斥")

    run_dir = Path(args.work_dir) if args.work_dir else Path(
        tempfile.mkdtemp(prefix="gui8-recycle.", dir=args.tmp_dir)
    )
    run_dir.mkdir(parents=True, exist_ok=True)
    log(f"recycle-gate run dir：{run_dir}")

    disk = Path(args.data_disk) if args.data_disk else run_dir / "recycle.img"
    if disk.exists():
        if not args.reuse_data_disk:
            raise SmokeError(f"数据盘已存在：{disk}（覆盖请删或 --reuse-data-disk）")
        verify_data_disk(disk, expect_smoke=False)
        read_layout_gate(disk)
        log(f"复用回收门盘：{disk}（无 GUI8.SMK，LAYOUT_GATE 精确读回）")
    else:
        disk.parent.mkdir(parents=True, exist_ok=True)
        create_data_disk(disk, with_smoke=False)
        # 单 cell BROWSER 布局：官方 app 在席 → APPDEMO/APPQUIET 按桌面
        # 建树规则隐藏（本门零依赖）；BROWSER 落 (0,0,0) = 首击靶位。
        write_layout_bytes(disk, LAYOUT_GATE)
        read_layout_gate(disk)
        log(
            f"新回收门盘：{disk}（{disk.stat().st_size}B，"
            f"sha256={sha256_file(disk)[:16]}…，LBA0 印章，零 GUI8.SMK）"
        )

    boot: Boot | None = None
    success = False
    try:
        boot = Boot("recycle", args, run_dir, disk, "recycle")
        boot.start()
        _wait_boot_batch(boot, args.boot_a_timeout)
        _warn_input_armed(boot)

        boot.log.poll()
        if MARK_CLICK_READY in boot.log.text:
            raise SmokeError(
                f"recycle gate: 哨兵缺席却打印 {MARK_CLICK_READY!r}",
                boot.log_path,
            )

        w, h, src = geometry_from_log(boot.log)
        cx, cy = cell0_center(w, h)
        rx, ry = recents_center(w, h)
        log(
            f"recycle-gate: display {w}x{h}（{src}）→ BROWSER cell(0,0) "
            f"({cx},{cy})；recents 键 ({rx},{ry})"
        )

        boot.qmp_connect()

        # 首击：图标 spawn（活体 console=false 实例入 APP_CHILDREN）。原：
        # 点一次、单等新 it 行 click_timeout=120s，超时即红；现：等「新 it
        # ∧ 新 fg+」，30s 内零消费证据重注（至多两次）、证据一出只续等，
        # 每击总帽仍 click_timeout。等价性：绿轮里 it 恒在数秒内到、fg+
        # 紧随其后，首击一次即过；帽短不改变断言面（it 行仍是 svcmgr 打
        # 的拉起活证），只把「该击零动作」从等满 120s 变成 30s 后补一击。
        log("recycle-gate: QMP 注入首击（BROWSER 图标）")
        first_latency, retry1 = _gate_click_wait_spawn(
            boot, cx, cy, w, h, "首击", total_timeout=args.click_timeout
        )

        # 二击：多任务键 = 同 id 回收再开。启动器自真拉起成功起进前台态——
        # 非导航键点击整击吞掉（同图标再击到不了 spawn 面，原「同位重击 =
        # already-alive 聚焦路」在新语义下不成立）；导航键恒最先：多任务键 =
        # 收前台态回桌面 + 重拉上次应用 → 同一 spawn_by_id_inner → svcmgr
        # 同 id 命中；BROWSER 不建 wmd 窗，already-alive 判据恒假 → 回收再
        # 开（recycle → killed → 新 it）。alive 行先到 = 走了聚焦路（载具
        # 换型或判据变化），即败（不等满帽、不重试——fail_fast）。
        # 重注边界（复审 M1）：recents 分支在 `app_fg.is_some()` 时**先**
        # leave_fg("recents") 再 take `recents_app`——RPC 在途时重注会在
        # 前台态置位后再清一次，返回键步必红。故本步以 `fg- recents` 作
        # 消费证据：它一出即视为已消费、只续等不重注（图标步以 it 为证据，
        # 同律）；`fg- recents` 恰一核数兜底。
        log("recycle-gate: QMP 注入二击（多任务键；同 id 回收再开）")
        recents_latency, retry2 = _gate_click_wait_spawn(
            boot, rx, ry, w, h, "二击", fail_fast=ECO_GATE_ALIVE,
            consumed_mark=MARK_FG_MINUS_RECENTS,
            total_timeout=args.click_timeout,
        )

        # 三击前置：back 键销窗不杀进程（真泄漏形）——此后同 id 窗不在场，
        # 三击才走回收（外部击杀形）。原：点返回键后固定 sleep(2.0)（销窗
        # RPC 往返 + 表广播沉淀），点了就算；现：点返回键后等启动器的
        # `launcher: fg- back`（leave_fg 清前台态的握手行），帽 10s 内未到
        # 重注（至多两次；back 的 leave_fg 以 app_fg 为门，重注恒安全）。
        # 等价性：绿轮里返回键一击即被消费、握手行当拍打出，等待时长
        # ≪ 2s；红轮（返回键未被消费）原要等满三击帽才红，现在 30s 内
        # 以明确文案红。
        bx, by = back_center(w, h)
        log("recycle-gate: QMP 注入返回键（清前台态）")
        retry_back = _gate_click_wait_back(boot, bx, by, w, h)

        # 三击：同图标 → recycle 路。时延帽 = max(floor, 首击 + headroom)：
        # 烧帽回归（+60s）必爆帽；headroom 默认 40s < 60s 且 >> 正常回收。
        # 原以帽本身作单等超时（到 = 过）；现零证据重注至多三次、每次
        # 30s，之后显式比帽——自首击起算的墙钟时延 > 帽即 FAIL（烧帽回归
        # 60s 的 it 迟到仍被此比抓住，重试不掩盖回归；重注本身也吃进时延，
        # 一次重注即近爆帽）。
        budget = max(
            args.recycle_budget_floor, first_latency + args.recycle_headroom
        )
        log(f"recycle-gate: QMP 注入三击（同图标；recycle 路；帽 {budget:.0f}s）")
        third_latency, retry3 = _gate_click_wait_spawn(
            boot, cx, cy, w, h, "三击", total_timeout=args.click_timeout
        )
        if third_latency > budget:
            raise SmokeError(
                f"recycle gate: 三击回收时延 {third_latency:.1f}s > 帽 "
                f"{budget:.0f}s（EOF 烧帽回归面；重试 {retry3} 次）",
                boot.log_path,
            )

        boot.log.poll()
        verify_recycle_gate(boot.log.text, boot.log_path)
        verify_data_disk(disk, expect_smoke=False)
        read_layout_gate(disk)
        log(
            f"recycle-gate: 门过（首击 {first_latency:.1f}s / 二击 "
            f"{recents_latency:.1f}s / 三击 {third_latency:.1f}s ≤ 帽 "
            f"{budget:.0f}s；重试 首击 {retry1} / 二击 {retry2} / 返回键 "
            f"{retry_back} / 三击 {retry3}；"
            "it → recycle → killed → it 行序成立，零 no EOF/exit code）"
        )

        success = True
        log(
            "PASS（--recycle-gate）：活体 console=false app 同 id 回收"
            "恒时延帽内（EOF 烧帽回归面 CI 可见）"
        )
        return 0
    finally:
        if boot is not None:
            boot.stop()
        if success and not args.keep:
            shutil.rmtree(run_dir, ignore_errors=True)
        else:
            log(f"artifacts 保留：{run_dir}")
            log(f"  串口：{run_dir}/boot-recycle.log")
            log(f"  数据盘：{disk}")
            log(f"  命令：{run_dir}/boot-recycle.cmd.txt")


# =====================================================================
# dry-run
# =====================================================================


def dry_run(args) -> int:
    preflight(args)
    print_marker_status()
    if args.recycle_gate:
        raise SmokeError("--dry-run 与 --recycle-gate 互斥（门无 dry-run 形——盘面/断言与 dual/normal 皆不同）")
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
        log(f"dry-run: input-path = {args.input_path}（boot A 单击注入通路）")
        log(f"dry-run: image = {args.image}")
        log(f"dry-run: code  = {args.code}")
        log(f"dry-run: vars  = {args.vars or '(未给；每 boot 仍产 fresh blank)'}")
        log(f"dry-run: disk  = {disk}（sha256={sha256_file(disk)}）")
        log(f"dry-run: disk 根目录 = {Fat12Volume(disk).list_root()}")
        if normal:
            markers = [
                MARK_BERRYBOOT,
                MARK_ROOTFS,
                MARK_APP_N10,
                MARK_LAUNCH,
                MARK_PAGE,
                MARK_APP_DEMO_UP,
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
                        MARK_APP_N10,
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
                        MARK_APP_N10,
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


class MockRfb:
    """最小 RFB server（自检）：无鉴权 3.8 握手 + 分段发送夹逼读侧
    leftover 缓冲 + 全量记录客户端字节流（三铁律字节级对拍）。"""

    # ServerInit 像素格式 16B（bpp32/depth24/LE/truecolor + rgb max +
    # shift + pad）——input-only 客户端不消费，只占位。
    PIXFMT = bytes(
        [32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 16, 8, 0, 0, 0]
    )

    def __init__(self):
        self.srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.srv.bind(("127.0.0.1", 0))
        self.srv.listen(1)
        self.srv.settimeout(10.0)
        self.port = self.srv.getsockname()[1]
        self.client_stream = b""
        self.error: Exception | None = None
        self.thread = threading.Thread(target=self._serve, daemon=True)

    def start(self):
        self.thread.start()
        return self

    def _serve(self):
        name = b"QEMU (gui8-mock)"
        try:
            conn, _ = self.srv.accept()
            conn.settimeout(10.0)
            # 分段计划：version 与安全类型清单挤一坨（12B 版本读须留 2B
            # leftover）；安全 result 拆 1B+3B（后半与 ServerInit 宽高同
            # 坨）；像素格式拆 7B+9B（后半与名长同坨）——粘包/半包双向
            # 夹逼 _read_exact 的 leftover 路径（铁律二）。
            conn.sendall(b"RFB 003.008\n" + b"\x01\x01")
            time.sleep(0.05)
            conn.sendall(b"\x00")
            time.sleep(0.05)
            conn.sendall(b"\x00\x00\x00" + struct.pack(">HH", 800, 600))
            time.sleep(0.05)
            conn.sendall(self.PIXFMT[:7])
            time.sleep(0.05)
            conn.sendall(
                self.PIXFMT[7:] + struct.pack(">I", len(name)) + name
            )
            # 客户端应发恰 40B：版本 12 + 安全选择 1 + ClientInit 1 +
            # SetEncodings 8 + PointerEvent×3 各 6（多一字节即 pad 违律）。
            buf = bytearray()
            while len(buf) < 40:
                chunk = conn.recv(40 - len(buf))
                if not chunk:
                    break
                buf += chunk
            self.client_stream = bytes(buf)
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
        log(
            "self-test: LAYOUT canonical ok（109B 精确双 cell；超长/错 id/错 page 均 FAIL）"
        )

        # 2) 几何
        if cell0_center(800, 600) != (95, 95):
            raise SmokeError(f"cell0_center 异常：{cell0_center(800, 600)}")
        # 刀 D 后：grid_h = 1000-(44+8+16+44+8)=880；cell_h=(880-48)/5=166。
        if cell0_center(480, 1000) != (55, 135):
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

        # 裁决 B 形：boot 批 deferral 行（非 up 行）+ 点击后恰 1 up 行；
        # guifbd 形：三标记 boot 侧落印（样本按现网时序拟真排布：WMSUB
        # 随首快照先落，BANDC/BANDR seed 对随后——门只计恰一数不锚序）。
        fake_a = (
            f"{MARK_APP_DEMO_DEFERRED}\n"
            f"{MARK_LAYOUTW}\n"
            f"{MARK_WMSUB}\n"
            f"{MARK_BANDC}\n"
            f"{MARK_CLICK_READY}\n"
            f"{MARK_CLICK}\n"
            f"{MARK_APP_DEMO_UP}\n"
            f"{MARK_BANDR}64\n"
        )
        pos_a = fake_a.index(MARK_CLICK_READY)
        verify_boot_a_click(fake_a, pos_a, 0, 0)
        expect_error(
            "Boot A 重复 CLICK",
            lambda: verify_boot_a_click(
                fake_a + MARK_CLICK + "\n", pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 重复 LAYOUTW",
            lambda: verify_boot_a_click(
                fake_a + MARK_LAYOUTW + "\n", pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 出现 LAYOUT",
            lambda: verify_boot_a_click(
                fake_a + "LAYOUT ok n=2\n", pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 缺新 APPDEMO",
            lambda: verify_boot_a_click(
                f"{MARK_APP_DEMO_DEFERRED}\n{MARK_CLICK_READY}\n"
                f"{MARK_CLICK}\n{MARK_LAYOUTW}\n",
                fake_a.index(MARK_CLICK_READY),
                0,
                0,
            ),
        )
        expect_error(
            "Boot A 缺 deferral 记账",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_APP_DEMO_DEFERRED + "\n", ""),
                pos_a,
                0,
                0,
            ),
        )
        expect_error(
            "Boot A boot 批仍拉起 APPDEMO（裁决 B 破）",
            lambda: verify_boot_a_click(
                MARK_APP_DEMO_UP + "\n" + fake_a, pos_a, 1, 0
            ),
        )
        expect_error(
            "Boot A 误 spawn APPQUIET",
            lambda: verify_boot_a_click(
                fake_a + MARK_APP_QUIET_UP + "\n", pos_a, 0, 0
            ),
        )
        order_bad = fake_a.replace(MARK_CLICK_READY + "\n", "").replace(
            MARK_CLICK + "\n", MARK_CLICK + "\n" + MARK_CLICK_READY + "\n"
        )
        expect_error(
            "Boot A CLICK 早于 click ready",
            lambda: verify_boot_a_click(
                order_bad, order_bad.index(MARK_CLICK_READY), 0, 0
            ),
        )
        expect_error(
            "Boot A 缺 click ready",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_CLICK_READY + "\n", ""),
                fake_a.index(MARK_CLICK_READY),
                0,
                0,
            ),
        )
        # 负样本：标记计数面（BANDC/WMSUB 各缺与重复两枚）。
        expect_error(
            "Boot A 缺 BANDC",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_BANDC + "\n", ""), pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 重复 BANDC",
            lambda: verify_boot_a_click(
                fake_a + MARK_BANDC + "\n", pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 缺 WMSUB",
            lambda: verify_boot_a_click(
                fake_a.replace(MARK_WMSUB + "\n", ""), pos_a, 0, 0
            ),
        )
        expect_error(
            "Boot A 重复 WMSUB",
            lambda: verify_boot_a_click(
                fake_a + MARK_WMSUB + "\n", pos_a, 0, 0
            ),
        )
        log(
            "self-test: Boot A 精确断言 ok（1/1/0 + deferral×1 + "
            "APPDEMO up 恰 1；重复/错序/缺 ready/boot 批拉起/APPQUIET "
            "均 FAIL；guifbd 三标记 BANDC/BANDR/WMSUB 各恰 1）"
        )

        fake_b = (
            f"LAUNCH ok\n{MARK_APP_DEMO_DEFERRED}\nLAYOUT ok n=2\n"
        )
        verify_boot_b(fake_b)
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
            lambda: verify_boot_b(
                f"LAUNCH ok\n{MARK_APP_DEMO_DEFERRED}\nLAYOUT ok n=3\n"
            ),
        )
        expect_error(
            "Boot B 缺 deferral 记账",
            lambda: verify_boot_b("LAUNCH ok\nLAYOUT ok n=2\n"),
        )
        log(
            "self-test: Boot B 精确断言 ok（LAYOUT×1 / LAYOUTW×0 / "
            "CLICK×0 / deferral×1）"
        )

        fake_n = f"{MARK_APP_DEMO_DEFERRED}\n"
        snap_n = len(fake_n)
        fake_n += f"{MARK_APP_DEMO_UP}\n"
        verify_normal_boot(fake_n, snap_n, 0, 0)
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
                    fake_n + n, snap_n, 0, 0
                ),
            )
        expect_error(
            "常态隔离无新 APPDEMO",
            lambda: verify_normal_boot(
                f"{MARK_APP_DEMO_DEFERRED}\n", snap_n, 0, 0
            ),
        )
        log(
            "self-test: 常态隔离断言 ok（新 APPDEMO×1；CLICK/LAYOUTW/"
            "LAYOUT/click ready/APPQUIET 全零）"
        )

        # 3c) 回收门（--recycle-gate）：LAYOUT_GATE 65B 精确读回 + 负样本、
        #     recents 键几何、verify_recycle_gate 正负样本（killed 缺席 =
        #     主修回退形态；no EOF 在席 = 烧帽回归形态）。
        gate_disk = tmp / "gate.img"
        create_data_disk(gate_disk, with_smoke=False)
        write_layout_bytes(gate_disk, LAYOUT_GATE)
        if read_layout_gate(gate_disk) != [dict(zip(("app", "page", "row", "col"), c)) for c in LAYOUT_GATE_CELLS]:
            raise SmokeError("LAYOUT_GATE 读回 cells 漂移")
        bad_gate = tmp / "bad-gate.img"
        create_data_disk(bad_gate, with_smoke=False)
        Fat12Volume(bad_gate).add_file(
            LAYOUT_NAME11, LAYOUT_GATE.replace(b"BROWSER", b"CAMERAX")
        )
        expect_error("LAYOUT_GATE 错 app id", lambda: read_layout_gate(bad_gate))
        if recents_center(800, 600) != (520, 570):
            raise SmokeError(
                f"recents_center 800x600 异常：{recents_center(800, 600)}（期望 (520,570)）"
            )
        # 样本按现网时序：it 由 svcmgr 在阻塞 RPC 内打、`fg+ spawned` 在
        # RPC 归来后打；`fg- recents` / `fg- back` 在导航键消费拍打。
        fake_g = (
            f"{MARK_GATE_DEFERRED}\n"
            "PAGE ok n=2\n"
            "eco: app BROWSER it 03\n"
            f"{MARK_FG_PLUS}\n"
            f"{MARK_FG_MINUS_PREFIX}recents\n"
            f"{ECO_GATE_RECYCLE}\n"
            f"{ECO_GATE_KILLED}\n"
            "eco: app BROWSER wm detach\n"
            "eco: app BROWSER it 03\n"
            f"{MARK_FG_PLUS}\n"
            f"{MARK_FG_MINUS_BACK}\n"
            f"{ECO_GATE_RECYCLE}\n"
            f"{ECO_GATE_KILLED}\n"
            "eco: app BROWSER it 03\n"
            f"{MARK_FG_PLUS}\n"
        )
        verify_recycle_gate(fake_g)
        # 多任务键被消费两次（复审 M1 假红形：RPC 在途时重注的那一击在
        # 前台态置位后再清一次）——`fg- recents` 恰一核数必须 FAIL，不得
        # 当合法重试形放过。
        expect_error(
            "回收门 fg- recents 重复（多任务键被消费两次）",
            lambda: verify_recycle_gate(
                fake_g.replace(
                    MARK_FG_MINUS_RECENTS + "\n",
                    MARK_FG_MINUS_RECENTS + "\n" + MARK_FG_MINUS_RECENTS + "\n",
                    1,
                )
            ),
        )
        expect_error(
            "回收门 fg- recents 缺席（二击未走多任务键路）",
            lambda: verify_recycle_gate(
                fake_g.replace(MARK_FG_MINUS_RECENTS + "\n", "")
            ),
        )
        expect_error(
            "回收门 fg+ 缺一（某次拉起未进前台态 / 握手行缺席）",
            lambda: verify_recycle_gate(
                fake_g.replace(MARK_FG_PLUS + "\n", "", 1)
            ),
        )
        expect_error(
            "回收门 fg- back 缺席（返回键交接无握手行）",
            lambda: verify_recycle_gate(
                fake_g.replace(MARK_FG_MINUS_BACK + "\n", "")
            ),
        )
        expect_error(
            "回收门 fg- back 重复",
            lambda: verify_recycle_gate(
                fake_g + MARK_FG_MINUS_BACK + "\n"
            ),
        )
        expect_error(
            "回收门清前台态的是 home 不是 back",
            lambda: verify_recycle_gate(
                fake_g.replace(MARK_FG_MINUS_BACK, MARK_FG_MINUS_PREFIX + "home")
            ),
        )
        expect_error(
            "回收门 killed 缺席（主修回退）",
            lambda: verify_recycle_gate(
                fake_g.replace(ECO_GATE_KILLED + "\n", "")
            ),
        )
        expect_error(
            "回收门 no EOF 在席（烧帽回归）",
            lambda: verify_recycle_gate(fake_g.replace(ECO_GATE_KILLED, ECO_GATE_NO_EOF)),
        )
        expect_error(
            "回收门 exit code 在席（分诊面回退）",
            lambda: verify_recycle_gate(
                fake_g + ECO_GATE_EXIT_CODE + "\n"
            ),
        )
        expect_error(
            "回收门行序倒置（killed 先于 recycle）",
            lambda: verify_recycle_gate(
                fake_g.replace(
                    f"{ECO_GATE_RECYCLE}\n{ECO_GATE_KILLED}\n",
                    f"{ECO_GATE_KILLED}\n{ECO_GATE_RECYCLE}\n",
                )
            ),
        )
        expect_error(
            "回收门 it 恰一（二击/三击未达）",
            lambda: verify_recycle_gate(
                f"{MARK_GATE_DEFERRED}\neco: app BROWSER it 03\n"
                f"{ECO_GATE_RECYCLE}\n{ECO_GATE_KILLED}\n"
            ),
        )
        expect_error(
            "回收门 alive 在席（BROWSER 无 wmd 窗不应走聚焦路）",
            lambda: verify_recycle_gate(
                fake_g.replace(ECO_GATE_RECYCLE, ECO_GATE_ALIVE, 1)
            ),
        )
        log(
            "self-test: 回收门 ok（LAYOUT_GATE 65B；recents(520,570)；"
            "正序通过；fg+ 缺一/fg- recents 缺席或重复/fg- back 缺席或重复/"
            "home 冒充/killed 缺席/no EOF/exit code/行序倒置/it 缺席/"
            "alive 在席均 FAIL）"
        )

        # 3d) 回收门握手重试单元（零 QEMU、零线程：伪 QMP 在第 N 次点击
        #     时同步追加串口行；伪 proc 在首次 poll() 时追加串口行——模拟
        #     「RPC 归来」这类点击之后才到的行。确定性复现「首击即过 / 首击
        #     零动作重注 / 只 it 无 fg+ 续等 / 在途超总帽红 / 二击 fg-
        #     recents 先到续等 / 全丢 / alive 先到 / 返回键三形」）。
        class _ScriptedQmp:
            def __init__(self, script):
                self.script = list(script)  # 每项：None=零动作，str=追加到串口
                self.clicks = 0

            def click(self, x, y, w, h):
                i = self.clicks
                self.clicks += 1
                act = self.script[i] if i < len(self.script) else None
                if act:
                    with lp_g.open("a", encoding="utf-8") as f:
                        f.write(act)

        class _LateAppendProc:
            """首次 poll() 把 text 追加到串口文件（wait_until 每轮先读串口
            再查谓词、谓词假才 poll proc——故首段若证据已齐根本不会 poll，
            续等段首轮 poll 即追加、下轮读到）。"""

            def __init__(self, path, text):
                self.path = path
                self.text = text
                self.done = False

            def poll(self):
                if not self.done:
                    self.done = True
                    with self.path.open("a", encoding="utf-8") as f:
                        f.write(self.text)
                return None

        class _FakeGateBoot:
            def __init__(self, script, proc=None):
                self.log = LogWatcher(lp_g)
                self.proc = proc if proc is not None else DummyProc()
                self.qmp = _ScriptedQmp(script)
                self.log_path = lp_g

        lp_g = tmp / "gate-fake.log"
        it_line = "eco: app BROWSER it 03\n"
        spawn_ok = it_line + MARK_FG_PLUS + "\n"
        lp_g.write_text(MARK_GATE_DEFERRED + "\n", encoding="utf-8")
        # 首击即过：一次点击、零重试。
        b = _FakeGateBoot([spawn_ok])
        lat, r = _gate_click_wait_spawn(
            b, 1, 1, 8, 8, "首击", step_timeout=1.0, total_timeout=2.0
        )
        if r != 0 or b.qmp.clicks != 1 or lat < 0:
            raise SmokeError(f"首击即过形异常：retries={r} clicks={b.qmp.clicks}")
        # 首击零动作 → 重注一次即过（共两击、重试 1）。
        b = _FakeGateBoot([None, spawn_ok])
        lat, r = _gate_click_wait_spawn(
            b, 1, 1, 8, 8, "三击", step_timeout=0.4, total_timeout=2.0
        )
        if r != 1 or b.qmp.clicks != 2:
            raise SmokeError(f"重注一次形异常：retries={r} clicks={b.qmp.clicks}")
        # 只来 it 不来 fg+（RPC 在途）→ 判已消费、续等、**零重击**；fg+ 由
        # 伪 proc 在续等段补到 → 零重试过（复审 M1）。
        b = _FakeGateBoot([it_line], proc=_LateAppendProc(lp_g, MARK_FG_PLUS + "\n"))
        lat, r = _gate_click_wait_spawn(
            b, 1, 1, 8, 8, "首击", step_timeout=0.4, total_timeout=2.0
        )
        if r != 0 or b.qmp.clicks != 1:
            raise SmokeError(f"只 it 无 fg+ 形异常：retries={r} clicks={b.qmp.clicks}（应续等不重注）")
        # 在途超总帽：证据已出、fg+ 永不来 → 总帽到 MissingMarkers，仍只一击。
        b = _FakeGateBoot([it_line])
        expect_error(
            "在途超总帽应 MissingMarkers",
            lambda: _gate_click_wait_spawn(
                b, 1, 1, 8, 8, "首击", step_timeout=1.0, total_timeout=0.6
            ),
        )
        if b.qmp.clicks != 1:
            raise SmokeError(f"在途超总帽形点击数 {b.qmp.clicks} != 1（不应重注）")
        # 二击：`fg- recents` 先出（RPC 尚未发）→ 以它为消费证据续等、零
        # 重击；it + fg+ 随后由伪 proc 补到。
        b = _FakeGateBoot(
            [MARK_FG_MINUS_RECENTS + "\n"],
            proc=_LateAppendProc(lp_g, spawn_ok),
        )
        lat, r = _gate_click_wait_spawn(
            b, 1, 1, 8, 8, "二击", consumed_mark=MARK_FG_MINUS_RECENTS,
            step_timeout=0.4, total_timeout=2.0,
        )
        if r != 0 or b.qmp.clicks != 1:
            raise SmokeError(
                f"二击 fg- recents 先到形异常：retries={r} clicks={b.qmp.clicks}（应续等不重注）"
            )
        # 二击：`fg- recents` 出了但 fg+ 永不来 → 超总帽红、仍只一击。
        b = _FakeGateBoot([MARK_FG_MINUS_RECENTS + "\n"])
        expect_error(
            "二击在途超总帽应 MissingMarkers",
            lambda: _gate_click_wait_spawn(
                b, 1, 1, 8, 8, "二击", consumed_mark=MARK_FG_MINUS_RECENTS,
                step_timeout=1.0, total_timeout=0.6,
            ),
        )
        if b.qmp.clicks != 1:
            raise SmokeError(f"二击在途超总帽形点击数 {b.qmp.clicks} != 1（不应重注）")
        # 全丢：零证据重注两次后 MissingMarkers（帽到即红，不假绿）。
        b = _FakeGateBoot([None, None, None, None])
        expect_error(
            "握手全丢应 MissingMarkers",
            lambda: _gate_click_wait_spawn(
                b, 1, 1, 8, 8, "首击", step_timeout=0.4, total_timeout=5.0
            ),
        )
        if b.qmp.clicks != 3:
            raise SmokeError(f"全丢形点击数 {b.qmp.clicks} != 3（1 + 2 重试）")
        # alive 先到（证据段）：立即 SmokeError、不重试。
        b = _FakeGateBoot([ECO_GATE_ALIVE + "\n", spawn_ok])
        expect_error(
            "alive 先到应 SmokeError",
            lambda: _gate_click_wait_spawn(
                b, 1, 1, 8, 8, "二击", fail_fast=ECO_GATE_ALIVE,
                step_timeout=1.0, total_timeout=2.0,
            ),
        )
        if b.qmp.clicks != 1:
            raise SmokeError(f"alive 先到形点击数 {b.qmp.clicks} != 1（不应重试）")
        # alive 在续等段到（it 已出、fg+ 未出）：同样 SmokeError、不重试。
        # alive 判据是对全文的子串查（非相对计数）——前一形已把 alive 行写进
        # 串口文件，这里重置为干净基线，免得证据段就被陈旧行触发。
        lp_g.write_text(MARK_GATE_DEFERRED + "\n", encoding="utf-8")
        b = _FakeGateBoot(
            [it_line], proc=_LateAppendProc(lp_g, ECO_GATE_ALIVE + "\n")
        )
        expect_error(
            "续等段 alive 到应 SmokeError",
            lambda: _gate_click_wait_spawn(
                b, 1, 1, 8, 8, "二击", fail_fast=ECO_GATE_ALIVE,
                step_timeout=1.0, total_timeout=2.0,
            ),
        )
        if b.qmp.clicks != 1:
            raise SmokeError(f"续等段 alive 形点击数 {b.qmp.clicks} != 1（不应重试）")
        # 返回键：一击即过 / 首击零动作重注 / 全丢。
        back_ok = MARK_FG_MINUS_BACK + "\n"
        b = _FakeGateBoot([back_ok])
        r = _gate_click_wait_back(b, 1, 1, 8, 8, step_timeout=1.0)
        if r != 0 or b.qmp.clicks != 1:
            raise SmokeError(f"返回键即过形异常：retries={r} clicks={b.qmp.clicks}")
        b = _FakeGateBoot([None, back_ok])
        r = _gate_click_wait_back(b, 1, 1, 8, 8, step_timeout=0.4)
        if r != 1 or b.qmp.clicks != 2:
            raise SmokeError(f"返回键重注形异常：retries={r} clicks={b.qmp.clicks}")
        b = _FakeGateBoot([None, None, None, None])
        expect_error(
            "返回键全丢应 MissingMarkers",
            lambda: _gate_click_wait_back(b, 1, 1, 8, 8, step_timeout=0.4),
        )
        if b.qmp.clicks != 3:
            raise SmokeError(f"返回键全丢形点击数 {b.qmp.clicks} != 3")
        log(
            "self-test: 回收门握手重试 ok（首击即过/零动作重注一次/只 it 无 fg+ "
            "续等零重击/在途超总帽红不重注/二击 fg- recents 先到续等零重击/"
            "二击在途超总帽红/全丢红/alive 证据段与续等段均即败不重试；"
            "返回键即过/重注/全丢红）"
        )

        # 4) QMP 协议 + no-device 广播 + 单批单击（abs+abs+down+up 同一条
        #    input-send-event——慢点击长按误判刀后的注入形）。
        sock = tmp / "qmp.sock"
        mock = MockQmp(sock).start()
        cli = QmpClient(sock, timeout=5.0)
        cli.click(95, 130, 800, 600)
        cli.quit()
        mock.join()
        if len(mock.batches) != 1:
            raise SmokeError(f"QMP 批数 {len(mock.batches)} != 1（单批同投形）")
        evs = mock.batches[0]
        shape = [(e["type"], e["data"].get("axis") or e["data"].get("down")) for e in evs]
        if shape != [("abs", "x"), ("abs", "y"), ("btn", True), ("btn", False)]:
            raise SmokeError(f"QMP 单批事件序异常：{shape}")
        abs_events = [e for e in evs if e["type"] == "abs"]
        got = {e["data"]["axis"]: e["data"]["value"] for e in abs_events}
        if got.get("x") != 3891 or got.get("y") != 7100:
            raise SmokeError(f"QMP 坐标异常：{got}（期望 x=3891 y=7100）")
        log("self-test: QMP ok（no-device 广播 1 批 abs+abs+down+up，坐标 3891/7100）")

        # 4b) VNC 直通注入臂：RFB 握手 + 字节级三铁律对拍（判例驳斥快照
        #     实验陷阱段：PointerEvent 恰 6B 无 pad / leftover 粘包读 /
        #     SetEncodings 在席 + 零 FramebufferUpdateRequest）。
        mock_rfb = MockRfb().start()
        rfb = RfbClient("127.0.0.1", mock_rfb.port, timeout=5.0)
        if (rfb.fb_w, rfb.fb_h) != (800, 600):
            raise SmokeError(
                f"RFB ServerInit 几何异常：{rfb.fb_w}x{rfb.fb_h} != 800x600"
            )
        rfb.click(95, 95)
        expect_error("RFB 坐标越界", lambda: rfb.pointer(0, 70000, 0))
        rfb.close()
        mock_rfb.join()
        stream = mock_rfb.client_stream
        expect_stream = (
            b"RFB 003.008\n"
            + b"\x01"
            + b"\x01"
            + b"\x02\x00\x00\x01\x00\x00\x00\x00"
            + b"\x05\x00\x00\x5f\x00\x5f"
            + b"\x05\x01\x00\x5f\x00\x5f"
            + b"\x05\x00\x00\x5f\x00\x5f"
        )
        if stream != expect_stream:
            raise SmokeError(
                f"RFB 客户端字节流漂移（{len(stream)}B != "
                f"{len(expect_stream)}B）：{stream!r} != {expect_stream!r}"
            )
        log(
            "self-test: RFB ok（分段粘包握手 800x600；SetEncodings RAW "
            "在席；PointerEvent 恰 6B×3 无 pad；零 FramebufferUpdateRequest）"
        )

        log(
            "SELFTEST PASS（精确哨兵/canonical 109B/精确计数与顺序/"
            "常态隔离/回收门+握手重试/geometry/log/QMP/RFB 直通臂；零 QEMU）"
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
            "  python3 tools/gui8-smoke.py --dry-run --input-path vnc\n"
            "  python3 tools/gui8-smoke.py --image out/berryos.img\n"
            "  python3 tools/gui8-smoke.py --image out/berryos.img --input-path vnc\n"
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
    ap.add_argument("--boot-b-timeout", type=float, default=300.0)
    ap.add_argument("--click-timeout", type=float, default=120.0)
    ap.add_argument(
        "--click-ready-timeout",
        type=float,
        default=60.0,
        help="dual Boot A 等 click ready 的秒数（缺席即 FAIL，无 sleep 回退）",
    )
    ap.add_argument(
        "--input-path",
        choices=("qmp", "vnc"),
        default="qmp",
        help="boot A 单击注入通路：qmp=QMP input-send-event 单批（默认，"
        "CI 零扰动）；vnc=RFB PointerEvent 直通（QEMU 加 -vnc 127.0.0.1:0，"
        "内建最小 RFB 客户端带 SetEncodings——给用户手走的 VNC 人面路立法"
        "测试面，断言面不变；--aged-minutes 与通路正交）",
    )
    ap.add_argument(
        "--aged-minutes",
        type=float,
        default=0.0,
        metavar="N",
        help="aged 注入用例：boot A click ready 门通过后、注入单击前静置 "
        "N 分钟再点（默认 0 关闭——CI 零扰动；boot B 只验隔离态不加）",
    )
    ap.add_argument(
        "--normal-boot",
        action="store_true",
        help="常态隔离：不造 GUI8.SMK，点击后断言 spawn 仍发生且零标记",
    )
    ap.add_argument(
        "--recycle-gate",
        action="store_true",
        help="活体 app 回收时延回归门（2026-11-21 冻结案）：单 boot，"
        "BROWSER 图标首击 + 多任务键二击 + 返回键 + 同图标三击（同 id "
        "回收），断言 it → recycle → killed → it 两轮行序、零 no EOF/"
        "exit code、三击回收时延 ≤ 首击 + headroom（EOF 烧帽回归必爆帽）；"
        "交接全走启动器握手行（fg+ spawned / fg- recents / fg- back），每击"
        "零消费证据 30s 才重注至多两次、证据一出只续等（总帽 --click-timeout）",
    )
    ap.add_argument(
        "--recycle-budget-floor",
        type=float,
        default=20.0,
        metavar="S",
        help="回收门二击时延帽下限秒（默认 %(default)s——快机上首击极短时"
        "仍留余量）",
    )
    ap.add_argument(
        "--recycle-headroom",
        type=float,
        default=40.0,
        metavar="S",
        help="回收门二击时延帽 = 首击时延 + 本值（默认 %(default)s——必须 "
        "< 60s EOF 烧帽，回归才必爆帽；>> 正常毫秒级回收与 TCG 抖动）",
    )
    ap.add_argument("--only-a", action="store_true", help="只跑 Boot A（plumbing）")
    ap.add_argument("--dry-run", action="store_true", help="只建盘并打印 QEMU 命令")
    ap.add_argument("--self-test", action="store_true", help="无 QEMU 自检")
    ap.add_argument("--require-markers", action="store_true", help="launcher 标记未合并即提前 FAIL")
    ap.add_argument("--keep", action="store_true", help="保留 run 目录/日志/数据盘")
    return ap.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    # aged 旗域校验（对抗审查 P2/P3）：非有限值（NaN 参与 >= 比较为
    # False 可穿过 _aged_wait 的 <=0 守卫、sleep(NaN) 直接 traceback）
    # 与负值一并显式拒绝；--normal-boot 腿不读 aged_minutes——组合
    # 显式拒绝防「静默忽略被误读为已 aged」。
    if not (args.aged_minutes >= 0) or args.aged_minutes == float("inf"):
        print(
            "gui8-smoke: FAIL: --aged-minutes 须为非负有限分钟数",
            file=sys.stderr, flush=True)
        return 2
    if args.aged_minutes > 0 and args.normal_boot:
        print(
            "gui8-smoke: FAIL: --aged-minutes 暂不支持 --normal-boot 组合"
            "（该腿无 click ready 门锚点，不加 aged 面）",
            file=sys.stderr, flush=True)
        return 2
    # vnc 直通臂本刀只立 dual boot A 面（任务域）；normal-boot 腿注入保
    # 持 QMP——显式拒绝防「静默忽略被误读为已走 VNC 直通」。
    if args.input_path == "vnc" and args.normal_boot:
        print(
            "gui8-smoke: FAIL: --input-path vnc 暂不支持 --normal-boot 组合"
            "（vnc 臂只立 dual boot A 注入面，该腿注入保持 QMP）",
            file=sys.stderr, flush=True)
        return 2
    if args.input_path == "vnc" and args.recycle_gate:
        print(
            "gui8-smoke: FAIL: --input-path vnc 暂不支持 --recycle-gate 组合"
            "（vnc 臂只立 dual boot A 注入面，该腿注入保持 QMP）",
            file=sys.stderr, flush=True)
        return 2
    try:
        if args.self_test:
            return self_test(args)
        if args.dry_run:
            return dry_run(args)
        if args.normal_boot:
            return run_normal_boot(args)
        # 回收门分支原在下面无条件 return run_dual_boot 之后——自加入起
        # 就是死码，导致 just gui8-recycle-gate 一直跑的是双 boot；挪到
        # 无条件返回之前才可达。
        if args.recycle_gate:
            return run_recycle_gate(args)
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
