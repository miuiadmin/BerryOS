#!/usr/bin/env bash
# BCTS v0 runner——兼容性测试套件回放器（BCTS 兼容性测试套件设计-v1.1 §4.2
# 五步算法；预 ADR #84）。宿主钉死现役配方树（#82 附录二 Q4 裁定）：复跑
# 全相走 tools/smoke.sh 同入口（八跑），零未开工件依赖；berry-assemble（V13）
# 成型后迁组装门禁。
#
# 两态（freeze/thaw 双轨，设计稿 §2.3）：
#   缺省                            活树自检态——清单取 tools/markers.sh 单源
#                                   （活树轨；活树绿 = 开发自证，不是兼容证据）；
#   BCTS_FROZEN=<bcts@*.toml>       回放态——冻结件对账（冻结轨；判定式 =
#                                   观测标记 ⊇ 冻结清单全集，一条不少；多打的
#                                   活树新标记不算 fail，设计稿 §2.2）。
#
# Tier 轴（设计稿 §3.2）：
#   --tier 1（缺省）  L0 syscall 面 + 装载谓词——marker 冻结件回放（上两态）；
#   --tier 2          L1 wire 契约 conformance（槽位合同四件套之④用例集）——
#                     工件前缀 bcts-t2@*.toml（BCTS_FROZEN 指定或自动识别；
#                     缺省取 frozen/ 最新 t2 件）。v0 消费面 = 清单回显 +
#                     合同条款静态对账（实现侧协议 crate op 全集 ⊇ 合同②件
#                     op 表；差集 = 开放缺口如实报，不硬绿）；动态 wire 回放
#                     随槽位开放（Q8 四件套全绿才开）。Tier 2 无活树单源
#                     （tools/slots.toml 未落，#83）——认证永不回放活树同构。
#
# 五步（设计稿 §4.2 原文照搬）：
#   ① 解析清单（上两态择一）；
#   ② 空集守卫——清单条目数 < tier1_floor → loud 拒（防假绿；markers.sh
#      尾部非空断言同族精神）；冻结态下限恒取工件自带 tier1_floor（守卫
#      不依赖外部常量——单源自证，§4.1）；
#   ③ host 门禁前置同跑——Tier 1：abi-check（清单 [[host_gate]] 数据驱动；
#      前置 = 先于 QEMU 回放，便宜且早响——smoke.sh 自身 QEMU 前也跑
#      gen_abi --check，同款分层）；
#   ④ 复跑全相——SMOKE_KEEP_LOG=1 调 tools/smoke.sh（八跑全相；构建依赖
#      同源 just build——本脚本由 `just bcts` 以 build 前置挂接，裸跑时
#      smoke.sh 自行 loud「内核未构建」）；回放后对账（③④同刀）；
#   ⑤ 产结果包——结果 + per-boot 串口日志（SMOKE_KEEP_LOG 轮转产物）+
#      清单/镜像 hash（hlkx 式 submission package 雏形，§4.2-⑤/§5.2）。
#
# 用法：
#   just bcts                                              # 活树自检态（build 前置）
#   BCTS_FROZEN=tools/bcts/frozen/bcts@0.toml just bcts    # 回放态（冻结件对账）
#   tools/bcts-run.sh --dry                                # 清单回显（零 QEMU；两态均可）
#   tools/bcts-run.sh --selftest                           # 工件 schema 自校验 + dry
#                                                          # 回显 + 拒面合成腿（零 QEMU）
#   tools/bcts-run.sh --tier 2 [--dry]                     # Tier 2：槽位合同冻结用例集
#                                                          # 回显 + 静态对账（零 QEMU）
#   BCTS_FROZEN=tools/bcts/frozen/bcts-t2@0.toml just bcts # Tier 2（前缀自动识别）
#
# v0 边界（如实记）：镜像侧版本声称位对账（manifest.txt platform_revision
# 列，V9）随组装镜像宿主（V13）启用——v0 回放对象 = QEMU virt -kernel ELF
# 直载形态（AGENTS.md 主验位），工件版本记入结果包；真机回放随 P8（闸 D）。
#
# 判定语义：exit 0 = 全绿（门禁 + 回放 + 对账）；非 0 = FAIL（含 loud 理由）。
# 依赖：Python ≥3.11（stdlib tomllib——gen_abi/berry_assemble 同先例），零
# 三方依赖；QEMU 仅复跑态需要（经 smoke.sh）。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.." || { echo "bcts: FAIL: 进不了仓库根" >&2; exit 1; }
REPO=$(pwd)
# 内核路径解析收敛在 paths.sh（单一事实源，smoke/justfile 同源）。
source "$SCRIPT_DIR/paths.sh"

FROZEN_HOME="$SCRIPT_DIR/bcts/frozen"
# 活树自检态守卫下限（Tier 1 在册值 88 = 裸相 32 + rootfs 54 + RX 2，设计稿
# §3.2；活树态无工件可携 floor，取在册常量——冻结态不用此值）。
TIER1_FLOOR_LIVE=88

# ---------- 参数 ----------
DRY=0
SELFTEST=0
TIER=1
TIER_EXPLICIT=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry) DRY=1 ;;
    --selftest) SELFTEST=1 ;;
    --tier)
      [ $# -ge 2 ] || { echo "bcts: --tier 缺值（1 | 2）" >&2; exit 2; }
      case "$2" in
        1|2) TIER=$2; TIER_EXPLICIT=1 ;;
        *) echo "bcts: --tier 须 1 或 2，实得 $2（v0 只开通 Tier 1/2）" >&2; exit 2 ;;
      esac
      shift ;;
    *) echo "bcts: 未知参数：${1}（--dry | --selftest | --tier 1|2；冻结件经 BCTS_FROZEN=<file> 指定）" >&2; exit 2 ;;
  esac
  shift
done

MODE="live"
FROZEN_FILE=""
if [ -n "${BCTS_FROZEN:-}" ]; then
  MODE="frozen"
  FROZEN_FILE="$BCTS_FROZEN"
  case "$FROZEN_FILE" in /*) ;; *) FROZEN_FILE="$REPO/$FROZEN_FILE" ;; esac
  [ -f "$FROZEN_FILE" ] || { echo "bcts: FAIL: BCTS_FROZEN 工件不存在：$FROZEN_FILE" >&2; exit 1; }
  # 工件前缀自动识别 Tier（bcts-t2@* → Tier 2；bcts@* → Tier 1）——仅显式
  # --tier 与前缀冲突才拒（缺省 --tier 1 让位于前缀识别）。
  case "$(basename "$FROZEN_FILE")" in
    bcts-t2@*.toml)
      if [ "$TIER_EXPLICIT" = 1 ] && [ "$TIER" = 1 ]; then
        echo "bcts: FAIL: 工件 $(basename "$FROZEN_FILE") 是 Tier 2 件，与 --tier 1 冲突（用 --tier 2 或去参自动识别）" >&2
        exit 2
      fi
      TIER=2 ;;
    bcts@*.toml)
      if [ "$TIER" = 2 ]; then
        echo "bcts: FAIL: 工件 $(basename "$FROZEN_FILE") 是 Tier 1 件，与 --tier 2 冲突（Tier 2 工件前缀 = bcts-t2@*.toml）" >&2
        exit 2
      fi ;;
    *)
      echo "bcts: FAIL: 工件名 $(basename "$FROZEN_FILE") 不符（bcts@<pr>.toml | bcts-t2@<pr>.toml）" >&2
      exit 2 ;;
  esac
fi
# Tier 2 缺省工件：无活树单源（slots.toml 未落）→ 取 frozen/ 最新 bcts-t2@*.toml。
if [ "$TIER" = 2 ] && [ -z "$FROZEN_FILE" ]; then
  FROZEN_FILE=$(ls "$FROZEN_HOME"/bcts-t2@*.toml 2>/dev/null | sort -t@ -k2 -n | tail -1)
  [ -n "$FROZEN_FILE" ] || { echo "bcts: FAIL: $FROZEN_HOME 无 bcts-t2@*.toml 冻结件（Tier 2 无活树单源）" >&2; exit 1; }
  MODE="frozen"
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/berryos-bcts-work.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

# ---------- 步①：解析清单 ----------
# 冻结件：tomllib 解析 + schema 校验（未知键硬拒——berry-manifest 七拒同构：
# 一行理由 + 键名 + 非零退）。出口协议（制表符分隔）：
#   META<TAB>platform_revision<TAB>tier1_floor<TAB>n_markers<TAB>n_gates<TAB>frozen_at<TAB>abi_snapshot<TAB>abi_snapshot_hash
#   MARK<TAB>phase<TAB>id<TAB>expect   （逐条）
#   GATE<TAB>id<TAB>cmd                （逐条）
# 拒面 = stderr「bcts: REJECT: <理由>」+ 退 2。
parse_frozen() {
  python3 - "$1" <<'PYEOF'
import os, re, sys, tomllib

path = sys.argv[1]

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
PYEOF
}

# 活树清单合成（markers.sh 单源 → 同构 TSV；id 按相内序号机械生成——冻结
# 件即本合成的快照形态，两态同构对账）。
build_live_tsv() {
  local tsv="$1"
  if ! bash -c '
      set -u
      src="'"$SCRIPT_DIR"'/markers.sh"
      source "$src"
      n=0; for m in "${MARKERS[@]}"; do n=$((n+1)); printf "MARK\tbare\tbare-%02d\t%s\n" "$n" "$m"; done
      n=0; for m in "${ROOTFS_MARKERS[@]}"; do n=$((n+1)); printf "MARK\trootfs\trootfs-%02d\t%s\n" "$n" "$m"; done
      n=0; for m in "${RX_MARKERS[@]}"; do n=$((n+1)); printf "MARK\trx\trx-%02d\t%s\n" "$n" "$m"; done
      printf "GATE\tabi-check\tjust abi-check\n"
    ' > "$tsv" 2> "$WORK/live-src.err"; then
    echo "bcts: REJECT: 活树清单合成失败（markers.sh 非空守卫拒或数组缺席）：" >&2
    cat "$WORK/live-src.err" >&2
    return 2
  fi
}

sha256_of() {
  shasum -a 256 "$1" 2>/dev/null | cut -d' ' -f1 \
    || sha256sum "$1" 2>/dev/null | cut -d' ' -f1
}

# ---------- Tier 2：槽位合同冻结用例集（L1 wire 契约 conformance v0） ----------
# 工件 = bcts-t2@<platform_revision>.toml；schema 纪律同 Tier 1（add-only、
# 未知键硬拒、文件名↔platform_revision 冗余自证、空集守卫进工件自带 floor）。
# 出口协议（制表符分隔；字符串字段一律无制表换行——parse 侧执法）：
#   META<TAB>pr<TAB>tier2_floor<TAB>n_contracts<TAB>n_cases<TAB>frozen_at<TAB>abi_snapshot<TAB>abi_hash
#   CONTRACT<TAB>id<TAB>title<TAB>interface<TAB>interface_src<TAB>ver_min<TAB>suite_id<TAB>suite_cases<TAB>consumers
#   OP<TAB>cid<TAB>num<TAB>name<TAB>wire              （合同②件 op 表，逐行）
#   CASE<TAB>cid<TAB>id<TAB>op<TAB>kind<TAB>class<TAB>group<TAB>input<TAB>expect（④件用例行）
#   SEM<TAB>cid<TAB>clause<TAB>facet<TAB>status<TAB>cases<TAB>value<TAB>note    （③→④映射行）
# 拒面 = stderr「bcts: REJECT: <理由>」+ 退 2。
parse_t2_frozen() {
  python3 - "$1" <<'PYEOF2'
import os, re, sys, tomllib

path = sys.argv[1]

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
PYEOF2
}

# 静态对账（v0 消费面核心）：实现侧协议 crate op 全集 ⊇ 合同②件 op 表。
# 提取法 = crate 源 `pub const OP_<NAME>: u8 = <num>;`（十/十六进制同收）；
# 差集（合同有 crate 无 / 错号）= 开放缺口 loud 报 + 退 1（不硬绿）；crate 侧
# 多出 op = add-only 合法（L1 纪律），计行不拦。另报④件五类用例覆盖（缺口类
# 如实记账不拦——Tier 2 开通条件随 #83 落数，设计稿 §3.2 注 a）。
reconcile_t2() {
  python3 - "$1" "$REPO" <<'PYEOF3'
import os, re, sys

tsv, repo = sys.argv[1], sys.argv[2]
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
PYEOF3
}

# Tier 2 装载/回显/执行壳（与 Tier 1 路径互不扰动；零 QEMU）。
t2_load() {  # $1=工件 → $WORK/entries-t2.tsv；拒 = 退 2
  parse_t2_frozen "$1" > "$WORK/entries-t2.tsv" || return 2
}
t2_dry() {  # 清单回显（零对账）
  t2_load "$1" || return 2
  echo "bcts: 模式 = frozen(t2)；清单 = $1"
  awk -F'\t' -v OFS='\t' '$1=="META"{print "bcts: 合同 "$4" 例 / 用例行 "$5"（≥ tier2_floor "$3"）；platform_revision = "$2"；frozen_at = "$6}' "$WORK/entries-t2.tsv"
  awk -F'\t' 'function opn(n){return (n<16)?sprintf("%d",n):sprintf("0x%02x",n)}
              $1=="CONTRACT"{printf "bcts: [%s] %s（%s，%s）ver_min=%s suite=%s\n",$2,$3,$4,$5,$6,$7}
              $1=="OP"{printf "bcts:   op %-4s %-12s %s\n",opn($3),$4,$5}
              $1=="CASE"{printf "bcts:   case %-24s op=%-4s %-3s %-7s [%s]\n            in : %s\n            exp: %s\n",$3,opn($4),$5,$6,$7,$8,$9}
              $1=="SEM"{printf "bcts:   sem %-18s [%s/%s] → %s\n            %s = %s\n",$3" ",$4,$5,($6==""?"（空）":$6),$3,$7}' "$WORK/entries-t2.tsv"
  echo "bcts: dry 完（Tier 2 v0 = 静态对账 + 动态回放随槽位开放；去 --dry 跑对账）"
}
t2_run() {  # 静态对账真跑；绿 = 0 / 缺口 = 1 / 拒 = 2
  t2_load "$1" || return 2
  echo "bcts: 模式 = frozen(t2)；清单 = $1"
  awk -F'\t' '$1=="META"{print "bcts: 合同 "$4" 例 / 用例行 "$5"（≥ tier2_floor "$3"）；platform_revision = "$2"；frozen_at = "$6"；abi_snapshot = "$7"（"$8"）"}' "$WORK/entries-t2.tsv"
  reconcile_t2 "$WORK/entries-t2.tsv"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "bcts: verdict = PASS（T2 v0 = 静态对账；动态 wire 回放随槽位开放——Q8 四件套全绿才开）"
  fi
  return "$rc"
}

# ---------- --selftest（零 QEMU；berry_assemble --selftest 同款合成腿风格）----------
# Tier 2 拒面/咬合合成用变异器（只读真件，产物进临时目录）。
t2_mut_drop_case() {  # $1=工件 $2=case id → 删该 [[contract.case]] 块
  awk -v drop_id="id = \"$2\"" '
    /^\[\[/ { if (hold != "") { if (hold !~ drop_id) printf "%s", hold; hold = "" }
              if ($0 ~ /^\[\[contract\.case\]\]/) { hold = $0 ORS; next }
              print; next }
    hold != "" { hold = hold $0 ORS; next }
    { print }
    END { if (hold != "" && hold !~ drop_id) printf "%s", hold }
  ' "$1"
}
t2_mut_rename_op() {  # $1=工件 $2=旧名 $3=新名（合同 op 改名 → 对账差集必红）
  sed "s/^name = \"$2\"$/name = \"$3\"/" "$1"
}

if [ "$SELFTEST" = 1 ]; then
  legs=0
  fail_legs=0
  leg() {  # $1=腿名 $2=期望 0过/2拒 $3=命令串
    legs=$((legs+1))
    eval "$3" > "$WORK/st.out" 2> "$WORK/st.err"
    rc=$?
    if [ "$rc" -eq "$2" ]; then
      echo "bcts: selftest 腿绿：$1"
    else
      echo "bcts: selftest 腿红：$1（期望 rc=$2 实得 rc=${rc}）：$(tail -2 "$WORK/st.err" | tr '\n' ' ')" >&2
      fail_legs=$((fail_legs+1))
    fi
  }

  # 现役工件清点（发布后只读、add-only——发布多件则逐件过检）。
  arts=$(ls "$FROZEN_HOME"/bcts@*.toml 2>/dev/null) || true
  [ -n "$arts" ] || { echo "bcts: FAIL: $FROZEN_HOME 无 bcts@*.toml 冻结件" >&2; exit 1; }
  for a in $arts; do
    leg "工件 schema 自校验（$(basename "$a")，含空集守卫+文件名自证）" 0 \
      "parse_frozen '$a' | grep -q $'^META\\t0\\t88\\t88\\t1\\t'"
  done
  ART=$(printf '%s\n' $arts | head -1)

  # 拒面合成腿（逐型；临时件名守 bcts@0.toml 自证形，失配腿除外）。
  BAD=$(mktemp -d "${TMPDIR:-/tmp}/berryos-bcts-selftest.XXXXXX")
  sed 's/^schema = 1$/schema = 1\nextra_key = 1/' "$ART" > "$BAD/bcts@0.toml"
  leg "顶层未知键硬拒" 2 "parse_frozen '$BAD/bcts@0.toml'"
  awk '$0=="[[marker]]" && !d {d=1; drop=1; next} drop && /^\[\[/ {drop=0} !drop {print}' "$ART" > "$BAD/bcts@0.toml"
  leg "空集守卫（87 件 < tier1_floor 88 拒跑）" 2 "parse_frozen '$BAD/bcts@0.toml'"
  sed 's/^phase = "bare"$/phase = "efi"/' "$ART" > "$BAD/bcts@0.toml"
  leg "phase 值域外拒（efi 非 v0 冻结相）" 2 "parse_frozen '$BAD/bcts@0.toml'"
  sed '/^tier1_floor = /d' "$ART" > "$BAD/bcts@0.toml"
  leg "必填键缺失拒（tier1_floor）" 2 "parse_frozen '$BAD/bcts@0.toml'"
  cp "$ART" "$BAD/bcts@7.toml"
  leg "文件名↔platform_revision 失配拒（冗余自证）" 2 "parse_frozen '$BAD/bcts@7.toml'"
  rm -rf "$BAD"

  # 两态 dry 回显（活树腿同时复核单源计数 32/54/2）。
  legs=$((legs+1))
  build_live_tsv "$WORK/live.tsv"
  live_n=$(grep -c $'^MARK\t' "$WORK/live.tsv")
  live_bare=$(grep -c $'^MARK\tbare\t' "$WORK/live.tsv")
  live_rootfs=$(grep -c $'^MARK\trootfs\t' "$WORK/live.tsv")
  live_rx=$(grep -c $'^MARK\trx\t' "$WORK/live.tsv")
  if [ "$live_n" -ge "$TIER1_FLOOR_LIVE" ] && [ "$live_bare" -eq 32 ] && [ "$live_rootfs" -eq 54 ] && [ "$live_rx" -eq 2 ]; then
    echo "bcts: selftest 腿绿：活树清单合成（${live_n} 件 = 裸相 ${live_bare} + rootfs ${live_rootfs} + RX ${live_rx}，≥ 下限 ${TIER1_FLOOR_LIVE}）"
  else
    echo "bcts: selftest 腿红：活树计数漂移（bare=${live_bare} rootfs=${live_rootfs} rx=${live_rx}）" >&2
    fail_legs=$((fail_legs+1))
  fi
  # 冻结件 ↔ 活树单源快照对拍（首冻件不变量：expect 序列双向全等——后续发版
  # 滚入新冻结件后活树增长属常态，本腿只对最新工件执法）。
  legs=$((legs+1))
  parse_frozen "$ART" | grep $'^MARK\t' | cut -f4 > "$WORK/frozen-expects"
  grep $'^MARK\t' "$WORK/live.tsv" | cut -f4 > "$WORK/live-expects"
  if cmp -s "$WORK/frozen-expects" "$WORK/live-expects"; then
    echo "bcts: selftest 腿绿：冻结件 ↔ markers.sh 单源快照全等（88 件）"
  else
    echo "bcts: selftest 腿红：冻结件与活树标记集漂移（$(diff "$WORK/frozen-expects" "$WORK/live-expects" | head -4 | tr '\n' ' ')）" >&2
    fail_legs=$((fail_legs+1))
  fi

  # ---------- Tier 2 腿（bcts-t2 工件 schema + 拒面 + 静态对账真跑 + 咬合） ----------
  arts2=$(ls "$FROZEN_HOME"/bcts-t2@*.toml 2>/dev/null) || true
  [ -n "$arts2" ] || { echo "bcts: FAIL: $FROZEN_HOME 无 bcts-t2@*.toml 冻结件" >&2; exit 1; }
  for a in $arts2; do
    leg "T2 工件 schema 自校验（$(basename "$a")，含空集守卫+逐 op 覆盖+文件名自证）" 0 \
      "parse_t2_frozen '$a' | grep -q $'^META\\t0\\t31\\t2\\t31\\t'"
  done
  ART2=$(printf '%s\n' $arts2 | head -1)

  # T2 拒面合成腿（逐型；临时件名守 bcts-t2@0.toml 自证形，失配腿除外）。
  BAD2=$(mktemp -d "${TMPDIR:-/tmp}/berryos-bcts-t2-selftest.XXXXXX")
  sed 's/^schema = 1$/schema = 1\nextra_key = 1/' "$ART2" > "$BAD2/bcts-t2@0.toml"
  leg "T2 顶层未知键硬拒" 2 "parse_t2_frozen '$BAD2/bcts-t2@0.toml'"
  t2_mut_drop_case "$ART2" hdi-prepare-neg-slotcap > "$BAD2/bcts-t2@0.toml"
  leg "T2 空集守卫（30 行 < tier2_floor 31 拒跑）" 2 "parse_t2_frozen '$BAD2/bcts-t2@0.toml'"
  sed 's/^class = "seq"$/class = "bogus"/' "$ART2" > "$BAD2/bcts-t2@0.toml"
  leg "T2 class 值域外拒（bogus 非五类）" 2 "parse_t2_frozen '$BAD2/bcts-t2@0.toml'"
  cp "$ART2" "$BAD2/bcts-t2@7.toml"
  leg "T2 文件名↔platform_revision 失配拒（冗余自证）" 2 "parse_t2_frozen '$BAD2/bcts-t2@7.toml'"
  rm -rf "$BAD2"

  # T2 静态对账真跑（两实例合同表 vs 现役协议 crate op 面）+ 咬合证明（不硬绿）
  # + 清单回显面。
  leg "T2 静态对账真跑（两实例：berry-fsproto/berry-guiproto op 全集 ⊇ 合同②件表）" 0 "t2_run '$ART2'"
  BAD3=$(mktemp -d "${TMPDIR:-/tmp}/berryos-bcts-t2-bite.XXXXXX")
  t2_mut_rename_op "$ART2" PUT PUTX > "$BAD3/bcts-t2@0.toml"
  leg "T2 对账咬合（合同 op 改名 PUT→PUTX → crate 差集必红）" 1 "t2_run '$BAD3/bcts-t2@0.toml'"
  rm -rf "$BAD3"
  leg "T2 --dry 清单回显（含两合同 suite id）" 0 "t2_dry '$ART2' | grep -q 'bcts-l1-hdi-v1'"

  if [ "$fail_legs" -gt 0 ]; then
    echo "bcts: FAIL: selftest ${fail_legs}/${legs} 腿红" >&2
    exit 1
  fi
  echo "bcts: selftest ${legs}/${legs} 腿绿（T1 工件 schema 自校验 + 拒面合成 + 两态清单对拍 + T2 对账真跑/咬合；零 QEMU）"
  exit 0
fi

# ---------- Tier 2 分流（与 Tier 1 路径互不扰动；零 QEMU——v0 无 wire 回放） ----------
if [ "$TIER" = 2 ]; then
  if [ "$DRY" = 1 ]; then
    t2_dry "$FROZEN_FILE"
  else
    t2_run "$FROZEN_FILE"
  fi
  exit $?
fi

# ---------- 步①（续）：装载清单 → $WORK/entries.tsv ----------
if [ "$MODE" = frozen ]; then
  parse_frozen "$FROZEN_FILE" > "$WORK/entries.tsv" || exit 2
else
  build_live_tsv "$WORK/entries.tsv" || exit 2
fi
N_MARKERS=$(grep -c $'^MARK\t' "$WORK/entries.tsv" || true)
N_GATES=$(grep -c $'^GATE\t' "$WORK/entries.tsv" || true)

# ---------- 步②：空集守卫 ----------
if [ "$MODE" = frozen ]; then
  FLOOR=$(head -1 "$WORK/entries.tsv" | cut -f3)
else
  FLOOR=$TIER1_FLOOR_LIVE
fi
if [ "${N_MARKERS:-0}" -lt "$FLOOR" ]; then
  echo "bcts: FAIL: 空集守卫——清单 ${N_MARKERS:-0} 件 < tier1_floor ${FLOOR}（防假绿；设计稿 §4.2-②）" >&2
  exit 1
fi

# ---------- --dry：清单回显（零 QEMU） ----------
if [ "$DRY" = 1 ]; then
  echo "bcts: 模式 = ${MODE}；清单 = ${FROZEN_FILE:-tools/markers.sh（活树单源）}"
  echo "bcts: 标记 ${N_MARKERS} 件（≥ tier1_floor ${FLOOR}）；host 门禁 ${N_GATES} 个"
  if [ "$MODE" = frozen ]; then
    head -1 "$WORK/entries.tsv" | awk -F'\t' \
      '{print "bcts: platform_revision = "$2"；frozen_at = "$6"；abi_snapshot = "$7"（"$8"）"}'
  fi
  awk -F'\t' '$1=="MARK"{printf "  marker %-9s %-10s %s\n",$2,$3,$4}
              $1=="GATE"{printf "  gate   %-10s %s\n",$2,$3}' "$WORK/entries.tsv"
  echo "bcts: dry 完（判定式 = 观测标记 ⊇ 清单全集；多打不算 fail）"
  exit 0
fi

# ---------- 步③：host 门禁前置同跑 ----------
GATE_SUMMARY=""
GATE_FAIL=0
while IFS=$'\t' read -r tag gid cmd; do
  [ "$tag" = GATE ] || continue
  echo "bcts: host 门禁 [$gid]：$cmd"
  if bash -c "$cmd" > "$WORK/gate-$gid.log" 2>&1; then
    echo "bcts: host 门禁 [$gid] PASS"
    GATE_SUMMARY="${GATE_SUMMARY}${GATE_SUMMARY:+；}${gid}: PASS"
  else
    echo "bcts: FAIL: host 门禁 [$gid] 拒（${cmd}）；日志尾 5 行：" >&2
    tail -5 "$WORK/gate-$gid.log" >&2
    GATE_SUMMARY="${GATE_SUMMARY}${GATE_SUMMARY:+；}${gid}: FAIL"
    GATE_FAIL=1
  fi
done < "$WORK/entries.tsv"
[ "$GATE_FAIL" = 0 ] || exit 1

# ---------- 步④：复跑全相（八跑；SMOKE_KEEP_LOG=1 留 per-boot 串口日志） ----------
# SMOKE_PHASES 显式 all：防外层环境带 boot 分层截断回放面（八跑全相 = 冻结
# 件的执行单元——设计稿 §3.2 宽口径）。端口锁/清理 trap/echod 对拍由
# smoke.sh 自理（与 just smoke 完全同入口）。
KERNEL=$(berryos_kernel debug) || { echo "bcts: FAIL: 内核路径解析失败（paths.sh）" >&2; exit 1; }
echo "bcts: 复跑全相（SMOKE_KEEP_LOG=1 tools/smoke.sh，八跑——回放对象 ${KERNEL}）"
SMOKE_OUT="$WORK/smoke.out"
if ! SMOKE_KEEP_LOG=1 SMOKE_PHASES=all bash "$SCRIPT_DIR/smoke.sh" > "$SMOKE_OUT" 2>&1; then
  echo "bcts: FAIL: smoke 全相复跑非零退（smoke.out 尾 40 行：）" >&2
  tail -40 "$SMOKE_OUT" >&2
  exit 1
fi
echo "bcts: smoke 八跑全相绿（回放日志采集中）"
# 保留串口日志行形如「smoke: 串口日志保留：<path>」（per-boot 轮转产物）。
# 串口日志路径单经列表文件流转（macOS /bin/bash=3.2 无 mapfile——不依赖）。
SERIAL_LIST="$WORK/serial-logs.txt"
sed -n 's/^smoke: 串口日志保留：//p' "$SMOKE_OUT" > "$SERIAL_LIST"
N_LOGS=$(grep -c . "$SERIAL_LIST" || true)
[ "${N_LOGS:-0}" -ge 1 ] || { echo "bcts: FAIL: 未取到保留串口日志（SMOKE_KEEP_LOG 未生效？）" >&2; exit 1; }
CORPUS="$WORK/observed.log"
: > "$CORPUS"
while IFS= read -r lg; do cat "$lg" >> "$CORPUS"; done < "$SERIAL_LIST"

# ---------- 步④（续）：对账（观测标记 ⊇ 清单全集） ----------
# 标记对拍 = smoke.sh assert_marker 同款语义镜像（DEMO/TAB 前邻特判防子串
# 假阳性——smoke.sh assert_marker 注；元素不含引号/右括号约束下 grep -qF
# 安全内插）。观测语料 = 八跑串口日志并集（标记在任一 boot 出现即计入）。
assert_marker() {
  case "$1" in
    "DEMO ok") grep -qE '(^|[^I])DEMO ok' "$CORPUS" ;;
    "TAB ok")  grep -qE '(^|[^G])TAB ok'  "$CORPUS" ;;
    *)         grep -qF "$1" "$CORPUS" ;;
  esac
}
PASS_N=0
MISSING=""
while IFS=$'\t' read -r tag phase mid expect; do
  [ "$tag" = MARK ] || continue
  if assert_marker "$expect"; then
    PASS_N=$((PASS_N+1))
  else
    MISSING="${MISSING:+$MISSING }$mid"
  fi
done < "$WORK/entries.tsv"

# ---------- 步⑤：产结果包（结果 + per-boot 日志 + 清单/镜像 hash） ----------
RESULT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/berryos-bcts-result.XXXXXX")
mkdir -p "$RESULT_DIR/logs"
i=0
while IFS= read -r lg; do
  i=$((i+1))
  cp "$lg" "$RESULT_DIR/logs/boot-$(printf '%02d' "$i").log"
done < "$SERIAL_LIST"
KERNEL_SHA=$(sha256_of "$KERNEL")
ART_LINE="artifact            = live（tools/markers.sh 单源）"
ART_SHA=""
if [ "$MODE" = frozen ]; then
  ART_SHA=$(sha256_of "$FROZEN_FILE")
  ART_LINE="artifact            = $FROZEN_FILE
artifact_sha256     = sha256:$ART_SHA"
fi
VERDICT=FAIL
[ -z "$MISSING" ] && VERDICT=PASS
{
  echo "# BCTS v0 结果包（hlkx 式 submission package 雏形——设计稿 §4.2-⑤/§5.2）"
  echo "mode                = $MODE"
  echo "$ART_LINE"
  if [ "$MODE" = frozen ]; then
    head -1 "$WORK/entries.tsv" | awk -F'\t' \
      '{print "platform_revision   = "$2; print "frozen_at           = "$6;
              print "abi_snapshot        = "$7; print "abi_snapshot_hash   = "$8}'
  fi
  echo "kernel_image        = $KERNEL"
  echo "kernel_sha256       = sha256:${KERNEL_SHA}（v0 镜像 = QEMU -kernel ELF 直载形态；组装镜像 hash 随 V13 宿主迁入）"
  echo "host_gates          = $GATE_SUMMARY"
  echo "smoke               = PASS（八跑全相，退出码 0）"
  echo "verdict             = ${VERDICT}（判定式 = 观测标记 ⊇ 清单全集，一条不少；多打不算 fail）"
  echo "markers             = ${PASS_N}/${N_MARKERS} 过；missing = ${MISSING:-（无）}"
  echo "serial_logs         = logs/boot-01.log … boot-$(printf '%02d' "$i").log（共 $i 份，八跑轮转）"
} > "$RESULT_DIR/bcts-result.txt"

# 串口日志原件（smoke 的 mktemp 保留件）已并入结果包——清原件防 /tmp 残留
# （BCTS_KEEP_LOG=1 时保留现场复验）。
if [ -z "${BCTS_KEEP_LOG:-}" ]; then
  while IFS= read -r lg; do rm -f "$lg"; done < "$SERIAL_LIST"
fi

echo "bcts: verdict = ${VERDICT}（${PASS_N}/${N_MARKERS}；missing = ${MISSING:-无}）"
echo "bcts: 结果包：$RESULT_DIR/bcts-result.txt（per-boot 日志同目录 logs/）"
[ "$VERDICT" = PASS ] || exit 1
exit 0
