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
for arg in "$@"; do
  case "$arg" in
    --dry) DRY=1 ;;
    --selftest) SELFTEST=1 ;;
    *) echo "bcts: 未知参数：${arg}（--dry | --selftest；冻结件经 BCTS_FROZEN=<file> 指定）" >&2; exit 2 ;;
  esac
done

MODE="live"
FROZEN_FILE=""
if [ -n "${BCTS_FROZEN:-}" ]; then
  MODE="frozen"
  FROZEN_FILE="$BCTS_FROZEN"
  case "$FROZEN_FILE" in /*) ;; *) FROZEN_FILE="$REPO/$FROZEN_FILE" ;; esac
  [ -f "$FROZEN_FILE" ] || { echo "bcts: FAIL: BCTS_FROZEN 工件不存在：$FROZEN_FILE" >&2; exit 1; }
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

# ---------- --selftest（零 QEMU；berry_assemble --selftest 同款合成腿风格）----------
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

  if [ "$fail_legs" -gt 0 ]; then
    echo "bcts: FAIL: selftest ${fail_legs}/${legs} 腿红" >&2
    exit 1
  fi
  echo "bcts: selftest ${legs}/${legs} 腿绿（工件 schema 自校验 + 拒面合成 + 两态清单对拍；零 QEMU）"
  exit 0
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
