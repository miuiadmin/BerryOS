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
  python3 tools/sched_ci.py --selftest      # 门禁自检（纯函数钉板，不 build/QEMU）

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


# ---- 纯函数面（--selftest 钉板位；编排逻辑的单源）----

def build_env(environ) -> dict:
    """cargo 构建环境组装（纯）：PATH 补 ~/.cargo/bin（非交互 shell 常缺，
    paths.sh 同病）+ CARGO_TARGET_DIR 钉 $PWD/target（防全局
    ~/.cargo/config.toml target-dir 覆盖）。which 查的是传入 env 的 PATH
    （不是进程 PATH）——判定与组装同源，selftest 可受控。"""
    env = dict(environ)
    if shutil.which("cargo", path=env.get("PATH")) is None:
        env["PATH"] = (os.path.expanduser("~/.cargo/bin") + os.pathsep
                       + env.get("PATH", ""))
    env["CARGO_TARGET_DIR"] = os.path.join(REPO, "target")
    return env


def boot_cmd(qemu: str, kernel: str) -> list:
    """QEMU 启动命令组装（纯）——justfile MACH 同款（run 配方单一事实源；
    不带 -smp = v1 单核定值）。"""
    return [qemu] + QEMU_ARGS + ["-kernel", kernel]


def ktest_seen(text: str) -> bool:
    """串口文本是否含 ktest done（纯）——缺席 = ktest 未跑完/未跑 = FAIL。"""
    return KTEST_DONE in text


def analyze_mode(have_base: bool, update: bool) -> str:
    """分析档位判定（纯）：返回 'baseline+save' / 'baseline' / 'save' / ''。
    baseline+save 走旧基线挪临时文件对拍——dashboard 内 --save 先落盘、
    --baseline 后读盘，同一路径会自比恒「稳」静默废掉回归门（陷阱钉板）。"""
    if have_base and update:
        return "baseline+save"
    if have_base:
        return "baseline"
    if update:
        return "save"
    return ""


def gate_label(have_base: bool, update: bool) -> str:
    """PASS 尾标签（纯）——与 analyze_mode 四态一一对应。"""
    if update:
        return "基线已更新"
    if have_base:
        return "无回归"
    return "首次运行未建基线"


def analyze_argv(mode: str, baseline: str, dashboard: str,
                 python: str, old_baseline: str = "") -> list:
    """dashboard 子进程 argv 组装（纯）——mode='baseline+save' 时旧基线
    挪临时文件对拍（挪/清理由调用方负责），真身路径留给 --save。"""
    args = [python, dashboard]
    if mode == "baseline+save":
        args += ["--baseline", old_baseline, "--save", baseline]
    elif mode == "baseline":
        args += ["--baseline", baseline]
    elif mode == "save":
        args += ["--save", baseline]
    return args


# ---- 1. 构建 ----

def do_build():
    r = subprocess.run(["cargo", "build", "-p", "berry-kernel"],
                       cwd=REPO, env=build_env(os.environ))
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
    cmd = boot_cmd(QEMU, KERNEL)
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
        return ktest_seen(f.read())


# ---- 4. 分析 + 基线（判定口径单源 = sched_dashboard.py） ----

def do_analyze(update: bool, have_base: bool) -> int:
    """子进程跑仪表盘 → 退出码（0 = 验收 3/3 且无回归）。"""
    mode = analyze_mode(have_base, update)
    old = None
    if mode == "baseline+save":
        # 陷阱：dashboard 内 --save 先落盘、--baseline 后读盘——同一路径会
        # 自比恒「稳」静默废掉回归门。旧基线先挪临时文件对拍，真身留给 --save。
        fd, old = tempfile.mkstemp(prefix="sched_baseline_old.", suffix=".json")
        os.close(fd)
        shutil.copyfile(BASELINE, old)
    args = analyze_argv(mode, BASELINE, DASHBOARD, sys.executable,
                        old_baseline=old or "")
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
    ap.add_argument("--selftest", action="store_true",
                    help="门禁自检（纯函数钉板 + demo 真路径全链，不 build/QEMU）")
    args = ap.parse_args()

    # 自引用守卫（shim_audit 同坑同防）：--selftest 必须独占——组合旗标
    # （如 --selftest --demo）若走 selftest() 则子进程嵌套无限递归；哨兵
    # 环境变量防 selftest 内子进程再触发。
    if args.selftest:
        if os.environ.get("SCHED_CI_SELFTEST") == "1" or len(sys.argv) != 2:
            print("sched-ci: --selftest 须独占使用（无组合旗标）",
                  file=sys.stderr)
            return 2
        os.environ["SCHED_CI_SELFTEST"] = "1"
        return selftest()

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
    print(f"\nsched-ci: PASS —— ktest done ✓ + 验收 3/3 + {gate_label(have_base, args.update)}")
    return 0


# ---- selftest：编排纯函数钉板 + 挪位陷阱全链实证（不 build/QEMU）----

def selftest() -> int:
    """门禁自检（a-3 U4 纪律同款）——调度 CI 编排面的纯函数钉板。

    本工具是调度器全流程判官（build → boot → dashboard 判定串联），
    编排面坏 = 判定链静默劣化：analyze_mode 档位翻面 = 回归门被废
    （--update 变豁免）；build_env 丢 CARGO_TARGET_DIR 钉 = 内核产物
    走全局 target-dir 落错位（paths.sh 同病复发）；boot_cmd 添 -smp =
    单核验收口径漂移。覆盖：
    - build_env 三律：PATH 补 ~/.cargo/bin（cargo 在 PATH 时不补）+
      CARGO_TARGET_DIR 恒钉 REPO/target（已设值也覆盖）+ 输入不变异
    - boot_cmd 钉板：QEMU_ARGS 逐项 + -kernel 尾置 + 无 -smp（v1 单核）
    - ktest_seen：demo 串口 True / 空文本 False / 前缀 "ktest don" False
    - analyze_mode/gate_label/analyze_argv 四态一一对应
    - 挪位陷阱全链（子进程真跑 dashboard，合成基线+回归遥测）：
      同路径 --save+--baseline 自比恒稳 rc=0（陷阱实证——回归静默漏放）；
      挪位对拍 rc=1（真回归被咬）；--save 落章后新基线 rc=0（更新生效）
    - exit-code 契约：未知参数/坏 --timeout rc=2 + usage；--selftest
      独占守卫（组合旗标 rc=2 立拒——自引用递归坑的防线，shim_audit 同坑）
    - 确定性：boot_cmd/analyze_argv 同输入两次相等
    """
    import json
    import subprocess as sp
    import tempfile as tf

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")

    # --- 1. build_env 三律 ---
    with tf.TemporaryDirectory() as td:
        fake_bin = os.path.join(td, "bin")
        os.mkdir(fake_bin)
        fake_cargo = os.path.join(fake_bin, "cargo")
        with open(fake_cargo, "w") as f:
            f.write("#!/bin/sh\n")
        os.chmod(fake_cargo, 0o755)
        e1 = build_env({"PATH": fake_bin, "CARGO_TARGET_DIR": "/global/override"})
        check("build_env: cargo 在 PATH 不前置 ~/.cargo/bin",
              e1["PATH"] == fake_bin, e1["PATH"])
        check("build_env: CARGO_TARGET_DIR 恒钉 REPO/target（覆盖全局设定）",
              e1["CARGO_TARGET_DIR"] == os.path.join(REPO, "target"),
              e1["CARGO_TARGET_DIR"])
        e2 = build_env({"PATH": td})
        check("build_env: cargo 缺席前置 ~/.cargo/bin",
              e2["PATH"].startswith(os.path.expanduser("~/.cargo/bin") + os.pathsep),
              e2["PATH"])
        src = {"PATH": td}
        build_env(src)
        check("build_env: 输入 dict 不变异（纯函数契约）", src == {"PATH": td},
              f"{src}")

    # --- 2. boot_cmd 钉板 ---
    cmd = boot_cmd("qemu-system-aarch64", "/k/berry-kernel")
    check("boot_cmd: argv 逐项（MACH 同款 + -kernel 尾置）",
          cmd == ["qemu-system-aarch64", "-machine", "virt", "-cpu",
                  "cortex-a72", "-m", "128M", "-nographic",
                  "-kernel", "/k/berry-kernel"], f"{cmd}")
    check("boot_cmd: 无 -smp（v1 单核定值）", "-smp" not in cmd, f"{cmd}")
    check("boot_cmd: 确定性", boot_cmd("q", "/k") == boot_cmd("q", "/k"))

    # --- 3. ktest_seen ---
    check("ktest_seen: demo 串口 True", ktest_seen(DEMO_SERIAL) is True)
    check("ktest_seen: 空文本 False", ktest_seen("") is False)
    check("ktest_seen: 前缀截断 False（'ktest don' 非子串命中）",
          ktest_seen("ktest don\n") is False)
    check("ktest_seen: 尾部无换行仍命中（子串口径）",
          ktest_seen("x\nktest done") is True)

    # --- 4. 四态纯函数族 ---
    check("analyze_mode: 四态",
          (analyze_mode(True, True), analyze_mode(True, False),
           analyze_mode(False, True), analyze_mode(False, False))
          == ("baseline+save", "baseline", "save", ""))
    check("gate_label: 四态（与 mode 一一对应）",
          (gate_label(True, True), gate_label(True, False),
           gate_label(False, True), gate_label(False, False))
          == ("基线已更新", "无回归", "基线已更新", "首次运行未建基线"))
    py, dash = sys.executable, "/x/sched_dashboard.py"
    check("analyze_argv: baseline+save 用挪位路径对拍",
          analyze_argv("baseline+save", "/b.json", dash, py,
                       old_baseline="/tmp/old.json")
          == [py, dash, "--baseline", "/tmp/old.json", "--save", "/b.json"])
    check("analyze_argv: baseline 态不传 --save",
          analyze_argv("baseline", "/b.json", dash, py)
          == [py, dash, "--baseline", "/b.json"])
    check("analyze_argv: save 态不传 --baseline（首次建章）",
          analyze_argv("save", "/b.json", dash, py)
          == [py, dash, "--save", "/b.json"])
    check("analyze_argv: 空态裸 argv（无基线无更新）",
          analyze_argv("", "/b.json", dash, py) == [py, dash])
    check("analyze_argv: 确定性",
          analyze_argv("baseline", "/b.json", dash, py)
          == analyze_argv("baseline", "/b.json", dash, py))

    # --- 5. 挪位陷阱全链（合成基线 + 回归遥测，子进程真跑 dashboard） ---
    # 遥测 = DEMO_INPUT 但 wake p50 63 → 630（+900%，远超容差 = 真回归）
    reg_text = DEMO_INPUT.replace("HDRWAKE ok p50=63", "HDRWAKE ok p50=630")
    base_json = json.dumps({"wake": {"p50": 63, "p99": 125, "n": 128}})
    with tf.TemporaryDirectory() as td:
        same = os.path.join(td, "same.json")      # 同路径自比（陷阱侧）
        brave = os.path.join(td, "brave.json")    # 挪位旧基线（对拍侧）
        with open(same, "w") as f:
            f.write(base_json)
        with open(brave, "w") as f:
            f.write(base_json)
        # 5a. 陷阱实证：--save 与 --baseline 同路径 → save 先落盘、baseline
        #     后读 = 自比恒「稳」，真回归静默漏放（rc=0）
        p1 = sp.run([py, DASHBOARD, "--save", same, "--baseline", same],
                    input=reg_text, capture_output=True, text=True, timeout=60)
        check("挪位陷阱: 同路径自比 rc=0（回归静默漏放——陷阱实证）",
              p1.returncode == 0, f"rc={p1.returncode}")
        # 5b. 挪位对拍（analyze_mode='baseline+save' 的组装）：旧基线另置
        #     → 真回归被咬（rc=1）
        saved = os.path.join(td, "saved.json")
        p2 = sp.run([py, DASHBOARD, "--baseline", brave, "--save", saved],
                    input=reg_text, capture_output=True, text=True, timeout=60)
        check("挪位对拍: 旧基线另置 rc=1（真回归被咬）",
              p2.returncode == 1, f"rc={p2.returncode}")
        # 5c. 落章生效：--save 落的新基线 = 本次遥测 → 再对拍 rc=0
        p3 = sp.run([py, DASHBOARD, "--baseline", saved],
                    input=reg_text, capture_output=True, text=True, timeout=60)
        check("落章生效: 新基线（=本次遥测）再对拍 rc=0",
              p3.returncode == 0, f"rc={p3.returncode}")

    # --- 6. exit-code 契约（argparse 口径：rc=2 + usage） ---
    p4 = sp.run([sys.executable, __file__, "--bogus"],
                capture_output=True, text=True, timeout=30)
    check("exit-code: 未知参数 rc=2 + usage",
          p4.returncode == 2 and "usage" in p4.stderr, f"rc={p4.returncode}")
    p5 = sp.run([sys.executable, __file__, "--timeout", "abc"],
                capture_output=True, text=True, timeout=30)
    check("exit-code: 坏 --timeout rc=2",
          p5.returncode == 2, f"rc={p5.returncode}")
    # --selftest 独占守卫：组合旗标（--selftest --demo）→ rc=2 立拒，
    # 不进 selftest 分支（自引用递归坑的防线——shim_audit 同坑同防）
    p6 = sp.run([sys.executable, __file__, "--selftest", "--demo"],
                capture_output=True, text=True, timeout=30)
    check("--selftest 独占守卫（组合旗标 rc=2 + 提示，不递归）",
          p6.returncode == 2 and "独占" in p6.stderr, f"rc={p6.returncode}")

    print(f"\nsched_ci selftest: {'PASS' if not failures else 'FAIL'}"
          f"{' x' + str(len(failures)) if failures else ''}")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
