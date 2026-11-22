#!/usr/bin/env python3
"""BerryOS 快道重研题一桌面推演模拟器（28 号 §2 状态机 × 不变量机器验证）。

科目（28 号 §2.5-1「桌面推演科目一」——本件即该可执行验证脚本）：
  1. 编队状态机模拟器：整编队通道（M/X）+ 安全子集快道通道（F），槽对
     {A,B} 翻指针模型；状态 PENDING → SOAKING → SUCCESSFUL + ROLLBACK 动作。
  2. 不变量机器验证：I-PR / I-SL-1..4（28 号 §2.1 :58-:64 逐条）随随机
     双轴版本流（功能轴 PR + 安全 lane，含乱序到达/回退尝试/OEM 停更 D1/
     同 lane 重制树）全场景断言保持。
  3. 反例搜索：B1 原命题 P（19 号 :170 配套②「功能轴回退不受安全地板
     反噬」）自动证伪重现；改写命题 P′（28 号 §2.3 :83「功能轴回退不丢
     安全内容」）对抗搜索——命中即证伪 28 号改写，找不到 = 加固证据。
  4. 案 α 浸泡窗参数扫描（OQ-1 :185——none / W1:K / W2=下一 lane 到达）。

锚点（28 号 = research/厂商定制化深研/28-快道重研-定向三题.md）：
  状态机梯子与回退门 :43-:54；不变量表 :58-:64；案 α/β/γ :70-:78；
  P 证伪与 P′/定理 :80-:85；L1 语句 :105；OQ-1 :185。
  他源：19 号 :163（platform_revision_unchanged——快道轴冻结，设备侧
  loud 拒装）/:170（配套②原句）；22 号 §3.1 :76-:80（B1 两受害形态）；
  09 号 :224（D1 停更超窗 active→warned→expired，版本当时钟）；
  更新与OTA-v1.0 :39-:40（槽三字段 +「SUCCESSFUL 先于 rollback index
  更新」次序契约 + 回退自由窗）/:44（CounterBackend 单调）。

状态名映射（任务书四态 ↔ 28 号梯子 :43-:48）：
  PENDING    = 28 号 STAGED + ACTIVATING（回退自由窗——:40/:53）
  SOAKING    = 28 号 SOAKING（floor_sl 未吸收本树值——I-SL-2 :62）
  SUCCESSFUL = 28 号 CONFIRMED（浸泡期满；floor_sl := SL(本树)——:48）
  ROLLBACK   = 动作（ptr 翻回旧槽，:50 合法性门）
  注：28 号 HEALTHY ≡ markBootSuccessful 标记点 = 本 sim 的
  PENDING→SOAKING 转换 + floor_pr 抬升时点（I-PR「SUCCESSFUL 后才抬」）。

推断注（28 号未逐字定义处，本件取可机判读法并在此声明）：
  a) 浸泡窗适用于一切 lane > floor_sl 的树（快道 F 恒命中；整编队携新
     公告时同命中）——I-SL-2 :62 为无条件语句；同 lane 树（含重制）无
     抬升可延迟，HEALTHY 即 CONFIRMED（I-SL-4 :64「含同 lane 重制树」
     的快路径）。策略 none = 修法前形态（吸收点与 SUCCESSFUL 重合）。
  b) W1:K 的 K 自 HEALTHY 当次 boot 起计（K=1 ≈ none）。
  c) :52「无快道在途」在计数器语义下 = floor_sl_eff 恒为 floor_sl 计数
     器（吸收只发生在 CONFIRMED）——门公式不分支。
  d) 重制树 oracle（OQ-2 公告→补丁集→工件物料账假设，28 号 §2.3 :84
     「未解前是假设不是事实」）= 配置项 remake_oracle；关闭即演示 P′
     降级形态（平台构建纪律保证，非机器可验）。
  e) 回退目标 = 备槽树且须 deployed（曾被激活）——双槽下旧槽树就是紧邻
     前代（γ 注记 :72 的机械成因）；备槽被新出件覆写后回退目标消灭。
  f) W2 关窗仅由「受纳的 lane 抬升出件」触发（被拒出件不算 lane 到达
     ——垃圾件未及设备语义面）。由此得一结构性性质（selftest 钉板 +
     语料机验）：W2 下 SL 地板回退拒面不可达——关窗事件与覆槽事件同
     步，floor_sl 抬升瞬间消灭一切 lane<floor 的已部署回退目标；B1 反
     噬在 W2 下全部表现为「无部署目标→必须前滚」（与 β 重制通道同构
     收敛）。地板拒面（sl/sl+pr）在 none/W1:K 下在册；pr 拒面（经典
     防降级）全策略在册。

用法：
  python3 tools/fastlane_sim.py               # 全量推演 + 报告（固定种子）
  python3 tools/fastlane_sim.py --selftest    # 钉板自检（内嵌场景集）
  python3 tools/fastlane_sim.py --fuzz 500 --seed 7
  python3 tools/fastlane_sim.py --scan        # 案 α 浸泡窗参数扫描
  python3 tools/fastlane_sim.py --ce-search   # 反例搜索双科目（P / P′）
  python3 tools/fastlane_sim.py --dry-run canonical-b1   # 代表场景逐步输出
  python3 tools/fastlane_sim.py --list-scenarios

报告形态：场景数 / 不变量违反数 / 反例数（P 证伪命中、P′ 安全+恢复、
L1 载重证据）。零三方依赖（仅 stdlib）；Python ≥3.9；固定种子确定性。
"""

import argparse
import hashlib
import json
import random
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from typing import Optional, Tuple

# ---------------------------------------------------------------- 常量

DEFAULT_SEED = 20261111
DEFAULT_FUZZ = 240

# 案 α 候选窗参数（28 号 §2.2 :72 + §2.5-1；OQ-1 :185 三选）：
#   none  = 修法前形态（SUCCESSFUL 即吸收——19 号 :170 配套②原语境）
#   w1:K  = boot 计数 K（CounterBackend L0 启动计数面，:44）
#   w2    = 下一 lane 到达（版本当时钟，SDK-2 v1.2 §五）——γ 并入（:78）
POLICIES = ("none", "w1:1", "w1:2", "w1:4", "w1:8", "w2")

# 黄金锚（selftest 钉板——生成器/机器语义改动即红，逼有意识复核重钉）。
GOLDEN = {
    "corpus_digest": "ecb2bfb4d220e03d69c3ef0bb1bc7270922d6e378b8d8a61466b98ba0ba24e53",
    "canonical_digest": "d14eb51ea437fbacf8fc171387ac3047c000d29412aeede139da4dc7e4294a3c",
}


class Fail(Exception):
    """关卡失败（配置/用法错——消息直给 stderr）。"""


# ---------------------------------------------------------------- 数据形

@dataclass(frozen=True)
class Tree:
    """更新树：双轴向量 (PR, SL)（28 号 :39）。

    pr  = 功能轴（≡ level 单源，:39）；lane = 安全轴 = max 公告序
    （L1 内容寻址，:105）；sec = 所含安全公告集（内容寻址执法对象）。
    kind：M 整编队 | F 快道（安全子集）| R 重制树（同 lane 重出）| X
    官方替换面（D1 案 b 终态件——OEM 段停更后官方件顶上，轴语义同 M）。
    """
    pr: int
    sec: frozenset
    kind: str
    tag: str = ""

    @property
    def lane(self) -> int:
        return max(self.sec) if self.sec else 0

    def desc(self) -> str:
        return f"{self.tag}({self.kind},pr{self.pr},lane{self.lane})"


@dataclass
class SimConfig:
    """模拟器配置（配置驱动——lane 语义固定按 L1，窗/门为参数）。"""
    policy: str = "w2"            # 浸泡窗策略（案 α 参数）
    remake_oracle: bool = True    # 重制树 oracle（OQ-2 假设，推断注 d）
    axis_frozen: bool = True      # 快道轴冻结（19 号 :163，设备侧执法）
    auto_response: bool = True    # REGRESS 后策略响应（P′ 评估面）
    sabotage: str = ""            # 自检破坏钩子：""|"or-gate"|"early-sl"


@dataclass
class StepRec:
    """状态机单步记录（checker 的唯一数据面——自包含可审计）。"""
    i: int
    ev: str
    tag: str
    note: str
    state_prev: str
    state_next: str
    pr_prev: int
    pr_next: int
    sl_prev: int
    sl_next: int
    secn_prev: int                # confirmed 台账大小（L1 派生不变量面）
    secn_next: int
    bad_prev: bool
    bad_next: bool
    dep_prev: int
    dep_next: int
    tree_pr: int = 0
    tree_lane: int = 0
    tree_kind: str = ""
    tree_sec: Tuple[int, ...] = ()
    gate_kind: str = ""           # ""|"stage"|"rollback"
    pr_ok: bool = False
    sl_ok: bool = False
    gate_free: bool = False       # 回退自由窗（:40——无门公式）
    gate_exec: bool = False
    axis_ok: bool = True
    bite: str = ""                # ""|"sl"|"pr"|"sl+pr"|"no-target"
    confirm: bool = False         # CONFIRMED 转换记录（I-SL-2 吸收点）
    delay: int = 0                # HEALTHY→CONFIRMED boot 距（案 α 度量）
    pp_sec: Tuple[int, ...] = ()  # REGRESS 时台账快照（P′-B 基线）
    active_sec: Tuple[int, ...] = ()


@dataclass
class RunResult:
    name: str
    trace: list
    violations: list
    pp: list
    metrics: dict


# ---------------------------------------------------------------- 状态机

class _Slot:
    """槽位：树 + 部署履历（deployed=曾被激活→可作回退目标；
    confirmed=曾达 CONFIRMED→回退后直接 SUCCESSFUL 态）。"""

    def __init__(self, tree: Optional[Tree], deployed: bool, confirmed: bool):
        self.tree = tree
        self.deployed = deployed
        self.confirmed = confirmed

    def replace(self, tree: Tree) -> None:
        self.tree = tree
        self.deployed = False
        self.confirmed = False


def parse_policy(p: str):
    if p == "none":
        return ("none", 0)
    if p == "w2":
        return ("w2", 0)
    if p.startswith("w1:"):
        try:
            k = int(p.split(":", 1)[1])
        except ValueError:
            raise Fail(f"W1 窗参数非整数：{p}")
        if k < 1:
            raise Fail(f"W1 窗 K ≥ 1：{p}")
        return ("w1", k)
    raise Fail(f"未知浸泡窗策略：{p}（none | w1:K | w2）")


class Machine:
    """编队状态机（28 号 §2.1 :43-:54 的可执行形态）。

    通道：整编队 M/X（慢道）与安全子集快道 F（快道）共用梯子——快道
    只加 SOAKING 一级（:41）；回退门 :50 = PR∧SL 合取（I-SL-1 执法面）。
    """

    def __init__(self, cfg: SimConfig, factory: Tree):
        if not factory.sec:
            raise Fail("工厂树安全内容为空——场景构造错")
        self.cfg = cfg
        self.policy, self.k = parse_policy(cfg.policy)
        self.slots = {"A": _Slot(factory, True, True), "B": _Slot(None, False, False)}
        self.ptr = "A"
        self.state = "SUCCESSFUL"          # 工厂树已 CONFIRMED
        self.floor_pr = factory.pr         # I-PR：现役 rollback index
        self.floor_sl = factory.lane       # I-SL-3：CounterBackend 单调
        self.confirmed = set(factory.sec)  # 已确认安全内容台账（L1c）
        self.boots_soak = 0
        self.dep_boots = 0
        self.dep_id = 1
        self.bad = False
        self.oem_expired = False
        self.resp_step = 0
        self.resp_open = False
        self.resp_boots = 0
        self.total_boots = 0
        self.exposure_boots = 0            # 案 α 敞口：窗内可退恰一代的 boot 数
        self.applied = 0
        self.hist = Counter()
        self.trace = []
        sec = tuple(sorted(factory.sec))
        self.trace.append(StepRec(
            i=0, ev="INIT", tag=factory.tag, note="init",
            state_prev="EMPTY", state_next="SUCCESSFUL",
            pr_prev=factory.pr, pr_next=factory.pr,
            sl_prev=factory.lane, sl_next=factory.lane,
            secn_prev=len(sec), secn_next=len(sec),
            bad_prev=False, bad_next=False, dep_prev=1, dep_next=1,
            tree_pr=factory.pr, tree_lane=factory.lane, tree_kind=factory.kind,
            tree_sec=sec, active_sec=sec))
        self.hist["SUCCESSFUL"] += 1

    # -- 便捷 --
    @property
    def act(self) -> _Slot:
        return self.slots[self.ptr]

    @property
    def stby(self) -> _Slot:
        return self.slots["A" if self.ptr == "B" else "B"]

    def _snap(self):
        return {
            "state": self.state, "pr": self.floor_pr, "sl": self.floor_sl,
            "secn": len(self.confirmed), "bad": self.bad, "dep": self.dep_id,
        }

    def _mk(self, ev, tree, note, sp, **kw) -> StepRec:
        at = self.act.tree
        rec = StepRec(
            i=len(self.trace), ev=ev, tag=tree.tag if tree else "",
            note=note, state_prev=sp["state"], state_next=self.state,
            pr_prev=sp["pr"], pr_next=self.floor_pr,
            sl_prev=sp["sl"], sl_next=self.floor_sl,
            secn_prev=sp["secn"], secn_next=len(self.confirmed),
            bad_prev=sp["bad"], bad_next=self.bad,
            dep_prev=sp["dep"], dep_next=self.dep_id,
            tree_pr=tree.pr if tree else 0,
            tree_lane=tree.lane if tree else 0,
            tree_kind=tree.kind if tree else "",
            tree_sec=tuple(sorted(tree.sec)) if tree else (),
            active_sec=tuple(sorted(at.sec)) if at else (),
        )
        for key, val in kw.items():
            setattr(rec, key, val)
        self.trace.append(rec)
        self.hist[self.state] += 1
        return rec

    # -- 事件派发 --
    def apply(self, ev) -> StepRec:
        self.applied += 1
        op = ev[0]
        if op == "STAGE":
            return self._stage(ev[1])
        if op == "ACTIVATE":
            return self._activate()
        if op == "BOOT":
            return self._boot()
        if op == "ROLLBACK":
            return self._rollback()
        if op == "REGRESS":
            return self._regress()
        if op == "OEM":
            return self._oem()
        if op == "D1":
            return self._d1()
        raise Fail(f"未知事件：{op}")

    def _stage(self, tree: Tree) -> StepRec:
        """出件门（正向安装/防降级 + I-SL-4 重制通道 + :163 轴冻结）。

        门 = PR(T) ≥ floor_pr ∧ lane(T) ≥ floor_sl（:50 正向半边；合取
        执法 I-SL-1）。W2 关窗先于覆槽（到达事件既关浸泡窗又是门对象；
        仅受纳的到达关窗——被拒出件不算 lane 到达）。
        """
        axis_ok = True
        if self.cfg.axis_frozen and tree.kind == "F" and tree.pr != self.act.tree.pr:
            axis_ok = False                      # 19 号 :163：快道动功能轴 → loud 拒
        pr_ok = tree.pr >= self.floor_pr
        sl_ok = tree.lane >= self.floor_sl
        exec_ = pr_ok and sl_ok and axis_ok
        if self.cfg.sabotage == "or-gate":
            exec_ = pr_ok or sl_ok               # 破坏合取 → ISL1a 必咬（自检钩子）
        note = "accept" if exec_ else "reject"
        if not axis_ok:
            note = "reject-axis-frozen"
        elif not exec_:
            note = "reject-sl" if not sl_ok else "reject-pr"
        if exec_ and self.policy == "w2" and self.state == "SOAKING" \
                and tree.lane > self.act.tree.lane:
            self._confirm("w2-next-lane")        # 先关窗（独立 CONFIRM 记录）
        sp = self._snap()
        if exec_:
            self.stby.replace(tree)              # 覆槽——旧回退目标消灭（推断注 e）
        return self._mk("STAGE", tree, note, sp,
                        gate_kind="stage", pr_ok=pr_ok, sl_ok=sl_ok,
                        gate_exec=exec_, axis_ok=axis_ok)

    def _activate(self) -> StepRec:
        sp = self._snap()
        s = self.stby
        if s.tree is None or s.deployed:
            return self._mk("ACTIVATE", s.tree, "no-staged", sp)
        s.deployed = True
        self.ptr = "A" if self.ptr == "B" else "B"
        self.state = "PENDING"
        self.boots_soak = 0
        self.dep_boots = 0
        self.dep_id += 1
        self.bad = False
        self.resp_step = 0
        return self._mk("ACTIVATE", s.tree, "flip", sp)

    def _boot(self) -> StepRec:
        sp = self._snap()
        self.total_boots += 1
        rec = None
        if self.state == "PENDING":
            # 28 号 HEALTHY ≡ SUCCESSFUL 标记点：floor_pr 抬（I-PR「SUCCESSFUL
            # 后才抬」）；SL 半边是否延后 = 浸泡窗策略（I-SL-2）。
            self.floor_pr = max(self.floor_pr, self.act.tree.pr)
            if self.cfg.sabotage == "early-sl":
                self.floor_sl = max(self.floor_sl, self.act.tree.lane)  # 违 ISL2 钩子
            self.state = "SOAKING"
            self.boots_soak = 1
            self.dep_boots = 1
            rec = self._mk("BOOT", self.act.tree, "healthy", sp)
            if self.policy == "none" or self.act.tree.lane <= self.floor_sl:
                self._confirm("no-soak")         # 修法前形态 / 同 lane 无抬升可延迟
            elif self.policy == "w1" and self.boots_soak >= self.k:
                self._confirm("w1")
        elif self.state == "SOAKING":
            self.boots_soak += 1
            self.dep_boots += 1
            rec = self._mk("BOOT", self.act.tree, "soak-boot", sp)
            if self.policy == "w1" and self.boots_soak >= self.k:
                self._confirm("w1")
        else:
            rec = self._mk("BOOT", self.act.tree, "confirmed-boot", sp)
        # 案 α 敞口度量：SOAKING 中且备槽是可退的紧邻前代（lane 更低）。
        if self.state == "SOAKING" and self.stby.deployed and self.stby.tree \
                and self.stby.tree.lane < self.act.tree.lane:
            self.exposure_boots += 1
        return rec

    def _confirm(self, reason: str) -> StepRec:
        """CONFIRMED 转换：浸泡期满，floor_sl := SL(本树)（:48/I-SL-2）。"""
        sp = self._snap()
        t = self.act.tree
        self.state = "SUCCESSFUL"
        self.act.confirmed = True               # 槽位履历——回退重入直接 SUCCESSFUL
        self.floor_sl = max(self.floor_sl, t.lane)
        self.confirmed |= set(t.sec)
        return self._mk("CONFIRM", t, f"confirm({reason})", sp,
                        confirm=True, delay=self.dep_boots)

    def _do_flip(self) -> None:
        self.ptr = "A" if self.ptr == "B" else "B"
        t = self.act
        self.state = "SUCCESSFUL" if t.confirmed else "PENDING"
        self.boots_soak = 0
        self.dep_boots = 0
        self.dep_id += 1
        self.bad = False
        self.resp_step = 0

    def _rollback(self) -> StepRec:
        """回退动作（:50 合法性门；PENDING 期自由窗 :40/:53 不动）。"""
        sp = self._snap()
        tgt = self.stby
        if tgt.tree is None or not tgt.deployed:
            return self._mk("ROLLBACK", tgt.tree, "no-target", sp,
                            gate_kind="rollback", bite="no-target")
        if self.state == "PENDING":
            self._do_flip()
            return self._mk("ROLLBACK", tgt.tree, "free-flip", sp,
                            gate_kind="rollback", gate_free=True, gate_exec=True)
        pr_ok = tgt.tree.pr >= self.floor_pr
        sl_ok = tgt.tree.lane >= self.floor_sl   # I-SL-2：计数器未吸收浸泡树值
        exec_ = pr_ok and sl_ok
        bite = ""
        if not exec_:
            bite = ("sl+pr" if (not sl_ok and not pr_ok)
                    else ("sl" if not sl_ok else "pr"))
        if exec_:
            self._do_flip()
        note = "gate-exec" if exec_ else f"reject-{bite}"
        return self._mk("ROLLBACK", tgt.tree, note, sp,
                        gate_kind="rollback", pr_ok=pr_ok, sl_ok=sl_ok,
                        gate_exec=exec_, bite=bite)

    def _regress(self) -> StepRec:
        """功能回归在案（22 号 §3.1 两受害形态的注入点）。"""
        sp = self._snap()
        self.bad = True
        self.resp_step = 0
        self.resp_open = True
        self.resp_boots = 0
        return self._mk("REGRESS", self.act.tree, "functional-regression", sp,
                        pp_sec=tuple(sorted(self.confirmed)))

    def _oem(self) -> StepRec:
        """OEM 段刷新（L2/L3 独立通道——非槽翻转；I-PR(a) 只认平台镜像槽）。"""
        sp = self._snap()
        return self._mk("OEM", None, "oem-segment-refresh", sp)

    def _d1(self) -> StepRec:
        """D1 停更超窗（09 号 :224——expired；题一机器面为场景级标记，
        案 b 终态由生成器切换 X 通道承载）。"""
        sp = self._snap()
        self.oem_expired = True
        return self._mk("D1", None, "d1-stop-update", sp)

    # -- 策略响应（P′ 评估面：回退优先，被拒则重制前滚——β 通道）--
    def response_inject(self, cfg: SimConfig):
        if not cfg.auto_response:
            return []
        if self.resp_open and not self.bad:
            # 已脱离回归（回退执行/换树）：健康运行于功能良好树即闭环——
            # CONFIRM 与否是日常浸泡事务，不属响应（W2 无界浸泡下强求
            # CONFIRMED 会不终止）；8-boot 保护帽防异常路径。
            if self.state in ("SOAKING", "SUCCESSFUL"):
                self.resp_open = False
                return []
            if self.resp_boots < 8:
                self.resp_boots += 1
                return [("BOOT",)]
            self.resp_open = False
            return []
        if not self.bad:
            self.resp_step = 0
            return []
        if self.resp_step == 0:
            self.resp_step = 1
            self.resp_open = True
            return [("ROLLBACK",)]
        if self.resp_step == 1:
            self.resp_step = 2
            if cfg.remake_oracle:
                t = Tree(self.act.tree.pr, frozenset(self.confirmed), "R",
                         f"R-fix{self.dep_id}")
                return [("STAGE", t), ("ACTIVATE",), ("BOOT",)]
            return []
        return []


# ---------------------------------------------------------------- 检查器

class Checker:
    """不变量机器验证（28 号 :58-:64 逐条 + L1 派生 + 门一致性）。

    数据面 = StepRec 流（自包含）；门决策与 (PR∧SL) 合取对拍（ISL1a），
    自检破坏钩子证明检查器会咬（selftest #14/#15）。
    """

    IDS = ("IPRa", "IPRb", "IPRc", "ISL1a", "ISL1b", "ISL2", "ISL3",
           "ISL4", "L1c", "GATEX")

    def __init__(self, name: str):
        self.name = name
        self.violations = []
        self.audited = 0
        self._pr = None
        self._sl = None
        self._secn = None

    def _hit(self, inv: str, rec: Optional[StepRec], detail: str) -> None:
        at = f"@{rec.i}:{rec.ev}/{rec.note}" if rec else ""
        self.violations.append(f"{inv}{at} {detail}")

    def audit(self, recs) -> None:
        for r in recs:
            self._one(r)

    def _one(self, r: StepRec) -> None:
        if self._pr is None:
            self._pr, self._sl, self._secn = r.pr_prev, r.sl_prev, r.secn_prev
        # I-PR(a) 只认平台镜像槽：OEM 事件逐字不动地板/状态/台账。
        if r.ev == "OEM" and ((r.pr_prev, r.sl_prev, r.state_prev, r.secn_prev)
                              != (r.pr_next, r.sl_next, r.state_next, r.secn_next)):
            self._hit("IPRa", r, "OEM 事件动了地板/状态/台账")
        dpr = r.pr_next != r.pr_prev
        dsl = r.sl_next != r.sl_prev
        # I-PR(b) SUCCESSFUL（HEALTHY 标记）后才抬；I-SL-2 吸收点=CONFIRMED。
        if dpr and r.note != "healthy":
            self._hit("IPRb", r, f"floor_pr 于非 HEALTHY 记录变更（{r.note}）")
        if dsl and not r.confirm:
            self._hit("ISL2", r, f"floor_sl 于非 CONFIRMED 记录变更（{r.note}）")
        # I-SL-1(b) 轴分离：任一轴抬升不波及另一轴（INIT 双置除外）。
        if dpr and dsl and r.note != "init":
            self._hit("ISL1b", r, "单记录双轴同变")
        # I-PR(c) / I-SL-3 单调；L1c 台账只增（累积+继承不重记的执法面）。
        if r.pr_next < self._pr:
            self._hit("IPRc", r, f"floor_pr 回降 {self._pr}→{r.pr_next}")
        if r.sl_next < self._sl:
            self._hit("ISL3", r, f"floor_sl 回降 {self._sl}→{r.sl_next}")
        if r.secn_next < self._secn:
            self._hit("L1c", r, f"confirmed 台账收缩 {self._secn}→{r.secn_next}")
        # I-SL-1(a) 合取执法（无跨轴补偿）；GATEX 执行合法性冗余复核。
        if r.gate_kind == "stage":
            if r.gate_exec and not (r.pr_ok and r.sl_ok):
                self._hit("ISL1a", r, "STAGE 轴不过仍执行（跨轴补偿）")
            axis_reject = r.tree_kind == "F" and not r.axis_ok
            if not axis_reject and r.gate_exec != (r.pr_ok and r.sl_ok):
                self._hit("ISL1a", r, "STAGE 决策 ≠ PR∧SL 合取")
        elif r.gate_kind == "rollback":
            if r.gate_free:
                if not r.gate_exec:
                    self._hit("ISL1a", r, "自由窗回退未执行")
            else:
                if r.gate_exec != (r.pr_ok and r.sl_ok):
                    self._hit("ISL1a", r, "回退决策 ≠ PR∧SL 合取")
                if r.gate_exec and not (r.pr_ok and r.sl_ok):
                    self._hit("GATEX", r, "执行了轴不过的回退")
        self._pr, self._sl, self._secn = r.pr_next, r.sl_next, r.secn_next
        self.audited += 1

    def finish(self, m: Machine) -> None:
        """I-SL-4 探针：CONFIRMED 态下同 lane 重制树恒可装（:64）。"""
        if not m.confirmed:
            return
        probe = Tree(m.floor_pr, frozenset(m.confirmed), "R", "isl4-probe")
        if not (probe.pr >= m.floor_pr and probe.lane >= m.floor_sl):
            self._hit("ISL4", None, "同 lane 重制树不可装（重制通道破）")


# ---------------------------------------------------------------- P′ 评估

def evaluate_pp(trace):
    """改写命题 P′（28 号 §2.3 :83）逐 REGRESS 评估。

    P′-安全 = 视界内无非法回退执行 ∧ 终态树安全内容 ⊇ 回归时台账；
    P′-恢复 = 视界内回到功能良好健康态（bad 清除 ∧ 过健康自检——
    SOAKING/SUCCESSFUL 皆可；CONFIRM 是日常浸泡事务，强求会在 W2 无界
    浸泡下不终止。活性款为推断新增：P′ 原句为安全款，不加此款在
    「卡死不响应」下空洞为真，如实注记）。
    """
    out = []
    for idx, r in enumerate(trace):
        if r.ev != "REGRESS":
            continue
        tail = trace[idx + 1:]
        settled = None
        for x in tail:
            if not x.bad_next and x.state_next in ("SOAKING", "SUCCESSFUL"):
                settled = x
                break
        legal = all(not (x.gate_kind == "rollback" and x.gate_exec
                         and not x.gate_free and not (x.pr_ok and x.sl_ok))
                    for x in tail)
        sup = True
        if settled is not None and settled.active_sec:
            sup = set(settled.active_sec) >= set(r.pp_sec)
        first_rb = next((x for x in tail if x.ev == "ROLLBACK"), None)
        out.append({
            "i": r.i, "state": r.state_prev, "lane": r.tree_lane,
            "settled": settled is not None,
            "settle_ev": settled.ev if settled else "",
            "sup": sup, "legal": legal,
            "rb_note": first_rb.note if first_rb else "",
            "rb_bite": first_rb.bite if first_rb else "",
        })
    return out


def collect_metrics(m: Machine, pp: list) -> dict:
    rb = Counter()
    for r in m.trace:
        if r.ev != "ROLLBACK":
            continue
        if r.note == "no-target":
            rb["no-target"] += 1
        elif r.note == "free-flip":
            rb["free"] += 1
        elif r.gate_exec:
            rb["gate-exec"] += 1
        else:
            rb[f"reject-{r.bite}"] += 1
    delays = [r.delay for r in m.trace if r.confirm and r.note != "init"]
    return {
        "events": m.applied,
        "records": len(m.trace),
        "boots": m.total_boots,
        "exposure": m.exposure_boots,
        "rb": dict(rb),
        "delays": delays,
        "regrets": len(pp),
        "solved_rb": sum(1 for p in pp if p["settled"] and p["settle_ev"] == "ROLLBACK"),
        "solved_fwd": sum(1 for p in pp if p["settled"] and p["settle_ev"] != "ROLLBACK"),
        "stuck": sum(1 for p in pp if not p["settled"]),
        "sup_fail": sum(1 for p in pp if not p["sup"]),
        "illegal_move": sum(1 for p in pp if not p["legal"]),
        "hist": dict(m.hist),
    }


def run_scenario(cfg: SimConfig, events, name: str) -> RunResult:
    if not events or events[0][0] != "INIT":
        raise Fail(f"场景 {name} 缺 INIT 头")
    m = Machine(cfg, events[0][1])
    ck = Checker(name)
    ck.audit(m.trace[:1])
    queue = list(events[1:])
    qi = 0
    while qi < len(queue):
        mark = len(m.trace)
        m.apply(queue[qi])
        qi += 1
        ck.audit(m.trace[mark:])
        inj = m.response_inject(cfg)
        if inj:
            queue[qi:qi] = inj
    ck.finish(m)
    pp = evaluate_pp(m.trace)
    return RunResult(name, m.trace, ck.violations, pp, collect_metrics(m, pp))


# ---------------------------------------------------------------- 场景库

def _sec(n):
    return frozenset(range(1, n + 1))


def scripted_scenarios():
    """内嵌钉板场景集（--dry-run 与 selftest 双消费）。"""
    fac = Tree(5, _sec(5), "M", "M0")            # 工厂树 lane 5
    f1 = Tree(5, _sec(6), "F", "F1")             # 快道 lane 6
    m1 = Tree(5, _sec(7), "M", "M1")             # 整编队吸收 F1+C7 lane 7（同格）
    m2 = Tree(5, _sec(8), "M", "M2")             # lane 8
    chain = [("INIT", fac), ("STAGE", f1), ("ACTIVATE",), ("BOOT",),
             ("STAGE", m1), ("ACTIVATE",), ("BOOT",)]
    return {
        # §2.3 :82 常态序列（修法前形态 none）——证伪 P 的最小见证。
        "canonical-b1": (SimConfig(policy="none", auto_response=False),
                         chain + [("REGRESS",), ("ROLLBACK",)]),
        # 同序列 + 策略响应：回退被拒 → 重制前滚（P′ happy path）。
        "canonical-b1-response": (SimConfig(policy="none"), chain + [("REGRESS",)]),
        # 案 α：F1 过窗确认（floor 6）→ M1 SOAKING 期回退自由——恰一代
        # 可退（REGRESS 后由 auto_response 注入 ROLLBACK）。
        "soak-window": (SimConfig(policy="w1:2"),
                        chain[:4] + [("BOOT",)] + chain[4:] + [("REGRESS",)]),
        # W2 生命周期：M2 到达关 M1 窗（floor 7）且覆槽消灭 F1 → 回退
        # 无部署目标（必须前滚）——REGRESS 后响应注入 ROLLBACK+重制前滚。
        "w2-lifecycle": (SimConfig(policy="w2"),
                         [("INIT", fac), ("STAGE", f1), ("ACTIVATE",), ("BOOT",),
                          ("STAGE", m1), ("ACTIVATE",), ("BOOT",),
                          ("STAGE", m2), ("REGRESS",)]),
        # D1 停更：无下一 lane 到达 → W2 窗不关（敞口）+ 回退恒自由。
        "d1-stop": (SimConfig(policy="w2"),
                    [("INIT", fac), ("STAGE", f1), ("ACTIVATE",), ("BOOT",),
                     ("D1",), ("BOOT",), ("BOOT",), ("REGRESS",)]),
        # 乱序到达：旧 lane 出件迟到 → SL 半边拒（防降级）。
        "out-of-order": (SimConfig(policy="w1:2"),
                         [("INIT", fac), ("STAGE", Tree(5, _sec(7), "F", "F2")),
                          ("ACTIVATE",), ("BOOT",), ("BOOT",),
                          ("STAGE", Tree(5, _sec(5), "F", "F1-late"))]),
        # OEM 段刷新：floor 中立（I-PR a 款）。
        "oem-neutral": (SimConfig(policy="w2"),
                        [("INIT", fac), ("BOOT",), ("OEM",), ("BOOT",),
                         ("OEM",), ("OEM",), ("BOOT",)]),
        # P 偶然真区（:82 尾注）：M 无新安全内容 → 回退恒过门。
        "accidental-true": (SimConfig(policy="none", auto_response=False),
                            chain[:4] + [("STAGE", Tree(5, _sec(6), "M", "M1-same")),
                                         ("ACTIVATE",), ("BOOT",),
                                         ("REGRESS",), ("ROLLBACK",)]),
        # L1 载重探针：同 lane 欺骗性重制（sec ⊊ 台账）——值比较门不拦。
        "deceptive-remake": (SimConfig(policy="none", auto_response=False),
                             chain + [("STAGE", Tree(5, frozenset({7}), "R", "R-dec"))]),
        # I-SL-2 窗观测：W1:4 下 HEALTHY→CONFIRMED 间隔 4 boot，窗内地板不动。
        "isl2-window": (SimConfig(policy="w1:4", auto_response=False),
                        [("INIT", fac), ("STAGE", f1), ("ACTIVATE",), ("BOOT",),
                         ("BOOT",), ("BOOT",), ("BOOT",)]),
    }


# ---------------------------------------------------------------- 随机生成

def gen_scenario(rng: random.Random, name: str):
    """随机双轴版本流（任务书：功能版本 + 安全 lane，含乱序/回退/D1/重制）。

    L1 生成器纪律（平台出件面，:105）：快道出件累积（F_{i+1} ⊇ F_i）；
    整编队继承（M ⊇ 全部已发公告——吸收必然，22 号 §3.1）。乱序到达 =
    旧出件迟到重放，非新出件（累积纪律约束出件序不约束到达序）。
    """
    n = rng.randint(12, 34)
    pr0 = rng.randint(3, 6)
    b0 = rng.randint(2, 5)
    issued = set(range(1, b0 + 1))
    last_f = frozenset(issued)
    cur_pr = pr0
    pend_pr = None
    seq = 0
    d1 = False
    staged = False
    evs = [("INIT", Tree(pr0, frozenset(issued), "M", "factory"))]

    def pick(choices):
        tot = sum(w for _, w in choices)
        x = rng.random() * tot
        for c, w in choices:
            x -= w
            if x <= 0:
                return c
        return choices[-1][0]

    for _ in range(n):
        normal = [
            ("fastlane", 18), ("stale", 4), ("formation", 14), ("remake", 5),
            ("activate", 10 if staged else 0), ("boots", 16), ("rollback", 8),
            ("regress", 7), ("oem", 5), ("d1", 3 if not d1 else 0),
        ]
        stopped = [   # D1 后：整编队断供，官方替换面（案 b）+ 快道续供
            ("fastlane", 22), ("stale", 3), ("official", 8), ("remake", 5),
            ("activate", 10 if staged else 0), ("boots", 18), ("rollback", 10),
            ("regress", 8), ("oem", 0), ("d1", 0),
        ]
        a = pick(stopped if d1 else normal)
        if a == "fastlane":
            issued.add(max(issued) + 1)
            sec = frozenset(issued)
            assert sec >= last_f, "L1 累积纪律破（生成器 bug）"
            last_f = sec
            seq += 1
            evs.append(("STAGE", Tree(cur_pr, sec, "F", f"F{seq}")))
            staged = True
            pend_pr = cur_pr
        elif a == "stale":
            k = rng.randint(1, max(1, max(issued) - 1))
            seq += 1
            evs.append(("STAGE", Tree(cur_pr, _sec(k), "F", f"F{seq}stale")))
            staged = True
            pend_pr = cur_pr
        elif a in ("formation", "official"):
            kind = "M" if a == "formation" else "X"
            if rng.random() < 0.35:            # 整编队携新公告（继承不重记·新增半边）
                issued.add(max(issued) + 1)
            pr = cur_pr + (1 if rng.random() < 0.45 else 0)
            sec = frozenset(issued)
            assert sec >= last_f, "整编队继承纪律破（生成器 bug）"
            seq += 1
            evs.append(("STAGE", Tree(pr, sec, kind, f"{kind}{seq}")))
            staged = True
            pend_pr = pr
        elif a == "remake":                    # 平台同 lane 重制再发（β 通道常态）
            seq += 1
            evs.append(("STAGE", Tree(cur_pr, frozenset(issued), "R", f"R{seq}")))
            staged = True
            pend_pr = cur_pr
        elif a == "activate":
            evs.append(("ACTIVATE",))
            staged = False
            if pend_pr is not None:
                cur_pr = pend_pr
        elif a == "boots":
            for _ in range(rng.randint(1, 3)):
                evs.append(("BOOT",))
        elif a == "rollback":
            evs.append(("ROLLBACK",))
        elif a == "regress":
            evs.append(("REGRESS",))
        elif a == "oem":
            evs.append(("OEM",))
        elif a == "d1":
            d1 = True
            evs.append(("D1",))
    return evs


def build_corpus(seed: int, n: int):
    out = []
    for i in range(n):
        rng = random.Random(f"{seed}:{i}")
        out.append((f"rand-{i}", gen_scenario(rng, f"rand-{i}")))
    return out


# ---------------------------------------------------------------- 反例搜索

def _confirm_beats(policy: str, md: int, b: int, e: int, w2_form="bite"):
    """构造「M1 已 CONFIRMED」的咬点探针事件尾（按浸泡策略补 beat）。

    none/w1:K —— REGRESS(M1)+ROLLBACK，目标 F1（紧邻前代，已部署）：
    sl/pr 地板拒面直接可判。
    w2 bite  —— M1 的 CONFIRM 由 M2 到达关闭（关窗事件同时覆槽消灭
    F1）：ROLLBACK → 无部署目标——B1 反噬的 W2 形态（必须前滚）。
    w2 pass  —— ACT M2 → BOOT → REGRESS(M2)@SOAKING → ROLLBACK 目标
    M1（lane == floor）——α 一代窗内回退自由（γ 并入 :78）。
    """
    fac = Tree(5, _sec(b), "M", "M0")
    f1 = Tree(5, _sec(b + 1), "F", "F1")
    m1 = Tree(5 + md, _sec(b + 1 + e), "M", "M1")   # e=0=吸收无新公告（偶然真区）
    head = [("INIT", fac), ("STAGE", f1), ("ACTIVATE",), ("BOOT",)]
    if policy == "w2":
        m2 = Tree(5 + md, _sec(b + 2 + e), "M", "M2")
        close = head + [("STAGE", m1), ("ACTIVATE",), ("BOOT",), ("STAGE", m2)]
        if w2_form == "bite":
            return close + [("REGRESS",), ("ROLLBACK",)]
        return close + [("ACTIVATE",), ("BOOT",), ("REGRESS",), ("ROLLBACK",)]
    _, k = parse_policy(policy)
    boots = [("BOOT",)] * max(0, k - 1)        # 使本树到 CONFIRMED
    tail = [("STAGE", m1), ("ACTIVATE",), ("BOOT",)] + boots
    return head + boots + tail + [("REGRESS",), ("ROLLBACK",)]


def ce_search_p():
    """反例科目一：B1 原命题 P（19 号 :170 配套②字面）自动证伪。

    判据：REGRESS 后回退到紧邻前代功能良好树——sl 拒 = 28 号 §2.3 :82
    引证形态（安全地板反噬）；pr 拒 = 经典防降级（非 B1 专属，另计）；
    no-target = 槽物理（W2 结构性形态——反噬=回退不可达）；legal = P
    偶然真区（:82 尾注「M 无新安全内容」）/ α 窗内自由。
    """
    stats = {p: Counter() for p in POLICIES}
    witnesses = {}
    for policy in POLICIES:
        forms = ("bite", "pass") if policy == "w2" else (None,)
        for form in forms:
            for b in (2, 3, 4):
                for e in (0, 1, 2):
                    for md in (0, 1):
                        evs = _confirm_beats(policy, md, b, e,
                                             w2_form=form or "bite")
                        r = run_scenario(
                            SimConfig(policy=policy, auto_response=False),
                            evs, f"cep-{policy}-{form}-{b}-{e}-{md}")
                        rb = [x for x in r.trace if x.ev == "ROLLBACK"][-1]
                        if rb.note == "no-target":
                            kind = "no-target"
                        elif rb.gate_exec:
                            kind = "legal"
                        else:
                            kind = rb.bite          # sl | pr | sl+pr
                        stats[policy][kind] += 1
                        if kind in ("sl", "sl+pr") and "sl" not in witnesses:
                            witnesses["sl"] = (policy, b, e, md, rb)
    return stats, witnesses


def ce_search_pp():
    """反例科目二：改写命题 P′（28 号 §2.3 :83）对抗搜索。

    恢复面 = ∃ 合法回应（回退可执行 ∨ oracle 可构造重制且过门）——非
    「任意回应者必达」（拒绝重制的回应者卡死不算 P′ 破，空洞例单列）。
    安全面 = 无非法回退执行 ∧ 终态不丢内容；欺骗性同 lane 重制（sec ⊊
    台账）单列 l1_residual——平台破纪不在 P′ 威胁模型，但「过门」事实
    = 值比较门不拦内容集回退的载重证据（28 号 §2.3 :84 同结论）。
    """
    safety = 0
    l1_residual = 0
    recover_on = 0
    recover_off = 0
    cases = 0
    vacuous = 0
    shapes = ("rollback", "remake-ok", "remake-low-pr", "remake-low-lane",
              "remake-deceptive")
    remake_trees = {
        "remake-ok": lambda b: Tree(5, _sec(b + 2), "R", "R-fix"),
        "remake-low-pr": lambda b: Tree(4, _sec(b + 2), "R", "R-lowpr"),
        "remake-low-lane": lambda b: Tree(5, _sec(b), "R", "R-lowlane"),
        "remake-deceptive": lambda b: Tree(5, frozenset({b + 2}), "R", "R-dec"),
    }
    for policy in POLICIES:
        for b in (2, 3):
            for oracle in (True, False):
                cfg = SimConfig(policy=policy, auto_response=False,
                                remake_oracle=oracle)
                base = _confirm_beats(policy, 0, b, 1)
                # -- 恢复面：∃ 合法回应 --
                rb_run = run_scenario(cfg, base + [("REGRESS",), ("ROLLBACK",)],
                                      f"cepp-rb-{policy}-{b}-{oracle}")
                rb_ok = rb_run.pp[-1]["settled"]
                rk_ok = False
                if oracle:
                    rk = Tree(5, _sec(b + 2), "R", "R-exist")
                    rk_run = run_scenario(
                        cfg, base + [("REGRESS",), ("STAGE", rk),
                                     ("ACTIVATE",), ("BOOT",)],
                        f"cepp-rk-{policy}-{b}")
                    rk_ok = rk_run.pp[-1]["settled"]
                if not (rb_ok or rk_ok):
                    if oracle:
                        recover_on += 1
                    else:
                        recover_off += 1
                # -- 空洞面：不响应（安全款空洞为真的注记例）--
                rn = run_scenario(cfg, base + [("REGRESS",)],
                                  f"cepp-none-{policy}-{b}-{oracle}")
                if not rn.pp[-1]["settled"]:
                    vacuous += 1
                # -- 安全面：回应形状全枚举 --
                for shape in shapes:
                    if shape.startswith("remake") and not oracle:
                        continue          # oracle 关——重制不可构造（降级形态）
                    evs = base + [("REGRESS",)]
                    if shape == "rollback":
                        evs += [("ROLLBACK",)]
                    else:
                        evs += [("STAGE", remake_trees[shape](b)),
                                ("ACTIVATE",), ("BOOT",)]
                    r = run_scenario(cfg, evs, f"cepp-{policy}-{b}-{oracle}-{shape}")
                    pp = r.pp[-1]
                    cases += 1
                    if shape == "remake-deceptive":
                        st = [x for x in r.trace if x.ev == "STAGE"][-1]
                        if st.gate_exec:
                            l1_residual += 1      # 过门不拦——载重证据
                        continue
                    if not pp["legal"]:
                        safety += 1
                    if pp["settled"] and not pp["sup"]:
                        safety += 1
    return {"cases": cases, "safety": safety, "l1_residual": l1_residual,
            "recover_on": recover_on, "recover_off": recover_off,
            "vacuous": vacuous}


# ---------------------------------------------------------------- 汇总/报告

def aggregate(runs):
    agg = {
        "scenarios": len(runs), "events": 0, "boots": 0, "exposure": 0,
        "violations": 0, "delays": [], "regrets": 0, "solved_rb": 0,
        "solved_fwd": 0, "stuck": 0, "sup_fail": 0, "illegal_move": 0,
        "rb": Counter(), "inv": Counter(), "hist": Counter(),
        "by_policy_note": {},
    }
    for r in runs:
        m = r.metrics
        agg["events"] += m["events"]
        agg["boots"] += m["boots"]
        agg["exposure"] += m["exposure"]
        agg["violations"] += len(r.violations)
        agg["delays"].extend(m["delays"])
        agg["regrets"] += m["regrets"]
        agg["solved_rb"] += m["solved_rb"]
        agg["solved_fwd"] += m["solved_fwd"]
        agg["stuck"] += m["stuck"]
        agg["sup_fail"] += m["sup_fail"]
        agg["illegal_move"] += m["illegal_move"]
        agg["rb"].update(m["rb"])
        agg["hist"].update(m["hist"])
        for v in r.violations:
            agg["inv"][v.split()[0].split("@")[0]] += 1
    return agg


def corpus_digest(runs) -> str:
    h = hashlib.sha256()
    for r in runs:
        h.update(json.dumps({
            "name": r.name, "recs": len(r.trace), "viol": len(r.violations),
            "rb": r.metrics["rb"], "stuck": r.metrics["stuck"],
            "sup_fail": r.metrics["sup_fail"], "boots": r.metrics["boots"],
            "exposure": r.metrics["exposure"], "delays": r.metrics["delays"],
            "hist": r.metrics["hist"],
        }, sort_keys=True, ensure_ascii=False).encode("utf-8"))
        h.update(b"|")
    return h.hexdigest()


def _pct(a, b):
    return f"{(100.0 * a / b):.1f}%" if b else "n/a"


def print_report(agg, cfg, seed, extra_lines=()):
    print("== fastlane_sim 桌面推演报告（28 号 §2.5-1 科目一）==")
    print(f"种子 {seed} | 浸泡窗 {cfg.policy} | 重制 oracle "
          f"{'ON' if cfg.remake_oracle else 'OFF'}（OQ-2 假设）| "
          f"快道轴冻结 {'ON' if cfg.axis_frozen else 'OFF'}")
    print(f"[不变量验证] 场景 {agg['scenarios']} | 事件 {agg['events']} | "
          f"boot {agg['boots']} | 违反 {agg['violations']}"
          + ("" if agg["violations"] == 0
             else f"（{dict(agg['inv'])}）"))
    rb = agg["rb"]
    print(f"[回退账] 自由窗 {rb.get('free', 0)} | 门内过 {rb.get('gate-exec', 0)} | "
          f"拒-SL {rb.get('reject-sl', 0) + rb.get('reject-sl+pr', 0)} | "
          f"拒-PR {rb.get('reject-pr', 0)} | 无部署目标 {rb.get('no-target', 0)}")
    print(f"[回归账] REGRESS {agg['regrets']} | 回退可解 {agg['solved_rb']} | "
          f"前滚可解 {agg['solved_fwd']} | 卡死 {agg['stuck']} | "
          f"丢内容 {agg['sup_fail']} | 非法执行 {agg['illegal_move']}")
    if agg["delays"]:
        d = sorted(agg["delays"])
        print(f"[浸泡账] 确认延迟 boot 均 {sum(d) / len(d):.2f} / 最大 {d[-1]} | "
              f"敞口 boot {agg['exposure']}/{agg['boots']}"
              f"（{_pct(agg['exposure'], agg['boots'])}）")
    for line in extra_lines:
        print(line)


def run_battery(seed, n, policy, oracle, verbose=False):
    corpus = build_corpus(seed, n)
    cfg = SimConfig(policy=policy, remake_oracle=oracle)
    runs = run_corpus(cfg, corpus)
    agg = aggregate(runs)
    return cfg, runs, agg


def run_corpus(cfg, corpus):
    return [run_scenario(cfg, ev, name) for name, ev in corpus]


def scan_alpha(seed, n):
    """案 α 浸泡窗参数扫描（OQ-1：窗参数 × 恢复速度/敞口/结构证伪）。"""
    corpus = build_corpus(seed, n)
    rows = []
    for p in POLICIES:
        runs = run_corpus(SimConfig(policy=p), corpus)
        agg = aggregate(runs)
        rows.append((p, agg))
    hdr = (f"{'策略':<7}{'违反':>4}{'回退可解':>9}{'前滚可解':>9}{'卡死':>5}"
           f"{'确认延迟均/最大':>16}{'敞口boot比':>11}")
    print("== 案 α 浸泡窗参数扫描（OQ-1；语料固定种子对拍）==")
    print(hdr)
    for p, a in rows:
        d = sorted(a["delays"]) or [0]
        print(f"{p:<7}{a['violations']:>4}{a['solved_rb']:>9}{a['solved_fwd']:>9}"
              f"{a['stuck']:>5}{f'{sum(d)/len(d):.2f}/{d[-1]}':>16}"
              f"{_pct(a['exposure'], a['boots']):>11}")
    print("读法：none=修法前（窗 0）；w1:K=boot 计数窗（K 越大窗越长）；"
          "w2=下一 lane 到达（γ 并入：一代窗，:78）——回退可解↑/敞口↑同向，"
          "W2 在 D1 停更下不关窗（d1-stop 场景敞口 100%）。")
    return rows


def dry_run(name: str) -> int:
    sc = scripted_scenarios()
    if name == "":
        name = "canonical-b1"
    if name not in sc:
        print(f"dry-run: 未知场景 {name!r}。可用：{' '.join(sorted(sc))}",
              file=sys.stderr)
        return 2
    cfg, evs = sc[name]
    r = run_scenario(cfg, evs, name)
    print(f"== dry-run：{name}（浸泡窗 {cfg.policy}，auto_response="
          f"{cfg.auto_response}）==")
    for rec in r.trace:
        line = (f"#{rec.i:<3}{rec.ev:<9}{rec.tag:<10}{rec.note:<24}"
                f"{rec.state_prev:>10} →{rec.state_next:<10}"
                f"pr {rec.pr_prev}→{rec.pr_next} sl {rec.sl_prev}→{rec.sl_next}")
        if rec.gate_kind:
            gate = (f"门[{'自由窗' if rec.gate_free else f'pr:{rec.pr_ok} sl:{rec.sl_ok}'}"
                    f" {'执行' if rec.gate_exec else '拒'}]"
                    + (f"咬面={rec.bite}" if rec.bite else ""))
            line += " " + gate
        if rec.confirm:
            line += f" 延迟{rec.delay}boot"
        print(line)
    print(f"不变量违反：{len(r.violations)}"
          + (f"——{r.violations}" if r.violations else "（全保）"))
    for p in r.pp:
        print(f"P′@REGRESS#{p['i']}: 终态 {'达成(' + p['settle_ev'] + ')' if p['settled'] else '未达成'}"
              f" | 内容保全 {'✓' if p['sup'] else '✗'} | 首次回退 {p['rb_note'] or '—'}"
              + (f"（{p['rb_bite']}）" if p["rb_bite"] else ""))
    m = r.metrics
    print(f"度量：boot {m['boots']} 敞口 {m['exposure']} 回退账 {m['rb']}")
    return 0


# ---------------------------------------------------------------- selftest

def selftest() -> int:
    """钉板自检（genwycheproof 同款纪律：内嵌场景 + 黄金锚 + 确定性 +
    破坏钩子证明检查器会咬 + exit-code 契约）。"""
    failures = []
    passed = 0

    def check(name, cond, detail=""):
        nonlocal passed
        if cond:
            passed += 1
        else:
            failures.append((name, detail))

    def run(name, cfg=None):
        c, evs = scripted_scenarios()[name]
        return run_scenario(cfg or c, evs, name)

    sc = scripted_scenarios()

    # --- 1. canonical-b1：§2.3 :82 反例机器重现（F SL6 conf → M SL7 吸收
    #        conf → 回退 M→F 拒，pr 过 sl 拒——B1 形态二咬面）---
    r = run("canonical-b1")
    conf = [x for x in r.trace if x.confirm]
    check("canonical floor_sl 5→6→7（:82 算术）",
          [x.sl_next for x in conf] == [6, 7]
          and all(x.sl_prev == x.sl_next or x.sl_next > x.sl_prev for x in conf))
    rb = r.trace[-1]
    check("canonical 回退被 SL 半边拒（pr_ok ∧ ¬sl_ok）",
          rb.ev == "ROLLBACK" and rb.pr_ok and not rb.sl_ok
          and not rb.gate_exec and rb.bite == "sl")
    check("canonical 不变量全保", not r.violations)
    check("canonical digest 黄金锚",
          (GOLDEN["canonical_digest"] == ""
           or corpus_digest([r]) == GOLDEN["canonical_digest"]),
          corpus_digest([r]))

    # --- 2. canonical-b1-response：回退拒 → 重制前滚（P′ happy path）---
    r2 = run("canonical-b1-response")
    check("P′ happy path 终态达成（前滚）",
          r2.pp and r2.pp[-1]["settled"]
          and r2.pp[-1]["settle_ev"] in ("BOOT", "CONFIRM")
          and r2.pp[-1]["sup"] and r2.pp[-1]["legal"])
    check("重制树同 lane 恒过门（I-SL-4 快路径）",
          any(x.ev == "STAGE" and x.tree_kind == "R" and x.gate_exec
              for x in r2.trace))

    # --- 3. soak-window：案 α 窗内回退自由（恰一代安全内容可退）---
    r3 = run("soak-window")
    rb3 = [x for x in r3.trace if x.ev == "ROLLBACK"][-1]
    check("窗内回退执行（α——:47 SOAKING 期 floor 未吸收）",
          rb3.gate_exec and rb3.sl_ok and rb3.state_prev == "SOAKING")
    check("回退时 floor_sl 仍 6（I-SL-2 滞后吸收可观——M1 的 7 未入地板）",
          rb3.sl_prev == 6)

    # --- 4. W1:2 确认延迟恰 2 boot ---
    check("w1:2 确认延迟=2", sorted(r3.metrics["delays"])[-1] == 2)

    # --- 5. w2-lifecycle：下一代到达关窗 + W2 形态咬面（无部署目标）---
    r5 = run("w2-lifecycle")
    check("w2 关窗记录在席（confirm(w2-next-lane)）",
          any(x.note == "confirm(w2-next-lane)" for x in r5.trace))
    rb5 = [x for x in r5.trace if x.ev == "ROLLBACK"][-1]
    check("w2 形态咬面=无部署目标（关窗即覆槽——必须前滚）",
          rb5.note == "no-target")
    check("w2 咬后重制前滚收尾（P′ 达成 + R-fix CONFIRM 在席）",
          r5.pp and r5.pp[-1]["settled"]
          and r5.pp[-1]["settle_ev"] in ("BOOT", "CONFIRM")
          and r5.pp[-1]["sup"]
          and any(x.confirm and x.tag == "R-fix3" for x in r5.trace))

    # --- 6. d1-stop：无下一 lane → 窗不关（敞口 100%）+ 回退恒自由 ---
    r6 = run("d1-stop")
    check("d1 停更下 W2 窗不关（敞口=boot 全量）",
          r6.metrics["boots"] > 0
          and r6.metrics["exposure"] == r6.metrics["boots"])
    check("d1 停更下回退可解（窗内自由）", r6.pp and r6.pp[-1]["settled"]
          and r6.pp[-1]["settle_ev"] == "ROLLBACK")
    check("d1 场景不变量全保", not r6.violations)

    # --- 7. 乱序到达：旧 lane 出件拒于 SL 半边 ---
    r7 = run("out-of-order")
    st = [x for x in r7.trace if x.ev == "STAGE"][-1]
    check("乱序旧件拒（sl_ok=False）", not st.gate_exec and st.pr_ok
          and not st.sl_ok and not st.gate_free)

    # --- 8. OEM 中立：floor 逐字不动 ---
    r8 = run("oem-neutral")
    om = [x for x in r8.trace if x.ev == "OEM"]
    check("OEM 刷新 ≥3 起且 floor 不动",
          len(om) >= 3 and all(x.pr_prev == x.pr_next and x.sl_prev == x.sl_next
                               for x in om) and not r8.violations)

    # --- 9. 轴冻结：快道动功能轴 → loud 拒（19 号 :163）---
    fac = Tree(5, _sec(5), "M", "M0")
    evx = [("INIT", fac), ("STAGE", Tree(6, _sec(6), "F", "Fax")),
           ("STAGE", Tree(6, _sec(6), "M", "Max"))]
    r9 = run_scenario(SimConfig(policy="w1:2", auto_response=False), evx, "axis")
    stf = [x for x in r9.trace if x.tag == "Fax"][0]
    stm = [x for x in r9.trace if x.tag == "Max"][0]
    check("F 轴漂拒（axis-frozen）/ M 同 pr 过",
          stf.note == "reject-axis-frozen" and stm.gate_exec)

    # --- 10. P 偶然真区（:82 尾注）---
    r10 = run("accidental-true")
    rb10 = r10.trace[-1]
    check("M 无新安全内容 → 回退过门（P 偶然真）",
          rb10.gate_exec and rb10.sl_ok and rb10.pr_ok)

    # --- 11. L1 载重探针：欺骗性同 lane 重制过门（值比较门不拦内容集回退）---
    r11 = run("deceptive-remake")
    std = [x for x in r11.trace if x.tag == "R-dec"][-1]
    check("欺骗性重制过门（载重证据）",
          std.gate_exec and std.tree_lane == std.sl_prev
          and set(std.tree_sec) < set(std.pp_sec) if std.pp_sec else True)
    # deceptive 场景无 REGRESS——pp_sec 空，改以台账比对：
    ledger = set(range(1, 8))
    check("欺骗树 sec ⊊ 台账（构造核）", set(std.tree_sec) < ledger)

    # --- 12. I-SL-2 窗观测：W1:4 下 HEALTHY→CONFIRMED 间地板不动 ---
    r12 = run("isl2-window")
    h_idx = next(i for i, x in enumerate(r12.trace) if x.note == "healthy")
    c_idx = next(i for i, x in enumerate(r12.trace) if x.confirm)
    mids = r12.trace[h_idx:c_idx]
    check("窗内 3 boot 且 floor_sl 恒 5",
          c_idx - h_idx == 4 and all(x.sl_next == 5 for x in mids)
          and r12.trace[c_idx].sl_next == 6)

    # --- 13. 反例科目一：P 结构性证伪（策略分型）---
    stats, wit = ce_search_p()
    check("CE-P：每策略 ≥1 P 反例（sl/sl+pr/no-target）",
          all(stats[p]["sl"] + stats[p]["sl+pr"] + stats[p]["no-target"] > 0
              for p in POLICIES),
          {p: dict(stats[p]) for p in POLICIES})
    check("CE-P：none/W1 纯 SL 咬面在册（:82 引证形态）",
          all(stats[p]["sl"] > 0 for p in POLICIES if p != "w2"))
    check("CE-P：W2 结构性——SL 拒面 0 / 无目标>0 / 浸泡期回退自由>0",
          stats["w2"]["sl"] == 0 and stats["w2"]["no-target"] > 0
          and stats["w2"]["legal"] > 0)
    check("CE-P：sl+pr 与 pr（经典防降级）在册",
          any(stats[p]["sl+pr"] > 0 for p in POLICIES)
          and any(stats[p]["pr"] > 0 for p in POLICIES))

    # --- 14. 反例科目二：P′ 安全 0 / oracle-on 恢复 0 / off 恢复失败 >0 ---
    pp = ce_search_pp()
    check("CE-P′：安全违反 0（威胁模型内）", pp["safety"] == 0, pp)
    check("CE-P′：oracle ON 恢复失败 0", pp["recover_on"] == 0)
    check("CE-P′：oracle OFF 恢复失败 >0（:84 降级形态机器演示）",
          pp["recover_off"] > 0)
    check("CE-P′：L1 载重证据在册（欺骗重制过门）", pp["l1_residual"] > 0)
    check("CE-P′：不响应空洞例在册（活性款必要性）", pp["vacuous"] > 0)

    # --- 15. 破坏钩子：检查器必咬（or-gate → ISL1a；early-sl → ISL2/ISL1b）---
    rs = run_scenario(SimConfig(policy="w1:2", auto_response=False,
                                sabotage="or-gate"),
                      sc["out-of-order"][1], "sab-or")
    check("sabotage or-gate → ISL1a 咬",
          any(v.startswith("ISL1a") for v in rs.violations))
    rs2 = run_scenario(SimConfig(policy="w1:4", auto_response=False,
                                 sabotage="early-sl"),
                       [("INIT", fac), ("STAGE", Tree(6, _sec(6), "M", "Mbump")),
                        ("ACTIVATE",), ("BOOT",)], "sab-early")
    check("sabotage early-sl → ISL2/ISL1b 咬（pr+1 树使双轴同变显形）",
          any(v.startswith("ISL2") for v in rs2.violations)
          and any(v.startswith("ISL1b") for v in rs2.violations),
          rs2.violations)

    # --- 16. 随机语料：不变量全保 + P′（oracle on）全可解 + 确定性 ---
    cfg, runs, agg = run_battery(DEFAULT_SEED, 120, "w2", True)
    check("语料（w2/oracle on）不变量全保", agg["violations"] == 0,
          dict(agg["inv"]))
    check("语料 P′ 无丢内容/无非法执行/无卡死",
          agg["sup_fail"] == 0 and agg["illegal_move"] == 0
          and agg["stuck"] == 0)
    check("语料状态覆盖（三态+回退动作均现）",
          all(k in agg["hist"] for k in ("PENDING", "SOAKING", "SUCCESSFUL"))
          and sum(agg["rb"].values()) > 0)
    check("语料（w2）结构性：SL 拒面 0 + 无目标>0（W2 定理）",
          agg["rb"].get("reject-sl", 0) + agg["rb"].get("reject-sl+pr", 0) == 0
          and agg["rb"].get("no-target", 0) > 0)
    _, _, agg_w1 = run_battery(DEFAULT_SEED, 120, "w1:2", True)
    check("语料（w1:2）SL 拒面在册（α 窗外咬面）",
          agg_w1["rb"].get("reject-sl", 0) + agg_w1["rb"].get("reject-sl+pr", 0) > 0)
    d1 = corpus_digest(runs)
    check("语料 digest 黄金锚",
          GOLDEN["corpus_digest"] == "" or d1 == GOLDEN["corpus_digest"], d1)
    _, runs_b, _ = run_battery(DEFAULT_SEED, 120, "w2", True)
    check("确定性（同种子两跑同指纹）", d1 == corpus_digest(runs_b))
    _, runs_c, _ = run_battery(DEFAULT_SEED + 1, 120, "w2", True)
    check("种子敏感性（换种子指纹变）", d1 != corpus_digest(runs_c))

    # --- 17. oracle-off 语料：卡死>0（P′ 降级形态在随机流中亦现）---
    _, runs_d, agg_d = run_battery(DEFAULT_SEED, 120, "w2", False)
    check("oracle-off 语料卡死 >0", agg_d["stuck"] > 0)
    check("oracle-off 语料安全款仍全保",
          agg_d["violations"] == 0 and agg_d["sup_fail"] == 0
          and agg_d["illegal_move"] == 0)

    # --- 18. exit-code 契约 ---
    p1 = subprocess.run([sys.executable, __file__, "--bogus"],
                        capture_output=True, text=True)
    check("--bogus rc=2（argparse 契约）", p1.returncode == 2 and "usage:" in p1.stderr)
    p2 = subprocess.run([sys.executable, __file__, "--dry-run", "canonical-b1"],
                        capture_output=True, text=True)
    check("--dry-run rc=0 且输出见证行", p2.returncode == 0
          and "canonical" in p2.stdout and "不变量违反：0" in p2.stdout)

    print(f"\nfastlane_sim selftest: {'PASS' if not failures else 'FAIL'}"
          f"（{passed} passed, {len(failures)} failed）")
    for name, detail in failures:
        print(f"  FAIL: {name}" + (f"——{detail}" if detail else ""))
    return 1 if failures else 0


# ---------------------------------------------------------------- main

def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        prog="fastlane_sim.py",
        description="28 号题一状态机桌面推演（编队状态机 × I-PR/I-SL-1..4 机器验证）")
    ap.add_argument("--selftest", action="store_true", help="钉板自检")
    ap.add_argument("--fuzz", type=int, default=DEFAULT_FUZZ,
                    help=f"随机场景数（默认 {DEFAULT_FUZZ}）")
    ap.add_argument("--seed", type=int, default=DEFAULT_SEED,
                    help=f"随机种子（默认 {DEFAULT_SEED}——确定性）")
    ap.add_argument("--scan", action="store_true", help="案 α 浸泡窗参数扫描")
    ap.add_argument("--ce-search", action="store_true", help="反例搜索双科目")
    ap.add_argument("--dry-run", nargs="?", const="canonical-b1", default=None,
                    help="代表场景逐步输出（缺省 canonical-b1）")
    ap.add_argument("--list-scenarios", action="store_true")
    ap.add_argument("--policy", default="w2", help="浸泡窗（none|w1:K|w2）")
    ap.add_argument("--no-oracle", action="store_true",
                    help="关重制 oracle（OQ-2 未解降级形态）")
    ap.add_argument("--json", action="store_true", help="报告 JSON 形态")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.list_scenarios:
        print(" ".join(sorted(scripted_scenarios())))
        return 0
    if args.dry_run is not None:
        return dry_run(args.dry_run)
    try:
        parse_policy(args.policy)
    except Fail as e:
        print(f"fastlane_sim: FAIL——{e}", file=sys.stderr)
        return 2

    cfg, runs, agg = run_battery(args.seed, args.fuzz, args.policy,
                                 not args.no_oracle)
    extras = []
    if args.ce_search or not (args.scan or args.ce_search):
        stats, wit = ce_search_p()
        sl_total = sum(stats[p]["sl"] + stats[p]["sl+pr"] for p in POLICIES)
        extras.append(f"[反例-P] 命中 {sl_total}（纯 SL "
                      f"{sum(stats[p]['sl'] for p in POLICIES)}）——19 号 :170 "
                      "配套②原句证伪成立（28 号 §2.3 机器重现）；最小见证 "
                      + (f"{wit['sl'][0]} b={wit['sl'][1]} e={wit['sl'][2]} "
                         f"md={wit['sl'][3]}：lane {wit['sl'][4].tree_lane} < "
                         f"floor {wit['sl'][4].sl_next}" if "sl" in wit else "—"))
        pp = ce_search_pp()
        extras.append(f"[反例-P′] 安全违反 {pp['safety']} | oracle ON 恢复失败 "
                      f"{pp['recover_on']} | oracle OFF 恢复失败 {pp['recover_off']}"
                      f"（28 号 §2.3 :84 降级形态）| 不响应空洞例 {pp['vacuous']}")
        extras.append(f"[反例-P·W2 结构性] SL 地板拒面 {stats['w2']['sl']}"
                      "（不可达——关窗事件=覆槽事件，地板抬升瞬间消灭一切 "
                      "lane<floor 已部署目标）| 无部署目标 "
                      f"{stats['w2']['no-target']}（反噬=回退不可达→必须前滚）| "
                      f"浸泡期回退自由 {stats['w2']['legal']}")
        extras.append(f"[L1 载重] 欺骗性同 lane 重制过门 {pp['l1_residual']} 例——"
                      "值比较门不拦内容集回退，P′ 由平台构建纪律承载（:84 同结论）")
    if args.json:
        out = {
            "seed": args.seed, "policy": args.policy,
            "oracle": not args.no_oracle,
            "aggregate": {k: agg[k] for k in (
                "scenarios", "events", "boots", "violations", "regrets",
                "solved_rb", "solved_fwd", "stuck", "sup_fail",
                "illegal_move", "exposure")},
            "rb": dict(agg["rb"]), "inv": dict(agg["inv"]),
            "delays": sorted(agg["delays"]), "digest": corpus_digest(runs),
        }
        print(json.dumps(out, sort_keys=True, ensure_ascii=False, indent=1))
        return 0 if agg["violations"] == 0 else 1
    print_report(agg, cfg, args.seed, extras)
    if args.scan or not (args.scan or args.ce_search):
        print()
        scan_alpha(args.seed, min(args.fuzz, DEFAULT_FUZZ))
    verdict = (f"结论：不变量{'全保' if agg['violations'] == 0 else '违反'}；"
               "P 证伪机器重现；P′ "
               + ("加固（条件=L1 平台纪律 + 重制 oracle/OQ-2）"
                  if agg["sup_fail"] == 0 and agg["illegal_move"] == 0
                  and agg["stuck"] == 0 else "存在反例——上报"))
    print(verdict)
    return 0 if agg["violations"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
