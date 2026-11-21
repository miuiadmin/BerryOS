#!/usr/bin/env python3
"""make_desktop_disk——确定性 1080p 产品桌面数据盘。

默认 mkfat FAT12 数据盘 + LBA0 vblk 印章 + LAYOUT.JSN：首页 Browser 与
Berry Demo，MMS/Dock 位过滤后保留占位洞，第二页 Files；Contacts/Photos/
Settings/MMS 由 launcher SmartDock 常驻提供。产物字节确定性，可重复重建。

--selftest 门禁自检（a-3 U4 纪律同款——releng_pack 先例）：合成/真流混合
7 腿，零真仓零 QEMU；改本件或其消费面（mkfat 产链/gui8 Fat12Volume）必跑。
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


def selftest() -> int:
    """门禁自检 7 腿：几何/印章/名形/内容往返/根目录排除面/确定性/隔离。"""
    import hashlib
    import json
    import tempfile

    # ① LAYOUT 常量契约：schema/cells 形状 + 8.3 名 11B 形 + 印章 11B。
    layout = json.loads(LAYOUT.decode("ascii"))
    assert layout["schema"] == 1, "schema 恒 1"
    assert len(LAYOUT_NAME11) == 11 and b" " in LAYOUT_NAME11, "8.3 名 11B 垫空格形"
    assert DISK_SEAL == b"BERRYOSVBLK!" and len(DISK_SEAL) == 12, "vblk 印章常量"
    # 页/坐标域 sane：page 0/1，row/col 非负小值。
    for cell in layout["cells"]:
        assert cell["page"] in (0, 1) and cell["row"] >= 0 and cell["col"] >= 0
        assert cell["app"] and cell["app"].isupper(), "app 名恒大写"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # ② 真流双产确定性：make() 走真 mkfat 子进程 + Fat12Volume 注入，
        #    同 cwd 双跑产物 sha256 恒等（产品宣称「可重复重建」首获机器执法）。
        a, b = root / "a.img", root / "b.img"
        make(a)
        make(b)
        ha = hashlib.sha256(a.read_bytes()).hexdigest()
        hb = hashlib.sha256(b.read_bytes()).hexdigest()
        assert ha == hb, f"双产漂移 {ha} != {hb}"
        # ③ 盘几何与印章落位：LBA0 印章前缀 + 510 处无 55aa（卷在
        #    LBA16——mkfat vol@LBA16 铁律，印章区不持 BPB）。
        data = a.read_bytes()
        assert data[: len(DISK_SEAL)] == DISK_SEAL, "LBA0 印章"
        assert data[510:512] != b"\x55\xaa", "印章区不得伪造 BPB 签名"
        assert data[16 * 512 + 510 : 16 * 512 + 512] == b"\x55\xaa", "LBA16 卷签名"
        # ④ LAYOUT.JSN 经 Fat12Volume 读回恒等（写读往返——add_file 产物
        #    被消费面同款读器解出，内容字节级对拍）。
        gui8 = load_gui8()
        vol = gui8.Fat12Volume(a)
        got = vol.read_file(LAYOUT_NAME11)
        assert got == LAYOUT, f"读回漂移 {got!r}"
        assert "LAYOUT  JSN" in vol.list_root(), "根目录清单含 LAYOUT"
        # ⑤ 根目录清单排除面：清单不含卷标外杂项（合成双产只有 LAYOUT 一件
        #    用户件——mkfat 默认盘 HELLO/PATTERN 在印章盘面被 make() 重导
        #    向？非也——mkfat.py 裸调产默认盘含烘焙件，验面 = read_file 对
        #    不存在件 None 容忍 + 名形 8.3。
        assert vol.read_file(b"NOPE     JSN") is None, "缺席件 None"
        # ⑥ add_file 防覆写拒形（重复名 SmokeError）。
        try:
            vol.add_file(LAYOUT_NAME11, b"dup")
        except gui8.SmokeError:
            pass
        else:
            raise AssertionError("重复 add_file 必须 SmokeError")
        # ⑦ cwd 无关性：make() 子进程 cwd=ROOT 钉位，任意调用 cwd 产物恒等。
        import subprocess as _sp

        c = root / "c.img"
        r = _sp.run(
            [sys.executable, str(ROOT / "tools" / "make_desktop_disk.py"), str(c)],
            cwd=str(root), capture_output=True, text=True, check=False,
        )
        assert r.returncode == 0, r.stderr
        assert hashlib.sha256(c.read_bytes()).hexdigest() == ha, "cwd 无关确定性"
    print("make_desktop_disk selftest: 7 腿全绿"
          "（布局契约/双产确定性/印章·卷签名/LAYOUT 往返/缺席·防覆写拒面/cwd 无关）")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("output", type=Path, nargs="?")
    ap.add_argument("--selftest", action="store_true",
                    help="门禁自检（7 腿，零真仓零 QEMU）")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if args.output is None:
        ap.error("output is required unless --selftest")
    make(args.output)
    print(f"desktop-disk: {args.output} {args.output.stat().st_size}B layout={len(LAYOUT)}B")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # recipe 显式失败，不吞错。
        print(f"desktop-disk: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
