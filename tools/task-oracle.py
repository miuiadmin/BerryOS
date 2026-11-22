#!/usr/bin/env python3
"""task-oracle.py —— plans/ 文档节的三态分类 oracle（任务 DAG 面；判定单源）。

三态与判据（消费面：e-guard 第②门、pack-work next、task-plan 图排序共用——
「什么算 done」只此一份，各消费面 import 不抄写）：
  DONE        节内双戳齐（Verified 严格合 check-docs.mjs 的 VERIFIED_RE + Quality
              reviewed 在场），或裸 Deferred 戳（终态 park：理由所述条件已出带外
              解决或永不复现 = done-for-now，合 e-guard「parked 合法在账」语义）。
  BLOCKED     Deferred 戳带 awaiting-* 令牌且未解（等答案/等裁决/等硬件……）——
              可恢复 park 不许读成 DONE：impossible-os 三态 oracle 的教训是
              可恢复 park 一旦读 DONE，run 在假 fixpoint 上收工（36 节等已答
              裁决永无人回）。双戳在场也不救：DR 先于 V+Q 判（closed-park
              re-judge，2026-11-13）。
  NEEDS_WORK  其余一切——无戳、单戳、Verified 语法不合、混合理由带答案……
              shipped-but-unstamped 不是 done（owner 门槛 = 交付且过审）。

与 punch_slice.py 的分工（两轨并存，勿混）：
  punch_slice 切活跃计划 punch-list（「现在做什么」的短队列，人工勾选面）；
  本 oracle 判 plans/ 文档编号节（「整个模块怎么建」的长路线，DAG 面）。节级
  完成态对接规划双戳——机读权威 = .agents/tools/check-docs.mjs 的 VERIFIED_RE/
  QUALITY_RE，运行时从该文件抽取翻译（REUSED, NOT REIMPLEMENTED）：抽不出或
  翻译自证不过即拒判 rc 2，宁拒不降——抄一份私有正则就是门与包对「什么算
  open」各执一词的老伤（吸收挖掘 P0）在 done 面复发。

节界与归属语义镜像 check-docs 的 checkDesignStamps：任一 `#{1,6} ` 标题封节
（不分层级——戳随物理节走，归属错节 = 把未戳节读 DONE 的隐形面）；```/~~~
  围栏整段掩蔽（围栏内戳/标题不参与——mermaid 图里写着 "> Verified:" 不是验收）；
戳行是物理 `> ` 行（不剥块引号）。节号三形：`## 1.` / `### 一、`（中文数字
归一为阿拉伯）/ `### 2.1`（点分子号原样）；无号节不是节点，其中的戳不归属
任何节点（防上游节吞戳）。同文档两节归一到同一 key = ref 撞名 = 拒判 rc 2
（图算法「同 ref 撞名构建硬错」在分类面提前拦截）。

awaiting-answer 读回（答案落盘自动重开，BLOCKED→NEEDS_WORK）：
  只等「已有答案的裁决」的 park 视为无 park——答案即指令。决策行文法
  （`A: (run decision, 日期) 选择. Reversal: 代价`）与 4-gram 主题匹配复用
  tools/answers-readback.py（importlib 装载，不抄写——判定单源律）。答案源
  默认扫 --root 下 .agents/notes/ 的 A: 行（零网络；PR body 是裁决正家，本地
  笔记是落盘镜像），可用 --decisions 指定 md 或 gh PR-JSON 形态。混合理由
  （同时等硬件/场子的 awaiting-* 令牌）即使答案已落也不重开。读不进答案源
  = 不重开（fail-closed 方向：坏状态不许自动开工）。

fail 方向（两向都立）：
  拒判 rc 2：权威正则抽不出、语料缺席（.agents/plans/*/design.md 一个都没
  有）、文档读不进、ref 撞名——绝不说「队列空」：坏状态读成 DONE 是静默
  完成未完成的工作，refuse 比 lie 好。
  fail-open：图工具缺席或调用失败（tools/task-plan.py / tools/task-graph.py
  未落库、ready_rows 抛错）——一切 NEEDS_WORK 可跑、按 (文档, 行号) 遍历序：
  规划故障不停跑，方向偏「多干活」不偏「假收工」。

用法：
  python3 tools/task-oracle.py                               # 整语料 classify → JSON
  python3 tools/task-oracle.py --classify <路径>             # 单文档 → JSON
  python3 tools/task-oracle.py --summary [--json]           # 逐文档一行 + 计数占比一行
  python3 tools/task-oracle.py --next-buildable [--limit N] # 三态交集 → JSON
  python3 tools/task-oracle.py --selftest
  可选：--root <dir>（语料根，自检/消费面用）、--decisions <md|PR-JSON>（答案源覆盖）

图接缝契约（给 task-plan 车道）：本件进程内 import tools/task-plan.py（次选
  tools/task-graph.py），调其 ready_rows(root) → [(文件相对路径, 节号)]；交集 =
  ready ∧ NEEDS_WORK；ready 但非 NEEDS_WORK 不入列（图说可建而分类说没活 =
  状态漂移，graph_matched 计数给消费面可见）。

exit：0 判成（含诚实的 DONE/BLOCKED/EMPTY 终态）；1 目标文档不在；2 拒判/用法错。
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent
AUTHORITY_MJS = ROOT / ".agents" / "tools" / "check-docs.mjs"

DONE = "DONE"
NEEDS_WORK = "NEEDS_WORK"
BLOCKED = "BLOCKED"
VALID_CLASSES = (DONE, NEEDS_WORK, BLOCKED)

# ——节界语义（镜像 check-docs checkDesignStamps：围栏自跟踪 + 任一 ATX 标题封节）——
FENCE_RE = re.compile(r"^\s*(```|~~~)")
HEADING_LINE_RE = re.compile(r"^#{1,6}\s")
HEADING_TITLE_RE = re.compile(r"^#{1,6}\s+(.+)$")
# ——节号三形：阿拉伯（含点分子号）/ 中文数字（一…九十九，{1,3} 字覆盖二十一）——
ARAB_NUM_RE = re.compile(r"^(\d{1,3}(?:\.\d{1,3})*)(?=[.、．:：\s]|$)")
CN_NUM_RE = re.compile(r"^([一二三四五六七八九十]{1,3})(?=[、.．:：\s]|$)")
CN_DIGITS = {"一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9}
# ——Deferred 戳（本 oracle 立法的新 DAG 面 park 形态；punch-list 的 [PARKED→人审]
#    仍是运营轨形态，两不混）。awaiting-* 令牌 = 可恢复；中文等答案/等裁决视同
#    answer 类令牌（BerryOS 实存形态的机读化）。——
DEFERRED_RE = re.compile(r"^>\s*Deferred:\s*(.*)$")
AWAITING_TOKEN_RE = re.compile(r"awaiting-[a-z0-9-]+", re.I)  # 全形令牌（带 awaiting- 前缀）——answer 类判定按全名对
ANSWER_ALIAS_RE = re.compile(r"等\s*(?:owner\s*裁决|owner裁决|裁决|答案)")
ANSWER_CLASS_TOKENS = {"awaiting-answer", "awaiting-decision", "awaiting-owner-decision"}
# 原始戳行收集面对齐 check-docs 第四红：大小写不敏感收 `> Verified:` 再严格验
VERIFIED_RAW_RE = re.compile(r"^>\s*Verified:", re.I)

GRAPH_CANDIDATES = ("task-plan.py", "task-graph.py")


class OracleRefusal(Exception):
    """拒判：状态载不入，绝不把看不见/读不进的面答成 DONE。rc 2。"""


# --- 权威正则：从 check-docs.mjs 抽取翻译，自证后才用 ------------------------

def _js_body_to_py(body: str) -> str:
    """JS 字面量正则体 → Python：唯一差异面是定界符转义 \\/（JS 因 / 定界才转义）。"""
    return body.replace("\\/", "/")


def load_stamp_authority(mjs_path: Path = AUTHORITY_MJS):
    """→ (VERIFIED_RE, QUALITY_RE)。机读权威是 check-docs.mjs 的字面量，不抄写。

    抽取后必须过「翻译自证」：用真仓现役戳行（纪律升级 design.md 的实戳）当
    正样例、带注记尾巴的坏样例当反样例——分不开说明 JS→Python 翻译不可信或
    权威已改形，拒判而非降级（宁拒不降：静默换私有正则 = 两处各执一词复发）。
    """
    try:
        text = mjs_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise OracleRefusal(f"戳语法权威读不进（{mjs_path}：{exc}）——拒判，不降级为私有正则") from None

    def pull(name: str) -> re.Pattern:
        m = re.search(r"const\s+" + name + r"\s*=\s*/(.+?)/\s*;", text)
        if not m:
            raise OracleRefusal(f"{mjs_path.name} 抽不出 {name}——权威改形，本 oracle 需要跟版")
        try:
            return re.compile(_js_body_to_py(m.group(1)))
        except re.error as exc:
            raise OracleRefusal(f"{name} 翻译失败（{exc}）——JS→Python 语义漂移，拒判") from None

    verified_re, quality_re = pull("VERIFIED_RE"), pull("QUALITY_RE")
    # 自证样例：正 = 真仓现役戳（.agents/plans/纪律升级/design.md 实戳）；反 = 带尾巴注记
    good_v = "> Verified: 2026-11-22 · 147005ed155d · 6/6 · tests 6"
    bad_v = "> Verified: 2026-11-22 · 147005ed155d + 收口笔 · 6/6 · tests 6"
    if not verified_re.fullmatch(good_v) or verified_re.fullmatch(bad_v):
        raise OracleRefusal("VERIFIED_RE 翻译自证不过（正/反样例分不开）——拒判")
    if not quality_re.match("> Quality reviewed: 对抗审查 2x 2026-11-21") or quality_re.match("> Quality reviewed:  "):
        raise OracleRefusal("QUALITY_RE 翻译自证不过——拒判")
    return verified_re, quality_re


# --- 节切分 -------------------------------------------------------------------

def _cn_num(s: str) -> Optional[int]:
    """中文数字一…九十九 → int；非纯数词返 None。"""
    if s == "十":
        return 10
    if "十" in s:
        left, _, right = s.partition("十")
        if left and left not in CN_DIGITS:
            return None
        if right and right not in CN_DIGITS:
            return None
        return (CN_DIGITS.get(left, 1) if left else 1) * 10 + (CN_DIGITS.get(right, 0) if right else 0)
    return CN_DIGITS.get(s)


def heading_number(heading_line: str) -> Optional[str]:
    """标题行 → 节号 key（str）：`## 1.`→"1"、`### 一、`→"1"、`### 2.1`→"2.1"；无号 None。"""
    m = HEADING_TITLE_RE.match(heading_line)
    if not m:
        return None
    title = m.group(1).strip()
    a = ARAB_NUM_RE.match(title)
    if a:
        return a.group(1)
    c = CN_NUM_RE.match(title)
    if c:
        n = _cn_num(c.group(1))
        if n is not None:
            return str(n)
    return None


def walk_sections(text: str) -> list[dict]:
    """→ [{heading, line(1 基), body[list[str]]}]。首标题前的文档前导不是节
    （其中的戳不归属任何节点——防前导吞戳把未编节读成完成）。"""
    sections: list[dict] = []
    cur: Optional[dict] = None
    fenced = False
    for i, raw in enumerate(text.splitlines(), 1):
        if FENCE_RE.match(raw):
            fenced = not fenced
            continue
        if fenced:
            continue
        if HEADING_LINE_RE.match(raw):
            if cur is not None:
                sections.append(cur)
            cur = {"heading": raw, "line": i, "body": []}
            continue
        if cur is None:
            continue
        cur["body"].append(raw)
    if cur is not None:
        sections.append(cur)
    return sections


# --- 决策面（答案读回；复用 answers-readback 的文法与匹配原语） --------------

_answers_mod = None


def _answers_module():
    """tools/answers-readback.py 的进程内装载（名带连字符，importlib 形态）。

    判定单源律：决策行文法 DECISION_RE 与 4-gram 主题匹配只此一家，本 oracle
    不抄第二份。装载失败 → 决策面缺席（park 不重开，fail-closed 方向）。
    """
    global _answers_mod
    if _answers_mod is None:
        try:
            src = ROOT / "tools" / "answers-readback.py"
            spec = importlib.util.spec_from_file_location("_answers_readback_for_oracle", str(src))
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            _answers_mod = mod
        except Exception:  # noqa: BLE001 — 答案面缺席不许 crash 分类主面
            _answers_mod = False
    return _answers_mod or None


def load_decisions(root: Path, override: Optional[Path]) -> tuple[list[dict], str]:
    """→ (决策列表, 来源描述)。PR-JSON 用真 title（题目带主题词参与匹配）；md
    形 title 置空（relpath 的路径段会制造 implement/notes 类假 4-gram）。"""
    mod = _answers_module()
    if mod is None:
        return [], "answers-readback 不可装载（决策面缺席，park 不自动重开）"
    if override is not None:
        try:
            raw = override.read_text(encoding="utf-8")
        except OSError as exc:
            raise OracleRefusal(f"--decisions 读不进（{override}：{exc}）") from None
        try:
            prs = json.loads(raw)
            if not isinstance(prs, list):
                raise ValueError("顶层不是数组")
            return mod.extract_decisions(prs), f"--decisions {override}（PR-JSON 形）"
        except ValueError:
            return mod.extract_decisions([{"number": None, "title": "", "body": raw}]), f"--decisions {override}（md 形）"
    notes = root / ".agents" / "notes"
    if not notes.is_dir():
        return [], f"{notes} 缺席（决策面空）"
    acc = []
    for md in sorted(notes.rglob("*.md")):
        try:
            acc.append({"number": None, "title": "", "body": md.read_text(encoding="utf-8", errors="replace")})
        except OSError:
            continue
    return mod.extract_decisions(acc), f"扫 {notes} 的 A: 决策行"


def _deferred_verdicts(body: list[str], decisions: list[dict]) -> list[dict]:
    """节内 Deferred 行 → [{rest, state, token, reopened_by}]。

    state 四值：unresolved（带令牌且未解——含 answer 类未答：等裁决没等到位
    就是 BLOCKED，不是终态）、answered（只等 answer 类且主题 4-gram 命中决策
    ——答案即指令）、terminal（无令牌裸理由 = done-for-now）、empty（空理由
    坏戳，不算 park——裸 Deferred 是 silently-defer-and-forget 的洞）。
    """
    out = []
    for ln in body:
        m = DEFERRED_RE.match(ln)
        if not m:
            continue
        rest = m.group(1).strip()
        if not rest:
            out.append({"rest": "", "state": "empty", "token": None, "reopened_by": None})
            continue
        tokens = {t.lower() for t in AWAITING_TOKEN_RE.findall(rest)}
        if ANSWER_ALIAS_RE.search(rest):
            tokens.add("awaiting-answer")
        if not tokens:
            out.append({"rest": rest, "state": "terminal", "token": None, "reopened_by": None})
            continue
        answered = False
        reopened_by = None
        if tokens <= ANSWER_CLASS_TOKENS:  # 混合理由（也等硬件/场子）即使答落也不重开
            mod = _answers_module()
            topic = AWAITING_TOKEN_RE.sub(" ", rest)  # 剥机读令牌：格式词不参与主题匹配
            grams = mod.fourgrams(topic) if mod else set()
            for d in decisions:
                if any(g in d["match_text"] for g in grams):
                    answered = True
                    reopened_by = (d.get("choice") or "")[:60]
                    break
        if answered:
            out.append({"rest": rest, "state": "answered", "token": None, "reopened_by": reopened_by})
        else:
            non_answer = sorted(tokens - ANSWER_CLASS_TOKENS)
            token = non_answer[0] if non_answer else sorted(tokens & ANSWER_CLASS_TOKENS)[0]
            out.append({"rest": rest, "state": "unresolved", "token": token, "reopened_by": None})
    return out


# --- 分类核心 -----------------------------------------------------------------

def classify_document(rel: str, stem: str, text: str, verified_re: re.Pattern,
                      quality_re: re.Pattern, decisions: list[dict]) -> dict:
    """单文档 → {file, stem, doc_status, file_class, nodes, note}。撞 ref 拒判。"""
    nodes: list[dict] = []
    seen_keys: dict[str, int] = {}
    for sec in walk_sections(text):
        key = heading_number(sec["heading"])
        if key is None:
            continue
        if key in seen_keys:
            raise OracleRefusal(
                f"{rel}: §{key} 撞名（行 {seen_keys[key]} 与行 {sec['line']} 两节归一同号）——"
                "ref 唯一性是图与分类的共同前置，先改文档节号")
        seen_keys[key] = sec["line"]
        verified = quality = False
        verified_malformed = False
        for ln in sec["body"]:
            if VERIFIED_RAW_RE.match(ln) and not verified_re.fullmatch(ln):
                verified_malformed = True  # 原始行在场但语法不合（对齐第四红口径）
            if verified_re.fullmatch(ln):
                verified = True  # 节内任一严格戳在场即计（历史戳不执法由 check-docs 管）
            if quality_re.match(ln):
                quality = True
        dv = _deferred_verdicts(sec["body"], decisions)
        faults = []
        if verified_malformed:
            faults.append("Verified 戳行在场但不合机读语法（check-docs 第四红同口径）")
        faults += ["Deferred 戳缺理由（空理由 park = silently-defer 洞，不计 park）" for v in dv if v["state"] == "empty"]
        # 判序镜像三态 oracle：未解 awaiting（含 answer 类未答）> 已答重开 > 终态
        # park > 双戳 > 其余——DR 先于 V+Q 判（closed-park re-judge，2026-11-13）。
        cls = NEEDS_WORK
        awaiting = None
        reopened = False
        reopened_by = None
        if any(v["state"] == "unresolved" for v in dv):
            cls = BLOCKED
            v = next(v for v in dv if v["state"] == "unresolved")
            awaiting = v["token"]
        elif any(v["state"] == "answered" for v in dv):
            cls = NEEDS_WORK  # 答案落盘自动重开（即使同节带双戳：重开残面必须被读回）
            reopened = True
            reopened_by = next(v["reopened_by"] for v in dv if v["state"] == "answered")
        elif any(v["state"] == "terminal" for v in dv):
            cls = DONE  # 终态 park（done-for-now）
            awaiting = "terminal-park"
        elif verified and quality:
            cls = DONE
        stamps = ([k for k, ok in (("V", verified), ("Q", quality)) if ok])
        title = HEADING_TITLE_RE.match(sec["heading"])
        nodes.append({
            "ref": f"{stem} §{key}", "file": rel, "stem": stem, "n": key, "line": sec["line"],
            "heading": (title.group(1).strip()[:60] if title else ""),
            "class": cls, "stamps": stamps, "awaiting": awaiting,
            "reopened": reopened, "reopened_by": reopened_by, "faults": faults,
        })
    sm = re.search(r"^Status:\s*(\S+)", text, re.M)
    doc_status = sm.group(1) if sm else None
    classes = {n["class"] for n in nodes}
    if not nodes:
        file_class = None
    elif NEEDS_WORK in classes:
        file_class = NEEDS_WORK
    elif BLOCKED in classes:
        file_class = BLOCKED
    else:
        file_class = DONE
    return {"file": rel, "stem": stem, "doc_status": doc_status, "file_class": file_class,
            "nodes": nodes,
            **({"note": "无编号节——非任务载体或骨架未编节"} if not nodes else {})}


def classify_corpus(root: Path, decisions_override: Optional[Path] = None) -> dict:
    """整语料三态分类。语料缺席拒判（rc 2 语义在 main 兑现）。"""
    verified_re, quality_re = load_stamp_authority()
    decisions, dec_src = load_decisions(root, decisions_override)
    plans = root / ".agents" / "plans"
    if not plans.is_dir():
        raise OracleRefusal(f"{plans} 不在——语料根可疑，拒判（绝不说「队列空」）")
    docs = sorted(plans.glob("*/design.md"))
    if not docs:
        raise OracleRefusal(f"{plans} 下一个 design.md 都没有——语料缺席拒判："
                            "把看不见的语料答成 DONE 是静默完成未完成的工作")
    out_docs, excluded = [], []
    for d in docs:
        try:
            text = d.read_text(encoding="utf-8")
        except OSError as exc:
            raise OracleRefusal(f"{d} 读不进（{exc}）——拒判") from None
        rel = str(d.relative_to(root))
        sm = re.search(r"^Status:\s*(\S+)", text, re.M)
        if sm and sm.group(1) == "superseded":
            excluded.append({"file": rel, "reason": "Status: superseded（现行面在取代链终点）"})
            continue
        out_docs.append(classify_document(rel, d.parent.name, text, verified_re, quality_re, decisions))
    counts = {DONE: 0, NEEDS_WORK: 0, BLOCKED: 0}  # 三键预置：两键字典在真出现 BLOCKED 那天 KeyError
    for doc in out_docs:
        for n in doc["nodes"]:
            counts[n["class"]] += 1
    total = sum(counts.values())
    return {"root": str(root), "stamp_authority": ".agents/tools/check-docs.mjs（运行时抽取翻译）",
            "decisions_source": dec_src, "decisions": len(decisions),
            "documents": out_docs, "excluded": excluded,
            "counts": {**counts, "total": total}}


# --- 图接缝（next-buildable） --------------------------------------------------

def _norm_rel(p: str) -> str:
    """路径归一（比较用）：反斜杠转正斜杠 + 剥 ./ 前缀。

    不用 lstrip("./")——它按字符集剥：会把 .agents 的点也剥掉（./.agents → agents）。
    """
    p = p.replace("\\", "/")
    while p.startswith("./"):
        p = p[2:]
    return p


def graph_ready(root: Path) -> dict:
    """进程内调图工具（学 plan_ready 免子进程超时）。→ {source, ready|None, note}。

    ready 为 None = 图无答（缺席/失败）→ 调用面 fail-open。失败方向固定：
    图故障绝不推出「无可跑项」（那是停跑），只退「一切 NEEDS_WORK 可跑」。
    """
    for name in GRAPH_CANDIDATES:
        p = root / "tools" / name
        if not p.is_file():
            continue
        try:
            spec = importlib.util.spec_from_file_location("_task_graph_for_oracle", str(p))
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            fn = getattr(mod, "ready_rows", None)
            if not callable(fn):
                return {"source": f"tools/{name}", "ready": None,
                        "note": f"tools/{name} 在场但无 ready_rows(root) —— fail-open"}
            rows = fn(root)
            ready = set()
            for r in rows or []:
                if isinstance(r, (tuple, list)) and len(r) == 2:
                    ready.add((_norm_rel(str(r[0])), str(r[1])))
            return {"source": f"tools/{name}", "ready": ready, "note": ""}
        except Exception as exc:  # noqa: BLE001 — fail-open：规划故障不停跑
            return {"source": None, "ready": None,
                    "note": f"tools/{name} 调用失败（{type(exc).__name__}: {str(exc)[:120]}）→ fail-open"}
    return {"source": None, "ready": None,
            "note": "图工具缺席（task-plan/task-graph 未落库）→ fail-open：一切 NEEDS_WORK 可跑"}


def cmd_next_buildable(root: Path, limit: Optional[int], decisions_override: Optional[Path]) -> int:
    corpus = classify_corpus(root, decisions_override)
    nodes = [n for doc in corpus["documents"] for n in doc["nodes"]]  # 已按 (路径, 行号) 序
    needs = [n for n in nodes if n["class"] == NEEDS_WORK]
    blocked = [n for n in nodes if n["class"] == BLOCKED]
    g = graph_ready(root)
    if g["ready"] is None:
        buildable, waiting = needs, []
    else:
        ready = g["ready"]
        buildable = [n for n in needs if (n["file"], n["n"]) in ready]
        waiting = [n for n in needs if (n["file"], n["n"]) not in ready]
    matched = 0
    if g["ready"] is not None:
        keys = {(n["file"], n["n"]) for n in nodes}
        matched = sum(1 for f, k in g["ready"] if (f, k) in keys)
    if limit is not None:
        buildable = buildable[:limit]
    total = corpus["counts"]["total"]
    if total == 0:
        status = "EMPTY"
    elif needs:
        status = "WORK"
    elif blocked:
        status = "BLOCKED"
    else:
        status = "DONE"
    out = {
        "status": status,
        "graph": {"source": g["source"], "note": g["note"], "ready_rows_matched": matched},
        "counts": corpus["counts"],
        "buildable": [{k: n[k] for k in ("ref", "file", "n", "line", "heading")} for n in buildable],
        "waiting": [{k: n[k] for k in ("ref", "file", "n", "line", "heading")} for n in waiting],
        **({"blockers": [{"ref": n["ref"], "file": n["file"], "awaiting": n["awaiting"]}
                         for n in blocked]} if status == "BLOCKED" else {}),
    }
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return 0


# --- 命令面 -------------------------------------------------------------------

def cmd_classify(root: Path, target: Optional[str], decisions_override: Optional[Path]) -> int:
    verified_re, quality_re = load_stamp_authority()
    if target is None:
        corpus = classify_corpus(root, decisions_override)
        print(json.dumps(corpus, ensure_ascii=False, indent=1))
        return 0
    p = Path(target) if Path(target).is_absolute() else (root / target if (root / target).is_file() else Path(target))
    if not p.is_file():
        print(f"task-oracle: 目标文档不在（{target}）", file=sys.stderr)
        return 1
    try:
        text = p.read_text(encoding="utf-8")
    except OSError as exc:
        raise OracleRefusal(f"目标文档读不进（{p}：{exc}）——拒判") from None
    try:
        rel = str(p.relative_to(root))
    except ValueError:
        rel = str(p)
    decisions, dec_src = load_decisions(root, decisions_override)
    doc = classify_document(rel, p.stem if p.name != "design.md" else p.parent.name,
                            text, verified_re, quality_re, decisions)
    counts = {DONE: 0, NEEDS_WORK: 0, BLOCKED: 0}
    for n in doc["nodes"]:
        counts[n["class"]] += 1
    print(json.dumps({**doc, "decisions_source": dec_src,
                      "counts": {**counts, "total": sum(counts.values())}}, ensure_ascii=False, indent=1))
    return 0


def cmd_summary(root: Path, decisions_override: Optional[Path], as_json: bool) -> int:
    corpus = classify_corpus(root, decisions_override)
    if as_json:
        print(json.dumps(corpus, ensure_ascii=False, indent=1))
        return 0
    for doc in corpus["documents"]:
        cls = doc["file_class"] or "-"
        print(f"{cls:12s} {len(doc['nodes']):3d} 节  {doc['file']}")
    for ex in corpus["excluded"]:
        print(f"{'EXCLUDED':12s}   -  {ex['file']}（{ex['reason']}）")
    c, t = corpus["counts"], corpus["counts"]["total"]
    if t:
        line = (f"-- DONE {c[DONE]}（{c[DONE] * 100 // t}%） | NEEDS_WORK {c[NEEDS_WORK]}"
                f"（{c[NEEDS_WORK] * 100 // t}%） | BLOCKED {c[BLOCKED]}（{c[BLOCKED] * 100 // t}%）"
                f" | 共 {t} 节 / {len(corpus['documents'])} 文档 --")
    else:
        line = f"-- 语料零节点（{len(corpus['excluded'])} 文档全被排除）——非 DONE 非 EMPTY 判 --"
    print(line)
    print(f"-- 决策源：{corpus['decisions_source']}（{corpus['decisions']} 条）--")
    return 0


# --- 自检（检测器自检律：必触发的坏样例腿全在） --------------------------------

GOOD_V = "> Verified: 2026-11-22 · 147005ed155d · 1/1 · tests 6"
GOOD_Q = "> Quality reviewed: 对抗审查 2x 2026-11-22"


def _doc(body_sections: str) -> str:
    return ("# 合成模块 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-22\nVersion: 1.0.0\n\n"
            + body_sections)


def selftest() -> int:
    import tempfile
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"task-oracle 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    # 腿 1：权威抽取（真 check-docs.mjs）+ 坏权威拒判
    v_re, q_re = load_stamp_authority()
    check("权威抽取（真 mjs 正/反样例分开 + 坏 mjs 拒判）",
          v_re.fullmatch(GOOD_V) is not None and v_re.fullmatch(GOOD_V + " 尾注") is None
          and q_re.match(GOOD_Q) is not None)
    with tempfile.TemporaryDirectory() as td:
        bad_mjs = Path(td) / "check-docs.mjs"
        bad_mjs.write_text("const VERIFIED_RE = 42;\n", encoding="utf-8")
        refused = False
        try:
            load_stamp_authority(bad_mjs)
        except OracleRefusal:
            refused = True
        check("权威改形拒判（宁拒不降）", refused)

    # 腿 2：节号三形
    check("节号三形（## 1. / ### 一、 / ### 2.1 / ### 十二、；无号节非节点）",
          heading_number("## 1. 模块定位") == "1"
          and heading_number("### 一、契约") == "1"
          and heading_number("### 2.1 子节") == "2.1"
          and heading_number("### 十二、远节") == "12"
          and heading_number("## 背景介绍") is None
          and heading_number("## 2026-11-13 计划") is None)

    decisions_md = (
        "# 决策\n\n"
        "A: (run decision, 2026-11-22) 采纳输入楔死修复自愈方案. Reversal: 撤回自愈保留观测\n"
        "A: (run decision, 2026-11-22) 采纳图标直进路线. Reversal: 还原脚本\n")

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        (t / ".agents" / "plans" / "mod1").mkdir(parents=True)
        (t / ".agents" / "notes").mkdir(parents=True)
        (t / ".agents" / "notes" / "d.md").write_text(decisions_md, encoding="utf-8")
        dec, _src = load_decisions(t, None)
        # 腿 3-8 载体：一文档八节覆盖核心真值表
        (t / ".agents" / "plans" / "mod1" / "design.md").write_text(_doc(
            "## 1. 甲（双戳）\n\n" + GOOD_V + "\n" + GOOD_Q + "\n\n"
            "## 2. 乙（无戳）\n\n正文。\n\n"
            "## 3. 丙（单戳 V）\n\n" + GOOD_V + "\n\n"
            "## 4. 丁（Verified 语法不合）\n\n> Verified: 2026-11-22 · abc + 收口笔 · 1/1 · tests 1\n\n"
            "## 5. 戊（等 owner 裁决）\n\n> Deferred: awaiting-owner-decision W1/W2/W3 采纳评估\n\n"
            "## 6. 己（等答案：输入楔死修复）\n\n> Deferred: awaiting-answer 输入楔死修复方案采纳\n\n"
            "## 7. 庚（裸终态 park）\n\n> Deferred: 条件触发面已随带外收口\n\n"
            "## 8. 辛（空节——下一节的戳不归我）\n\n"
            "## 9. 壬（双戳+围栏假戳）\n\n```text\n" + GOOD_V + "\n```\n\n" + GOOD_V + "\n" + GOOD_Q + "\n"), encoding="utf-8")
        corpus = classify_corpus(t)
        nodes = {n["ref"]: n for doc in corpus["documents"] for n in doc["nodes"]}
        check("双戳齐=DONE；无戳/单戳/坏语法=NEEDS_WORK（四小面）",
              nodes["mod1 §1"]["class"] == DONE
              and nodes["mod1 §2"]["class"] == NEEDS_WORK
              and nodes["mod1 §3"]["class"] == NEEDS_WORK
              and nodes["mod1 §4"]["class"] == NEEDS_WORK
              and nodes["mod1 §4"]["faults"] != [])
        check("awaiting=BLOCKED（记令牌）；裸 Deferred=终态 DONE",
              nodes["mod1 §5"]["class"] == BLOCKED and nodes["mod1 §5"]["awaiting"] == "awaiting-owner-decision"
              and nodes["mod1 §7"]["class"] == DONE and nodes["mod1 §7"]["awaiting"] == "terminal-park")
        check("答案落盘自动重开（4-gram 命中→NEEDS_WORK reopened）",
              nodes["mod1 §6"]["class"] == NEEDS_WORK and nodes["mod1 §6"]["reopened"]
              and "输入楔死修复" in (nodes["mod1 §6"]["reopened_by"] or ""))
        check("节界腿（§8 空节不吞 §9 的戳）+ 围栏腿（fence 内戳不计）",
              nodes["mod1 §8"]["class"] == NEEDS_WORK and nodes["mod1 §9"]["class"] == DONE)
        # 腿：文档级三态 + 三键计数
        c = corpus["counts"]
        check("计数三键预置（DONE/NEEDS_WORK/BLOCKED/total 齐）",
              set(c) == {DONE, NEEDS_WORK, BLOCKED, "total"} and c["total"] == 9
              and c[DONE] == 3 and c[BLOCKED] == 1)
        # 腿：混合理由不重开（答案已落也 BLOCKED）
        (t / ".agents" / "plans" / "mod2").mkdir()
        (t / ".agents" / "plans" / "mod2" / "design.md").write_text(_doc(
            "## 1. 混合理由\n\n> Deferred: awaiting-answer 输入楔死修复方案；同场还 awaiting-hardware 触控板\n\n"
            "## 2. 中文别名重开\n\n> Deferred: 等裁决 输入楔死修复方案\n\n"
            "## 3. 空理由 Deferred\n\n> Deferred:\n\n"), encoding="utf-8")
        (t / ".agents" / "plans" / "mod3").mkdir()
        (t / ".agents" / "plans" / "mod3" / "design.md").write_text(_doc(
            "## 1. 完成节\n\n" + GOOD_V + "\n" + GOOD_Q + "\n\n"
            "## 2. 挂起节\n\n> Deferred: awaiting-infra 观察期\n\n"), encoding="utf-8")
        (t / ".agents" / "plans" / "mod4").mkdir()
        (t / ".agents" / "plans" / "mod4" / "design.md").write_text(
            _doc("## 1. 旧节\n\n").replace("Status: active", "Status: superseded"), encoding="utf-8")
        corpus2 = classify_corpus(t)
        n2 = {n["ref"]: n for doc in corpus2["documents"] for n in doc["nodes"]}
        check("混合理由不重开（答已落仍 BLOCKED）；中文别名等裁决可重开；空理由不算 park",
              n2["mod2 §1"]["class"] == BLOCKED
              and n2["mod2 §2"]["class"] == NEEDS_WORK and n2["mod2 §2"]["reopened"]
              and n2["mod2 §3"]["class"] == NEEDS_WORK and n2["mod2 §3"]["faults"] != [])
        fc = {doc["stem"]: doc["file_class"] for doc in corpus2["documents"]}
        check("文档级三态（DONE+BLOCKED 无 NEEDS_WORK → BLOCKED；全 DONE → DONE）",
              fc["mod3"] == BLOCKED and fc["mod1"] == NEEDS_WORK)
        check("superseded 排除（列名不静默）",
              any(ex["file"].endswith("mod4/design.md") for ex in corpus2["excluded"])
              and "mod4" not in fc)
        # 腿：撞 ref 拒判
        (t / ".agents" / "plans" / "mod5").mkdir()
        (t / ".agents" / "plans" / "mod5" / "design.md").write_text(_doc(
            "## 1. 甲\n\n### 一、乙\n\n"), encoding="utf-8")
        refused = False
        try:
            classify_corpus(t)
        except OracleRefusal:
            refused = True
        check("撞 ref 拒判（## 1. 与 ### 一、 同文档）", refused)
        (t / ".agents" / "plans" / "mod5" / "design.md").unlink()
        (t / ".agents" / "plans" / "mod5").rmdir()
        # 腿：next-buildable fail-open（输出 JSON 静默捕获后验字段，不刷屏）
        import io
        import contextlib
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            r = main(["--root", str(t), "--next-buildable"])
        try:
            nb = json.loads(buf.getvalue())
        except ValueError:
            nb = {}
        check("next-buildable fail-open（图缺席→一切 NEEDS_WORK 可跑）",
              r == 0 and nb.get("graph", {}).get("source") is None
              and nb.get("status") == "WORK"
              and all(n["ref"].startswith(("mod1", "mod2", "mod3")) for n in nb.get("buildable", [])))
        # 真图腿：夹具 task-plan.py 给 ready_rows 子集
        (t / "tools").mkdir()
        (t / "tools" / "task-plan.py").write_text(
            "def ready_rows(root):\n"
            "    return [(\".agents/plans/mod1/design.md\", \"2\"), (\".agents/plans/mod1/design.md\", \"5\")]\n",
            encoding="utf-8")
        g = graph_ready(t)
        corpus3 = classify_corpus(t)
        needs3 = [n for doc in corpus3["documents"] for n in doc["nodes"] if n["class"] == NEEDS_WORK]
        buildable = [n for n in needs3 if (n["file"], n["n"]) in g["ready"]]
        waiting = [n for n in needs3 if (n["file"], n["n"]) not in g["ready"]]
        check("真图交集（ready ∧ NEEDS_WORK；BLOCKED 节 §5 不入列；waiting 计余量）",
              g["source"] == "tools/task-plan.py"
              and [n["ref"] for n in buildable] == ["mod1 §2"]
              and any(n["ref"] == "mod1 §6" for n in waiting))
    # 腿：空语料拒判 + 真树集成
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        (t / ".agents" / "plans").mkdir(parents=True)
        refused = False
        try:
            classify_corpus(t)
        except OracleRefusal:
            refused = True
        check("空语料拒判（rc 2 语义：绝不说队列空）", refused)
    real = classify_corpus(ROOT)
    ok_real = (real["counts"]["total"] >= 1
               and all(n["class"] in VALID_CLASSES for doc in real["documents"] for n in doc["nodes"])
               and all(doc["file_class"] in VALID_CLASSES or doc["file_class"] is None
                       for doc in real["documents"]))
    check(f"真树集成（{len(real['documents'])} 文档 / {real['counts']['total']} 节全类值合法）", ok_real)

    print(f"task-oracle 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="plans/ 文档节三态分类 oracle（DAG 面）")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--classify", metavar="PATH", help="单文档路径")
    g.add_argument("--summary", action="store_true", help="逐文档一行 + 计数占比")
    g.add_argument("--next-buildable", action="store_true", help="三态交集（图 ready ∧ NEEDS_WORK）")
    g.add_argument("--selftest", action="store_true")
    ap.add_argument("--json", action="store_true", help="--summary 的机器面")
    ap.add_argument("--limit", type=int, default=None, help="--next-buildable 截前 N 条")
    ap.add_argument("--root", type=Path, default=ROOT, help="语料根（默认本仓根）")
    ap.add_argument("--decisions", type=Path, default=None, help="答案源覆盖（md 或 gh PR-JSON）")
    args = ap.parse_args(argv)
    try:
        if args.selftest:
            return selftest()
        if args.next_buildable:
            return cmd_next_buildable(args.root, args.limit, args.decisions)
        if args.summary:
            return cmd_summary(args.root, args.decisions, args.json)
        return cmd_classify(args.root, args.classify, args.decisions)
    except OracleRefusal as exc:
        print(f"task-oracle: REFUSING to classify——{exc}\n"
              "  这不是「队列完成」，是「队列读不进」：把拒判读成 DONE 会静默完成未完成的工作。",
              file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
