#!/usr/bin/env python3
"""make_desktop_disk——确定性 1080p 产品桌面数据盘。

默认 mkfat FAT12 数据盘 + LBA0 vblk 印章 + LAYOUT.JSN：首页 Browser 与
Berry Demo，MMS/Dock 位过滤后保留占位洞，第二页 Files；Contacts/Photos/
Settings/MMS 由 launcher SmartDock 常驻提供。产物字节确定性，可重复重建。
"""
from __future__ import annotations

import argparse
import importlib.util
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DISK_SEAL = b"BERRYOSVBLK!"
LAYOUT_NAME11 = b"LAYOUT  JSN"
LAYOUT = (
    b'{"schema":1,"cells":['
    b'{"app":"BROWSER","page":0,"row":0,"col":0},'
    b'{"app":"BERRYDEMO","page":0,"row":0,"col":1},'
    b'{"app":"MMS","page":0,"row":0,"col":2},'
    b'{"app":"FILES","page":1,"row":0,"col":0},'
    b'{"app":"APPDEMO","page":1,"row":0,"col":1}]}'
)


def load_gui8():
    path = ROOT / "tools" / "gui8-smoke.py"
    spec = importlib.util.spec_from_file_location("berry_gui8_smoke", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load gui8-smoke.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def make(path: Path) -> None:
    path.unlink(missing_ok=True)
    proc = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "mkfat.py"), str(path)],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or proc.stdout.strip())
    with path.open("r+b") as f:
        f.seek(0)
        f.write(DISK_SEAL)
    gui8 = load_gui8()
    gui8.Fat12Volume(path).add_file(LAYOUT_NAME11, LAYOUT)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("output", type=Path)
    args = ap.parse_args()
    make(args.output)
    print(f"desktop-disk: {args.output} {args.output.stat().st_size}B layout={len(LAYOUT)}B")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # recipe 显式失败，不吞错。
        print(f"desktop-disk: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
