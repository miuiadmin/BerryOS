#!/usr/bin/env python3
"""Reproducible product-shell e2e suites for the 1080x1920 desktop image.

The historical recents/frame-cache runs lived only in gitignored ``out/`` and
hard-coded one developer's absolute worktree.  This tool keeps the same real
QEMU/HVF + virtio-gpu + Berry Room path while making the rigs reviewable and
repeatable from any checkout.

Suites:
  recents      boot progress -> desktop -> app -> TASK -> Recents -> restore -> HOME
  frame-cache  two apps -> TASK -> Recents -> cached-frame replay

Both suites also pin the product BootProgress milestone chain m=1..11.  The
GUI8 probe topology intentionally uses a different chain and remains covered by
``tools/gui8-smoke.py --vgpu``.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
from collections import Counter
from typing import Iterable

ROOT = pathlib.Path(__file__).resolve().parents[1]
WIDTH = 1080
HEIGHT = 1920


class ProductE2EError(RuntimeError):
    pass


def load_gui8():
    path = ROOT / "tools" / "gui8-smoke.py"
    spec = importlib.util.spec_from_file_location("berry_gui8_smoke", path)
    if spec is None or spec.loader is None:
        raise ProductE2EError(f"cannot import gui8 helper: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def default_edk2_code() -> pathlib.Path:
    candidates = []
    if value := os.environ.get("BERRY_EDK2_CODE"):
        candidates.append(pathlib.Path(value))
    candidates.extend(
        [
            pathlib.Path("/opt/homebrew/share/qemu/edk2-aarch64-code.fd"),
            pathlib.Path("/usr/share/AAVMF/AAVMF_CODE.fd"),
            pathlib.Path("/usr/share/qemu-efi-aarch64/AAVMF_CODE.fd"),
        ]
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise ProductE2EError("no EDK2 AArch64 code flash found; pass --code")


class ProductRig:
    def __init__(
        self,
        suite: str,
        image: pathlib.Path,
        work_root: pathlib.Path,
        code: pathlib.Path,
        qemu: str,
        timeout: float,
        keep: bool,
    ) -> None:
        self.suite = suite
        self.image = image
        self.work = work_root / suite
        self.code = code
        self.qemu_bin = qemu
        self.timeout = timeout
        self.keep = keep
        self.gui8 = load_gui8()

        if not self.image.is_file():
            raise ProductE2EError(f"image absent: {self.image}")
        if shutil.which(self.qemu_bin) is None:
            raise ProductE2EError(f"QEMU absent from PATH: {self.qemu_bin}")

        if self.work.exists() and not self.keep:
            shutil.rmtree(self.work)
        self.work.mkdir(parents=True, exist_ok=True)
        # `--keep` preserves old screenshots for inspection, but live QEMU
        # endpoints must never be reused: reading an old serial.log before the
        # new QEMU truncates it can satisfy a marker from the previous run.
        (self.work / "serial.log").unlink(missing_ok=True)
        (self.work / "qmp.sock").unlink(missing_ok=True)
        (self.work / "qemu.log").unlink(missing_ok=True)
        self.vars_fd = self.work / "vars.fd"
        self.vars_fd.write_bytes(b"\xff" * (64 << 20))
        self.data = self.work / "data.img"
        subprocess.run(
            [
                sys.executable,
                str(ROOT / "tools" / "make_desktop_disk.py"),
                str(self.data),
            ],
            cwd=ROOT,
            check=True,
        )
        self.serial = self.work / "serial.log"
        self.qmp = self.work / "qmp.sock"
        self.qemu_log = self.work / "qemu.log"
        self._log_handle = self.qemu_log.open("wb")
        self.proc = subprocess.Popen(
            self.command(),
            cwd=ROOT,
            stdout=self._log_handle,
            stderr=subprocess.STDOUT,
        )
        self.client = None

    def command(self) -> list[str]:
        return [
            self.qemu_bin,
            "-machine",
            "virt",
            "-cpu",
            "host",
            "-accel",
            "hvf",
            "-m",
            "256M",
            "-display",
            "none",
            "-monitor",
            "none",
            "-no-reboot",
            "-serial",
            f"file:{self.serial}",
            "-qmp",
            f"unix:{self.qmp},server=on,wait=off",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={self.code}",
            "-drive",
            f"if=pflash,format=raw,file={self.vars_fd}",
            "-drive",
            f"if=none,file={self.image},format=raw,id=bb,snapshot=on",
            "-device",
            "qemu-xhci",
            "-device",
            "usb-storage,drive=bb",
            "-global",
            "virtio-mmio.force-legacy=false",
            "-drive",
            f"if=none,id=drv0,format=raw,file={self.data},cache.writeback=off",
            "-device",
            "virtio-blk-device,drive=drv0",
            "-device",
            "virtio-tablet-device,id=tab0",
            "-device",
            "virtio-keyboard-device,id=kbd0",
            "-device",
            "virtio-gpu-device,id=vgpu0,xres=1080,yres=1920",
            "-device",
            "ramfb",
            "-fw_cfg",
            "name=opt/berry/display,string=1080x1920@60",
        ]

    def text(self) -> str:
        return self.serial.read_text(errors="ignore") if self.serial.exists() else ""

    def wait(self, marker: str, timeout: float | None = None) -> None:
        timeout = self.timeout if timeout is None else timeout
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if marker in self.text():
                return
            if self.proc.poll() is not None:
                raise ProductE2EError(
                    f"QEMU exited rc={self.proc.returncode} before {marker!r}; "
                    f"serial={self.serial}, qemu_log={self.qemu_log}"
                )
            time.sleep(0.05)
        raise TimeoutError(
            f"marker {marker!r} absent after {timeout:.1f}s; "
            f"serial={self.serial}, qemu_log={self.qemu_log}"
        )

    def connect(self):
        if self.client is None:
            self.client = self.gui8.QmpClient(self.qmp)
        return self.client

    def shot(self, name: str) -> pathlib.Path:
        path = self.work / f"{name}.ppm"
        self.connect().execute(
            "screendump",
            {"filename": str(path), "device": "vgpu0"},
        )
        return path

    def tap(self, x: int, y: int) -> None:
        client = self.connect()
        ax = round(x * 32767 / WIDTH)
        ay = round(y * 32767 / HEIGHT)
        client.execute(
            "input-send-event",
            {
                "events": [
                    {"type": "abs", "data": {"axis": "x", "value": ax}},
                    {"type": "abs", "data": {"axis": "y", "value": ay}},
                ]
            },
        )
        time.sleep(0.12)
        client.execute(
            "input-send-event",
            {
                "events": [
                    {
                        "type": "btn",
                        "data": {"down": True, "button": "left"},
                    },
                    {"type": "abs", "data": {"axis": "x", "value": ax}},
                    {"type": "abs", "data": {"axis": "y", "value": ay}},
                ]
            },
        )
        time.sleep(0.15)
        client.execute(
            "input-send-event",
            {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]},
        )

    def key(self, code: int) -> None:
        client = self.connect()
        for down in (True, False):
            client.execute(
                "input-send-event",
                {
                    "events": [
                        {
                            "type": "key",
                            "data": {
                                "down": down,
                                "key": {"type": "number", "data": code},
                            },
                        }
                    ]
                },
            )
            if down:
                time.sleep(0.12)

    def verify_product_milestones(self) -> None:
        text = self.text()
        got = [
            int(value)
            for value in re.findall(r"VGPUBOOT ok [^\n]*? m=([0-9]+)", text)
        ]
        expected = list(range(1, 12))
        if got != expected or "VGPUBOOT fail" in text:
            raise ProductE2EError(
                f"product VGPUBOOT chain={got}, expected={expected}; "
                f"serial={self.serial}"
            )

    @staticmethod
    def pixels(path: pathlib.Path) -> bytes:
        raw = path.read_bytes()
        # P6 header has three newline-terminated/comment-separated lines.
        pos = raw.index(b"\n", raw.index(b"\n", raw.index(b"\n") + 1) + 1) + 1
        return raw[pos:]

    @classmethod
    def hist(cls, path: pathlib.Path) -> Counter[tuple[int, int, int]]:
        pixels = cls.pixels(path)
        return Counter(
            tuple(pixels[index : index + 3])
            for index in range(0, len(pixels) - 2, 3)
        )

    @classmethod
    def recents_card_center(cls, path: pathlib.Path) -> tuple[int, int, tuple[int, int]]:
        pixels = cls.pixels(path)
        target = (36, 44, 54)
        points: list[tuple[int, int]] = []
        for y in range(HEIGHT):
            row = y * WIDTH * 3
            for x in range(WIDTH):
                if tuple(pixels[row + x * 3 : row + x * 3 + 3]) == target:
                    points.append((x, y))
                    break
        if not points:
            raise ProductE2EError(f"no Recents card pixels in {path}")
        x0 = min(x for x, _ in points)
        x1 = max(x for x, _ in points)
        y0 = min(y for _, y in points)
        y1 = max(y for _, y in points)
        return ((x0 + x1) // 2, (y0 + y1) // 2, (x1 - x0 + 1, y1 - y0 + 1))

    def stop(self) -> None:
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(3)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self._log_handle.close()

    def __enter__(self) -> "ProductRig":
        return self

    def __exit__(self, exc_type, exc, traceback) -> None:
        self.stop()


def wait_boot_and_desktop(rig: ProductRig) -> tuple[pathlib.Path, pathlib.Path]:
    rig.wait("VGPUBOOT ok stage=5 permille=700", 240)
    # The serial marker follows the resource flush, but QEMU's screendump arm
    # can still capture the prior scanout under host contention. Retry until
    # the presented 70% face is observable instead of turning latency into a
    # false regression.
    progress = rig.shot("progress-70")
    progress_deadline = time.monotonic() + 5.0
    while True:
        histogram = rig.hist(progress)
        if (
            histogram[(0, 0, 0)] > 2_000_000
            and histogram[(255, 255, 255)] > 10_000
        ):
            break
        if time.monotonic() >= progress_deadline:
            raise ProductE2EError(
                f"70% progress face not presented: black={histogram[(0, 0, 0)]} "
                f"white={histogram[(255, 255, 255)]}, screenshot={progress}"
            )
        time.sleep(0.2)
        progress = rig.shot("progress-70")
    rig.wait("VGPUBOOT ok stage=5 permille=1000", 180)
    rig.wait("LAUNCH ok", 180)
    rig.verify_product_milestones()
    time.sleep(2)
    desktop = rig.shot("desktop")
    return progress, desktop


def print_checks(name: str, checks: dict[str, bool]) -> None:
    print(f"{name.upper()}_CHECKS {json.dumps(checks, sort_keys=True)}")
    if not all(checks.values()):
        raise ProductE2EError(f"{name} pixel/marker checks failed: {checks}")


def run_recents(rig: ProductRig) -> None:
    progress, desktop = wait_boot_and_desktop(rig)
    rig.tap(412, 282)
    rig.wait("BERRYDEMO up", 180)
    rig.wait("BERRYDEMOFRAME ok", 180)
    rig.wait("WMAPPFRAME ok owner=2", 240)
    time.sleep(0.5)
    app = rig.shot("app-first-frame")

    # The persistent bottom SYSTEM reserve maps right third to TASK, left to
    # BACK, and middle to HOME. Use the same reserve end to end.
    rig.tap(900, 1870)
    rig.wait("SHELLNAV ok action=3", 120)
    rig.wait("WMRECENTS ok id=", 120)
    time.sleep(2)
    recents = rig.shot("recents")
    card_x, card_y, card_size = rig.recents_card_center(recents)
    print("RECENTS_CARD", card_x, card_y, card_size)
    rig.tap(card_x, card_y)
    rig.wait("WMTASKSEL ok owner=2", 120)
    time.sleep(1)
    selected = rig.shot("selected")
    rig.tap(540, 1870)
    rig.wait("SHELLNAV ok action=2", 120)
    time.sleep(1)
    home = rig.shot("home")

    histograms = list(map(rig.hist, [progress, desktop, app, recents, selected]))
    progress_h, desktop_h, app_h, recents_h, selected_h = histograms
    checks = {
        "progress_black": progress_h[(0, 0, 0)] > 2_000_000,
        "progress_white": progress_h[(255, 255, 255)] > 10_000,
        "desktop": sum(desktop_h.values()) > 0 and desktop_h[(0, 0, 0)] < 1_000_000,
        "app_first_frame": sum(app_h.values()) > 0 and app_h[(0, 0, 0)] < 1_000_000,
        "recents_visible": recents_h[(18, 22, 28)] > 1_000_000
        and recents_h[(36, 44, 54)] > 100_000,
        "selected_restored": sum(selected_h.values()) > 0
        and selected_h[(0, 0, 0)] < 1_000_000,
    }
    print_checks("recents", checks)
    print("RECENTS_TOP", recents_h.most_common(8))
    print("APP_TOP", app_h.most_common(6))
    print("SELECTED_TOP", selected_h.most_common(6))

    text = rig.text()
    markers = {
        key: text.count(value)
        for key, value in {
            "BOOTPROG": "BOOTPROG ok display 1080x1920",
            "VGPU": "VGPU ok",
            "BOOT100": "VGPUBOOT ok stage=5 permille=1000",
            "LAUNCH": "LAUNCH ok",
            "APP": "BERRYDEMO up",
            "APPFRAME": "WMAPPFRAME ok owner=2",
            "TASK": "SHELLNAV ok action=3",
            "RECENTS": "WMRECENTS ok id=",
            "TASKSEL": "WMTASKSEL ok owner=2",
            "HOME": "SHELLNAV ok action=2",
            "RECENTS_PRESS_ON": "SHELLPRESS ok surface=recents index=0 pressed=true",
            "RECENTS_PRESS_OFF": "SHELLPRESS ok surface=recents index=0 pressed=false",
            "PANIC": "KERNEL PANIC",
        }.items()
    }
    print("RECENTS_MARKERS", json.dumps(markers, sort_keys=True))
    expected = {
        "BOOTPROG": 1,
        "VGPU": 1,
        "BOOT100": 1,
        "LAUNCH": 1,
        "APP": 1,
        "APPFRAME": 1,
        "TASK": 1,
        "RECENTS": 1,
        "TASKSEL": 1,
        "HOME": 1,
        "RECENTS_PRESS_ON": 1,
        "RECENTS_PRESS_OFF": 2,
        "PANIC": 0,
    }
    if markers != expected:
        raise ProductE2EError(f"recents marker mismatch: got={markers}, want={expected}")
    print("RECENTS PASS")


def run_frame_cache(rig: ProductRig) -> None:
    progress, desktop = wait_boot_and_desktop(rig)
    rig.tap(412, 282)
    rig.wait("BERRYDEMO up", 180)
    rig.wait("BERRYDEMOFRAME ok", 180)
    rig.wait("WMAPPFRAME ok owner=2", 240)
    time.sleep(0.3)
    app1 = rig.shot("app1-berrydemo")

    rig.key(60)
    rig.wait("WMNAV ok home owner=1", 120)
    time.sleep(0.8)
    rig.tap(665, 1668)
    rig.wait("SETTINGS up", 180)
    rig.wait("SETTINGSFRAME ok", 180)
    rig.wait("WMAPPFRAME ok owner=3", 240)
    time.sleep(0.4)
    app2 = rig.shot("app2-settings")

    rig.key(61)
    rig.wait("WMNAV ok task owner=2", 120)
    rig.wait("WMRECENTS ok id=", 120)
    rig.wait("WMCAPTURE ok owner=3", 120)
    time.sleep(0.8)
    recents = rig.shot("recents-two")
    rig.tap(540, 473)
    rig.wait("WMTASKSEL ok owner=2", 120)
    rig.wait("WMREPLAY ok owner=2", 120)
    time.sleep(0.8)
    selected = rig.shot("selected-berrydemo-from-cache")

    histograms = list(
        map(rig.hist, [progress, desktop, app1, app2, recents, selected])
    )
    progress_h, desktop_h, app1_h, app2_h, recents_h, selected_h = histograms
    checks = {
        "progress_black": progress_h[(0, 0, 0)] > 2_000_000,
        "progress_white": progress_h[(255, 255, 255)] > 10_000,
        "desktop": sum(desktop_h.values()) > 0 and desktop_h[(0, 0, 0)] < 1_000_000,
        "berrydemo_first": sum(app1_h.values()) > 0
        and app1_h[(0, 0, 0)] < 1_000_000,
        "settings_second": sum(app2_h.values()) > 0
        and app2_h[(0, 0, 0)] < 1_000_000,
        "recents_visible": recents_h[(18, 22, 28)] > 1_000_000
        and recents_h[(36, 44, 54)] > 100_000,
        "cache_replay_restores_app": sum(selected_h.values()) > 0
        and selected_h[(0, 0, 0)] < 1_000_000
        and selected_h[(18, 22, 28)] <= 260_000
        and selected_h[(36, 44, 54)] < 1_000,
    }
    print_checks("frame_cache", checks)
    print("APP1_TOP", app1_h.most_common(5))
    print("APP2_TOP", app2_h.most_common(5))
    print("RECENTS_TOP", recents_h.most_common(6))
    print("SELECTED_TOP", selected_h.most_common(6))

    text = rig.text()
    markers = {
        key: text.count(value)
        for key, value in {
            "BOOTPROG": "BOOTPROG ok display 1080x1920",
            "VGPU": "VGPU ok",
            "BOOT100": "VGPUBOOT ok stage=5 permille=1000",
            "LAUNCH": "LAUNCH ok",
            "BERRYDEMO": "BERRYDEMO up",
            "SETTINGS": "SETTINGS up",
            "APPFRAME2": "WMAPPFRAME ok owner=2",
            "APPFRAME3": "WMAPPFRAME ok owner=3",
            "HOME": "WMNAV ok home owner=1",
            "TASK": "WMNAV ok task owner=2",
            "RECENTS": "WMRECENTS ok id=",
            "CAPTURE3": "WMCAPTURE ok owner=3",
            "TASKSEL2": "WMTASKSEL ok owner=2",
            "REPLAY2": "WMREPLAY ok owner=2",
            "PANIC": "KERNEL PANIC",
        }.items()
    }
    print("FRAME_CACHE_MARKERS", json.dumps(markers, sort_keys=True))
    if markers["PANIC"] != 0 or any(
        count < 1 for key, count in markers.items() if key != "PANIC"
    ):
        raise ProductE2EError(f"frame-cache marker mismatch: {markers}")
    print("FRAME-CACHE PASS")


def run_shell_press(rig: ProductRig) -> None:
    """Pin the persistent dock's paint-only press feedback and real launch."""
    wait_boot_and_desktop(rig)
    client = rig.connect()
    time.sleep(0.3)
    baseline = rig.shot("dock-baseline")

    # Product constants from desktop.rs: dock row y=1551, slot 0 is 251px
    # wide. Center (125,1668) hits the Contacts dock face.
    client.pointer_down(125, 1668, WIDTH, HEIGHT)
    rig.wait("SHELLPRESS ok surface=dock index=0 pressed=true", 30)
    pressed = rig.shot("dock-pressed")
    deadline = time.monotonic() + 5.0
    while True:
        # The Argb dock face shifts by 2px. Compare exact pixels rather than
        # color histograms, whose counts are unchanged by translation.
        before = ProductRig.pixels(baseline)
        after = ProductRig.pixels(pressed)
        differences = 0
        for y in range(1450, 1800):
            row = y * WIDTH * 3
            for x in range(0, 300):
                index = row + x * 3
                if before[index : index + 3] != after[index : index + 3]:
                    differences += 1
        if differences > 100:
            break
        if time.monotonic() >= deadline:
            raise ProductE2EError(
                f"dock press produced no visible pixel shift: diff={differences}, "
                f"screenshot={pressed}"
            )
        time.sleep(0.2)
        pressed = rig.shot("dock-pressed")

    client.pointer_up()
    rig.wait("SHELLPRESS ok surface=dock index=0 pressed=false", 30)
    rig.wait("CONTACTS up", 180)
    rig.wait("CONTACTSFRAME ok", 180)
    rig.key(60)
    rig.wait("WMNAV ok home owner=1", 120)

    text = rig.text()
    checks = {
        "press_on": text.count(
            "SHELLPRESS ok surface=dock index=0 pressed=true"
        ) == 1,
        "press_off": text.count(
            "SHELLPRESS ok surface=dock index=0 pressed=false"
        ) == 2,
        "contacts_launched": "CONTACTS up" in text,
        "home_restored": "WMNAV ok home owner=1" in text,
        "panic": "KERNEL PANIC" not in text,
        "visible_shift": differences > 100,
    }
    print("SHELL_PRESS_CHECKS", json.dumps(checks, sort_keys=True))
    print("SHELL_PRESS_PIXEL_DIFF", differences)
    if not all(checks.values()):
        raise ProductE2EError(f"shell press checks failed: {checks}")
    print("SHELL-PRESS PASS")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--suite",
        choices=("recents", "frame-cache", "shell-press", "all"),
        default="all",
        help="product suite to run (default: all)",
    )
    parser.add_argument(
        "--image",
        type=pathlib.Path,
        default=ROOT / "out" / "berryos-desktop.img",
        help="product desktop image",
    )
    parser.add_argument(
        "--work-dir",
        type=pathlib.Path,
        default=ROOT / "out" / "product-e2e",
        help="artifact root; one subdirectory is created per suite",
    )
    parser.add_argument(
        "--code",
        type=pathlib.Path,
        default=default_edk2_code(),
        help="EDK2 AArch64 code flash",
    )
    parser.add_argument("--qemu", default="qemu-system-aarch64", help="QEMU binary")
    parser.add_argument("--timeout", type=float, default=180.0, help="marker timeout")
    parser.add_argument("--keep", action="store_true", help="reuse suite work dir")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    args.image = args.image.resolve()
    args.work_dir = args.work_dir.resolve()
    args.code = args.code.resolve()
    suites: Iterable[str]
    if args.suite == "all":
        suites = ("recents", "frame-cache", "shell-press")
    else:
        suites = (args.suite,)
    args.work_dir.mkdir(parents=True, exist_ok=True)
    for suite in suites:
        print(f"product-e2e: suite={suite} image={args.image} work={args.work_dir / suite}")
        with ProductRig(
            suite=suite,
            image=args.image,
            work_root=args.work_dir,
            code=args.code,
            qemu=args.qemu,
            timeout=args.timeout,
            keep=args.keep,
        ) as rig:
            if suite == "recents":
                run_recents(rig)
            elif suite == "frame-cache":
                run_frame_cache(rig)
            else:
                run_shell_press(rig)
    print("PRODUCT E2E PASS")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ProductE2EError, TimeoutError, subprocess.CalledProcessError) as error:
        print(f"product-e2e: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
