#!/bin/sh
# fwcfg 键表对拍：tools/fwcfg-keys.toml（单源键表）vs 仓内实际注入/消费的
# opt/berry/<name> 键名集合。零漂移 = 两侧键名集合相等；漂移 = 键表漏登记
# 或死键新增。用法：bash tools/fwcfg-check.sh（退出 0=对拍过，1=有漂移）。
#
# 引擎 = git grep（tracked 单源，-I 跳二进制）：FW_CFG_DIR 对拍只对源码面
# 有义——构建产物/未跟踪草稿不是注入面；rg 依赖 runner 缺件判例（#371
# rsync 同款），git 为 CI 第七门 --run 面可依赖件。
set -e
cd "$(dirname "$0")/.."

# ---------- --selftest（合成沙箱钉板，零网络零真仓；改对拍引擎/排除面/键长门必跑） ----------
if [ "${1:-}" = "--selftest" ]; then
  SELF=$(mktemp -d "${TMPDIR:-/tmp}/berryos-fwcfgcheck-selftest.XXXXXX")
  cleanup() { rm -rf "$SELF"; }
  trap cleanup EXIT
  mkdir -p "${SELF}/root/tools" "${SELF}/root/fwcfgface/src" "${SELF}/root/kernel/src"
  cp "$0" "${SELF}/root/tools/fwcfg-check.sh"
  S="${SELF}/root/tools/fwcfg-check.sh"

  # 合成仓：真注入/消费面 + 解析 crate 合成 fixture（应被排除）两树。
  ( cd "${SELF}/root" && git init -q . \
    && git config user.email t@t && git config user.name t \
    && printf 'find("opt/berry/accel")\n' > kernel/src/main.rs \
    && printf 'find_file_in("opt/berry/trac", &mut s)\n' > fwcfgface/src/tests.rs \
    && git add -A && git commit -qm init ) >/dev/null 2>&1

  mk_table() {  # mk_table <键名...> —— 重写合成键表并入库（腿间独立）
    printf '# 合成键表\n\nversion = 1\n' > "${SELF}/root/tools/fwcfg-keys.toml"
    for k in "$@"; do
      printf '\n[[keys]]\nname = "%s"\n' "$k" >> "${SELF}/root/tools/fwcfg-keys.toml"
    done
    ( cd "${SELF}/root" && git add -A && git commit -qm table ) >/dev/null 2>&1
  }
  track() {  # track <文件> —— 追加内容入库
    ( cd "${SELF}/root" && git add -A && git commit -qm track ) >/dev/null 2>&1
  }

  legs=0; fails=0
  leg() {  # leg <名> <期望rc> <锚:stdout|stderr|-> <pattern> <cmd…>
    name="$1"; want="$2"; face="$3"; pat="$4"; shift 4
    rc=0
    ( "$@" ) >"${SELF}/out.log" 2>&1 || rc=$?
    legs=$((legs+1))
    why=""
    [ "$rc" = "$want" ] || why="rc 期望 ${want} 实得 ${rc}"
    if [ -z "$why" ] && [ "$face" != "-" ] && ! grep -qF "$pat" "${SELF}/out.log"; then
      why="输出缺锚「${pat}」"
    fi
    if [ -n "$why" ]; then
      fails=$((fails+1)); echo "fwcfg-check selftest 腿红：${name}（${why}）" >&2
      tail -4 "${SELF}/out.log" >&2
    else
      echo "fwcfg-check selftest 腿绿：${name}"
    fi
  }

  run() { sh "$S"; }

  # 腿 1：对拍绿——fixture 排除（trac 不进代码面）+ rootfsN 归并。
  printf 'find("opt/berry/rootfs3")\n' >> "${SELF}/root/kernel/src/main.rs"
  track
  mk_table accel rootfs0..9
  leg "对拍绿（fixture 排除 + rootfsN 归并）" 0 stdout "对拍过（2 键" run
  # 腿 2-3：漏登记红（accel 撤表 → 代码在用键表没有）。
  mk_table rootfs0..9
  leg "漏登记红" 1 stderr "键表漏登记" run
  leg "漏登记点名 accel" 1 stderr "accel" run
  # 腿 4：死键红（表多 unknown-key）。
  mk_table accel rootfs0..9 unknown-key
  leg "死键红" 1 stderr "键表多余" run
  leg "死键点名 unknown-key" 1 stderr "unknown-key" run
  # 腿 5：键长上限红（name 46B → 全名 56B > 55——FW_CFG_DIR 定长项）。
  longk=$(printf 'a%.0s' $(seq 1 46))
  mk_table accel rootfs0..9 "$longk"
  leg "键长上限红（56B > 55）" 1 stderr "键名超长" run
  # 腿 6：键长边界绿（name 45B → 全名恰 55B）。
  edgek=$(printf 'b%.0s' $(seq 1 45))
  printf 'find("opt/berry/%s")\n' "$edgek" >> "${SELF}/root/kernel/src/main.rs"
  track
  mk_table accel rootfs0..9 "$edgek"
  leg "键长边界绿（恰 55B）" 0 stdout "对拍过" run
  # 腿 7：.md 例文不进代码面（文档例文非注入面）。
  printf 'opt/berry/doconly-key\n' > "${SELF}/root/NOTE.md"
  track
  leg ".md 例文不进代码面" 0 stdout "对拍过" run
  # 腿 8：非 git 树零命中红——扫描面损坏显式拒（吞错静默缩面的对面纪律）。
  mkdir -p "${SELF}/norepo/tools"
  cp "$S" "${SELF}/norepo/tools/fwcfg-check.sh"
  leg "非 git 树扫描面损坏红" 1 stderr "扫描面损坏" sh "${SELF}/norepo/tools/fwcfg-check.sh"

  echo "fwcfg-check --selftest: ${legs} 腿，${fails} 红"
  [ "$fails" -eq 0 ] || exit 1
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# 代码侧：tracked 面提取 opt/berry/<name>（键名字符集含下划线/大写——正则
# 宽于现行全小写键名，防未来键名形态让对拍假阴性）；rootfsN 数字卷归并
# rootfs0..9。排除：*.md/.agents/docs（文档例文非注入）；
# fwcfgface/src/tests.rs（纯解析 crate 合成 fixture——trac/missing 等非
# 等值匹配腿非注入/消费面，base 曾因此假红）。
# 零命中 = 扫描面构造性损坏（排除面过宽/git 缺件），显式红不静默缩面。
# 排除检测器自身（本脚本）：selftest 夹具字面量（trac/doconly-key 等）非
# 注入面——检测器与被测面分离，同 fwcfgface fixture 纪律。
git grep -hoIE 'opt/berry/[A-Za-z0-9_-]+' -- \
  ':!*.md' ':!.agents' ':!docs' ':!fwcfgface/src/tests.rs' \
  ':!tools/fwcfg-check.sh' \
  | sed -E -e 's|opt/berry/||' -e 's/^rootfs[0-9]+$/rootfs0..9/' \
  | sort -u > "$tmp/code.txt"
[ -s "$tmp/code.txt" ] || {
  echo "fwcfg-check: 代码面零命中——扫描面损坏（排除面过宽或 git 缺件）" >&2
  exit 1
}

# 键表侧：[[keys]] name 字段。
sed -n 's/^name = "\(.*\)"$/\1/p' tools/fwcfg-keys.toml | sort -u > "$tmp/toml.txt"

# 键长上限（键表公约：全名 opt/berry/<name> ≤ 55B——FW_CFG_DIR 项定长，
# 前缀 10B 余量 45B）。超限键 QEMU 侧静默截断/拒收，先在此红。
while IFS= read -r k; do
  len=$(printf 'opt/berry/%s' "$k" | wc -c | tr -d ' ')
  [ "$len" -le 55 ] || {
    echo "fwcfg-check: 键名超长（${k}=${len}B > 55B 上限——FW_CFG_DIR 定长项）" >&2
    exit 1
  }
done < "$tmp/toml.txt"

status=0
if comm -23 "$tmp/code.txt" "$tmp/toml.txt" | grep -q .; then
  echo "fwcfg-check: 键表漏登记（代码在用、键表没有）:" >&2
  comm -23 "$tmp/code.txt" "$tmp/toml.txt" | sed 's/^/  /' >&2
  status=1
fi
if comm -13 "$tmp/code.txt" "$tmp/toml.txt" | grep -q .; then
  echo "fwcfg-check: 键表多余（键表在册、代码无踪——死键须注记手拼专递或删）:" >&2
  comm -13 "$tmp/code.txt" "$tmp/toml.txt" | sed 's/^/  /' >&2
  status=1
fi
[ $status -eq 0 ] && echo "fwcfg-check: 对拍过（$(wc -l < "$tmp/toml.txt" | tr -d ' ') 键两侧一致）"
exit $status
