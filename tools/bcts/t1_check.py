# BCTS Tier 1 冻结件 schema 校验器（模块面——bcts-run.sh PYEOF①段机械抽出，
# #658 后续件首刀的 parity oracle 基座）。抽取源 = tools/bcts-run.sh:237-339
# heredoc 体逐字（`python3 - "$1" <<'PYEOF'` 调用位），+4 缩进与局部 import
# 归位外零变更：bcts-run.sh 改经 `python3 "$SCRIPT_DIR/bcts/t1_check.py" "$1"`
# 同参调用，文案/退码逐字节不变。模块化动机 = Rust 件（tools/bctsrun）parity
# 对拍需要稳定驱动面（mkcpio「Python 件保模块面」先例——六处 import 图根
# 同理）；后续 PYEOF②③段抽出同款。
#
# 出口协议（stdout 制表符分隔，bcts-run.sh 步①消费）：
#   META<TAB>platform_revision<TAB>tier1_floor<TAB>n_markers<TAB>n_gates<TAB>frozen_at<TAB>abi_snapshot<TAB>abi_snapshot_hash
#   MARK<TAB>phase<TAB>id<TAB>expect   （逐条）
#   GATE<TAB>id<TAB>cmd                （逐条）
# 拒面 = stderr「bcts: REJECT: <理由>」+ 退 2。
import sys


def check(path: str) -> None:
    import os, re, tomllib



    def reject(msg):
        print(f"bcts: REJECT: {msg}", file=sys.stderr)
        sys.exit(2)

    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        reject(f"TOML 解析失败：{e}")
    except OSError as e:
        reject(f"读不了工件：{e}")

    TOP_KEYS = {"schema", "suite", "platform_revision", "frozen_at",
                "abi_snapshot", "abi_snapshot_hash", "tier1_floor"}
    ARRAY_KEYS = {"marker", "host_gate"}   # 数组节键（[[…]]——tomllib 顶层展开，单列一档）
    MARKER_KEYS = {"id", "expect", "phase", "tier", "source"}
    GATE_KEYS = {"id", "cmd", "tier"}
    PHASES = {"bare", "rootfs", "rx"}

    unknown = set(data) - TOP_KEYS - ARRAY_KEYS
    if unknown:
        reject(f"顶层未知键：{sorted(unknown)}（schema add-only、未知键硬拒——设计稿 §4.1）")
    missing = TOP_KEYS - set(data)
    if missing:
        reject(f"顶层必填缺失：{sorted(missing)}")
    if data["schema"] != 1:
        reject(f"schema 须 = 1，实得 {data['schema']!r}")
    if data["suite"] != "bcts":
        reject(f"suite 须 = \"bcts\"，实得 {data['suite']!r}")
    pr = data["platform_revision"]
    if not isinstance(pr, int) or isinstance(pr, bool) or not 0 <= pr <= 2**32 - 1:
        reject(f"platform_revision 须 u32 域 [0, 4294967295]（V9 同源词表，mkimage 同款），实得 {pr!r}")
    if not re.fullmatch(r"[0-9a-f]{40}", data["frozen_at"]):
        reject(f"frozen_at 须 40 位小写 commit sha，实得 {data['frozen_at']!r}")
    if not data["abi_snapshot"]:
        reject("abi_snapshot 不得为空")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", data["abi_snapshot_hash"]):
        reject(f"abi_snapshot_hash 须 sha256:<64 小写 hex>，实得 {data['abi_snapshot_hash']!r}")
    floor = data["tier1_floor"]
    if not isinstance(floor, int) or isinstance(floor, bool) or floor < 1:
        reject(f"tier1_floor 须 ≥1 整数，实得 {floor!r}")

    markers = data.get("marker", [])
    gates = data.get("host_gate", [])
    if not isinstance(markers, list) or not markers:
        reject("[[marker]] 缺席或为空（marker 条目 = 冻结件最小单元，删除 = 破冻——设计稿 §4.1）")
    ids = set()
    for i, m in enumerate(markers, 1):
        unknown = set(m) - MARKER_KEYS
        if unknown:
            reject(f"marker[{i}] 未知键：{sorted(unknown)}")
        missing = MARKER_KEYS - set(m)
        if missing:
            reject(f"marker[{i}] 必填缺失：{sorted(missing)}")
        if not m["id"] or m["id"] in ids:
            reject(f"marker id 空或重复：{m['id']!r}")
        ids.add(m["id"])
        e = m["expect"]
        if not isinstance(e, str) or not e or re.search(r'[)"\x27\t\n]', e):
            reject(f"marker {m['id']} expect 违例（空/含引号/右括号/制表换行——markers.sh 单源约束沿袭）：{e!r}")
        if m["phase"] not in PHASES:
            reject(f"marker {m['id']} phase 须 ∈ {{bare, rootfs, rx}}，实得 {m['phase']!r}")
        if m["tier"] != 1:
            reject(f"marker {m['id']} tier 须 = 1（v0 只开通 Tier 1——设计稿 §3.2），实得 {m['tier']!r}")
        if not m["source"]:
            reject(f"marker {m['id']} source 不得为空")
    if not isinstance(gates, list) or not gates:
        reject("[[host_gate]] 缺席或为空（Tier 1 host 门禁 = abi-check 起步——设计稿 §4.2-④）")
    gids = set()
    for i, g in enumerate(gates, 1):
        unknown = set(g) - GATE_KEYS
        if unknown:
            reject(f"host_gate[{i}] 未知键：{sorted(unknown)}")
        missing = GATE_KEYS - set(g)
        if missing:
            reject(f"host_gate[{i}] 必填缺失：{sorted(missing)}")
        if not g["id"] or g["id"] in gids:
            reject(f"host_gate id 空或重复：{g['id']!r}")
        gids.add(g["id"])
        if not g["cmd"]:
            reject(f"host_gate {g['id']} cmd 不得为空")
        if g["tier"] != 1:
            reject(f"host_gate {g['id']} tier 须 = 1，实得 {g['tier']!r}")

    # 步②空集守卫（回放侧；冻结侧同式执法——守卫两层，设计稿 §4.4）。
    if len(markers) < floor:
        reject(f"空集守卫：marker {len(markers)} 件 < tier1_floor {floor}——拒跑防假绿")

    # 文件名 ↔ 字段冗余自证（版本绑定的机器表达，设计稿 §2.4-1）。
    base = os.path.basename(path)
    if base != f"bcts@{pr}.toml":
        reject(f"文件名 {base!r} 与 platform_revision={pr} 不符（bcts@<platform_revision>.toml 冗余自证）")

    print(f"META\t{pr}\t{floor}\t{len(markers)}\t{len(gates)}\t"
          f"{data['frozen_at']}\t{data['abi_snapshot']}\t{data['abi_snapshot_hash']}")
    for x in markers:
        print(f"MARK\t{x['phase']}\t{x['id']}\t{x['expect']}")
    for g in gates:
        print(f"GATE\t{g['id']}\t{g['cmd']}")


if __name__ == "__main__":
    check(sys.argv[1])
