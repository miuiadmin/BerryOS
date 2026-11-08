#!/usr/bin/env python3
"""调度器 CI 一管到底——build → QEMU boot → 遥测分析 → 基线对拍，一次跑完。

编排层不重复造轮子：构建走 cargo、启动走 QEMU、分析/验收/回归判定全部
subprocess 调 tools/sched_dashboard.py（阈值与口径单源在那边），本脚本只管
流程串联与退出码汇总。

流程（4 步）：
  1. build   cargo build -p berry-kernel（CARGO_TARGET_DIR 钉 $PWD/target，
            防全局 ~/.cargo/config.toml target-dir 覆盖——tools/paths.sh 同注）
  2. boot    qemu-system-aarch64 virt 串口 → /tmp/sched_ci_serial.log
            （等价 `timeout N qemu … | tee log`，Python 内实现——macOS 无
            GNU timeout 亦可跑；内核 ktest 后常驻不退，**超时收杀是正常
            路径，超时≠失败**，成败只看 ktest done 是否打出来）
  3. check   串口日志 grep "ktest done"——缺席 = FAIL
  4. analyze python3 tools/sched_dashboard.py < log（验收 3/3 门）
            - 无基线 → 报告「首次运行——用 --update 建基线」（不影响判定）
            - 有基线 → --baseline tools/sched_baseline.json（回归门）
            - --update → --save 落新基线；对旧基线的回归仍计 FAIL
              （更新不是豁免——先看清回归再落章）

用法：
  python3 tools/sched_ci.py                 # 完整 CI
  python3 tools/sched_ci.py --update        # 同上 + 保存新基线
  python3 tools/sched_ci.py --skip-build    # 跳过构建（沿用已有 binary）
  python3 tools/sched_ci.py --timeout 60    # QEMU 超时秒（默认 45；TCG 慢机上调）
  python3 tools/sched_ci.py --demo          # 样例数据走分析+基线（不 build/QEMU）

基线 tools/sched_baseline.json：**git track 不 gitignore**——CI 演进可追溯，
--update 产出后随代码一起提交（每次性能演变都有 commit 对得上）。

退出码：0 = PASS（ktest done + 验收 3/3 + 无回归）；1 = FAIL。
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import threading

TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS_DIR)
DASHBOARD = os.path.join(TOOLS_DIR, "sched_dashboard.py")
BASELINE = os.path.join(TOOLS_DIR, "sched_baseline.json")   # git track（模块头注）
SERIAL_LOG = "/tmp/sched_ci_serial.log"
KERNEL = os.path.join(REPO, "target", "aarch64-unknown-none",
                      "debug", "berry-kernel")   # CARGO_TARGET_DIR 钉 $PWD/target 后确定位
QEMU = os.environ.get("QEMU", "qemu-system-aarch64")
# justfile MACH 同款（run 配方单一事实源；不带 -smp = v1 单核定值）
QEMU_ARGS = ["-machine", "virt", "-cpu", "cortex-a72", "-m", "128M", "-nographic"]
KTEST_DONE = "ktest done"
DEFAULT_TIMEOUT = 45     # 秒；TCG 慢机 ktest done 可能 >45s，--timeout 上调
STEPS = 4

sys.path.insert(0, TOOLS_DIR)
from sched_dashboard import DEMO_INPUT  # noqa: E402 —— --demo 样例遥测单源（兄弟工具 import 约定）

# demo 也走真路径：样例串口落盘 → grep → 子进程仪表盘，与实跑同一条代码道。
DEMO_SERIAL = DEMO_INPUT + KTEST_DONE + "\n"


def die(msg):
    print(f"sched-ci: FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def step(n, title):
    print(f"\n== [{n}/{STEPS}] {title}", flush=True)


# ---- 1. 构建 ----

def do_build():
    env = os.environ.copy()
    if shutil.which("cargo") is None:  # 非交互 shell 常缺 ~/.cargo/bin（paths.sh 同病）
        env["PATH"] = (os.path.expanduser("~/.cargo/bin") + os.pathsep
                       + env.get("PATH", ""))
    env["CARGO_TARGET_DIR"] = os.path.join(REPO, "target")   # 任务钉的 $PWD/target
    r = subprocess.run(["cargo", "build", "-p", "berry-kernel"], cwd=REPO, env=env)
    if r.returncode != 0:
        die(f"cargo build 失败（exit {r.returncode}）")
    if not os.path.isfile(KERNEL):
        die(f"构建成功但内核产物缺席: {KERNEL}")


# ---- 2. QEMU 启动 ----

def do_boot(timeout_s: int) -> bool:
    """串口收割 → SERIAL_LOG，返回是否超时收杀（正常路径之一）。"""
    if shutil.which(QEMU) is None:
        die(f"找不到 {QEMU}（QEMU= 环境变量可覆盖）")
    if not os.path.isfile(KERNEL):
        die(f"内核未构建: {KERNEL}（去掉 --skip-build 先构建）")
    cmd = [QEMU] + QEMU_ARGS + ["-kernel", KERNEL]
    timed_out = False
    with open(SERIAL_LOG, "wb") as logf, open(os.devnull, "rb") as null:
        p = subprocess.Popen(cmd, stdin=null, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, cwd=REPO)

        def pump():   # tee 语义：串口流式上屏 + 落盘
            for line in iter(p.stdout.readline, b""):
                logf.write(line)
                logf.flush()
                sys.stdout.buffer.write(line)
                sys.stdout.buffer.flush()

        t = threading.Thread(target=pump, daemon=True)
        t.start()
        try:
            p.wait(timeout=timeout_s)
        except subprocess.TimeoutExpired:
            timed_out = True   # 内核常驻不退——帽到收杀，ktest done 见日志即可
            p.kill()
            p.wait()
        except BaseException:  # Ctrl-C 等：不留活 QEMU
            p.kill()
            p.wait()
            raise
        t.join(timeout=3)
    return timed_out


# ---- 3. ktest done 检查 ----

def ktest_done_seen() -> bool:
    with open(SERIAL_LOG, "r", errors="replace") as f:
        return KTEST_DONE in f.read()


# ---- 4. 分析 + 基线（判定口径单源 = sched_dashboard.py） ----

def do_analyze(update: bool, have_base: bool) -> int:
    """子进程跑仪表盘 → 退出码（0 = 验收 3/3 且无回归）。"""
    args = [sys.executable, DASHBOARD]
    old = None
    if have_base and update:
        # 陷阱：dashboard 内 --save 先落盘、--baseline 后读盘——同一路径会
        # 自比恒「稳」静默废掉回归门。旧基线先挪临时文件对拍，真身留给 --save。
        fd, old = tempfile.mkstemp(prefix="sched_baseline_old.", suffix=".json")
        os.close(fd)
        shutil.copyfile(BASELINE, old)
        args += ["--baseline", old, "--save", BASELINE]
    elif have_base:
        args += ["--baseline", BASELINE]
    elif update:
        args += ["--save", BASELINE]
    with open(SERIAL_LOG, "r", errors="replace") as logf:
        rc = subprocess.run(args, stdin=logf).returncode
    if old:
        os.unlink(old)
    return rc


def main() -> int:
    ap = argparse.ArgumentParser(
        description="BerryOS 调度器 CI 一管到底（build → boot → analyze → 基线对拍）")
    ap.add_argument("--update", action="store_true",
                    help="跑完把本次遥测存为新基线 tools/sched_baseline.json"
                         "（对旧基线的回归仍计 FAIL）")
    ap.add_argument("--skip-build", action="store_true",
                    help="跳过构建（沿用已有 binary；--demo 下无效果）")
    ap.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT, metavar="SEC",
                    help=f"QEMU 串口收割超时秒数（默认 {DEFAULT_TIMEOUT}）")
    ap.add_argument("--demo", action="store_true",
                    help="跳过 build/QEMU，内置样例数据走分析+基线全流程")
    args = ap.parse_args()

    if args.demo:
        print("sched-ci: demo 模式——内置样例串口（跳过 build/QEMU，"
              "分析+基线与实跑同一条代码道）")
        with open(SERIAL_LOG, "w") as f:
            f.write(DEMO_SERIAL)
    else:
        if args.skip_build:
            step(1, "build: 跳过（--skip-build）")
            if not os.path.isfile(KERNEL):
                die(f"内核未构建: {KERNEL}")
        else:
            step(1, "build: cargo build -p berry-kernel（CARGO_TARGET_DIR=target）")
            do_build()
        step(2, f"boot: {QEMU} virt，{args.timeout}s 帽 → {SERIAL_LOG}")
        if do_boot(args.timeout):
            print(f"sched-ci: {args.timeout}s 帽到，收杀 QEMU"
                  "（正常——内核 ktest 后常驻不退）")

    step(3, f"check: grep '{KTEST_DONE}' ← {SERIAL_LOG}")
    if not ktest_done_seen():
        die(f"串口未见 '{KTEST_DONE}'（ktest 未跑完/未跑；日志留档 {SERIAL_LOG}）")
    print(f"'{KTEST_DONE}' ✓")

    have_base = os.path.isfile(BASELINE)
    modes = (" --baseline" if have_base else "") + (" --save" if args.update else "")
    step(4, f"analyze: sched_dashboard.py{modes} < {SERIAL_LOG}")
    rc = do_analyze(args.update, have_base)
    if not have_base and not args.update:
        print("首次运行——用 --update 建基线")

    if rc != 0:
        die("仪表盘门未过（验收 3/3 / 回归 / 遥测行——见上表）")
    gate = ("基线已更新" if args.update
            else "无回归" if have_base else "首次运行未建基线")
    print(f"\nsched-ci: PASS —— ktest done ✓ + 验收 3/3 + {gate}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
