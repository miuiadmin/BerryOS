#!/usr/bin/env python3
# BCTS Tier 2 静态对账（reconcile_t2 模块面——bcts-run.sh PYEOF③段机械抽出，
# bctsrun Rust 件三刀的 parity oracle 基座；t1_check/t2_check 同款先例）。
# 抽取源 = tools/bcts-run.sh reconcile_t2() heredoc 体逐字（`python3 - "$1"
# "$REPO" <<'PYEOF3'` 调用位），+2 缩进归位外零变更：bcts-run.sh 改经
# `python3 "$SCRIPT_DIR/bcts/reconcile_t2.py" "$1" "$2"` 同参调用（$2 = 仓库
# 根），文案/退码逐字节不变。模块化动机同 t1/t2_check——Rust 件 parity 对
# 拍需要稳定驱动面。
#
# 出口协议（stdout 人读行；绿 = 退 0 / 开放缺口 = 退 1 / IO 拒 = 退 2）。
import sys


def reconcile(tsv: str, repo: str) -> None:
    import os, re


    contracts = []  # (cid, interface, src, [(num, name)])
    cases = {}      # cid -> {"pos": n, "neg": n, "class": {c: n}}
    with open(tsv, encoding="utf-8") as f:
        for line in f:
            p = line.rstrip("\n").split("\t")
            if p[0] == "CONTRACT":
                contracts.append((p[1], p[3], p[4], []))
                cases[p[1]] = {"pos": 0, "neg": 0, "class": {}}
            elif p[0] == "OP":
                for c in contracts:
                    if c[0] == p[1]:
                        c[3].append((int(p[2]), p[3]))
            elif p[0] == "CASE":
                cases[p[1]][p[4]] += 1
                cases[p[1]]["class"][p[5]] = cases[p[1]]["class"].get(p[5], 0) + 1

    CLASS_ZH = {"seq": "合法op序列", "reject": "非法op拒绝", "version": "version协商",
                "error": "错误域", "timeout": "超时失败隔离"}
    def opn(n):  # 合同记数形：fs 1..10 十进制 / hdi 0x40.. 十六进制（<16 十进制）
        return str(n) if n < 16 else f"0x{n:02x}"
    fail = 0
    total_gap = 0
    for cid, interface, src, ops in contracts:
        if not os.path.isfile(os.path.join(repo, src)):
            print(f"bcts: T2 对账 [{cid}]：interface_src 不可读：{src}——开放缺口（合同②件单源指向断链）")
            fail = 1
            total_gap += len(ops)
            continue
        with open(os.path.join(repo, src), encoding="utf-8") as f:
            text = f.read()
        found = {}
        for m in re.finditer(r"pub\s+const\s+(OP_[A-Z0-9_]+)\s*:\s*u8\s*=\s*(0x[0-9A-Fa-f]+|\d+)\s*;", text):
            found[m.group(1)] = int(m.group(2), 0)
        gaps = []
        for num, name in ops:
            key = f"OP_{name}"
            if key not in found:
                gaps.append(f"{key} 缺席（合同 op {opn(num)}）")
            elif found[key] != num:
                gaps.append(f"{key} 错号（合同 {opn(num)} vs crate {opn(found[key])}）")
        want = {f"OP_{n}" for _, n in ops}
        extras = sorted(k for k in found if k not in want)
        hit = len(ops) - len(gaps)
        tail = f"；crate 侧多 {len(extras)} op（add-only 合法不拦：{','.join(extras) if extras else '无'}）"
        if gaps:
            for g in gaps:
                print(f"bcts: T2 对账 [{cid}]（{interface}）：开放缺口——{g}")
            print(f"bcts: T2 对账 [{cid}]：合同 op 表 {len(ops)} 项，crate 命中 {hit}，差集 {len(gaps)}{tail}")
            fail = 1
        else:
            print(f"bcts: T2 对账 [{cid}]（{interface}）：合同 op 表 {len(ops)} 项，crate 命中 {hit}，差集 0（开放缺口 = 0）{tail}")
        total_gap += len(gaps)
        cv = cases[cid]
        cls = " ".join(f"{k}={cv['class'].get(k, 0)}" for k in ("seq", "reject", "version", "error", "timeout"))
        miss_cls = [CLASS_ZH[k] for k in ("seq", "reject", "version", "error", "timeout") if cv["class"].get(k, 0) == 0]
        miss = f"——缺口类：{'、'.join(miss_cls)}（如实记账不拦：Tier 2 开通条件随 #83 落数）" if miss_cls else ""
        print(f"bcts: T2 覆盖 [{cid}]：wire 用例 {cv['pos'] + cv['neg']} 行（pos {cv['pos']}/neg {cv['neg']}）；五类 {cls}{miss}")
    if fail:
        print(f"bcts: FAIL: T2 静态对账开放缺口 {total_gap} 项（crate op 全集 ⊉ 合同表——如实报，不硬绿）")
        sys.exit(1)
    print("bcts: T2 静态对账全绿（实现侧协议 crate op 全集 ⊇ 合同②件 op 表）")


if __name__ == "__main__":
    reconcile(sys.argv[1], sys.argv[2])
