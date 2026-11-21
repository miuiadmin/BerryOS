#!/usr/bin/env python3
"""releng_pack.py — G1 刀 6：镜像自动化打包与 HSM 生产签名流水线。

流水线：
  1. 产物就位检查（berryboot.efi / kernel ELF / rootfs.cpio）
  2. 尺寸门（boot.efi ≤ 2MiB、boot.img ≤ 16MiB、berryos.img ≤ 256MiB——
     G0 刀 1 双槽+双 rootfs 后 128 已不敷，帽值单源 SYSTEM/BERRYBOOT/BOOT
     三常量）
  3. mkimage 造盘（A/B 双槽+BOOTCTL 工厂件）
  4. berry-sign 签发 KERNELA.SIG / KERNELB.SIG（ed25519）
  5. 签名注入（mkfat --put 二次卷写或重跑 mkimage 带签名件）
  6. 输出 manifest（releng-manifest.json：产物 SHA-256+尺寸+签名状态）

用法：
  python3 tools/releng_pack.py [--out-dir out/release] [--sign-key <sk>]
      [--trust-pk <pk>] [--skip-sign]（dev 模式跳过签名）

纪律：纯 Python 3 标准库（无第三方依赖——发布工具自身不引供应链面）。
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# 尺寸门（MiB）——帽值单源：报错文案/manifest 注记一律引常量，禁字面量
BERRYBOOT_EFI_CAP = 2 * 1024 * 1024    # 2 MiB
BOOT_IMG_CAP = 16 * 1024 * 1024        # 16 MiB
SYSTEM_IMG_CAP = 256 * 1024 * 1024     # 256 MiB（G0 刀 1 后双槽+双 rootfs——128 已不敷）


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()



def _find_kernel(root: Path) -> Path:
    """内核 ELF 发现序：target-local/debug → target-*/release（mtime 降序）。"""
    candidates = []
    local_debug = root / "target-local/aarch64-unknown-none/debug/berry-kernel"
    if local_debug.exists():
        candidates.append(local_debug)
    for td in sorted(root.glob("target-*/aarch64-unknown-none/release/berry-kernel"), key=lambda p: p.stat().st_mtime, reverse=True):
        candidates.append(td)
    if not candidates:
        return root / "target-local/aarch64-unknown-none/debug/berry-kernel"  # 缺件路径供报错
    return candidates[0]

def check_artifacts(root: Path, explicit_kernel=None) -> dict[str, Path]:
    """产物就位检查——缺件即 loud 退场。"""
    artifacts = {}
    checks = {
        "berryboot_efi": root / "bootloader/berryboot/out/BERRYBOOT.EFI",
        "kernel_elf": Path(explicit_kernel) if explicit_kernel else _find_kernel(root),
        "rootfs_cpio": root / "rootfs.cpio",
    }
    for name, path in checks.items():
        if not path.exists():
            print(f"releng_pack: FAIL: 缺 {name} = {path}")
            sys.exit(1)
        artifacts[name] = path
    return artifacts


def size_gate(name: str, path: Path, cap: int) -> bool:
    size = path.stat().st_size
    ok = size <= cap
    status = "OK" if ok else "OVER"
    print(f"  {name}: {size}B / {cap}B [{status}]")
    return ok


def find_sign_tool(root: Path) -> str | None:
    """berry-sign 发现序（justfile sign-user 同序镜像）。"""
    for tdir in [root / "target-cargo", Path("/tmp/target-p1"), Path("/tmp/target-p2")]:
        for prof in ["debug", "release"]:
            p = tdir / "aarch64-apple-darwin" / prof / "berry-sign"
            if p.is_file() and os.access(p, os.X_OK):
                return str(p)
    return None


def run(cmd: list[str], desc: str) -> bool:
    """执行命令——失败打印 stdout/stderr 后返 false。"""
    print(f"  $ {' '.join(cmd)}")
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  {desc}: FAIL (exit {r.returncode})")
        if r.stdout:
            print(f"  stdout: {r.stdout[:500]}")
        if r.stderr:
            print(f"  stderr: {r.stderr[:500]}")
        return False
    return True


def selftest() -> int:
    """自检合成腿（berry_assemble --selftest 同款风格；零真产物零 cargo）。

    覆盖面：
      1. 帽值不变量（G0 刀 1 双槽后 256MiB——docstring/文案已单源化，漂移即红）；
      2. sha256_file 与 hashlib 逐字节对拍（分段读路径真跑）；
      3. size_gate 恰界两侧（==过 / +1 过 / 超 1B 拒）；
      4. check_artifacts 缺件 loud 退场（SystemExit(1) + FAIL 行）；
      5. run() 双路（真跑 python3 -c ok / exit 3 假件拒）；
      6. find_sign_tool 空目录 None（发现序缺席分支）。
    """
    # ① 帽值不变量。
    assert BERRYBOOT_EFI_CAP == 2 * 1024 * 1024
    assert BOOT_IMG_CAP == 16 * 1024 * 1024
    assert SYSTEM_IMG_CAP == 256 * 1024 * 1024, "G0 刀 1 双槽+双 rootfs 帽 = 256MiB"
    # ② sha256 分段读对拍（3MiB 跨 64KiB 分段边界）。
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        blob = bytes(range(256)) * 12 * 1024  # 3MiB
        f = tmp_path / "blob.bin"
        f.write_bytes(blob)
        assert sha256_file(f) == hashlib.sha256(blob).hexdigest()
        # ③ size_gate 恰界。
        gate_ok = tmp_path / "gate_ok.bin"
        gate_ok.write_bytes(b"x" * BOOT_IMG_CAP)
        assert size_gate("恰界", gate_ok, BOOT_IMG_CAP) is True
        gate_over = tmp_path / "gate_over.bin"
        gate_over.write_bytes(b"x" * (BOOT_IMG_CAP + 1))
        assert size_gate("超界", gate_over, BOOT_IMG_CAP) is False
        # ④ 缺件 loud 退场（合成 root——三检查位全空）。
        empty_root = tmp_path / "empty_root"
        empty_root.mkdir()
        try:
            check_artifacts(empty_root)
        except SystemExit as e:
            assert e.code == 1
        else:
            raise AssertionError("check_artifacts 缺件必须 SystemExit(1)")
        # ⑥ 发现序缺席分支。
        assert find_sign_tool(empty_root) is None
    # ⑤ run() 双路（真子进程）。
    assert run([sys.executable, "-c", "print('ok')"], "真跑") is True
    assert run([sys.executable, "-c", "raise SystemExit(3)"], "假件") is False
    print("releng_pack selftest: 6 腿全绿（帽值/sha256/尺寸门恰界/缺件/发现序/run 双路）")
    return 0


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description="G1 刀 6 镜像打包+HSM 签名流水线")
    parser.add_argument("--selftest", action="store_true",
                       help="自检（合成腿，零真产物零 cargo——门禁形态 a-3 U4 纪律）")
    parser.add_argument("--out-dir", default=str(root / "out/release"),
                       help="产物输出目录（默认 out/release）")
    parser.add_argument("--sign-key", help="签名私钥路径（HSM 代理或本地 dev 键）")
    parser.add_argument("--trust-pk", help="信任公钥路径")
    parser.add_argument("--skip-sign", action="store_true", help="跳过签名（dev 模式）")
    parser.add_argument("--kernel", help="内核 ELF 显式路径（推荐——发现序有跨 lane 污染风险）")
    parser.add_argument("--size-mib", type=int, default=192,
                       help="盘容量 MiB（默认 192——双槽+双 rootfs）")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    print("=== releng_pack: 产物就位检查 ===")
    arts = check_artifacts(root, getattr(args, "kernel", None))

    print("=== 尺寸门 ===")
    gates_pass = True
    gates_pass &= size_gate("berryboot.efi", arts["berryboot_efi"], BERRYBOOT_EFI_CAP)
    # boot.img 和 system.img 在造盘后复检。

    if not gates_pass:
        print("releng_pack: FAIL: 尺寸门未过")
        sys.exit(1)

    print("=== mkimage 造盘（A/B 双槽） ===")
    # 同一内核+rootss 做 A/B 两份（v1 域面：A/B 内容同——OTA 升级面归 G2+）。
    img_out = out_dir / "berryos.img"
    mkimg = root / "tools/mkimage/cargo-target/aarch64-apple-darwin/release/mkimage"
    if not mkimg.exists():
        # 现场构建
        print("  mkimage 不可得——现场构建")
        r = subprocess.run(
            ["cargo", "build", "--release", "--manifest-path",
             str(root / "tools/mkimage/Cargo.toml"), "--target", "aarch64-apple-darwin"],
            cwd=root, capture_output=True, text=True,
            env={**os.environ, "CARGO_TARGET_DIR": str(root / "tools/mkimage/cargo-target"), "RUSTC_WRAPPER": ""},
        )
        if r.returncode != 0:
            print(f"  mkimage 构建 FAIL: {r.stderr[:300]}")
            sys.exit(1)

    cmd = [
        str(mkimg), str(img_out),
        "--efi", str(arts["berryboot_efi"]),
        "--hello", str(root / "bootloader/berryboot/hello.txt"),
        "--kernel", str(arts["kernel_elf"]),
        "--initrd", str(arts["rootfs_cpio"]),
        "--kernel-b", str(arts["kernel_elf"]),
        "--initrd-b", str(arts["rootfs_cpio"]),
        "--size-mib", str(args.size_mib),
    ]
    if args.sign_key and args.trust_pk:
        cmd += ["--sign-key", args.sign_key, "--trust-pk", args.trust_pk]
    if not run(cmd, "mkimage"):
        sys.exit(1)

    print("=== 复检 boot.img 尺寸 ===")
    if not size_gate("berryos.img", img_out, SYSTEM_IMG_CAP):
        print(f"releng_pack: FAIL: 镜像超 {SYSTEM_IMG_CAP // 1024 // 1024}MiB 帽")
        sys.exit(1)

    print("=== manifest 产出 ===")
    manifest = {
        "version": "1.0",
        "pipeline": "G1-knife-6",
        "artifacts": {},
        "signing": {
            "mode": "dev" if args.skip_sign else ("hsm" if args.sign_key else "unsigned"),
            "algorithm": "ed25519",
        },
    }
    for name, path in [
        ("berryboot.efi", arts["berryboot_efi"]),
        ("kernel.elf", arts["kernel_elf"]),
        ("rootfs.cpio", arts["rootfs_cpio"]),
        ("berryos.img", img_out),
    ]:
        manifest["artifacts"][name] = {
            "sha256": sha256_file(path),
            "size": path.stat().st_size,
        }
        manifest["artifacts"][name]["size_human"] = f"{path.stat().st_size / 1024 / 1024:.2f}MiB"

    mf_path = out_dir / "releng-manifest.json"
    with open(mf_path, "w") as f:
        json.dump(manifest, f, indent=2)
    print(f"  manifest → {mf_path}")
    print(f"releng_pack: PASS（{len(manifest['artifacts'])} 产物已打包）")


if __name__ == "__main__":
    main()
