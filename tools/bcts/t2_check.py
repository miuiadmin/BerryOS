# BCTS Tier 2 冻结件 schema 校验器（模块面——bcts-run.sh PYEOF②段机械抽出，
# bctsrun Rust 件二刀的 parity oracle 基座；t1_check.py 同款先例）。
# 抽取源 = tools/bcts-run.sh:277-497 heredoc 体逐字（`python3 - "$1" <<'PYEOF2'`
# 调用位），局部 import 归位外零变更：bcts-run.sh 改经
# `python3 "$SCRIPT_DIR/bcts/t2_check.py" "$1"` 同参调用，文案/退码逐字节不变。
#
# 出口协议（stdout 制表符分隔，bcts-run.sh t2_load 步消费）：
#   META/CONTRACT/OP/CASE/SEM 五形行（bcts-run.sh:272-275 头注原文）。
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

    TOP_KEYS = {"schema", "suite", "tier", "platform_revision", "frozen_at",
                "abi_snapshot", "abi_snapshot_hash", "tier2_floor", "contract_doc"}
    ARRAY_KEYS = {"contract"}
    CONTRACT_KEYS = {"id", "title", "interface", "interface_src", "ver_min",
                     "consumers", "suite_id", "suite_cases"}
    CONTRACT_ARRAY_KEYS = {"op", "case", "semantics"}  # 嵌套数组节（tomllib 同档单列）
    OP_KEYS = {"num", "name", "wire"}
    CASE_KEYS = {"id", "op", "kind", "class", "input", "expect", "group"}
    SEM_KEYS = {"clause", "value", "facet", "cases", "status", "note"}
    KINDS = {"pos", "neg"}
    # 五类用例（设计稿 §3.2 注 a）：合法 op 序列/非法 op 拒绝/version 协商/错误域/
    # 超时失败隔离——v0 记账不拦（开通条件随 #83 落数），值域外拒。
    CLASSES = {"seq", "reject", "version", "error", "timeout"}
    FACETS = {"串行模型", "错误码", "帽执法", "挂载语义", "诚实义务", "档位声明",
              "失败隔离", "时延", "协商", "生命周期", "呈现原子"}
    STATUS = {"frozen", "deferred"}

    def clean(v, what):
        if not isinstance(v, str) or not v or re.search(r"[\t\n\r]", v):
            reject(f"{what} 违例（空/含制表换行）：{v!r}")
        return v

    def u8(v, what):
        if not isinstance(v, int) or isinstance(v, bool) or not 0 <= v <= 255:
            reject(f"{what} 须 u8 域 [0,255]，实得 {v!r}")
        return v

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
    if data["tier"] != 2:
        reject(f"tier 须 = 2（bcts-t2 工件），实得 {data['tier']!r}")
    pr = data["platform_revision"]
    if not isinstance(pr, int) or isinstance(pr, bool) or not 0 <= pr <= 2**32 - 1:
        reject(f"platform_revision 须 u32 域 [0, 4294967295]（V9 同源词表），实得 {pr!r}")
    if not re.fullmatch(r"[0-9a-f]{40}", data["frozen_at"]):
        reject(f"frozen_at 须 40 位小写 commit sha，实得 {data['frozen_at']!r}")
    if not data["abi_snapshot"]:
        reject("abi_snapshot 不得为空")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", data["abi_snapshot_hash"]):
        reject(f"abi_snapshot_hash 须 sha256:<64 小写 hex>，实得 {data['abi_snapshot_hash']!r}")
    floor = data["tier2_floor"]
    if not isinstance(floor, int) or isinstance(floor, bool) or floor < 1:
        reject(f"tier2_floor 须 ≥1 整数，实得 {floor!r}")
    clean(data["contract_doc"], "contract_doc")

    contracts = data.get("contract", [])
    if not isinstance(contracts, list) or not contracts:
        reject("[[contract]] 缺席或为空（每开放槽位 ≥ 合同④件全集——设计稿 §3.2）")
    cids = set()
    case_ids = set()
    n_cases = 0
    for i, c in enumerate(contracts, 1):
        unknown = set(c) - CONTRACT_KEYS - CONTRACT_ARRAY_KEYS
        if unknown:
            reject(f"contract[{i}] 未知键：{sorted(unknown)}")
        missing = CONTRACT_KEYS - set(c)
        if missing:
            reject(f"contract[{i}] 必填缺失：{sorted(missing)}")
        cid = clean(c["id"], f"contract[{i}].id")
        if cid in cids:
            reject(f"contract id 重复：{cid!r}")
        cids.add(cid)
        if not re.fullmatch(r"[a-z][a-z0-9-]*", cid):
            reject(f"contract id 须 <层>-<名> 连字符串（合同 §2.6 命名规范），实得 {cid!r}")
        clean(c["title"], f"{cid}.title")
        clean(c["interface"], f"{cid}.interface")
        clean(c["interface_src"], f"{cid}.interface_src")
        if not isinstance(c["ver_min"], int) or isinstance(c["ver_min"], bool) or c["ver_min"] < 1:
            reject(f"{cid} ver_min 须 ≥1 整数，实得 {c['ver_min']!r}")
        if not isinstance(c["consumers"], list) or not c["consumers"]:
            reject(f"{cid} consumers 须非空列表")
        for x in c["consumers"]:
            clean(x, f"{cid}.consumers 元素")
        clean(c["suite_id"], f"{cid}.suite_id")
        if not isinstance(c["suite_cases"], list) or not c["suite_cases"]:
            reject(f"{cid} suite_cases 须非空列表（合同④件骨架照录）")
        sc_ids = set()
        for x in c["suite_cases"]:
            clean(x, f"{cid}.suite_cases 元素")
            if x in sc_ids:
                reject(f"{cid} suite_cases 重复：{x!r}")
            sc_ids.add(x)

        ops = c.get("op", [])
        if not isinstance(ops, list) or not ops:
            reject(f"{cid} [[contract.op]] 缺席或为空（②件 op 表 = 静态对账数据源）")
        op_nums = {}
        for j, o in enumerate(ops, 1):
            unknown = set(o) - OP_KEYS
            if unknown:
                reject(f"{cid}.op[{j}] 未知键：{sorted(unknown)}")
            missing = OP_KEYS - set(o)
            if missing:
                reject(f"{cid}.op[{j}] 必填缺失：{sorted(missing)}")
            num = u8(o["num"], f"{cid}.op[{j}].num")
            name = clean(o["name"], f"{cid}.op[{j}].name")
            if not re.fullmatch(r"[A-Z][A-Z0-9_]*", name):
                reject(f"{cid}.op[{j}] name 须协议 crate OP_ 常量后缀形，实得 {name!r}")
            if num in op_nums:
                reject(f"{cid}.op[{j}] op 号重复：{num}（{op_nums[num]} 已占）")
            op_nums[num] = name
            clean(o["wire"], f"{cid}.op[{j}].wire")

        cases = c.get("case", [])
        if not isinstance(cases, list) or not cases:
            reject(f"{cid} [[contract.case]] 缺席或为空（④件用例集 = 冻结最小单元）")
        # group 锚定域 = suite_cases ∪ ③条款键（预扫 semantics——映射表在后，锚定域在前）。
        clause_keys = {s.get("clause") for s in c.get("semantics", []) if isinstance(s, dict)}
        pos_ops, neg_ops = set(), set()
        for j, x in enumerate(cases, 1):
            unknown = set(x) - CASE_KEYS
            if unknown:
                reject(f"{cid}.case[{j}] 未知键：{sorted(unknown)}")
            missing = CASE_KEYS - {"group"} - set(x)
            if missing:
                reject(f"{cid}.case[{j}] 必填缺失：{sorted(missing)}")
            xid = clean(x["id"], f"{cid}.case[{j}].id")
            if xid in case_ids:
                reject(f"case id 全局重复：{xid!r}")
            case_ids.add(xid)
            u8(x["op"], f"{cid}.case[{j}] {xid} op")
            if x["kind"] not in KINDS:
                reject(f"{cid}.case[{j}] {xid} kind 须 ∈ {{pos, neg}}，实得 {x['kind']!r}")
            if x["class"] not in CLASSES:
                reject(f"{cid}.case[{j}] {xid} class 须五类之一（设计稿 §3.2 注 a），实得 {x['class']!r}")
            clean(x["input"], f"{cid}.case[{j}] {xid} input")
            clean(x["expect"], f"{cid}.case[{j}] {xid} expect")
            if "group" in x:
                g = clean(x["group"], f"{cid}.case[{j}] {xid} group")
                if g not in sc_ids and g not in clause_keys:
                    reject(f"{cid}.case[{j}] {xid} group {g!r} 须 ∈ suite_cases ∪ semantics 条款键（未锚定）")
            if x["kind"] == "pos":
                pos_ops.add(x["op"])
            else:
                neg_ops.add(x["op"])
        n_cases += len(cases)
        # 逐 op 覆盖守卫（Tier 2 空集守卫的逐槽口径：每 op ≥1 正例 ≥1 负例）。
        for num, name in sorted(op_nums.items()):
            if num not in pos_ops:
                reject(f"{cid} op {num}({name}) 无正例行（每 op ≥1 正例——空集守卫逐槽口径）")
            if num not in neg_ops:
                reject(f"{cid} op {num}({name}) 无负例行（每 op ≥1 负例——空集守卫逐槽口径）")

        sems = c.get("semantics", [])
        if not isinstance(sems, list) or not sems:
            reject(f"{cid} [[contract.semantics]] 缺席或为空（③→④映射表 = 合同语义条款逐条）")
        seen_clauses = set()
        for j, s in enumerate(sems, 1):
            unknown = set(s) - SEM_KEYS
            if unknown:
                reject(f"{cid}.semantics[{j}] 未知键：{sorted(unknown)}")
            missing = SEM_KEYS - {"note"} - set(s)
            if missing:
                reject(f"{cid}.semantics[{j}] 必填缺失：{sorted(missing)}")
            cl = clean(s["clause"], f"{cid}.semantics[{j}].clause")
            if cl in seen_clauses:
                reject(f"{cid}.semantics clause 重复：{cl!r}")
            seen_clauses.add(cl)
            clean(s["value"], f"{cid}.semantics[{j}] {cl} value")
            if s["facet"] not in FACETS:
                reject(f"{cid}.semantics[{j}] {cl} facet 值域外：{s['facet']!r}")
            if s["status"] not in STATUS:
                reject(f"{cid}.semantics[{j}] {cl} status 须 frozen|deferred，实得 {s['status']!r}")
            if not isinstance(s["cases"], list):
                reject(f"{cid}.semantics[{j}] {cl} cases 须列表")
            c_ids = {x["id"] for x in cases}
            for x in s["cases"]:
                clean(x, f"{cid}.semantics[{j}] {cl} cases 元素")
                if x not in c_ids and x not in sc_ids:
                    reject(f"{cid}.semantics[{j}] {cl} cases 引用未锚定用例：{x!r}")
            if s["status"] == "frozen" and not s["cases"]:
                reject(f"{cid}.semantics[{j}] {cl} status=frozen 但 cases 空（frozen 须映射已冻结 wire 行）")
            if "note" in s:
                clean(s["note"], f"{cid}.semantics[{j}] {cl} note")

    # 空集守卫（回放侧；冻结侧同式执法——守卫两层，设计稿 §4.4）。
    if n_cases < floor:
        reject(f"空集守卫：case {n_cases} 行 < tier2_floor {floor}——拒跑防假绿")

    # 文件名 ↔ 字段冗余自证（版本绑定的机器表达，设计稿 §2.4-1）。
    base = os.path.basename(path)
    if base != f"bcts-t2@{pr}.toml":
        reject(f"文件名 {base!r} 与 platform_revision={pr} 不符（bcts-t2@<platform_revision>.toml 冗余自证）")

    print(f"META\t{pr}\t{floor}\t{len(contracts)}\t{n_cases}\t"
          f"{data['frozen_at']}\t{data['abi_snapshot']}\t{data['abi_snapshot_hash']}")
    for c in contracts:
        print(f"CONTRACT\t{c['id']}\t{c['title']}\t{c['interface']}\t{c['interface_src']}\t"
              f"{c['ver_min']}\t{c['suite_id']}\t{','.join(c['suite_cases'])}\t{','.join(c['consumers'])}")
        for o in c.get("op", []):
            print(f"OP\t{c['id']}\t{o['num']}\t{o['name']}\t{o['wire']}")
        for x in c.get("case", []):
            print(f"CASE\t{c['id']}\t{x['id']}\t{x['op']}\t{x['kind']}\t{x['class']}\t"
                  f"{x.get('group', '-')}\t{x['input']}\t{x['expect']}")
        for s in c.get("semantics", []):
            print(f"SEM\t{c['id']}\t{s['clause']}\t{s['facet']}\t{s['status']}\t"
                  f"{','.join(s['cases'])}\t{s['value']}\t{s.get('note', '')}")


if __name__ == "__main__":
    check(sys.argv[1])
