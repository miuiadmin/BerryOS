#!/usr/bin/env python3
"""advisor.py —— 4-6 B 半 AI 最小演示的 host 侧脚本（F1 管线第 2 段）。

输入：BerryOS 串口日志里的 `tele:` 行（demo 任务 tele_dump 产出，F6 第一级
schema：切换分型 + 唤醒分型，环内样本口径）。输出：Rust `policy::Entry`
注入表片段——即 demo 第 ④ 步 `Box::leak` 的那张表的 advisor 版快照。

规则确定性（无随机、无时间依赖——同日志同输出，「AI 测试友好化」的机器
可复现口径）：
  1. block-share：Block 切换占环内样本 > 50% → 系统睡眠主导，给出包络内
     权重微调 +8（tag_P 合法项——demo 实际绑定的就是这条建议的形状）。
  2. tmo-share：Timeout 唤醒占环内样本 > 10% → 超时偏多，附注建议（v1
     无第二消费点，仅打印不入表——诚实缺口）。
  3. 越界探针恒发：weight +200 的 tag_D 项——内核在 apply/weight_of 双点
     夹到 ±64（包络是内核执法，F9② 单调收窄），机器断言交 probe。

注意：本脚本是**工具非关卡**（4-6 草案 F1 的「脚本 advisor」段）——演示
host 侧「遥测进、建议出」的最小闭环；运行时协议（policy_bind syscall）
随 ADR #21 实验轨道 v3 再立。task_key 是运行期表指针，脚本只能留占位
（demo 侧运行期填真值）。

用法：
  python3 tools/advisor.py <串口日志>          # 解析 tele: 行 → 打印建议 + 表片段
  python3 tools/advisor.py <串口日志> --rust   # 只输出 Rust 表片段（可 diff）
"""
import re
import sys

TELE_SW = re.compile(
    r"tele: sw total=(\d+) span=(\d+) y=(\d+) p=(\d+) e=(\d+) b=(\d+)")
TELE_WAKE = re.compile(
    r"tele: wake total=(\d+) span=(\d+) sig=(\d+) tmo=(\d+)")

# 与 kernel/src/main.rs 4-6 段同一对常量：合法项 +8 / 越界探针 +200（demo 第
# ④ 步绑定值的单一事实源在内核侧，此处镜像并断言一致由人工对读维护）。
W_LEGIT = 8
W_PROBE = 200
ENVELOPE = 64  # 内核执法包络（apply/weight_of 双点 clamp ±64）


def parse(path: str):
    sw = wake = None
    with open(path, "r", errors="replace") as f:
        for line in f:
            m = TELE_SW.search(line)
            if m:
                sw = tuple(int(g) for g in m.groups())
                continue
            m = TELE_WAKE.search(line)
            if m:
                wake = tuple(int(g) for g in m.groups())
    return sw, wake


def advise(sw, wake):
    """确定性规则 → (建议列表, 注入表条目列表)。"""
    notes, entries = [], []
    if sw is None:
        notes.append("WARN: 日志无 tele: sw 行——遥测未落盘，建议保守：仅越界探针")
    else:
        _total, span, _y, _p, _e, b = sw
        if span and b * 100 > span * 50:
            notes.append(
                f"block-share {b}/{span} > 50%：睡眠主导负载 → "
                f"包络内权重微调 +{W_LEGIT}（tag_P）")
            entries.append(("tag_p /* 运行期表指针 */", W_LEGIT))
        else:
            notes.append(
                f"block-share {b}/{span} ≤ 50%：负载非睡眠主导 → 不加权重项")
    if wake is None:
        notes.append("WARN: 日志无 tele: wake 行——唤醒分型缺省跳过")
    else:
        _t, span, _s, tmo = wake
        if span and tmo * 100 > span * 10:
            notes.append(
                f"tmo-share {tmo}/{span} > 10%：超时唤醒偏多 → 附注：检查 "
                f"deadline 预算（v1 无第二消费点，仅建议不入表）")
    # 越界探针恒发（内核 clamp 执法面——demo/probe 的机器断言对象）。
    entries.append(("tag_d /* 运行期表指针 */", W_PROBE))
    return notes, entries


def rust_table(entries) -> str:
    lines = [
        "// advisor.py 产出快照（demo 第 ④ 步绑定；task_key 运行期填真值）",
        "const AI_TABLE: [berry_sched::policy::Entry; {}] = [".format(len(entries)),
    ]
    for key, w in entries:
        lines.append(
            "    berry_sched::policy::Entry {{ task_key: {} as u64,".format(key))
        lines.append(
            "        directive: berry_sched::policy::Directive {{ placement: 0, weight: {}, epp: None }} }},".format(w))
    lines.append("];")
    return "\n".join(lines)


def main() -> int:
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        return 0
    path = sys.argv[1]
    rust_only = "--rust" in sys.argv[2:]
    sw, wake = parse(path)
    notes, entries = advise(sw, wake)
    if rust_only:
        print(rust_table(entries))
        return 0
    print("advisor: 输入 {}".format(path))
    print("  tele:sw   = {}".format(sw))
    print("  tele:wake = {}".format(wake))
    for n in notes:
        print("advisor: {}".format(n))
    print("advisor: 越界探针 +{} → 内核 clamp 到 ±{}（单调收窄，机器断言交 probe）".format(
        W_PROBE, ENVELOPE))
    print()
    print(rust_table(entries))
    return 0


if __name__ == "__main__":
    sys.exit(main())
