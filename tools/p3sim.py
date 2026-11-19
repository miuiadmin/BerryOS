#!/usr/bin/env python3
"""p3sim——P3-0 装载 admit 场景编排器（SDK-2 P3-0 件三；设计母法 =
docs/设备侧验签与账本落盘设计-v1.0.md §二 2.1 件三——「场景编排器：
布景（mkapp+berry-sign 测试钥现签现验，钥落 /tmp 不落仓）→ 调件二
（berry-sign admit）→ 断言」）。

用法：
  p3sim.py                # 全谱 E2E：DoD 九场景（正/三负/水位/防剥离/
                          #   三档表/TOFU+重叠窗/sideload/诚实降级/落盘
                          #   幂等+损坏保守拒）——需 berry-sign 与 mkapp
  p3sim.py --selftest     # 零构建依赖自检（gensourceindex :23 先例——
                          #   布景基件/决策行解析/场景注册表/损坏生成器
                          #   的 Python 侧不变量，不产不签不跑二进制）

形态纪律（设计 §二 2.1 三硬约束）：
  - 单解释器纪律——装载决策/验签/账本语义全在 Rust（berry-crypto admit
    模块单源），本文件只编排：布景（mkapp 产 .bap + berry-sign 现签）、
    调件二（berry-sign admit 子命令）、断言决策行。策略档词表
    （TIER_WORDS）与 eco 拒因词（ECO_WORDS）是 CLI 行面的**对读镜像**
    （同步义务：Rust 侧 Policy::WORDS / 各枚举 word() 改义随笔）。
  - 零三方依赖（gensourceindex/berry_assemble 同纪律；stdlib only）。
  - 测试钥永不真签：本工具产钥只签测试夹具（dev/test 面纪律，berry-sign
    main.rs 头注）；钥/签发件全落 /tmp 临时目录，退出清场，不落仓。

确定性（固定种子）：场景参数全确定性——app 集/版本/代际/文件字节/目录
序（排序消费）固定；random.seed(SEED) 钉随机面（现无消费，防将来漂移）；
密钥每跑 berry-sign keygen 现产（/dev/urandom——测试钥无跨跑稳定需求，
断言全关系型非绝对值）。

场景集 = 规格 DoD 场景 + 账本 TOFU 首见/水位回退/关门后拒载（任务书）：
  S1 正样例（DoD 1）     bootseed system 锚 + 官方件 .sig → Allow + 水位不降
  S2 三负样例（DoD 2）   篡改载荷 / 断链（.sig 截断）/ 跨 lineage
  S3 水位负（DoD 3）     gen 回降拒 + ver 回降拒（check_gen/check_ver 单源）
  S4 防剥离负（DoD 4）   已签 id 无 .sig → 三档同拒（stripped）
  S5 策略三档表（DoD 5） 无 .sig 三档 + 未知 lineage 三档（关门后拒载面）
  S6 TOFU（DoD 6）       dev 首见落增量账本（tofu 锚 + per-app 四字段）+
                         重叠窗双锚任一过（双锚账本态 = 轮换仪式源侧产物；
                         链到达/断链走链在制件 = berry-sign selftest admit
                         腿 + berry-crypto lib 测试——如实注，E2E 不重演）
  S7 sideload（DoD 7）   关 = 拒装 + 显式错误；开 = 装 + 账本记录来源
  S8 诚实降级（DoD 8）   --no-increment → 「回放防护未生效」可见行 +
                         水位跳检（同 S3 回退件放行——回放窗口重开如实）
  S9 落盘模拟（DoD 9）   emit→parse 幂等（空目录零突变字节恒等）；损坏
                         增量（截断/坏枚举）→ 保守拒 + 回 bootseed 重导

berry-sign 发现序：env BERRY_SIGN → <repo>/target-cargo 与
/tmp/target-p1、/tmp/target-p2、/tmp/target-p30（P3-0 lane 惯用位）
× debug/release（mkapp `_find_berry_sign` 同序 + p30 扩位——同步义务）。
"""

import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.abspath(__file__))            # .../tools
REPO = os.path.dirname(ROOT)
MKAPP = os.path.join(ROOT, "mkapp.py")

SEED = 20261111                 # 固定种子（头注「确定性」段）
LIN_OFFICIAL = "official"       # 官方 lineage（bootseed system 锚）
LIN_TOFU = "tofu-lin"           # TOFU 场景 lineage（首见未知）
LIN_DUAL = "dual"               # 重叠窗双锚 lineage

# CLI 行面对读镜像（同步义务：Rust 侧 admit::Policy::WORDS / word()）
TIER_WORDS = ("dev", "product-open", "product-locked")
ECO_REJECT_WORDS = (
    "nosig", "unknown-src", "sideload", "stripped",          # 策略族
    "gen-rollback", "ver-rollback",                          # 水位族
    "top-sig-bad", "bad-len", "lineage-echo",                # 信封族（S2 用）
)
ALLOW_KINDS = ("verified", "tofu-first-seen", "degrade")

DECISION_RE = re.compile(r"^admit: (\S+) (allow|reject)(?: (.*))?$")
REPLAY_NOTE = "回放防护未生效"


def die(msg: str) -> None:
    raise SystemExit(f"p3sim: FAIL: {msg}")


def leg(ok: bool, label: str, fails: list) -> None:
    print(f"    {'过（预期）' if ok else 'FAIL'} : {label}")
    if not ok:
        fails.append(label)


# ---- 布景基件（policy/ledger 生成器 + mkapp/berry-sign 调用）----

def policy_toml(tier: str) -> bytes:
    """策略件（admit::parse_policy 单源消费的源形——两键闭集）。"""
    if tier not in TIER_WORDS:
        die(f"策略档 {tier!r} 不在词表 {TIER_WORDS}")
    return f"# load-policy v1（p3sim 布景）\nschema = 1\ntier = \"{tier}\"\n".encode()


def ledger_toml(lineages, apps=()) -> bytes:
    """账本源形（ledger.rs TrustLedger::parse 单源消费——九键 lineage +
    五键 app 节，canonical 序 lineage 前 app 后）。lineages =
    [(id, [(pk_hex, anchor_type)], gen_max)]；apps = [(id, pk_hex,
    source, boots, ver_max)]。"""
    out = ["schema = 1\n"]
    for lid, anchors, gen_max in lineages:
        out.append(f"\n[[lineage]]\nid = \"{lid}\"\nanchors = [\n")
        for pk_hex, ty in anchors:
            out.append(f"  {{ pk = \"{pk_hex}\", type = \"{ty}\" }},\n")
        out.append(f"]\ngen_max = {gen_max}\nver_max = 0\n"
                   "kind = \"\"\nissued_by = \"\"\ncert_status = \"\"\n"
                   "last_revision = 0\n")
    for aid, pk_hex, source, boots, ver_max in apps:
        out.append(f"\n[[app]]\nid = \"{aid}\"\npk = \"{pk_hex}\"\n"
                   f"source = \"{source}\"\nboots = {boots}\n"
                   f"ver_max = {ver_max}\n")
    return "".join(out).encode()


def staging(app_id: str, version: str, lineage: str | None, path: str) -> str:
    """mkapp 打包脚手架：{manifest.toml, bin/<entry>}（ELF 魔数占位——
    mkapp 门禁只验头 4 字节）。lineage=None = 无 [security] 三键件。"""
    os.makedirs(os.path.join(path, "bin"), exist_ok=True)
    sec = ("\n[security]\nprofile = \"ed25519-main\"\n"
           f"key_lineage = \"{lineage}\"\nintegrator = \"berryos-dev\"\n") if lineage else ""
    mf = (f"[app]\nid = \"{app_id}\"\nversion = \"{version}\"\n"
          f"entry = \"bin/{app_id.lower()}\"\ntype = \"app\"\nintent = \"utility\"\n"
          "\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n"
          "\n[capabilities]\nconsole = true\n" + sec)
    with open(os.path.join(path, "manifest.toml"), "w") as f:
        f.write(mf)
    with open(os.path.join(path, "bin", app_id.lower()), "wb") as f:
        f.write(b"\x7fELF" + bytes((i * 7 + 3) % 256 for i in range(512)))
    return path


def find_berry_sign() -> str:
    """berry-sign 发现（mkapp `_find_berry_sign` 同序 + /tmp/target-p30
    P3-0 lane 惯用位——同步义务）。"""
    env = os.environ.get("BERRY_SIGN")
    if env and os.path.isfile(env) and os.access(env, os.X_OK):
        return env
    for tdir in (os.path.join(REPO, "target-cargo"), "/tmp/target-p1",
                 "/tmp/target-p2", "/tmp/target-p30"):
        for profile in ("debug", "release"):
            cand = os.path.join(tdir, "aarch64-apple-darwin", profile,
                                "berry-sign")
            if os.path.isfile(cand) and os.access(cand, os.X_OK):
                return cand
    die("berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR="
        "/tmp/target-p30 cargo build --manifest-path user/Cargo.toml "
        "-p berry-crypto --bin berry-sign --target aarch64-apple-darwin"
        "——mkapp sign.tool-missing 同文案）")


def run(argv: list, env_extra: dict | None = None) -> subprocess.CompletedProcess:
    e = dict(os.environ)
    if env_extra:
        e.update(env_extra)
    return subprocess.run(argv, capture_output=True, text=True, env=e)


def build_app(bs: str, keys: dict, out_dir: str, app_id: str, version: str,
              lineage: str | None, gen: int | None = None,
              signer: str = "a") -> tuple:
    """布景单件：mkapp 无签打包 → （可选）berry-sign 现签 →
    (bap 路径, sig 路径|None)。固定种子面：文件字节确定性（staging 载荷
    图案固定）。"""
    stage = staging(app_id, version, lineage,
                    os.path.join(out_dir, f"stage-{app_id.lower()}"))
    bap = os.path.join(out_dir, f"{app_id}.bap")
    r = run([sys.executable, MKAPP, bap, stage])
    if r.returncode != 0:
        die(f"mkapp 打包拒 {app_id}：{(r.stderr or r.stdout).strip()}")
    if gen is None:
        return bap, None
    r = run([bs, "sign", keys[f"{signer}.sk"], bap, "--gen", str(gen)])
    if r.returncode != 0:
        die(f"berry-sign sign 拒 {app_id}：{(r.stderr or r.stdout).strip()}")
    return bap, f"{bap}.sig"


# ---- 件二消费：berry-sign admit 子命令 ----

def admit(bs: str, policy: bytes | str, ledger: bytes | str, app_dir: str,
          *, sideload=(), emit=None, no_increment=False) -> dict:
    """调 berry-sign admit（件二）→ {rc, lines, decisions, emit_text}。
    decisions = {app_id: (allow|reject, detail)}。"""
    tmp = tempfile.mkdtemp(prefix="p3sim-admit-", dir=out_tmp)
    pol = policy if isinstance(policy, str) else _dump(tmp, "p.toml", policy)
    led = ledger if isinstance(ledger, str) else _dump(tmp, "l.toml", ledger)
    argv = [bs, "admit", pol, led, app_dir]
    for sid in sideload:
        argv += ["--sideload", sid]
    if emit is not None:
        argv += ["--emit-ledger", emit]
    if no_increment:
        argv.append("--no-increment")
    r = run(argv)
    lines = (r.stdout or "").splitlines()
    decisions = {}
    for ln in lines:
        mo = DECISION_RE.match(ln)
        if mo:
            decisions[mo.group(1)] = (mo.group(2), mo.group(3) or "")
    emit_text = ""
    if emit is not None and os.path.isfile(emit):
        with open(emit) as f:
            emit_text = f.read()
    return {"rc": r.returncode, "lines": lines, "decisions": decisions,
            "stderr": (r.stderr or "").strip(), "emit_text": emit_text}


out_tmp = ""  # 全局临时根（main 设置；admit 复用）


def _dump(tmp: str, name: str, data: bytes) -> str:
    p = os.path.join(tmp, name)
    with open(p, "wb") as f:
        f.write(data)
    return p


def corrupt_truncate(data: bytes) -> bytes:
    """损坏增量生成器①：截断（半长）。"""
    return data[: len(data) // 2]


def corrupt_bad_enum(data: bytes) -> bytes:
    """损坏增量生成器②：坏锚枚举（system→vendor——UnknownAnchorType 面）。"""
    return data.replace(b'type = "system"', b'type = "vendor"')


# ---- 场景（DoD 1-9 逐条；每场景一函数，fails 出参累计）----

def s1_positive(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s1")
    build_app(bs, keys, d, "OFFIC1", "0.1.0", LIN_OFFICIAL, gen=3)
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 3)])
    out = os.path.join(d, "led-out.toml")
    r = admit(bs, _dump(d, "pol.toml", policy_toml("product-locked")),
              led, d, emit=out)
    det = r["decisions"].get("OFFIC1", ("", ""))
    leg(r["rc"] == 0 and det[0] == "allow"
        and "verified" in det[1] and "gen=3" in det[1],
        "S1 正样例：system 锚 + 官方件 .sig → allow verified gen=3", fails)
    leg("gen_max = 3" in r["emit_text"] and "[[app]]" in r["emit_text"]
        and 'id = "OFFIC1"' in r["emit_text"],
        "S1 水位不降（emit 账本 gen_max=3）+ per-app 记录落账", fails)


def s2_negatives(bs, keys, work: str, fails: list) -> None:
    # 双 lineage 在册（official + lin-y）：跨 lineage 回声对账用例需要
    # face lineage 在册——否则先撞未知 lineage 档位拒（unknown-src）。
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1),
                       ("lin-y", [(keys["b.pk"], "system")], 1)])
    pol = _dump(work, "s2-pol.toml", policy_toml("product-locked"))
    # ① 篡改载荷。
    d = _dir(work, "s2-tamper")
    bap, sig = build_app(bs, keys, d, "TAMPER1", "0.1.0", LIN_OFFICIAL, gen=1)
    with open(bap, "rb") as f:
        blob = bytearray(f.read())
    blob[-3] ^= 0x01                      # 尾部垫区翻位（全字节摘要覆盖面）
    with open(bap, "wb") as f:
        f.write(blob)
    r = admit(bs, pol, led, d)
    det = r["decisions"].get("TAMPER1", ("", ""))
    leg(r["rc"] == 1 and det[0] == "reject" and "top-sig-bad" in det[1],
        "S2① 篡改载荷拒（top-sig-bad——BSIG-TOP 覆盖 bap 全字节）", fails)
    # ② 断链负样例（.sig 截断——信封 BadLen 单一解释面；链节点级断链
    #    （RotSigBad）= lib 测试 + berry-sign selftest 在制件）。
    d = _dir(work, "s2-trunc")
    bap, sig = build_app(bs, keys, d, "TRUNC1", "0.1.0", LIN_OFFICIAL, gen=1)
    with open(sig, "rb") as f:
        sb = f.read()
    with open(sig, "wb") as f:
        f.write(sb[:-1])
    r = admit(bs, pol, led, d)
    det = r["decisions"].get("TRUNC1", ("", ""))
    leg(r["rc"] == 1 and det[0] == "reject" and "bad-len" in det[1],
        "S2② 断链负样例拒（.sig 截断 bad-len）", fails)
    # ③ 跨 lineage：lin-x 签发件 + lin-y manifest 对读（回声不等）。
    d = _dir(work, "s2-xlin")
    bap, sig = build_app(bs, keys, d, "XLIN1", "0.1.0", "lin-x", gen=1)
    shutil.move(sig, sig + ".keep")
    staging("XLIN1", "0.1.0", "lin-y", os.path.join(d, "stage-xlin1"))
    r = run([sys.executable, MKAPP, bap, os.path.join(d, "stage-xlin1")])
    if r.returncode != 0:
        die(f"S2③ 重打包拒：{(r.stderr or r.stdout).strip()}")
    shutil.move(sig + ".keep", sig)
    r = admit(bs, pol, led, d)
    det = r["decisions"].get("XLIN1", ("", ""))
    leg(r["rc"] == 1 and det[0] == "reject" and "lineage-echo" in det[1],
        "S2③ 跨 lineage 拒（lineage-echo——信封/manifest 回声对账）", fails)


def s3_water(bs, keys, work: str, fails: list) -> None:
    pol = _dump(work, "s3-pol.toml", policy_toml("dev"))
    # gen 回降：bootseed gen_max=5，信封 gen 3。
    d = _dir(work, "s3-gen")
    build_app(bs, keys, d, "GENRB1", "0.1.0", LIN_OFFICIAL, gen=3)
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 5)])
    r = admit(bs, pol, led, d)
    det = r["decisions"].get("GENRB1", ("", ""))
    leg(r["rc"] == 1 and "gen-rollback" in det[1],
        "S3① gen 回降拒（check_gen 单源：3 < gen_max 5）", fails)
    # ver 回降：per-app ver_max=2000（0.2.0），装载 0.1.0（→1000）。
    d = _dir(work, "s3-ver")
    build_app(bs, keys, d, "VERRB1", "0.1.0", LIN_OFFICIAL, gen=6)
    led = ledger_toml(
        [(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1)],
        [("VERRB1", keys["a.pk"], "builtin", 2, 2000)])
    r = admit(bs, pol, led, d)
    det = r["decisions"].get("VERRB1", ("", ""))
    leg(r["rc"] == 1 and "ver-rollback" in det[1],
        "S3② ver 回降拒（check_ver：1000 < ver_max 2000）", fails)


def s4_anti_strip(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s4")
    build_app(bs, keys, d, "STRIP1", "0.1.0", LIN_OFFICIAL, gen=None)  # 无 .sig
    led = ledger_toml(
        [(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1)],
        [("STRIP1", keys["a.pk"], "builtin", 1, 0)])   # 已签 id 在案
    for tier in TIER_WORDS:
        r = admit(bs, _dump(work, f"s4-pol-{tier}.toml", policy_toml(tier)),
                  led, d)
        det = r["decisions"].get("STRIP1", ("", ""))
        leg(r["rc"] == 1 and det[0] == "reject" and "stripped" in det[1],
            f"S4 防剥离负：已签 id 无 .sig 拒（{tier}——全档恒执法）", fails)


def s5_tier_table(bs, keys, work: str, fails: list) -> None:
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1)])
    # 无 .sig 三档。
    d = _dir(work, "s5-uns")
    build_app(bs, keys, d, "UNS5", "0.1.0", LIN_OFFICIAL, gen=None)
    want = {"dev": ("allow", "degrade"), "product-open": ("allow", "degrade"),
            "product-locked": ("reject", "nosig")}
    for tier, (kind, word) in want.items():
        r = admit(bs, _dump(work, f"s5u-pol-{tier}.toml", policy_toml(tier)),
                  led, d)
        det = r["decisions"].get("UNS5", ("", ""))
        ok = det[0] == kind and word in det[1]
        if kind == "allow":
            ok = ok and r["rc"] == 0 and "eco: app UNS5 unsigned" in det[1]
        leg(ok, f"S5a 无 .sig @ {tier} → {kind}{'+loud' if word == 'degrade' else ''}"
                f"（{word}）", fails)
    # 未知 lineage 带签三档（关门后拒载面——product-* 档）。
    d = _dir(work, "s5-unk")
    build_app(bs, keys, d, "UNKN5", "0.1.0", LIN_TOFU, gen=1)
    want = {"dev": ("allow", "tofu-first-seen"),
            "product-open": ("reject", "unknown-src"),
            "product-locked": ("reject", "unknown-src")}
    for tier, (kind, word) in want.items():
        r = admit(bs, _dump(work, f"s5k-pol-{tier}.toml", policy_toml(tier)),
                  led, d)
        det = r["decisions"].get("UNKN5", ("", ""))
        leg(det[0] == kind and word in det[1],
            f"S5b 未知 lineage 带签 @ {tier} → {kind}（{word}）", fails)


def s6_tofu(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s6")
    build_app(bs, keys, d, "TOFU6", "0.1.0", LIN_TOFU, gen=2)
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1)])
    out = os.path.join(d, "led-inc.toml")
    r = admit(bs, _dump(d, "pol.toml", policy_toml("dev")), led, d, emit=out)
    det = r["decisions"].get("TOFU6", ("", ""))
    leg(r["rc"] == 0 and "tofu-first-seen" in det[1] and "gen=2" in det[1],
        "S6a dev 首见 → tofu-first-seen", fails)
    et = r["emit_text"]
    leg(f'id = "{LIN_TOFU}"' in et and 'type = "tofu"' in et
        and "[[app]]" in et and 'id = "TOFU6"' in et
        and 'source = "builtin"' in et and "boots = 1" in et
        and "ver_max = 1000" in et,
        "S6a 增量账本落盘（tofu 锚 + per-app 四字段 pk/来源/计数/水位）",
        fails)
    # 二次装载同件：verified（首见即锚后）+ gen 水位保持。
    r2 = admit(bs, _dump(d, "pol.toml", policy_toml("dev")), out, d)
    det = r2["decisions"].get("TOFU6", ("", ""))
    leg(r2["rc"] == 0 and "verified" in det[1] and "tofu-first-seen" not in det[1],
        "S6a 二次装载 verified（非再 TOFU——幂等无害）", fails)
    # 重叠窗双锚任一过：lineage dual 双锚账本态（轮换仪式源侧产物），
    # skA/skB 各签一件 → 双 allow。
    dd = _dir(work, "s6-dual")
    build_app(bs, keys, dd, "DUALA", "0.1.0", LIN_DUAL, gen=2, signer="a")
    build_app(bs, keys, dd, "DUALB", "0.1.0", LIN_DUAL, gen=2, signer="b")
    led = ledger_toml([(LIN_DUAL, [(keys["a.pk"], "system"),
                                   (keys["b.pk"], "system")], 2)])
    r = admit(bs, _dump(dd, "pol.toml", policy_toml("product-locked")),
              led, dd)
    da, db = r["decisions"].get("DUALA", ("", "")), r["decisions"].get("DUALB", ("", ""))
    leg(r["rc"] == 0 and da[0] == "allow" and db[0] == "allow",
        "S6b 重叠窗双锚任一过（旧锚件/新锚件双 allow）", fails)


def s7_sideload(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s7")
    build_app(bs, keys, d, "SIDE7", "0.1.0", LIN_OFFICIAL, gen=1)
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 1)])
    # 关（product-locked）= 拒装 + 显式错误。
    r = admit(bs, _dump(d, "pol-lock.toml", policy_toml("product-locked")),
              led, d, sideload=("SIDE7",))
    det = r["decisions"].get("SIDE7", ("", ""))
    leg(r["rc"] == 1 and det[0] == "reject" and "sideload" in det[1],
        "S7 sideload 关 = 拒装 + 显式错误（§九 :133 字面）", fails)
    # 开（dev）= 装 + 账本记录来源 sideload。
    out = os.path.join(d, "led-side.toml")
    r = admit(bs, _dump(d, "pol-dev.toml", policy_toml("dev")), led, d,
              sideload=("SIDE7",), emit=out)
    det = r["decisions"].get("SIDE7", ("", ""))
    leg(r["rc"] == 0 and det[0] == "allow"
        and 'source = "sideload"' in r["emit_text"],
        "S7 sideload 开 = 装 + 账本记录来源（emit 面）", fails)


def s8_no_increment(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s8")
    build_app(bs, keys, d, "NOINC8", "0.1.0", LIN_OFFICIAL, gen=3)  # < gen_max 5
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 5)])
    pol = _dump(d, "pol.toml", policy_toml("product-locked"))
    r_guard = admit(bs, pol, led, d)                        # 对照：常态拒
    r = admit(bs, pol, led, d, no_increment=True)           # 增量缺席
    det = r["decisions"].get("NOINC8", ("", ""))
    leg(r_guard["rc"] == 1 and r["rc"] == 0 and det[0] == "allow"
        and any(REPLAY_NOTE in ln for ln in r["lines"]),
        "S8 诚实降级：增量缺席 → 「回放防护未生效」可见行 + 水位跳检"
        "（同件常态拒 → 放行——回放窗口重开如实）", fails)


def s9_persist(bs, keys, work: str, fails: list) -> None:
    d = _dir(work, "s9")
    build_app(bs, keys, d, "PERSIST9", "0.1.0", LIN_OFFICIAL, gen=3)
    led = ledger_toml([(LIN_OFFICIAL, [(keys["a.pk"], "system")], 3)])
    pol = _dump(d, "pol.toml", policy_toml("dev"))
    out1 = os.path.join(d, "led-1.toml")
    r = admit(bs, pol, led, d, emit=out1)
    if r["rc"] != 0:
        die(f"S9 布景拒：{r['lines']}")
    with open(out1, "rb") as f:
        inc = f.read()
    # ① emit→parse 幂等：空目录（零突变）重跑同账本 → emit 字节恒等。
    empty = _dir(work, "s9-empty")
    out2 = os.path.join(d, "led-2.toml")
    r = admit(bs, pol, out1, empty, emit=out2)
    with open(out2, "rb") as f:
        inc2 = f.read()
    leg(r["rc"] == 0 and inc2 == inc,
        "S9① emit→parse 幂等（零突变重跑字节恒等——canonical 单形）", fails)
    # ② 损坏增量：截断/坏枚举 → 保守拒（exit 2，不装不评）。
    for name, blob in (("截断", corrupt_truncate(inc)),
                       ("坏枚举", corrupt_bad_enum(inc))):
        bad = _dump(d, f"led-bad-{name}.toml", blob)
        r = admit(bs, pol, bad, d)
        leg(r["rc"] == 2 and "保守拒" in (r["stderr"] or ""),
            f"S9② 损坏增量（{name}）→ 保守拒（exit 2 + loud）", fails)
    # ③ 回 bootseed 重导路径：换 bootseed 重跑同目录 = 过。
    r = admit(bs, pol, led, d)
    leg(r["rc"] == 0, "S9③ 回 bootseed 重导路径过（重导后增量可再生）",
        fails)


SCENARIOS = (s1_positive, s2_negatives, s3_water, s4_anti_strip, s5_tier_table,
             s6_tofu, s7_sideload, s8_no_increment, s9_persist)


def _dir(work: str, name: str) -> str:
    d = os.path.join(work, name)
    os.makedirs(d, exist_ok=True)
    return d


def full_run() -> int:
    global out_tmp
    random.seed(SEED)
    bs = find_berry_sign()
    if not os.path.isfile(MKAPP):
        die(f"mkapp 不在席：{MKAPP}")
    work = tempfile.mkdtemp(prefix="p3sim-", dir=tempfile.gettempdir())
    out_tmp = os.path.join(work, "admit-tmp")
    os.makedirs(out_tmp, exist_ok=True)
    fails: list = []
    try:
        # 密钥（测试钥纪律：/tmp 现产，清场不落仓；永不真签——只签夹具）。
        keys = {}
        for k in ("a", "b"):
            sk, pk = os.path.join(work, f"{k}.sk"), os.path.join(work, f"{k}.pk")
            r = run([bs, "keygen", sk, pk])
            if r.returncode != 0:
                die(f"keygen 拒：{(r.stderr or r.stdout).strip()}")
            keys[f"{k}.sk"] = sk
            with open(pk, "rb") as f:
                keys[f"{k}.pk"] = f.read().hex()
        for i, sc in enumerate(SCENARIOS, 1):
            print(f"  场景 S{i} {sc.__name__}:")
            sc(bs, keys, work, fails)
    finally:
        shutil.rmtree(work, ignore_errors=True)   # dev 纪律：钥随临时目录清场
    if fails:
        print(f"p3sim: FAIL x{len(fails)}")
        return 1
    print("p3sim: ok（DoD 九场景全过——berry-sign admit 实跑 + mkapp 布景）")
    return 0


# ---- --selftest（零构建依赖——布景基件/解析面/注册表不变量）----

def selftest() -> int:
    fails: list = []

    def ok(cond: bool, label: str):
        print(f"  selftest {'过（预期）' if cond else 'FAIL'} : {label}")
        if not cond:
            fails.append(label)

    # ① 策略件黄金字节（admit::parse_policy 源形——两键闭集 + 注释容许）。
    ok(policy_toml("dev")
       == '# load-policy v1（p3sim 布景）\nschema = 1\ntier = "dev"\n'.encode(),
       "policy_toml 黄金字节（dev）")
    try:
        policy_toml("vendor")
        ok(False, "policy_toml 越域档 loud 拒")
    except SystemExit:
        ok(True, "policy_toml 越域档 loud 拒")

    # ② 账本生成器黄金形（九键 lineage + 五键 app；canonical 序）。
    blob = ledger_toml([("official", [("ab" * 32, "system")], 3)],
                       [("APP", "cd" * 32, "builtin", 1, 1000)])
    ok(blob.startswith(b"schema = 1\n\n[[lineage]]\nid = \"official\"\n")
       and b'pk = "abababab' in blob and b'type = "system"' in blob
       and b"gen_max = 3" in blob and b'kind = ""' in blob
       and b"last_revision = 0" in blob
       and blob.index(b"[[lineage]]") < blob.index(b"[[app]]")
       and b'[[app]]\nid = "APP"\npk = "cdcd' in blob
       and b'source = "builtin"' in blob and b"boots = 1" in blob
       and b"ver_max = 1000" in blob,
       "ledger_toml 黄金形（lineage 前/app 后 + 字段齐）")

    # ③ 决策行解析（CLI 行面合同——DECISION_RE 对拍三型）。
    cases = {
        "admit: A1 allow verified lineage=official gen=3": ("A1", "allow", "verified"),
        "admit: A2 allow tofu-first-seen lineage=x gen=1": ("A2", "allow", "tofu-first-seen"),
        "admit: A3 allow degrade eco: app A3 unsigned": ("A3", "allow", "degrade"),
        "admit: A4 reject eco: app A4 nosig": ("A4", "reject", "nosig"),
        "admit: 注: 增量账本缺席——回放防护未生效（水位跳检；诚实降级，设计 §1.3.2/§1.4）": None,
        "admit: done apps=2 allow=1 reject=1 policy=dev": None,
    }
    ok(all((DECISION_RE.match(ln) is not None) == (want is not None)
           and (want is None or DECISION_RE.match(ln).groups()[0:2] == want[0:2])
           for ln, want in cases.items()),
       "决策行解析三型 + 注记/footer 行不误吃")

    # ④ 词表镜像完整性（对读镜像同步义务的 Python 侧执法点）。
    ok(TIER_WORDS == ("dev", "product-open", "product-locked")
       and set(ECO_REJECT_WORDS) >= {"nosig", "unknown-src", "sideload",
                                     "stripped", "gen-rollback", "ver-rollback"}
       and ALLOW_KINDS == ("verified", "tofu-first-seen", "degrade"),
       "词表镜像完整（档三 + eco 拒因 + allow 三型）")

    # ⑤ 损坏生成器：产物 ≠ 源且可复现（确定性）。
    src = ledger_toml([("official", [("ab" * 32, "system")], 1)])
    t1, t2 = corrupt_truncate(src), corrupt_truncate(src)
    e1, e2 = corrupt_bad_enum(src), corrupt_bad_enum(src)
    ok(t1 != src and t1 == t2 and len(t1) == len(src) // 2
       and e1 != src and e1 == e2 and b"vendor" in e1,
       "损坏生成器（截断/坏枚举）确定性")

    # ⑥ mkapp 脚手架 manifest 过 mkapp 单源校验（berry-manifest host 面
    #    ——check_manifest_text 复用，不长第二套；[security] 在/缺两臂）。
    import mkapp
    for lineage, want_sec in ((LIN_OFFICIAL, True), (None, False)):
        d = tempfile.mkdtemp(prefix="p3sim-st-")
        try:
            staging("ST" + ("X" if lineage else "Y"), "0.1.0", lineage, d)
            with open(os.path.join(d, "manifest.toml")) as f:
                m = mkapp.check_manifest_text(f.read())
            ok((m.sec is not None) == want_sec,
               f"脚手架 manifest 过 mkapp 校验（[security] {'在' if want_sec else '缺'}臂）")
        finally:
            shutil.rmtree(d, ignore_errors=True)

    # ⑦ 场景注册表 = DoD 1-9 逐条（任务书场景集完整性）。
    ok(len(SCENARIOS) == 9
       and [sc.__name__ for sc in SCENARIOS] ==
       ["s1_positive", "s2_negatives", "s3_water", "s4_anti_strip",
        "s5_tier_table", "s6_tofu", "s7_sideload", "s8_no_increment",
        "s9_persist"],
       "场景注册表 = DoD 九场景（正/三负/水位/防剥离/三档/TOFU/sideload/"
       "诚实降级/落盘）")

    if fails:
        print(f"p3sim: selftest FAIL x{len(fails)}")
        return 1
    print("p3sim: selftest ok")
    return 0


def main() -> int:
    import argparse
    ap = argparse.ArgumentParser(
        description="BerryOS P3-0 装载 admit 场景编排器（DoD 九场景 E2E + "
                    "零构建自检）")
    ap.add_argument("--selftest", action="store_true",
                    help="零构建依赖自检（布景基件/解析面/注册表不变量）")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    return full_run()


if __name__ == "__main__":
    sys.exit(main())
