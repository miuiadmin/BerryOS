#!/usr/bin/env python3
"""desktop-calib —— Mac 1080×1920 GPU/ramfb 双臂性能标定。

同一产品镜像、同一数据盘、同一 QMP 动作，分别以 virtio-gpu 和 ramfb
后端启动，记录 boot/cold-launch/back/page/switch 延迟与 VGPUPERF 尾行。
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
        perf = []
        for line in serial_text.splitlines():
            idx = line.find("VGPUPERF ")
            if idx >= 0:
                perf.append(line[idx:].strip())
        return ArmResult(
            backend="virtio-gpu" if vgpu else "ramfb",
            boot_ready_s=round(boot_ready, 3),
            berrydemo_launch_s=round(launch, 3),
            appbar_back_s=round(back, 3),
            page_transition_s=round(page, 3),
            switch_s=round(switch, 3),
            vgpu_ok="VGPU ok " in serial_text,
            vgpuhdi_marker="VGPUHDI ok " in serial_text,
            kernel_panic="KERNEL PANIC" in serial_text,
            perf_lines=perf[-4:],
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
    args = ap.parse_args()
    if sys.platform != "darwin":
        print("desktop-calib: FAIL: Mac HVF only", file=sys.stderr)
        return 1
    gui8 = load_gui8()
    if args.keep_dir is not None:
        run_dir = args.keep_dir
        run_dir.mkdir(parents=True, exist_ok=True)
        results = [
            run_arm(gui8, "vgpu", True, args.image, args.code, run_dir),
            run_arm(gui8, "ramfb", False, args.image, args.code, run_dir),
        ]
    else:
        with tempfile.TemporaryDirectory(prefix="berry-desktop-calib.") as tmp:
            run_dir = Path(tmp)
            results = [
                run_arm(gui8, "vgpu", True, args.image, args.code, run_dir),
                run_arm(gui8, "ramfb", False, args.image, args.code, run_dir),
            ]
    args.out_json.parent.mkdir(parents=True, exist_ok=True)
    args.out_json.write_text(json.dumps([asdict(r) for r in results], indent=2) + "\n")
    vgpu, ramfb = results
    args.out_md.write_text(
        "# Mac desktop calibration\n\n"
        f"- image: `{args.image}`\n"
        f"- resolution: 1080×1920\n"
        f"| backend | boot s | demo launch s | appbar back s | page s | switch s | VGPUHDI |\n"
        f"|---|---:|---:|---:|---:|---:|---|\n"
        f"| {vgpu.backend} | {vgpu.boot_ready_s} | {vgpu.berrydemo_launch_s} | "
        f"{vgpu.appbar_back_s} | {vgpu.page_transition_s} | {vgpu.switch_s} | "
        f"{vgpu.vgpuhdi_marker} |\n"
        f"| {ramfb.backend} | {ramfb.boot_ready_s} | {ramfb.berrydemo_launch_s} | "
        f"{ramfb.appbar_back_s} | {ramfb.page_transition_s} | {ramfb.switch_s} | n/a |\n\n"
        "```text\n" + "\n".join(vgpu.perf_lines) + "\n```\n",
        encoding="utf-8",
    )
    print(json.dumps([asdict(r) for r in results], indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"desktop-calib: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
