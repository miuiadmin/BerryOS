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

横滑 move 间隔由 --swipe-step-ms 控制（默认 12ms，历史 45ms；move 数按
~225ms 总跨度换算，仍逐 move 发真实 QMP 事件），为 60Hz 评估提供足够内
容态。帧间隔口径 = host 首次观察到该 submit 的 VGPUPERF 行的时刻差，串
口轮询量化 SWIPE_POLL_INTERVAL_S（历史 20ms 轮询无法分辨 16.7ms 帧距，
报告以 obs_quantum_ms 显式标注）；不与 VGPUDMA 完成行、ACK 或行内
dma_us/last_us 耗时字段混淆。另输出 median/p95/max frame gap、等效
fps、240ms snap 理论/实际帧数与 1s 等效 fps；--fps-target（默认 60）
只产出到期达标布尔 launcher_swipe_fps_target_met，不作失败条件。"""
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
    launcher_swipe_frame_gap_ms_median: float | None
    launcher_swipe_frame_gap_ms_p95: float | None
    launcher_swipe_frame_gap_ms_max: float | None
    launcher_swipe_fps_equiv: float | None
    launcher_swipe_snap240_theoretical_frames: float
    launcher_swipe_snap240_actual_frames: int
    launcher_swipe_fps_1s_equiv: float | None
    launcher_swipe_fps_target_met: bool
    launcher_swipe_runs: list[dict]
    swipe_step_ms: float
    swipe_obs_quantum_ms: float
    fps_target: float
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


class SerialTail:
    """增量尾读 QEMU serial 文件：只取新完成行，支撑高频轮询不重读全量。"""

    def __init__(self, path: Path) -> None:
        self._path = path
        self._file = None
        self._pending = b""

    def read_new_lines(self) -> str:
        """返回自上次调用以来的完整新行（\\n 连接；半行留待下次）。"""
        if self._file is None:
            try:
                self._file = self._path.open("rb")
            except FileNotFoundError:
                return ""
        data = self._file.read()
        if not data:
            return ""
        self._pending += data
        *lines, self._pending = self._pending.split(b"\n")
        if not lines:
            return ""
        return b"\n".join(lines).decode("utf-8", errors="replace")

    def close(self) -> None:
        if self._file is not None:
            self._file.close()
            self._file = None


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


def percentile(values: list[float], q: float) -> float:
    """线性插值分位（numpy 默认口径）；空表返回 0.0。"""
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    pos = q * (len(ordered) - 1)
    lo = int(pos)
    hi = min(lo + 1, len(ordered) - 1)
    frac = pos - lo
    return ordered[lo] + (ordered[hi] - ordered[lo]) * frac


def snap_window_frames(arrivals_s: list[float], window_s: float) -> int:
    """最优滑窗帧数：任意 [t, t+window_s] 内的 arrival 数（含窗口首帧）。

    arrivals 需单调不减（轮询到达时刻天然满足）。对轮询量化不敏感：
    同一 poll 内多帧共享同一时刻仍各计 1 帧，适合 60Hz 达标判读。
    """
    best = 0
    lo = 0
    for hi, t in enumerate(arrivals_s):
        while t - arrivals_s[lo] > window_s:
            lo += 1
        count = hi - lo + 1
        if count > best:
            best = count
    return best


def snap240_theoretical_frames(fps_target: float) -> float:
    """240ms snap 窗的理论帧数：60Hz → 14.4（窗口含首帧，理想排布 ≈15）。"""
    return round(fps_target * SNAP_WINDOW_MS / 1000.0, 1)


def fps_target_met(snap_actuals: list[int], fps_target: float) -> bool:
    """达标布尔：各 run snap240 实际帧数的中位数 ≥ 理论值取整。

    只作输出，不影响退出码；无 run 时 False。取整（而非 ceil）以吸收
    ~1 帧的轮询量化损失：60Hz 理想 15/14.4→阈值 14，50Hz 恒 13 仍拒。
    """
    if not snap_actuals:
        return False
    return median(snap_actuals) >= round(fps_target * SNAP_WINDOW_MS / 1000.0)


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


# 横滑手势与帧率评估常量。历史口径 = 5 move × 45ms；--swipe-step-ms 只改
# 密度不改总跨度（~225ms），45ms 步长仍精确还原 5 move 旧行为。
SWIPE_GESTURE_SPAN_S = 0.225
SWIPE_STEP_MS_DEFAULT = 12.0
SNAP_WINDOW_MS = 240.0
FPS_TARGET_DEFAULT = 60.0
# 横滑窗口的串口轮询间隔：帧间隔（submit arrival 差）以它为量化粒度并
# 写入 obs_quantum_ms。历史 20ms 无法分辨 16.7ms（60Hz）帧距，收紧到
# 2ms 增量尾读；实际粒度还含 sleep 精度与读取耗时。
SWIPE_POLL_INTERVAL_S = 0.002


def swipe_move_count(step_s: float) -> int:
    """步长 → move 数：总跨度 ≈225ms 恒定（45ms→5，12ms→19），下限 2。"""
    return max(2, round(SWIPE_GESTURE_SPAN_S / step_s))


@dataclass
class SwipeInjection:
    """Host injection timing truth for one real QMP swipe."""

    start: float
    qmp_ms: list[float]
    interval_ms: list[float]
    target_step_ms: float


def quick_swipe(
    qmp, x0: int, y0: int, x1: int, y1: int, w: int, h: int,
    step_s: float = 0.045,
) -> SwipeInjection:
    """注入一次真实横滑；计时从首个 ABS 事件被 QMP 接收后开始。

    逐 move 发真实 input-send-event（不合成、不批处理）。move 间隔 =
    step_s 宿主 sleep + QMP 往返开销，非硬实时注入器；qmp_ms/interval_ms
    保留宿主注入真实节奏，防止把注入器瓶颈误归因给 guest。
    """
    def ax(x: int) -> int:
        return max(0, min(32767, round(x * 32768 / w)))

    def ay(y: int) -> int:
        return max(0, min(32767, round(y * 32768 / h)))

    def send(events: list[dict], last_start: float | None = None) -> float:
        t0 = time.monotonic()
        qmp.execute("input-send-event", {"events": events})
        t1 = time.monotonic()
        if last_start is not None:
            injection.interval_ms.append(round((t0 - last_start) * 1000.0, 3))
        injection.qmp_ms.append(round((t1 - t0) * 1000.0, 3))
        return t0

    moves = swipe_move_count(step_s)
    injection = SwipeInjection(
        start=0.0, qmp_ms=[], interval_ms=[], target_step_ms=step_s * 1000.0
    )
    start = time.monotonic()
    injection.start = start
    _ = send([
        {"type": "abs", "data": {"axis": "x", "value": ax(x0)}},
        {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
    ])
    time.sleep(0.08)
    _ = send([
        {"type": "btn", "data": {"down": True, "button": "left"}},
        {"type": "abs", "data": {"axis": "x", "value": ax(x0)}},
        {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
    ])
    last_start = injection.start
    for i in range(1, moves + 1):
        frac = i / moves
        x = round(x0 + (x1 - x0) * frac)
        time.sleep(step_s)
        last_start = send([
            {"type": "abs", "data": {"axis": "x", "value": ax(x)}},
            {"type": "abs", "data": {"axis": "y", "value": ay(y0)}},
        ], last_start)
    time.sleep(0.08)
    send([
        {"type": "btn", "data": {"down": False, "button": "left"}},
    ])
    return injection


@dataclass
class SwipeRun:
    """一次横滑的归因窗口：全部字段只来自 submits>基线 且 token>基线 的行。"""

    duration_s: float
    first_frame_s: float
    input_target_step_ms: float
    input_qmp_ms_median: float
    input_qmp_ms_p95: float
    input_interval_ms_median: float
    input_interval_ms_p95: float
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
    frame_gap_ms_median: float | None
    frame_gap_ms_p95: float | None
    frame_gap_ms_max: float | None
    fps_equiv: float | None
    snap240_theoretical_frames: float
    snap240_actual_frames: int
    fps_1s_equiv: float | None
    obs_quantum_ms: float
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
    injection: SwipeInjection,
    quiesce_s: float,
    timed_out: bool,
    fps_target: float = FPS_TARGET_DEFAULT,
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
    # 帧间隔口径：相邻两次 submit 的 VGPUPERF 行在 host 侧被首次观察到
    # 的时刻差（poll 量化，见 obs_quantum_ms）；不用 VGPUDMA 完成时刻，
    # 也不用行内 dma_us/last_us 耗时字段。
    gaps_ms = [
        round((b - a) * 1000.0, 1) for a, b in zip(arrivals, arrivals[1:])
    ]
    gap_median = round(median(gaps_ms), 1) if gaps_ms else None
    gap_p95 = (
        round(percentile(gaps_ms, 0.95), 1) if gaps_ms else None
    )
    gap_max = max(gaps_ms) if gaps_ms else None
    snap_actual = snap_window_frames(arrivals, SNAP_WINDOW_MS / 1000.0)
    return SwipeRun(
        duration_s=round(
            (arrivals[-1] - injection.start) if arrivals else 0.0, 3
        ),
        first_frame_s=round(
            (arrivals[0] - injection.start) if arrivals else 0.0, 3
        ),
        input_target_step_ms=round(injection.target_step_ms, 3),
        input_qmp_ms_median=round(median(injection.qmp_ms), 3)
        if injection.qmp_ms else 0.0,
        input_qmp_ms_p95=round(percentile(injection.qmp_ms, 0.95), 3)
        if injection.qmp_ms else 0.0,
        input_interval_ms_median=round(median(injection.interval_ms), 3)
        if injection.interval_ms else 0.0,
        input_interval_ms_p95=round(percentile(injection.interval_ms, 0.95), 3)
        if injection.interval_ms else 0.0,
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
        frame_gaps_ms=gaps_ms,
        frame_gap_ms_median=gap_median,
        frame_gap_ms_p95=gap_p95,
        frame_gap_ms_max=gap_max,
        fps_equiv=(
            round(1000.0 / gap_median, 1)
            if gap_median is not None and gap_median > 0 else None
        ),
        snap240_theoretical_frames=snap240_theoretical_frames(fps_target),
        snap240_actual_frames=snap_actual,
        fps_1s_equiv=(
            round(snap_actual * 1000.0 / SNAP_WINDOW_MS, 1)
            if snap_actual >= 2 else None
        ),
        obs_quantum_ms=round(SWIPE_POLL_INTERVAL_S * 1000.0, 1),
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


def measure_vgpu_swipe(
    serial: Path, proc: subprocess.Popen, qmp,
    step_s: float = 0.045,
    fps_target: float = FPS_TARGET_DEFAULT,
) -> SwipeRun:
    """注入一次真实横滑并按 submit 序号/token 精确归因窗口。

    基线（累计 submits 与已见最大 DMA token）在注入前用同一份 serial 快照
    采集（同刻，不再注入后补读 dma_before）；窗口 = 所有 submits>基线 的
    VGPUPERF 行与 token>基线 的 VGPUDMA 行，token==submits 一一对应。
    帧到达时刻 = 各 VGPUPERF 行首次被 host 观察到的 poll 时刻，量化粒度
    SWIPE_POLL_INTERVAL_S（写入 obs_quantum_ms）。frames 不足、超时或
    DMA 未排水均不抛错，只落到字段供上层 gate。
    """
    base = snapshot_baseline(text_of(serial))
    tail = SerialTail(serial)
    tail.read_new_lines()  # 丢弃历史行，只看手势窗口内新出现的行
    injection = quick_swipe(qmp, 864, 700, 216, 700, 1080, 1920, step_s)
    start = injection.start
    frames: list[PerfFrame] = []
    arrivals: list[float] = []
    dmas: list[DmaDone] = []
    quiesce_need = SWIPE_QUIESCE_FLOOR_S
    timed_out = False
    try:
        while True:
            now = time.monotonic()
            chunk = tail.read_new_lines()
            if chunk:
                new_frames = [
                    f for f in parse_perf_frames(chunk)
                    if f.submits > base.submits
                ]
                if new_frames:
                    arrivals.extend([now] * len(new_frames))
                    frames.extend(new_frames)
                dmas.extend(
                    d for d in parse_dma_done(chunk) if d.token > base.token
                )
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
            time.sleep(SWIPE_POLL_INTERVAL_S)
    finally:
        tail.close()
    run = build_swipe_run(
        frames, dmas, arrivals, injection, quiesce_need, timed_out,
        fps_target=fps_target,
    )
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
    swipe_step_ms: float = SWIPE_STEP_MS_DEFAULT,
    fps_target: float = FPS_TARGET_DEFAULT,
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
            [
                measure_vgpu_swipe(
                    serial, proc, qmp,
                    step_s=swipe_step_ms / 1000.0, fps_target=fps_target,
                )
                for _ in range(swipe_repeat)
            ]
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
        # 帧间隔 pacing 汇总：gap 统计只池化 run 内间隔（跨 run 边界是
        # HOME 复位 + 静默，不构成帧距）；snap240/1s 等效 fps 取各 run 中
        # 位数；达标布尔只作输出，不作失败条件。
        pooled_gaps = [g for run in swipe_runs for g in run.frame_gaps_ms]
        gap_med = round(median(pooled_gaps), 1) if pooled_gaps else None
        snap_actuals = [run.snap240_actual_frames for run in swipe_runs]
        med_snap = int(round(median(snap_actuals))) if snap_actuals else 0
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
            launcher_swipe_frame_gap_ms_median=gap_med,
            launcher_swipe_frame_gap_ms_p95=(
                round(percentile(pooled_gaps, 0.95), 1)
                if pooled_gaps else None
            ),
            launcher_swipe_frame_gap_ms_max=(
                max(pooled_gaps) if pooled_gaps else None
            ),
            launcher_swipe_fps_equiv=(
                round(1000.0 / gap_med, 1)
                if gap_med is not None and gap_med > 0 else None
            ),
            launcher_swipe_snap240_theoretical_frames=(
                snap240_theoretical_frames(fps_target)
            ),
            launcher_swipe_snap240_actual_frames=med_snap,
            launcher_swipe_fps_1s_equiv=(
                round(med_snap * 1000.0 / SNAP_WINDOW_MS, 1)
                if med_snap >= 2 else None
            ),
            launcher_swipe_fps_target_met=fps_target_met(snap_actuals, fps_target),
            launcher_swipe_runs=[asdict(r) for r in swipe_runs],
            swipe_step_ms=(swipe_step_ms if swipe_runs else 0.0),
            swipe_obs_quantum_ms=(
                round(SWIPE_POLL_INTERVAL_S * 1000.0, 1) if swipe_runs else 0.0
            ),
            fps_target=fps_target,
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
    ap.add_argument("--swipe-step-ms", type=float, default=SWIPE_STEP_MS_DEFAULT,
                    help=f"launcher 横滑 move 间隔毫秒（默认 "
                         f"{SWIPE_STEP_MS_DEFAULT:g}，历史 45）；move 数按 "
                         f"~225ms 总跨度换算（45→5、12→19），仍逐 move 发"
                         f"真实 QMP 事件")
    ap.add_argument("--fps-target", type=float, default=FPS_TARGET_DEFAULT,
                    help=f"帧率达标目标 fps（默认 {FPS_TARGET_DEFAULT:g}）；"
                         f"只输出 launcher_swipe_fps_target_met 布尔，"
                         f"不作为失败条件")
    args = ap.parse_args()
    if args.swipe_repeat < 1:
        ap.error("--swipe-repeat must be >= 1")
    if args.swipe_step_ms <= 0:
        ap.error("--swipe-step-ms must be > 0")
    if args.fps_target <= 0:
        ap.error("--fps-target must be > 0")
    if sys.platform != "darwin":
        print("desktop-calib: FAIL: Mac HVF only", file=sys.stderr)
        return 1
    gui8 = load_gui8()
    if args.keep_dir is not None:
        run_dir = args.keep_dir
        run_dir.mkdir(parents=True, exist_ok=True)
        results = [
            run_arm(gui8, "vgpu", True, args.image, args.code, run_dir,
                    swipe_repeat=args.swipe_repeat,
                    swipe_step_ms=args.swipe_step_ms,
                    fps_target=args.fps_target),
            run_arm(gui8, "ramfb", False, args.image, args.code, run_dir,
                    fps_target=args.fps_target),
        ]
    else:
        with tempfile.TemporaryDirectory(prefix="berry-desktop-calib.") as tmp:
            run_dir = Path(tmp)
            results = [
                run_arm(gui8, "vgpu", True, args.image, args.code, run_dir,
                        swipe_repeat=args.swipe_repeat,
                        swipe_step_ms=args.swipe_step_ms,
                        fps_target=args.fps_target),
                run_arm(gui8, "ramfb", False, args.image, args.code, run_dir,
                        fps_target=args.fps_target),
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
    md.append(
        f"- swipe step: {args.swipe_step_ms:g} ms → "
        f"{swipe_move_count(args.swipe_step_ms / 1000.0)} moves "
        f"(span ≈{SWIPE_GESTURE_SPAN_S * 1000.0:g} ms, real QMP events)"
    )
    md.append(f"- fps target: {args.fps_target:g}")
    md.append(
        f"- serial observation quantum: "
        f"{SWIPE_POLL_INTERVAL_S * 1000.0:g} ms "
        f"(frame gaps resolve only ≥ this; the historical 20 ms poll could "
        f"not resolve a 16.7 ms / 60 Hz frame gap)"
    )
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
            f"gap_med/p95/max_ms={run['frame_gap_ms_median']}/"
            f"{run['frame_gap_ms_p95']}/{run['frame_gap_ms_max']} "
            f"snap240={run['snap240_actual_frames']}"
            f"(theo {run['snap240_theoretical_frames']}) "
            f"quiesce_satisfied={run['quiesce_satisfied']}"
        )
        evidence.extend(run["perf_lines"])
    if not vgpu.launcher_swipe_runs:
        md.append("| (no swipe runs) | | | | | | | | | | | | | |")
    md.append("")
    md.append("## vgpu launcher 帧间隔 pacing（submit arrival 口径）\n")
    md.append(
        f"帧间隔 = 相邻 submit 的 VGPUPERF 行在 host 首次被观察到的时刻差"
        f"（poll 量化 ~{vgpu.swipe_obs_quantum_ms:g} ms，旧 20 ms 轮询无法"
        f"分辨 16.7 ms）；不用 VGPUDMA 完成时刻，也不用行内 dma_us/last_us"
        f" 耗时字段。snap240 = 最优 240 ms 滑窗内帧数（含窗口首帧）；"
        f"1s 等效 fps = snap240 实际帧数 × 1000/240。达标布尔"
        f"`launcher_swipe_fps_target_met` 只作输出，不影响退出码。\n"
    )
    md.append(
        "| run | frames | gap med ms | gap p95 ms | gap max ms | fps equiv "
        "| snap240 act | snap240 theo | fps 1s equiv |"
    )
    md.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for i, run in enumerate(vgpu.launcher_swipe_runs):
        md.append(
            f"| r{i} | {run['frames']} | {run['frame_gap_ms_median']} | "
            f"{run['frame_gap_ms_p95']} | {run['frame_gap_ms_max']} | "
            f"{run['fps_equiv']} | {run['snap240_actual_frames']} | "
            f"{run['snap240_theoretical_frames']} | {run['fps_1s_equiv']} |"
        )
    if vgpu.launcher_swipe_runs:
        md.append(
            f"| **pooled/med** | — | "
            f"{vgpu.launcher_swipe_frame_gap_ms_median} | "
            f"{vgpu.launcher_swipe_frame_gap_ms_p95} | "
            f"{vgpu.launcher_swipe_frame_gap_ms_max} | "
            f"{vgpu.launcher_swipe_fps_equiv} | "
            f"{vgpu.launcher_swipe_snap240_actual_frames} | "
            f"{vgpu.launcher_swipe_snap240_theoretical_frames} | "
            f"{vgpu.launcher_swipe_fps_1s_equiv} |"
        )
        md.append(
            f"\nfps_target_met({vgpu.fps_target:g}) = "
            f"{vgpu.launcher_swipe_fps_target_met}"
        )
    else:
        md.append("| (no swipe runs) | | | | | | | | |")
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
