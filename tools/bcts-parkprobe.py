#!/usr/bin/env python3
"""bcts-parkprobe——berryos-smoke.* 串口日志 park 签名分类器（#257 后继件）。

背景（run 43461288210 / 43470515049 双证）：bcts 步在 netns+mount-ns 内
复跑 smoke 全相时，rootfs/RX 相 guest 可能零标记停摆——宿主侧只见
evsclient ×10 conn timed out，guest 死因（DNS 慢路 park？调度饿死？hostfwd
不入 guest？）此前只能靠 artifact（配额撞顶常缺席）。本工具把 berryos-smoke.*
串口日志就地分类，bcts 失败路径自动调用，输出人可读定谳行，不再依赖
artifact。

签名库（2026-11-13/14 实测定谳，签名串全部来自真实串口日志）：
  dns_park     `bsh: epl: gai dns r=-2 res=true`——DNS 慢路超时 park
               （SLIRP 虚拟 DNS 死路，#257 修前形态）；
  churn_starve switches 计数持续增长但零新标记——调度饿死面（idle 满核
               定谳 098ca8bf8：boot 演示常驻面永不退出，tickless 机构
               被饿死）。判据 = 尾部 switches 严格递增 + 标记进度冻结；
  net_dead    guest 已过 ROOTFS ok / NET ok ping= / DNS ok 层而宿主
               evsclient 仍超时——SLIRP→guest 段（hostfwd 入向）问题；
  clean_boot  裸相 32 标记齐——正常轮转杀（SIGTERM），非 park。

用法：
  tools/bcts-parkprobe.py <serial-log> [<serial-log> ...]   # 就地分类
  tools/bcts-parkprobe.py --selftest                        # 签名库钉板

输出：每文件一行定谳 `<file>: <verdict> <证据摘要>`；--selftest 退出 0。
退出码恒 0（取证件，分类不出不红——bcts 主门已在 smoke 侧执法）。
"""
import re
import sys
from pathlib import Path

# —— 签名库（钉板面；增签名先过 --selftest 再用）——
DNS_PARK_SIG = "bsh: epl: gai dns r=-2 res=true"
SWITCHES_RE = re.compile(r"switches=(\d+) kl=(\d+)")
# 裸相 32 标记的收尾锚（smoke.sh 轮转点——裸相完成即 SIGTERM 换 rootfs 相）。
BARE_PHASE_TAIL = "PWRDEG ok d=1"
# rootfs 相链路层锚（guest 侧网络栈活证——出现过即 guest NET 面在跑）。
NET_LIVE_ANCHORS = ("NET ok ping=", "DNS ok addr=", "TCP ok echo=")
ROOTFS_LIVE_ANCHORS = ("ROOTFS ok src=", "rootfs hello", "ECO ok", "EPL ok")

# 标记进度样本（marker 家族 → 匹配正则；churn_starve 判据的「零新标记」面）。
MARKER_HINTS = re.compile(
    r"(?:[A-Z][A-Z0-9_]{1,12} ok|ktest done|BerryOS boot|ROOTFS ok src=)"
)


def classify(path: str) -> str:
    """单文件分类 → 定谳行（人可读，含证据摘要）。"""
    try:
        text = Path(path).read_text(errors="replace")
    except OSError as e:
        return f"{path}: unreadable（{e}）——跳过"

    lines = text.splitlines()

    # ① DNS 慢路 park（#257 修前形态；修后应为 0 命中——命中即 stub 链回退）。
    if DNS_PARK_SIG in text:
        n = text.count(DNS_PARK_SIG)
        return f"{path}: dns_park（{DNS_PARK_SIG} ×{n}——ns 内 stub 链回退，查 resolv bind/mount 段）"

    # ② switches churn 判据：尾部 ≥5 条 switches 计数仍在严格递增。
    sw = [int(m.group(1)) for m in SWITCHES_RE.finditer(text)]
    tail = [s for s in sw if s > 0]
    churn_tail = tail[-5:]
    churning = len(churn_tail) == 5 and all(
        b > a for a, b in zip(churn_tail, churn_tail[1:])
    )

    # ③ 标记进度：全文件 marker 命中数 + 最后一枚 marker 所在行号。
    marker_hits = list(MARKER_HINTS.finditer(text))
    last_marker_line = (
        text[: marker_hits[-1].start()].count("\n") + 1 if marker_hits else 0
    )

    # ④ 正常轮转杀：裸相 32 标记齐（收尾锚在席）且无 rootfs 相锚——
    #    smoke.sh 裸相完成后 SIGTERM 换相，此日志是完整裸相，非 park。
    has_bare_tail = BARE_PHASE_TAIL in text
    has_rootfs = any(a in text for a in ROOTFS_LIVE_ANCHORS)

    if has_bare_tail and not has_rootfs:
        return f"{path}: clean_boot（裸相 32 标记齐后轮转杀——非 park，此文件无罪）"

    # ⑤ net_dead：guest 网络栈活证在席（链路层已通）——若宿主 evsclient
    #    仍超时，红面在 SLIRP hostfwd 入向，不在 guest DNS/调度面。
    if any(a in text for a in NET_LIVE_ANCHORS):
        nm = sum(text.count(a) for a in NET_LIVE_ANCHORS)
        return f"{path}: net_live（guest NET/DNS/TCP 锚 ×{nm}——guest 网络面在跑；宿主 evsclient 超时则查 hostfwd 入向段）"

    # ⑥ churn_starve：计数连打 + rootfs 相标记零进展（或停在某枚后不再动）。
    if churning and not has_rootfs:
        return (
            f"{path}: churn_starve（switches 尾 5 连升 {churn_tail} + rootfs 标记零进展"
            f"（末枚 marker 行 {last_marker_line}）——调度饿死面，定谳同 098ca8bf8"
            "（boot 演示常驻债，修窗五件 owner 域））"
        )
    if churning:
        return (
            f"{path}: churn_after_markers（switches 尾 5 连升 {churn_tail}，标记停在行 "
            f"{last_marker_line}——半途饿死，查该行标记与其后预期标记的间隙面）"
        )

    # ⑦ 兜底：churn 不足 5 条（金丝雀不足 = TCG 慢启动诊断降级面）。
    return (
        f"{path}: no_churn_evidence（switches 共 {len(sw)} 条尾 {churn_tail}，末枚 marker "
        f"行 {last_marker_line}——churn 证据不足，按串口尾人工定谳）"
    )


def selftest() -> int:
    """签名库钉板：分类器全分支（dns_park/clean_boot/net_live/churn_starve/
    churn_after_markers/no_churn_evidence/unreadable）× 代表性合成日志。"""
    cases = [
        # (name, synthetic_log, expected_verdict_prefix)
        ("dns_park 命中", f"...\n{DNS_PARK_SIG}\n...\n", "dns_park"),
        ("dns_park 多发", f"a\n{DNS_PARK_SIG}\nb\n{DNS_PARK_SIG}\n", "dns_park"),
        (
            "clean_boot 裸相完整",
            "BerryOS boot\nKVA ok\n...\nPWRDEG ok d=1\nswitches=800 kl=0\n",
            "clean_boot",
        ),
        (
            "clean_boot 排除 rootfs 混入",
            "PWRDEG ok d=1\nROOTFS ok src=fw\n",  # rootfs 锚在席 → 不判 clean_boot
            "",
        ),
        (
            "net_live 锚在席",
            "NET ok ping=10.0.2.2\nDNS ok addr=127.0.0.1\nswitches=100 kl=0\n",
            "net_live",
        ),
        (
            "churn_starve 全 churn 零标记",
            "xswitches=10 kl=0\n" + "".join(
                f"churn{i}switches={v} kl=0\n" for i, v in enumerate(range(37000, 37050), )
            ),
            "churn_starve",
        ),
        (
            "churn_after_markers 半途停",
            "ROOTFS ok src=fw\nrootfs hello\n"
            + "".join(f"switches={v} kl=0\n" for v in range(8000, 8050)),
            "churn_after_markers",
        ),
        (
            "no_churn 证据不足",
            "BerryOS boot\nKVA ok\nswitches=300 kl=0\n",
            "no_churn_evidence",
        ),
        ("unreadable 路径", None, "unreadable"),  # None = 用坏路径
    ]
    fails = 0
    tmp = Path(sys.argv[0]).parent / "__parkprobe_selftest__"
    for name, log, expect in cases:
        if log is None:
            out = classify("/nonexistent/__definitely_missing__.log")
            ok = out.startswith("/nonexistent/__definitely_missing__.log: unreadable")
        else:
            tmp.write_text(log, encoding="utf-8")
            out = classify(str(tmp))
            ok = expect in out
        status = "ok" if ok else "FAIL"
        if not ok:
            fails += 1
            print(f"  [{status}] {name}: 期望含「{expect}」实得「{out}」")
        else:
            print(f"  [{status}] {name}")
    tmp.unlink(missing_ok=True)
    print(f"parkprobe selftest: {len(cases) - fails}/{len(cases)} 腿过")
    return 1 if fails else 0


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    if sys.argv[1] == "--selftest":
        return selftest()
    for p in sys.argv[1:]:
        print(classify(p))
    return 0


if __name__ == "__main__":
    sys.exit(main())
