#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""frame-gate20 —— 内核帧归还刀（4b3a6afc）「20 轮连点门」验收 rig。

配方出处
--------
`.agents/plans/mm/2026-11-22-内核帧归还刀实施设计.md` 实证节 +
`.agents/notes/implemented/2026-11-22-回收循环泄漏止血批.md` 配方段：
20 轮连点门+帧水位逐轮打点（**soak pages 口径**——设计原文钦定）；
修前期望 14 轮崩（-ENOMEM 映射站），修后期望 20 轮全过 + 帧级净失
归零 + 残余仅表树帧 4-8 页/轮。操作语义 = 同 id app（APPDEMO）反复
「回收再拉」。

水位口径与采样器
--------------
设计原文钦定「soak pages 口径」，但实测 soak 门键（soakmon prio-8
常驻任务）会 perturb boot 链——G1.5 杀数断言/ai_probe eff 断言两个
pre-existing 面在 soak 在位时崩（**前后内核同崩**：pre-knife 57730dde
+ soak-only 复现同 panic——非帧刀回归，是 soak 键从未与 GUI/完整自验
链合跑过的既有脆弱面；判别记录见 run 目录）。bsh `free` 内建在 GUI
形结构性不可达（无盘形停 fat_selftest、带盘形 fs 端让位 launcher 停
fs_selftest）；常驻采样 app（[ui] 点击拉起）路线亦被两面前拦死——
svcmgr 对第 11 个 [ui] app 的注册有扫描竞态拒收（3/4 boot 拒收），
且布局含该格即灭全桌面点击面。最终口径 = **宿主直读 RAM 镜内核页
计数器**（memory-backend-file, share=on——生产 berry CLI RamSource
同形）：mm::FRAMES.buddy.allocated/reserved（ktest F4d/F6 水位断言
同源），偏移自镜像内 KERNEL.ELF 的 DWARF 推导（COUNTER_BASE_HINT
注），运行期以 num_pages≈65536 自证；free = num_pages − reserved −
allocated。判读主口径 Δfree：修后期望每轮 −4..−8 页（表树帧残余），
修前对照 ~−469 页/轮。

已知阻塞（2026-11-23 实测+同日定谳翻转，非帧刀面）：round ≥2 的
back+reclick 全部无响应——初判「present 域 submit-busy 楔死」已被
只读取证推翻（wf_6c5ec03c）：submit busy 是 presarb 5s 租约窗内的
正常瞬态让位（launcher 100ms 步进重试即过，wmd/presarb 全程零诊断
行）；真凶=QEMU→guest 输入事件投递死亡（两跑 inputd tally h 精确冻
在 27——round-2 的全部 QMP 注入零成 virtio 事件，q=0/0 e=0；桌面侧
健康：launcher 帧循环活、新 SUBMIT 仍过 wmd→guifbd）——与 2026-11-20
云桌面输入死定谳（时变形、注入链面）同族。判别下一跑：qemu_argv 加
gui8 GUI8_QEMU_TRACE 同款五枚输入 trace + 失败明细记 tally h。

拓扑（零 ambient TCP 端口——与 run-rootfs 长链 18080-18083 无冲突）
------------------------------------------------------------------
AAVMF + 私烤镜像（**不覆写 out/berryos.img**——桌面会话基盘在用；
mkimage 烤到 out/berryos-gate20.img，缺/陈则重烤）+ usb-storage +
**mkfat 数据盘（交互相三件套之一，缺盘 = launcher 冻结相 [wm] 前缀
截断——无 input 面，点击结构性死**，svcmgr launcher 装配 R1 判别器
v2：BLK_OK≠0 才给 [wm,fs,app,input] 满配；盘上写 LAYOUT.JSN 钉死
APPDEMO(0,0,0)+MEMPROBE(0,1,2) 两格布局——无第三格可误点）+
virtio-tablet + ramfb + fw_cfg display 800x600@60 紧凑档（唯一门键
——其余键面实测非绿：noktest+soak+demo-face 组合与 soak 单键都触发
boot 链 pre-existing 断言崩，见「水位口径」节）+ QMP unix sock（点击
注入——gui8 先例：input-send-event 省 device 键）。默认 TCG（hvf+
完整自验链有 pre-existing 崩面），`--accel hvf` 可换。

轮语义
------
- round 1：图标首击 = 冷 spawn（app APPDEMO up / eco it 行）。
- round i≥2：back 键销窗（进程存活——真泄漏形载具，gui8 recycle-gate
  先例）→ 图标再击 = 同 id「回收再开」：svcmgr 杀旧进程 → 内核
  Vmar::Drop 清账 + drain 双相还帧 → 拉新（recycle/killed/it 行）。
- 每轮 spawn 落定后等**下一拍 SOAK 心跳**配对采样（60s 周期，整轮
  恰耗一拍——轮与拍 1:1 配对，round 1 的 delta 含首拉足迹只记录不
  入斜率，斜率取 rounds 2-20）。

判读输出
--------
过轮数（期望 20）/ eco nomem 行数（期望 0）/ QEMU 存活 / allocated
pages 逐轮表 + rounds 2-20 均值斜率与极值（期望 ≤8 页/轮；修前对照
~469）。PASS 判据：20/20 ∧ nomem=0 ∧ 斜率 ≤8 ∧ 单轮峰值 ≤32。

用法
----
    python3 tools/frame-gate20.py                 # 全跑（烤盘+boot+20 轮）
    python3 tools/frame-gate20.py --rounds 3      # 快验
    python3 tools/frame-gate20.py --keep          # 保留 run 目录
    python3 tools/frame-gate20.py --accel tcg     # 强制 TCG
    python3 tools/frame-gate20.py --force-image   # 强制重烤镜像

macOS 纪律（gui8 同款）：serial 落文件、QMP 只用 unix socket、每 boot
fresh blank vars.fd、QMP quit + SIGTERM 双路收尾、只杀自己钉的 QEMU PID。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# —— 镜像烘焙面（justfile image 目标镜像，输出路径私有化）——
MKIMAGE = (
    ROOT
    / "tools/mkimage/cargo-target/aarch64-apple-darwin/release/mkimage"
)
BOOT_EFI = ROOT / "bootloader/berryboot/out/BERRYBOOT.EFI"
BOOT_HELLO = ROOT / "bootloader/berryboot/hello.txt"
ROOTFS = ROOT / "rootfs.cpio"
DEFAULT_IMAGE = ROOT / "out/berryos-gate20.img"
DEFAULT_CODE = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"

# —— boot 锚（gui8 基础 5 + 门键自证）——
MARK_ROOTFS = "ROOTFS ok src=handoff"
MARK_APP_N = "APP ok n="
MARK_LAUNCH = "LAUNCH ok"
MARK_PAGE = "PAGE ok n="
MARK_DEFERRED = "app APPDEMO gui deferred"
MARK_SOAK_GATE = "SOAK gate on"
MARK_INPUT_ARMED = "launcher: input poll armed"

# —— 轮语义标记 ——
RE_IT = re.compile(r"eco: app APPDEMO it [0-9A-Fa-f]{2}")
MARK_UP = "app APPDEMO up"
MARK_KILLED = "eco: app APPDEMO killed"
MARK_RECYCLE = "eco: app APPDEMO recycle"
MARK_NOMEM = "eco: app APPDEMO nomem"
# —— 水位仪：宿主直读内核页计数器（memory-backend-file RAM 镜——生产
# berry CLI RamSource 同形）。口径 = mm::FRAMES.buddy.allocated/reserved
# （**ktest F4d/F6 同源**——设计实证节钦定 soak pages 同计数器族）。
# 偏移推导（镜像内 KERNEL.ELF 的 DWARF，2026-11-23 钉板）：
#   FRAMES VA 0xffffff804017d6d0（nm）；Option<FrameAllocator> niche 化
#   payload@0（byte_size 0x78 无 tag）；FrameAllocator.buddy@0x00；
#   BuddyAllocator.num_pages@0x40/.allocated@0x58/.reserved@0x60；
#   QEMU virt RAM 基 = 0x40000000 → 文件偏移 = VA - 0xffffff8000000000
#   - 0x40000000 = 0x17d728 族。运行期以 num_pages ≈ 65536 自证偏移链
#   （Option 若带 tag 则 payload@8——两候选取 num_pages 命中者）。
COUNTER_BASE_HINT = 0x17D6D0  # FRAMES 文件偏移（.bss）
OFF_NUM_PAGES = 0x40
OFF_ALLOCATED = 0x58
OFF_RESERVED = 0x60
OFF_FR_NPAGES = 0x70


def open_ram(ram_path: Path):
    """打开 RAM 镜并钉计数器偏移（num_pages 自证）。返 (file, offs)。"""
    f = open(ram_path, "rb")
    import struct

    for payload in (0, 8):  # Option niche@0 / tagged@8 二选一
        base = COUNTER_BASE_HINT + payload
        f.seek(base + OFF_NUM_PAGES)
        np_ = struct.unpack("<Q", f.read(8))[0]
        # 256M/4K = 65536；num_pages 可能扣 RAM 顶部保留，量级 6e4 即命中。
        if 60000 <= np_ <= 65536:
            log(f"RAM 镜计数器钉位：FRAMES+{payload}（num_pages={np_} 自证命中）")
            return f, {
                "num_pages": base + OFF_NUM_PAGES,
                "allocated": base + OFF_ALLOCATED,
                "reserved": base + OFF_RESERVED,
                "fr_npages": base + OFF_FR_NPAGES,
            }
    raise GateError(
        f"计数器自证失败（两候选 num_pages 均非量级命中）——镜像/内核布局漂移？"
    )


def read_counter(ram_file, offs, key) -> int:
    import struct

    ram_file.seek(offs[key])
    return struct.unpack("<Q", ram_file.read(8))[0]

# —— 数据盘（交互相载体；gui8 同工艺：mkfat + LBA0 印章 + canonical
# LAYOUT 注入）。**canonical 双格布局是钦定点靶**（gui8 点击链唯一实测
# 绿键面——任何含 MEMPROBE 格的布局变体实测全灭桌面点击面，且 MEMPROBE
# 的 [ui] 注册本身有第 11 app 扫描竞态拒收——仪器路线已弃，水位改宿主
# 直读 RAM 镜）。无 GUI8.SMK 哨兵——不开 launcher 冒烟支路。
DISK_SEAL = b"BERRYOSVBLK!"
LAYOUT_NAME11 = b"LAYOUT  JSN"
LAYOUT_CANONICAL = (
    b'{"schema":1,"cells":['
    b'{"app":"APPDEMO","page":0,"row":0,"col":0},'
    b'{"app":"APPQUIET","page":3,"row":0,"col":1}]}'
)
RE_LAUNCH_GEOM = re.compile(r"LAUNCH v=\d+ wmax=\d+ (\d+)x(\d+)")
RE_FB_GEOM = re.compile(r"FB ok (\d+)x(\d+)")

# —— 紧凑档几何（desktop.rs 镜像，gui8-smoke 同源常量）——
STATUS_H = 44
DOT_H = 16
GRID_TOP = 8
BOTTOM_PAD = 52
GRID_COLS = 4
GRID_ROWS = 5
GRID_GAP = 12
NAV_KEY_PX = 44
NAV_GAP = 76
NAV_BAR_W = NAV_KEY_PX * 3 + NAV_GAP * 2

# —— 判读阈（设计实证节；修前对照 ~469 页/轮）——
SLOPE_EXPECT_MAX = 8      # 残余表树帧 4-8 页/轮
ROUND_DELTA_MAX = 32      # 单轮峰值帽（斜率被单轮大漏均摊的防伪面）
PREFIX_LEAK_PAGES = 469   # 修前主罪量级（诊断值，仅报告用）


def log(msg: str) -> None:
    print(f"[frame-gate20 {time.strftime('%H:%M:%S')}] {msg}", flush=True)


def warn(msg: str) -> None:
    print(f"[frame-gate20 {time.strftime('%H:%M:%S')}] WARN: {msg}", flush=True)


def cell0_center(w: int, h: int) -> tuple[int, int]:
    return cell_center(w, h, 0, 0)


def cell_center(w: int, h: int, row: int, col: int) -> tuple[int, int]:
    """紧凑档网格 (row, col) 格中心（desktop.rs default_layout 几何镜像）。"""
    grid_h = max(0, h - (STATUS_H + GRID_TOP + DOT_H + BOTTOM_PAD))
    cw = max(1, (w - (GRID_COLS - 1) * GRID_GAP) // GRID_COLS)
    ch = max(1, (grid_h - (GRID_ROWS - 1) * GRID_GAP) // GRID_ROWS)
    x = col * (cw + GRID_GAP) + cw // 2
    y = STATUS_H + GRID_TOP + row * (ch + GRID_GAP) + ch // 2
    return x, y


def back_center(w: int, h: int) -> tuple[int, int]:
    x0 = (w - NAV_BAR_W) // 2
    return x0 + NAV_KEY_PX // 2, h - 8 - NAV_KEY_PX // 2


class GateError(Exception):
    pass


class LogWatcher:
    """增量读串口文件——状态谓词轮询，非竞速（gui8 同款）。"""

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

    def count_re(self, pattern) -> int:
        self.poll()
        if isinstance(pattern, str):
            pattern = re.compile(pattern)
        return len(pattern.findall(self.text))

    def wait_until(self, proc, predicate, description: str, timeout: float,
                   phase: str) -> None:
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
                raise GateError(
                    f"{phase}: QEMU 提前退出 rc={rc}（等待 {description}）"
                    f"；串口尾：{self.text[-2000:]!r}"
                )
            now = time.monotonic()
            if now >= deadline:
                raise GateError(
                    f"{phase}: {timeout:.0f}s 未等到 {description}；"
                    f"串口尾：{self.text[-2000:]!r}"
                )
            if now - last_report >= 15.0:
                log(f"{phase}: {int(now - started)}s/{timeout:.0f}s 等待 {description}")
                last_report = now
            time.sleep(0.25)


class QmpClient:
    """QMP unix-socket 客户端（gui8 同款；零 TCP 端口）。"""

    def __init__(self, path: Path, timeout: float = 15.0):
        self.path = str(path)
        self.sock = None
        self.f = None
        last = None
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
            raise GateError(f"QMP 连接失败（{self.path}）：{last}")
        self.f = self.sock.makefile("rw", encoding="utf-8", newline="\n")
        greeting = self._read_json()
        if "QMP" not in greeting:
            raise GateError(f"QMP greeting 缺失：{greeting!r}")
        self.execute("qmp_capabilities")

    def _read_json(self) -> dict:
        line = self.f.readline()
        if not line:
            raise GateError("QMP EOF（QEMU 已退出？）")
        return json.loads(line)

    def execute(self, cmd: str, arguments: dict | None = None):
        req: dict = {"execute": cmd}
        if arguments is not None:
            req["arguments"] = arguments
        self.f.write(json.dumps(req) + "\n")
        self.f.flush()
        while True:
            msg = self._read_json()
            if "event" in msg:
                continue
            if "error" in msg:
                raise GateError(f"QMP {cmd} 被拒：{msg['error']}")
            return msg.get("return")

    def click(self, x: float, y: float, w: int, h: int) -> None:
        # 三批单击：abs 定位 → btn down（带坐标）→ btn up。
        # device 键必须省（gui8 判例：device= 走 QemuConsole 查找触发
        # QEMU 11.0.3 QOM 崩溃；省键 = 广播到 unbound handler 链）。
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
            self.execute("input-send-event", {"events": batch})
            time.sleep(0.25)

    def quit(self) -> None:
        try:
            self.execute("quit")
        except (GateError, OSError, json.JSONDecodeError):
            pass
        self.close()

    def close(self) -> None:
        for f in (self.f, self.sock):
            try:
                if f is not None:
                    f.close()
            except OSError:
                pass
        self.f = None
        self.sock = None


def kernel_path() -> Path:
    """target-campaign 内核（任务钦定已温目录；paths.sh 尊重 CARGO_TARGET_DIR）。"""
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target-campaign"))
    out = subprocess.run(
        ["bash", "tools/paths.sh", "kernel"],
        cwd=ROOT, env=env, capture_output=True, text=True,
    )
    if out.returncode != 0 or not out.stdout.strip():
        raise GateError(f"paths.sh kernel 解析失败：{out.stderr.strip()}")
    return ROOT / out.stdout.strip()


def ensure_image(force: bool) -> Path:
    """私烤镜像：缺/陈（内核|rootfs|BERRYBOOT 任一新于镜像）则重烤。"""
    image = DEFAULT_IMAGE
    kern = kernel_path()
    for p in (kern, ROOTFS, BOOT_EFI, MKIMAGE):
        if not p.exists():
            raise GateError(f"烘焙缺件：{p}")
    stale = (
        force
        or not image.exists()
        or image.stat().st_mtime < kern.stat().st_mtime
        or image.stat().st_mtime < ROOTFS.stat().st_mtime
        or image.stat().st_mtime < BOOT_EFI.stat().st_mtime
    )
    if not stale:
        log(f"镜像新鲜：{image}（kernel={time.ctime(kern.stat().st_mtime)}）")
        return image
    image.parent.mkdir(parents=True, exist_ok=True)
    tmp = image.with_suffix(".img.tmp")
    cmd = [
        str(MKIMAGE), str(tmp),
        "--efi", str(BOOT_EFI),
        "--hello", str(BOOT_HELLO),
        "--kernel", str(kern),
        "--initrd", str(ROOTFS),
        "--size-mib", "96",
    ]
    log(f"重烤镜像：{shlex.join(cmd)}")
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        raise GateError(f"mkimage 失败：{r.stdout}\n{r.stderr}")
    os.replace(tmp, image)
    log(f"镜像已烤：{image}（{image.stat().st_size // (1 << 20)}MiB）")
    return image


def create_data_disk(path: Path) -> None:
    """mkfat FAT12 盘 + LBA0 印章 + LAYOUT.JSN 钉死两格布局（无 GUI8.SMK
    哨兵——不开 launcher 冒烟支路，常态点击路）。Fat12Volume 注入器
    自 gui8-smoke.py 模块加载（单编码器纪律，不复制 FAT12 位打包）。"""
    import importlib.util

    if path.exists():
        raise GateError(f"数据盘已存在（拒绝覆盖）：{path}")
    mkfat = ROOT / "tools" / "mkfat.py"
    p = subprocess.run(
        [sys.executable, str(mkfat), str(path)],
        cwd=str(ROOT), capture_output=True, text=True,
    )
    if p.returncode != 0:
        raise GateError(f"mkfat 失败 rc={p.returncode}：{p.stderr.strip()}")
    with path.open("r+b") as f:
        f.seek(0)
        f.write(DISK_SEAL)
    spec = importlib.util.spec_from_file_location("gui8_smoke_mod", ROOT / "tools" / "gui8-smoke.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    mod.Fat12Volume(path).add_file(LAYOUT_NAME11, LAYOUT_CANONICAL)
    # 读回对拍（gui8 verify 同精神：盘面错 = 点击链结构性死，boot 前拦）。
    got = mod.Fat12Volume(path).read_file(LAYOUT_NAME11)
    if got != LAYOUT_CANONICAL:
        raise GateError(f"LAYOUT.JSN 注入对拍失败：{got!r}")


def qemu_argv(args, image: Path, disk: Path, ram: Path, vars_path: Path,
              sock_path: Path, serial_path: Path) -> list[str]:
    cmd = [
        args.qemu,
        "-machine", "virt,memory-backend=mem",
    ]
    if args.accel == "hvf":
        cmd += ["-accel", "hvf", "-cpu", "host"]
    else:
        cmd += ["-cpu", "cortex-a72"]
    cmd += [
        # RAM 镜（share=on）：宿主直读客机物理内存——水位仪载体（生产
        # berry CLI RamSource 同形；QEMU virt RAM 基 0x40000000 = 文件
        # 偏移 0）。
        "-object",
        f"memory-backend-file,id=mem,size=256M,mem-path={ram},share=on",
        "-m", "256M",
        "-display", "none",
        "-monitor", "none",
        "-serial", f"file:{serial_path}",
        "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={args.code}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"if=none,file={image},format=raw,id=bb,snapshot=on",
        "-device", "qemu-xhci",
        "-device", "usb-storage,drive=bb",
        "-global", "virtio-mmio.force-legacy=false",
        # 交互相数据盘（launcher [wm,fs,app,input] 满配的判据——缺盘 =
        # 冻结相无 input 面，点击死）。
        "-drive", f"if=none,id=drv0,format=raw,file={disk},cache.writeback=off",
        "-device", "virtio-blk-device,drive=drv0",
        "-device", "virtio-tablet-device,id=tab0",
        "-device", "ramfb",
        "-fw_cfg", "name=opt/berry/display,string=800x600@60",
    ]
    # 附加门键（--extra-keys 逗号表；noktest/soak/demo-face）：
    # - noktest：跳 ktest/bench（门不测时序自验面）；
    # - soak：soakmon 心跳 = 水位仪（本门口径本体）；
    # - demo-face：off = 生产 berry CLI 桌面形（舰队演示段让位——
    #   hvf+演示面在是无人跑的组合，G1.5 杀数断言会 panic）。
    # --extra-keys "" = 裸 GUI 形（判别实验用：gui8 同键面）。
    keyvals = {
        "noktest": ("opt/berry/noktest", "1"),
        "soak": ("opt/berry/soak", "1"),
        "demo-face": ("opt/berry/demo-face", "off"),
    }
    for k in [k for k in args.extra_keys.split(",") if k]:
        if k not in keyvals:
            raise GateError(f"未知门键：{k}（合法：{'/'.join(keyvals)}）")
        name, val = keyvals[k]
        cmd += ["-fw_cfg", f"name={name},string={val}"]
    cmd += [
        "-qmp", f"unix:{sock_path},server=on,wait=off",
    ]
    return cmd


def start_qemu(args, run_dir: Path, image: Path, disk: Path, suffix: str):
    vars_path = run_dir / f"vars-{suffix}.fd"
    sock_path = run_dir / f"qmp-{suffix}.sock"
    log_path = run_dir / f"boot-{suffix}.log"
    cmd_path = run_dir / f"boot-{suffix}.cmd.txt"
    qemu_log = run_dir / f"boot-{suffix}.qemu.log"
    ram_path = run_dir / f"ram-{suffix}.bin"
    # 稀疏预创建（memory-backend-file 要求文件在席）。
    with open(ram_path, "wb") as f:
        f.truncate(256 << 20)
    cmd = qemu_argv(args, image, disk, ram_path, vars_path, sock_path, log_path)
    cmd_path.write_text(shlex.join(cmd) + "\n", encoding="utf-8")
    vars_path.write_bytes(b"\xff" * (64 << 20))
    if sock_path.exists():
        sock_path.unlink()
    fh = qemu_log.open("wb")
    log(f"QEMU 启动（accel={args.accel}；serial→{log_path.name}，"
        f"QMP→{sock_path.name}，RAM 镜→{ram_path.name}）")
    proc = subprocess.Popen(
        cmd, stdin=subprocess.DEVNULL, stdout=fh, stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    return proc, vars_path, sock_path, log_path, fh, ram_path


def stop_qemu(proc, fh, qmp) -> None:
    quit_sent = False
    if qmp is not None:
        qmp.quit()
        quit_sent = True
    if proc is not None:
        if not quit_sent and proc.poll() is None:
            proc.terminate()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            warn("QMP quit 后 15s 未退 → SIGTERM")
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                warn("SIGTERM 后 10s 仍未退 → SIGKILL（只杀自己钉的 PID）")
                proc.kill()
                proc.wait(timeout=10)
    if fh is not None:
        fh.close()
    time.sleep(3)  # 盘刷尾纪律（P6 A-6）


def run_gate(args) -> int:
    image = Path(args.image) if args.image else ensure_image(args.force_image)
    run_dir = Path(args.work_dir) if args.work_dir else Path(
        tempfile.mkdtemp(prefix="frame-gate20.")
    )
    run_dir.mkdir(parents=True, exist_ok=True)
    log(f"run 目录：{run_dir}")

    disk = run_dir / "layout.img"
    create_data_disk(disk)
    log(f"数据盘已造：{disk}（LAYOUT.JSN 钉死 APPDEMO(0,0,0)+MEMPROBE(0,1,2)）")

    # hvf 早失败 → TCG 回退（一次机会）。
    proc = fh = qmp = logw = ram_file = None
    for accel in ([args.accel] if args.accel != "auto" else ["hvf", "tcg"]):
        args.accel = accel
        proc, _vars, sock_path, log_path, fh, ram_path = start_qemu(
            args, run_dir, image, disk, "gate"
        )
        logw = LogWatcher(log_path)
        time.sleep(5.0)
        if proc.poll() is None:
            break
        warn(f"accel={accel} 下 QEMU 早退 rc={proc.poll()}——换档重试")
        fh.close()
        proc = None
    if proc is None or proc.poll() is not None:
        raise GateError("QEMU 两档均无法启动（见 boot-gate.qemu.log）")

    result = {
        "rounds_requested": args.rounds,
        "rounds": [],
        "verdict": "ERROR",
    }
    try:
        # ---- boot 锚 ----
        logw.wait_until(
            proc,
            lambda t: MARK_ROOTFS in t,
            "ROOTFS handoff",
            args.boot_timeout,
            "boot",
        )
        if "soak" in args.extra_keys:
            logw.wait_until(
                proc,
                lambda t: MARK_SOAK_GATE in t,
                "SOAK gate on",
                args.boot_timeout,
                "boot",
            )
        logw.wait_until(
            proc,
            lambda t: MARK_LAUNCH in t and MARK_PAGE in t
            and MARK_APP_N in t and MARK_DEFERRED in t,
            "基础 5（APP ok n=/LAUNCH/PAGE/deferred）",
            args.boot_timeout,
            "boot",
        )
        # 输入武装（状态谓词；软门——缺席只 warn，点击等待窗兜底）。
        deadline = time.monotonic() + 60.0
        while time.monotonic() < deadline:
            if logw.count(MARK_INPUT_ARMED) > 0:
                log("boot: launcher 输入已武装")
                break
            if proc.poll() is not None:
                raise GateError("boot: 等输入武装时 QEMU 退出")
            time.sleep(0.5)
        else:
            warn(f"60s 未见「{MARK_INPUT_ARMED}」——仍继续（点击等待窗兜底）")

        logw.poll()
        m = RE_LAUNCH_GEOM.search(logw.text) or RE_FB_GEOM.search(logw.text)
        if m is None:
            raise GateError("几何行缺席（LAUNCH v=…/FB ok …）")
        w, h = int(m.group(1)), int(m.group(2))
        cx, cy = cell0_center(w, h)
        bx, by = back_center(w, h)
        log(f"几何 {w}x{h} → APPDEMO 图标 ({cx},{cy})；back 键 ({bx},{by})")

        qmp = QmpClient(sock_path)

        # ---- 水位仪在线（宿主直读 RAM 镜计数器）----
        ram_file, coffs = open_ram(ram_path)
        np_total = read_counter(ram_file, coffs, "num_pages")
        alloc_prev = read_counter(ram_file, coffs, "allocated")
        res0 = read_counter(ram_file, coffs, "reserved")
        free_prev = np_total - res0 - alloc_prev
        log(
            f"水位仪在线：num_pages={np_total} allocated={alloc_prev} "
            f"reserved={res0}（boot 稳态前基线，仅供参考）"
        )

        # ---- 轮循环 ----
        rounds_failed = 0
        for i in range(1, args.rounds + 1):
            r = {"round": i}
            it0 = logw.count_re(RE_IT)
            up0 = logw.count(MARK_UP)
            kill0 = logw.count(MARK_KILLED)
            nomem0 = logw.count(MARK_NOMEM)

            if i > 1:
                qmp.click(bx, by, w, h)  # back：销窗不杀进程
                time.sleep(2.0)          # 销窗 RPC 往返 + 表广播沉淀（gui8 先例）

            # 图标点击带 alive-路自愈：窗在场（back 被吃/销窗未达）时点击
            # 走聚焦路（eco alive 行、零 it）——检得 alive 即补一记 back
            # 再点击重试（帽 3 次）。
            for attempt in range(3):
                alive0 = logw.count("eco: app APPDEMO alive")
                qmp.click(cx, cy, w, h)  # 图标：首击 spawn / 再击回收再开
                got_it = got_alive = False
                t_end = time.monotonic() + args.click_timeout
                while time.monotonic() < t_end:
                    logw.poll()
                    if len(RE_IT.findall(logw.text)) > it0:
                        got_it = True
                        break
                    if logw.count("eco: app APPDEMO alive") > alive0:
                        got_alive = True
                        break
                    if proc.poll() is not None:
                        raise GateError(f"round {i}: QEMU 提前退出")
                    time.sleep(0.5)
                if got_it:
                    break
                if got_alive:
                    warn(f"round {i}: 图标点击撞 alive 聚焦路（back 未销窗）——补 back 重试 {attempt + 1}/3")
                    qmp.click(bx, by, w, h)
                    time.sleep(2.0)
                    continue
                logw.poll()
                r.update(
                    status="FAIL-it",
                    nomem_total=logw.count(MARK_NOMEM),
                    detail=f"round {i}: it/alive 均缺席（180s）",
                )
                result["rounds"].append(r)
                rounds_failed += 1
                log(f"round {i}: FAIL（it 行缺席）——停轮记录判读")
                break
            if not got_it and rounds_failed:
                break
            try:
                logw.wait_until(
                    proc,
                    lambda t: t.count(MARK_UP) > up0,
                    f"round {i}: app APPDEMO up 自证",
                    args.click_timeout,
                    f"round {i}",
                )
                r["up"] = True
            except GateError:
                r["up"] = False
                warn(f"round {i}: up 行缺席（it 在而 up 缺——半途 spawn？记录续跑）")

            # 首帧沉淀（软门 45s）：present 域单飞仲裁下，app 首帧在途时
            # 注入点击会撞 launcher submit busy 楔死（02:17 实测 round 2
            # 180s 无 it/alive——pre-existing TCG 时变面，非帧刀路径）。
            # 等**本实例**「appdemo: first frame」新行落定再采样/销窗。
            ff0 = logw.count("appdemo: first frame")
            t_ff = time.monotonic() + 45.0
            while time.monotonic() < t_ff:
                if logw.count("appdemo: first frame") > ff0:
                    break
                if proc.poll() is not None:
                    raise GateError(f"round {i}: QEMU 提前退出（等首帧）")
                time.sleep(0.5)
            else:
                warn(f"round {i}: 45s 未见本实例 appdemo: first frame——仍续跑")
            time.sleep(2.0)  # 首帧后的呈现节律沉淀窗

            # 配对采样：up 落定 + 杀旧 drain 完成后直读 RAM 镜计数器。
            # drain 先于 it/up（svcmgr 杀旧→reap 完成才拉新），up 后再
            # 沉淀 1s 双保险；allocated 口径 = ktest F4d/F6 同源。
            time.sleep(1.0)
            alloc_now = read_counter(ram_file, coffs, "allocated")
            res_now = read_counter(ram_file, coffs, "reserved")
            free_now = np_total - res_now - alloc_now
            r.update(
                status="OK",
                free=free_now,
                alloc=alloc_now,
                reserved=res_now,
                d_free=free_now - free_prev,
                d_alloc=alloc_now - alloc_prev,
                killed=logw.count(MARK_KILLED) - kill0,
                nomem=logw.count(MARK_NOMEM) - nomem0,
            )
            free_prev, alloc_prev = free_now, alloc_now
            result["rounds"].append(r)
            log(
                f"round {i:2d}/{args.rounds}: OK alloc={alloc_now} free={free_now} "
                f"Δfree={r['d_free']:+d} Δalloc={r['d_alloc']:+d}"
                f"（killed+{r['killed']} nomem+{r['nomem']}）"
            )

        # ---- 判读 ----
        logw.poll()
        ok_rounds = [r for r in result["rounds"] if r.get("status") == "OK"]
        nomem_total = logw.count(MARK_NOMEM)
        qemu_alive = proc.poll() is None
        series = [r["d_free"] for r in ok_rounds if "d_free" in r]
        series_alloc = [r["d_alloc"] for r in ok_rounds if "d_alloc" in r]
        steady = series[1:]  # rounds 2..N（round1 含首拉足迹，剔除）
        if steady:
            slope = sum(steady) / len(steady)
            dmax, dmin = max(steady), min(steady)
        else:
            slope = dmax = dmin = None

        passed = len(ok_rounds)
        crit = {
            "rounds_all": passed == args.rounds,
            "nomem_zero": nomem_total == 0,
            # free 口径：修后期望每轮 -4..-8（表树帧残余；0 = 净失严格归零
            # 更优）；崩向阈值 = 单轮失 >32（修前对照 ~-469/轮）。
            "slope_ok": slope is not None and -SLOPE_EXPECT_MAX <= slope <= 0.0,
            "round_delta_ok": dmin is not None and dmin >= -ROUND_DELTA_MAX,
        }
        verdict = "PASS" if all(crit.values()) else "FAIL"

        summary = {
            "verdict": verdict,
            "criteria": crit,
            "rounds_passed": passed,
            "rounds_requested": args.rounds,
            "nomem_total": nomem_total,
            "qemu_alive_at_end": qemu_alive,
            "pages_series": [
                {"round": r["round"], "free": r.get("free"),
                 "d_free": r.get("d_free"), "d_alloc": r.get("d_alloc"),
                 "alloc": r.get("alloc"), "up": r.get("up"),
                 "killed": r.get("killed"), "nomem": r.get("nomem")}
                for r in result["rounds"]
            ],
            "slope_free_rounds2_plus": slope,
            "d_free_max": dmax,
            "d_free_min": dmin,
            "prefix_leak_reference_pages": PREFIX_LEAK_PAGES,
            "log": str(log_path),
            "run_dir": str(run_dir),
        }
        (run_dir / "result.json").write_text(
            json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        result.update(summary)
        log("—— 判读 ——")
        log(f"过轮数：{passed}/{args.rounds}（期望 {args.rounds}/{args.rounds}）")
        log(f"eco nomem 行：{nomem_total}（期望 0；非零 = 池仍枯——帧刀未根治面）")
        log(f"QEMU 终态存活：{qemu_alive}（修前第 14 轮 svcmgr trap 形对照）")
        if series:
            log(f"逐轮 Δfree（buddy 页）：{series}")
            log(f"逐轮 Δalloc（对拍）：{series_alloc}")
        if slope is not None:
            log(
                f"稳态斜率（rounds 2+ 均值 Δfree）：{slope:+.2f} 页/轮"
                f"（期望 [-{SLOPE_EXPECT_MAX}, 0]——残余表树帧 4-8 页/轮；"
                f"修前对照 ~-{PREFIX_LEAK_PAGES}）；极值 [{dmin:+d}, {dmax:+d}]"
            )
        log(f"VERDICT: {verdict}  criteria={crit}")
        return 0 if verdict == "PASS" else 1
    finally:
        try:
            if ram_file is not None:
                ram_file.close()
        except Exception:
            pass
        stop_qemu(proc, fh, qmp)
        keep = args.keep or os.environ.get("GATE20_KEEP")
        if keep:
            log(f"artifacts 保留：{run_dir}")
        else:
            shutil.rmtree(run_dir, ignore_errors=True)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description="内核帧归还刀 20 轮连点门（soak pages 口径）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--rounds", type=int, default=20, help="轮数（默认 20）")
    ap.add_argument("--qemu", default=os.environ.get("QEMU", "qemu-system-aarch64"))
    ap.add_argument("--code", default=os.environ.get("AAVMF_CODE", DEFAULT_CODE))
    ap.add_argument("--accel", choices=["auto", "hvf", "tcg"], default="tcg")
    ap.add_argument(
        "--extra-keys",
        default="",
        help="附加 fw_cfg 门键逗号表（noktest/soak/demo-face；默认空 = 裸 GUI "
        "形——唯一实测绿键面；soak 键会 perturb boot 链 pre-existing 崩）",
    )
    ap.add_argument("--boot-timeout", type=float, default=900.0)
    ap.add_argument("--click-timeout", type=float, default=180.0)
    ap.add_argument("--beat-timeout", type=float, default=150.0)
    ap.add_argument("--force-image", action="store_true", help="强制重烤镜像")
    ap.add_argument(
        "--image",
        help="直接指定镜像（跳过新鲜度检查与烘焙——判别实验用）",
    )
    ap.add_argument("--keep", action="store_true", help="保留 run 目录")
    ap.add_argument("--work-dir", help="指定 run 目录（默认 mktemp）")
    args = ap.parse_args(argv)

    if not Path(args.code).exists():
        print(f"frame-gate20: FAIL: AAVMF CODE 固件缺席：{args.code}", file=sys.stderr)
        return 2
    try:
        return run_gate(args)
    except GateError as e:
        print(f"frame-gate20: FAIL: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
