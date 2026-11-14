#!/usr/bin/env python3
"""desktop-calib —— Mac 1080×1920 GPU/ramfb 双臂性能标定。

同一产品镜像、同一数据盘、同一 QMP 动作，分别以 virtio-gpu 和 ramfb
后端启动，记录 boot/cold-launch/back/page/switch 与 launcher 真实横滑窗口。
横滑窗口按 VGPUPERF 累计 submit 序号与对应 VGPUDMA token 精确切片：vgpud
中 submit_seq 与 perf_submits 同起点、每 submit 同步 +1，故第 N 次 submit 的
VGPUPERF 行 submits=N ↔ VGPUDMA token=N；perf_lines 取手势窗口本身而非全
日志尾行。基线（submit 序号 + DMA token）在注入前用同一份 serial 快照采集。
frames 不足/DMA 未排水只如实输出字段供上层 gate，不在此默认判死。
绝对延迟只写入本机报告，不做跨机器 CI 阈值。
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@dataclass
class ArmResult:
    backend: str
    boot_ready_s: float
    berrydemo_launch_s: float
    appbar_back_s: float
    page_transition_s: float
    switch_s: float
    launcher_swipe_s: float | None
    launcher_swipe_s_worst: float | None
    launcher_swipe_frames: int
    launcher_swipe_frames_min: int
    launcher_swipe_submit_us: int
    launcher_swipe_dma_us: int
    launcher_swipe_dma_us_median: float
    launcher_swipe_first_frame_s: float | None
    launcher_swipe_dma_drained: bool
    launcher_swipe_runs: list[dict]
    vgpu_ok: bool
    vgpuhdi_marker: bool
    kernel_panic: bool
    perf_lines: list[str]


def load_gui8():
    path = ROOT / "tools" / "gui8-smoke.py"
    spec = importlib.util.spec_from_file_location("berry_gui8_smoke", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load gui8-smoke.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def wait_marker(path: Path, marker: str, proc: subprocess.Popen, timeout: float) -> float:
    start = time.monotonic()
    deadline = start + timeout
    while time.monotonic() < deadline:
        try:
            text = path.read_text(errors="ignore")
        except FileNotFoundError:
            text = ""
        if marker in text:
            return time.monotonic() - start
        if proc.poll() is not None:
            raise RuntimeError(f"QEMU exited rc={proc.returncode} before {marker}")
        time.sleep(0.02)
    raise RuntimeError(f"timeout waiting {marker}")


def count_marker(path: Path, marker: str) -> int:
    return text_of(path).count(marker)


def wait_new_marker(
    path: Path, marker: str, before: int, proc: subprocess.Popen, timeout: float
) -> float:
    start = time.monotonic()
    deadline = start + timeout
    while time.monotonic() < deadline:
        if count_marker(path, marker) > before:
            return time.monotonic() - start
        if proc.poll() is not None:
            raise RuntimeError(f"QEMU exited rc={proc.returncode} before new {marker}")
        time.sleep(0.02)
    raise RuntimeError(f"timeout waiting new {marker}")


def text_of(path: Path) -> str:
    try:
        return path.read_text(errors="ignore")
    except FileNotFoundError:
        return ""


@dataclass(frozen=True)
class PerfFrame:
    """一条 VGPUPERF 行（ACK 时刻打印）。submits 为累计 submit 序号。"""

    submits: int
    avg_us: int
    last_us: int  # 本次 submit 的入队/校验耗时（非呈现延迟）
    dma_us: int  # ACK 时刻最近一次已完成 DMA 的耗时，非本帧 DMA
    reattaches: int
    jitter_us: int
    rects: int
    area: int  # 计划传输面积和（裁剪后矩形）
    union: int  # flush 并集面积
    raw: str = ""

    @property
    def token(self) -> int:
        # vgpud 的 submit_seq(token) 与 perf_submits 同起点 0、每 submit
        # 同步 +1，因此第 N 次 submit 的行 submits=N 对应 VGPUDMA token=N。
        return self.submits


@dataclass(frozen=True)
class DmaDone:
    """一条 VGPUDMA 完成行：token 即对应 submit 序号。"""

    token: int
    us: int


@dataclass(frozen=True)
class SerialBaseline:
    """注入前同刻快照：累计 submit 序号与已见最大 DMA token。"""

    submits: int
    token: int


def _int_fields(payload: str) -> dict[str, int]:
    fields: dict[str, int] = {}
    for token in payload.split():
        key, sep, value = token.partition("=")
        if not sep:
            continue
        try:
            fields[key] = int(value)
        except ValueError:
            continue
    return fields


def parse_perf_frames(text: str) -> list[PerfFrame]:
    frames = []
    for line in text.splitlines():
        idx = line.find("VGPUPERF ")
        if idx < 0:
            continue
        kv = _int_fields(line[idx + len("VGPUPERF "):])
        if "submits" not in kv:
            continue
        frames.append(PerfFrame(
            submits=kv["submits"],
            avg_us=kv.get("avg_us", 0),
            last_us=kv.get("last_us", 0),
            dma_us=kv.get("dma_us", 0),
            reattaches=kv.get("reattaches", 0),
            jitter_us=kv.get("jitter_us", 0),
            rects=kv.get("rects", 0),
            area=kv.get("area", 0),
            union=kv.get("union", 0),
            raw=line[idx:].strip(),
        ))
    return frames


def parse_dma_done(text: str) -> list[DmaDone]:
    done = []
    for line in text.splitlines():
        idx = line.find("VGPUDMA ")
        if idx < 0:
            continue
        kv = _int_fields(line[idx + len("VGPUDMA "):])
        if "token" in kv and "us" in kv:
            done.append(DmaDone(token=kv["token"], us=kv["us"]))
    return done


def snapshot_baseline(text: str) -> SerialBaseline:
    """一份 serial 文本 → 注入前基线；submit/DMA 两类计数出自同一快照。"""
    frames = parse_perf_frames(text)
    dmas = parse_dma_done(text)
    return SerialBaseline(
        submits=max((f.submits for f in frames), default=0),
        token=max((d.token for d in dmas), default=0),
    )


def median(values: list[float]) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2.0


def quick_click(qmp, x: int, y: int, w: int, h: int) -> float:
    """三批稳定点击；计时从 UP 命令被 QMP 接收后开始，不含宿主固定睡眠。"""
    ax = max(0, min(32767, round(x * 32768 / w)))
    ay = max(0, min(32767, round(y * 32768 / h)))
    qmp.execute("input-send-event", {"events": [
        {"type": "abs", "data": {"axis": "x", "value": ax}},
        {"type": "abs", "data": {"axis": "y", "value": ay}},
    ]})
    # 与 GUI8 相同的 200ms 批界保证 inputd 逐批消费；性能计时从 UP
    # 命令被 QMP 接收后开始，不把宿主固定睡眠算进 guest 延迟。
    time.sleep(0.2)
    qmp.execute("input-send-event", {"events": [
        {"type": "btn", "data": {"down": True, "button": "left"}},
        {"type": "abs", "data": {"axis": "x", "value": ax}},
        {"type": "abs", "data": {"axis": "y", "value": ay}},
    ]})
    time.sleep(0.2)
    qmp.execute("input-send-event", {"events": [
        {"type": "btn", "data": {"down": False, "button": "left"}},
    ]})
    return time.monotonic()




def linux_key_pulse(qmp, code: int) -> None:
    for down in (True, False):
        qmp.execute("input-send-event", {"events": [
            {"type": "key", "data": {"down": down, "key": {"type": "number", "data": code}}},
        ]})
        if down:
            time.sleep(0.12)


def quick_swipe(qmp, x0: int, y0: int, x1: int, y1: int, w: int, h: int) -> float:
    """注入一次真实横滑；计时从首个 ABS 事件被 QMP 接收后开始。"""
    def ax(x: int) -> int:
        return max(0, min(32767, round(x * 32768 / w)))

    def ay(y: int) -> int:
        return max(0, min(32767, round(y * 32768 / h)))

    start = time.monotonic()
    qmp.execute("input-send-event", {"events": [
        {"type": "abs", "data": {"axis": "x", "value": ax(x0)}},
        {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
    ]})
    time.sleep(0.08)
    qmp.execute("input-send-event", {"events": [
        {"type": "btn", "data": {"down": True, "button": "left"}},
        {"type": "abs", "data": {"axis": "x", "value": ax(x0)}},
        {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
    ]})
    for i in range(1, 6):
        frac = i / 5
        x = round(x0 + (x1 - x0) * frac)
        time.sleep(0.045)
        qmp.execute("input-send-event", {"events": [
            {"type": "abs", "data": {"axis": "x", "value": ax(x)}},
            {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
        ]})
    time.sleep(0.08)
    qmp.execute("input-send-event", {"events": [
        {"type": "btn", "data": {"down": False, "button": "left"}},
    ]})
    return start


@dataclass
class SwipeRun:
    """一次横滑的归因窗口：全部字段只来自 submits>基线 且 token>基线 的行。"""

    duration_s: float
    first_frame_s: float
    frames: int
    submit_from: int
    submit_to: int
    submits: list[int]
    submit_us: list[int]
    submit_us_max: int
    rects: list[int]
    rects_min: int
    rects_max: int
    area: list[int]
    area_min: int
    area_max: int
    union: list[int]
    union_min: int
    union_max: int
    dma_us_by_token: dict[int, int]
    dma_us_max: int
    dma_us_median: float
    dma_missing_tokens: list[int]
    dma_drained: bool
    dma_us_per_mpixel: float | None
    frame_gaps_ms: list[float]
    quiesce_s: float
    quiesce_satisfied: bool
    perf_lines: list[str]


# 静默下限 1.0s + 自适应 2× 最大帧距（封顶 4.0s），避免吞掉迟到的
# settle 第 3 帧；DMA 未排水再延 2.0s 硬帽；整窗硬上限 8.0s。
SWIPE_QUIESCE_FLOOR_S = 1.0
SWIPE_QUIESCE_CAP_S = 4.0
SWIPE_DMA_DRAIN_CAP_S = 2.0
SWIPE_OVERALL_S = 8.0


def build_swipe_run(
    frames: list[PerfFrame],
    dmas: list[DmaDone],
    arrivals: list[float],
    start: float,
    quiesce_s: float,
    timed_out: bool,
) -> SwipeRun:
    dma_map = {d.token: d.us for d in dmas}
    submit_us = [f.last_us for f in frames]
    areas = [f.area for f in frames]
    unions = [f.union for f in frames]
    rects = [f.rects for f in frames]
    missing = [f.submits for f in frames if f.submits not in dma_map]
    dma_us_max = max(dma_map.values(), default=0)
    area_max = max(areas, default=0)
    per_mpixel = None
    if dma_us_max > 0 and area_max > 0:
        per_mpixel = round(dma_us_max / (area_max / 1_000_000.0), 2)
    drained = bool(frames) and frames[-1].submits in dma_map
    return SwipeRun(
        duration_s=round((arrivals[-1] - start) if arrivals else 0.0, 3),
        first_frame_s=round((arrivals[0] - start) if arrivals else 0.0, 3),
        frames=len(frames),
        submit_from=frames[0].submits if frames else 0,
        submit_to=frames[-1].submits if frames else 0,
        submits=[f.submits for f in frames],
        submit_us=submit_us,
        submit_us_max=max(submit_us, default=0),
        rects=rects,
        rects_min=min(rects, default=0),
        rects_max=max(rects, default=0),
        area=areas,
        area_min=min(areas, default=0),
        area_max=area_max,
        union=unions,
        union_min=min(unions, default=0),
        union_max=max(unions, default=0),
        dma_us_by_token=dma_map,
        dma_us_max=dma_us_max,
        dma_us_median=round(median(list(dma_map.values())), 3) if dma_map else 0.0,
        dma_missing_tokens=missing,
        dma_drained=drained,
        dma_us_per_mpixel=per_mpixel,
        frame_gaps_ms=[
            round((b - a) * 1000.0, 1) for a, b in zip(arrivals, arrivals[1:])
        ],
        quiesce_s=round(quiesce_s, 3),
        quiesce_satisfied=bool(frames) and drained and not timed_out,
        perf_lines=[f.raw for f in frames],
    )


def home_reset(serial: Path, proc: subprocess.Popen, qmp) -> None:
    """产品 HOME 键复位 page 0 并等复位帧静默，供下一次测量干净起步。"""
    home_before = count_marker(serial, "WMNAV ok home owner=")
    linux_key_pulse(qmp, 60)
    home_deadline = time.monotonic() + 10.0
    while time.monotonic() < home_deadline:
        if count_marker(serial, "WMNAV ok home owner=") > home_before:
            break
        if proc.poll() is not None:
            raise RuntimeError(f"QEMU exited rc={proc.returncode} after HOME")
        time.sleep(0.02)
    else:
        raise RuntimeError("HOME reset marker missing")
    last_increase = time.monotonic()
    last_count = count_marker(serial, "VGPUPERF ")
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline:
        count = count_marker(serial, "VGPUPERF ")
        now = time.monotonic()
        if count > last_count:
            last_count = count
            last_increase = now
        if now - last_increase >= 1.2:
            return
        time.sleep(0.02)
    raise RuntimeError("vgpu reset swipe did not quiesce")


def measure_vgpu_swipe(serial: Path, proc: subprocess.Popen, qmp) -> SwipeRun:
    """注入一次真实横滑并按 submit 序号/token 精确归因窗口。

    基线（累计 submits 与已见最大 DMA token）在注入前用同一份 serial 快照
    采集（同刻，不再注入后补读 dma_before）；窗口 = 所有 submits>基线 的
    VGPUPERF 行与 token>基线 的 VGPUDMA 行，token==submits 一一对应。
    frames 不足、超时或 DMA 未排水均不抛错，只落到字段供上层 gate。
    """
    base = snapshot_baseline(text_of(serial))
    start = quick_swipe(qmp, 864, 700, 216, 700, 1080, 1920)
    frames: list[PerfFrame] = []
    arrivals: list[float] = []
    dmas: list[DmaDone] = []
    quiesce_need = SWIPE_QUIESCE_FLOOR_S
    timed_out = False
    while True:
        now = time.monotonic()
        text = text_of(serial)
        new_frames = [f for f in parse_perf_frames(text) if f.submits > base.submits]
        new_dmas = [d for d in parse_dma_done(text) if d.token > base.token]
        if len(new_frames) > len(frames):
            arrivals.extend([now] * (len(new_frames) - len(frames)))
            frames = new_frames
        dmas = new_dmas
        last_arrival = arrivals[-1] if arrivals else start
        max_gap = max(
            (b - a for a, b in zip(arrivals, arrivals[1:])), default=0.0
        )
        quiesce_need = min(
            max(SWIPE_QUIESCE_FLOOR_S, 2.0 * max_gap), SWIPE_QUIESCE_CAP_S
        )
        dma_map = {d.token: d.us for d in dmas}
        drained = bool(frames) and frames[-1].submits in dma_map
        if frames and now - last_arrival >= quiesce_need:
            if drained:
                break
            # 窗口内最后 submit 的 DMA 尚未完成：延后到排水硬帽再收窗，
            # 收窗时保留 dma_drained=False 与缺失 token 清单。
            if now - last_arrival >= quiesce_need + SWIPE_DMA_DRAIN_CAP_S:
                break
        if now - start >= SWIPE_OVERALL_S:
            timed_out = True
            break
        if proc.poll() is not None:
            raise RuntimeError(f"QEMU exited rc={proc.returncode} during swipe")
        time.sleep(0.02)
    run = build_swipe_run(frames, dmas, arrivals, start, quiesce_need, timed_out)
    home_reset(serial, proc, qmp)
    return run


def latest_layout(gui8, serial: Path):
    matches = list(gui8.BERRYDEMO_LAYOUT_RE.finditer(text_of(serial)))
    if not matches:
        raise RuntimeError("missing BERRYDEMOLAYOUT")
    return matches[-1]


def rect_center(match, name: str) -> tuple[int, int]:
    groups = {"back": (2, 3, 4, 5), "extra": (6, 7, 8, 9)}[name]
    x, y, w, h = (int(match.group(i)) for i in groups)
    return x + w // 2, y + h // 2


def run_arm(
    gui8, label: str, vgpu: bool, image: Path, code: Path, run_dir: Path,
    swipe_repeat: int = 1,
) -> ArmResult:
    work = run_dir / label
    work.mkdir(parents=True)
    vars_path = work / "vars.fd"
    vars_path.write_bytes(b"\xff" * (64 << 20))
    data = work / "data.img"
    subprocess.run(
        [sys.executable, str(ROOT / "tools/make_desktop_disk.py"), str(data)],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
    )
    serial = work / "serial.log"
    qmp_path = work / "qmp.sock"
    cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "host",
        "-m", "256M", "-accel", "hvf", "-display", "none", "-monitor", "none",
        "-no-reboot", "-serial", f"file:{serial}",
        "-qmp", f"unix:{qmp_path},server=on,wait=off",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"if=none,file={image},format=raw,id=bb,snapshot=on",
        "-device", "qemu-xhci", "-device", "usb-storage,drive=bb",
        "-global", "virtio-mmio.force-legacy=false",
        "-drive", f"if=none,id=drv0,format=raw,file={data},cache.writeback=off",
        "-device", "virtio-blk-device,drive=drv0",
        "-device", "virtio-tablet-device,id=tab0",
        "-device", "virtio-keyboard-device,id=kbd0",
    ]
    if vgpu:
        cmd += ["-device", "virtio-gpu-device,id=vgpu0,xres=1080,yres=1920"]
    cmd += [
        "-device", "ramfb",
        "-fw_cfg", "name=opt/berry/display,string=1080x1920@60",
    ]
    (work / "qemu.cmd.txt").write_text(shlex.join(cmd) + "\n")
    qemu_log = (work / "qemu.log").open("wb")
    proc = subprocess.Popen(cmd, cwd=ROOT, stdout=qemu_log, stderr=subprocess.STDOUT)
    try:
        boot_start = time.monotonic()
        wait_marker(serial, "LAUNCH ok", proc, 60)
        boot_ready = time.monotonic() - boot_start
        qmp = gui8.QmpClient(qmp_path)
        if vgpu:
            qmp.default_console = "vgpu0"

        # Product launcher boot selftest can space VGPUPERF frames widely on a
        # loaded host; wait it out before attributing new lines to user swipe.
        time.sleep(5.0 if vgpu else 0.3)
        # 每次重复都在注入前重新取同刻基线；重复间由 measure 内部 HOME 复位。
        swipe_runs = (
            [measure_vgpu_swipe(serial, proc, qmp) for _ in range(swipe_repeat)]
            if vgpu
            else []
        )

        home_layouts_before = count_marker(serial, "BERRYDEMOLAYOUT ok page=0")
        start = quick_click(qmp, 412, 282, 1080, 1920)
        wait_new_marker(
            serial, "BERRYDEMOLAYOUT ok page=0", home_layouts_before, proc, 20
        )
        launch = time.monotonic() - start

        home = latest_layout(gui8, serial)
        nav_x, nav_y = rect_center(home, "extra")
        page1_before = count_marker(serial, "BERRYDEMOPAGE ok page=1")
        start = quick_click(qmp, nav_x, nav_y, 1080, 1920)
        wait_new_marker(
            serial, "BERRYDEMOPAGE ok page=1", page1_before, proc, 20
        )
        page = time.monotonic() - start

        detail = latest_layout(gui8, serial)
        back_x, back_y = rect_center(detail, "back")
        page0_before = count_marker(serial, "BERRYDEMOPAGE ok page=0")
        start = quick_click(qmp, back_x, back_y, 1080, 1920)
        wait_new_marker(
            serial, "BERRYDEMOPAGE ok page=0", page0_before, proc, 20
        )
        back = time.monotonic() - start

        home = latest_layout(gui8, serial)
        nav_x, nav_y = rect_center(home, "extra")
        page1_before = count_marker(serial, "BERRYDEMOPAGE ok page=1")
        quick_click(qmp, nav_x, nav_y, 1080, 1920)
        wait_new_marker(
            serial, "BERRYDEMOPAGE ok page=1", page1_before, proc, 20
        )
        time.sleep(0.1)
        detail = latest_layout(gui8, serial)
        switch_x, switch_y = rect_center(detail, "extra")
        start = quick_click(qmp, switch_x, switch_y, 1080, 1920)
        wait_marker(serial, "BERRYDEMOSWITCH ok checked=1", proc, 20)
        switch = time.monotonic() - start

        serial_text = text_of(serial)
        # 汇总口径：时长/帧数取中位数（代表 run 供 perf_lines 证据），submit/
        # DMA 耗时取各次窗口最差的 max；frames 不足不在此判死，交给上层 gate
        # （frames_min / quiesce_satisfied / dma_drained / dma_missing_tokens）。
        durations = [r.duration_s for r in swipe_runs]
        med_duration = median(durations)
        all_dma_us = [
            us for run in swipe_runs for us in run.dma_us_by_token.values()
        ]
        first_frames = [r.first_frame_s for r in swipe_runs]
        representative = min(
            swipe_runs, key=lambda r: abs(r.duration_s - med_duration)
        ) if swipe_runs else None
        return ArmResult(
            backend="virtio-gpu" if vgpu else "ramfb",
            boot_ready_s=round(boot_ready, 3),
            berrydemo_launch_s=round(launch, 3),
            appbar_back_s=round(back, 3),
            page_transition_s=round(page, 3),
            switch_s=round(switch, 3),
            launcher_swipe_s=round(med_duration, 3) if swipe_runs else None,
            launcher_swipe_s_worst=round(max(durations), 3) if swipe_runs else None,
            launcher_swipe_frames=(
                int(round(median([r.frames for r in swipe_runs])))
                if swipe_runs else 0
            ),
            launcher_swipe_frames_min=(
                min((r.frames for r in swipe_runs), default=0)
            ),
            launcher_swipe_submit_us=(
                max((r.submit_us_max for r in swipe_runs), default=0)
            ),
            launcher_swipe_dma_us=(
                max((r.dma_us_max for r in swipe_runs), default=0)
            ),
            launcher_swipe_dma_us_median=(
                round(median(all_dma_us), 3) if all_dma_us else 0.0
            ),
            launcher_swipe_first_frame_s=(
                round(median(first_frames), 3) if first_frames else None
            ),
            launcher_swipe_dma_drained=(
                bool(swipe_runs) and all(r.dma_drained for r in swipe_runs)
            ),
            launcher_swipe_runs=[asdict(r) for r in swipe_runs],
            vgpu_ok="VGPU ok " in serial_text,
            vgpuhdi_marker="VGPUHDI ok " in serial_text,
            kernel_panic="KERNEL PANIC" in serial_text,
            perf_lines=list(representative.perf_lines) if representative else [],
        )
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        qemu_log.close()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--image", type=Path, default=ROOT / "out/berryos-desktop.img")
    ap.add_argument("--code", type=Path,
                    default=Path("/opt/homebrew/share/qemu/edk2-aarch64-code.fd"))
    ap.add_argument("--out-json", type=Path, default=ROOT / "out/desktop-calib.json")
    ap.add_argument("--out-md", type=Path, default=ROOT / "out/desktop-calib.md")
    ap.add_argument("--keep-dir", type=Path, help="保留双臂 serial/QMP artifacts 的调试目录")
    ap.add_argument("--swipe-repeat", type=int, default=1,
                    help="launcher 横滑测量次数（默认 1）；>1 时输出中位数与最坏值")
    args = ap.parse_args()
    if args.swipe_repeat < 1:
        ap.error("--swipe-repeat must be >= 1")
    if sys.platform != "darwin":
        print("desktop-calib: FAIL: Mac HVF only", file=sys.stderr)
        return 1
    gui8 = load_gui8()
    if args.keep_dir is not None:
        run_dir = args.keep_dir
        run_dir.mkdir(parents=True, exist_ok=True)
        results = [
            run_arm(gui8, "vgpu", True, args.image, args.code, run_dir,
                    swipe_repeat=args.swipe_repeat),
            run_arm(gui8, "ramfb", False, args.image, args.code, run_dir),
        ]
    else:
        with tempfile.TemporaryDirectory(prefix="berry-desktop-calib.") as tmp:
            run_dir = Path(tmp)
            results = [
                run_arm(gui8, "vgpu", True, args.image, args.code, run_dir,
                        swipe_repeat=args.swipe_repeat),
                run_arm(gui8, "ramfb", False, args.image, args.code, run_dir),
            ]
    args.out_json.parent.mkdir(parents=True, exist_ok=True)
    args.out_json.write_text(json.dumps([asdict(r) for r in results], indent=2) + "\n")
    vgpu, ramfb = results

    def num(value: float | None, digits: int = 3) -> str:
        return "n/a" if value is None else f"{value:.{digits}f}"

    md = []
    md.append("# Mac desktop calibration\n")
    md.append(f"- image: `{args.image}`")
    md.append("- resolution: 1080×1920")
    md.append(f"- swipe repeats: {args.swipe_repeat}")
    md.append("")
    md.append(
        "| backend | boot s | demo launch s | appbar back s | page s | switch s "
        "| swipe s med | swipe s worst | swipe frames med | swipe frames min "
        "| swipe submit µs | swipe DMA µs max | swipe DMA µs med | first frame s "
        "| DMA drained | VGPUHDI |"
    )
    md.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|")
    md.append(
        f"| {vgpu.backend} | {vgpu.boot_ready_s} | {vgpu.berrydemo_launch_s} | "
        f"{vgpu.appbar_back_s} | {vgpu.page_transition_s} | {vgpu.switch_s} | "
        f"{num(vgpu.launcher_swipe_s)} | {num(vgpu.launcher_swipe_s_worst)} | "
        f"{vgpu.launcher_swipe_frames} | {vgpu.launcher_swipe_frames_min} | "
        f"{vgpu.launcher_swipe_submit_us} | {vgpu.launcher_swipe_dma_us} | "
        f"{vgpu.launcher_swipe_dma_us_median} | "
        f"{num(vgpu.launcher_swipe_first_frame_s)} | "
        f"{vgpu.launcher_swipe_dma_drained} | {vgpu.vgpuhdi_marker} |"
    )
    md.append(
        f"| {ramfb.backend} | {ramfb.boot_ready_s} | {ramfb.berrydemo_launch_s} | "
        f"{ramfb.appbar_back_s} | {ramfb.page_transition_s} | {ramfb.switch_s} | "
        " | ".join(["n/a"] * 11) + " |"
    )
    md.append("")
    md.append("## vgpu launcher swipe windows（submit 序号 ↔ DMA token 归因）\n")
    md.append(
        "| run | first s | dur s | frames | submits | submit µs max "
        "| DMA µs max | DMA µs med | area min | area max | union min | union max | rects "
        "| DMA µs/MPix | drained | quiesce s |"
    )
    md.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|")
    evidence: list[str] = []
    for i, run in enumerate(vgpu.launcher_swipe_runs):
        md.append(
            f"| r{i} | {run['first_frame_s']} | {run['duration_s']} | {run['frames']} | "
            f"{run['submit_from']}–{run['submit_to']} | {run['submit_us_max']} | "
            f"{run['dma_us_max']} | {run['dma_us_median']} | "
            f"{run['area_min']} | {run['area_max']} | "
            f"{run['union_min']} | {run['union_max']} | "
            f"{run['rects_min']}–{run['rects_max']} | "
            f"{run['dma_us_per_mpixel'] if run['dma_us_per_mpixel'] is not None else 'n/a'} | "
            f"{run['dma_drained']} | {run['quiesce_s']} |"
        )
        evidence.append(
            f"r{i}: first_frame_s={run['first_frame_s']} "
            f"window submits={run['submit_from']}..{run['submit_to']} "
            f"dma_tokens_done={sorted(run['dma_us_by_token'])} "
            f"dma_missing={run['dma_missing_tokens']} "
            f"frame_gaps_ms={run['frame_gaps_ms']} "
            f"quiesce_satisfied={run['quiesce_satisfied']}"
        )
        evidence.extend(run["perf_lines"])
    if not vgpu.launcher_swipe_runs:
        md.append("| (no swipe runs) | | | | | | | | | | | | | |")
    md.append("")
    md.append("```text")
    md.extend(evidence)
    md.append("```")
    args.out_md.write_text("\n".join(md) + "\n", encoding="utf-8")
    print(json.dumps([asdict(r) for r in results], indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"desktop-calib: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
