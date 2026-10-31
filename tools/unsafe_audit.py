#!/usr/bin/env python3
"""unsafe 台账审计（SEC-01 §4.5 件 22 / SEC-8 §5.1-1——「全 Rust 安全叙事」第一张可出示数据）。

扫描 kernel/ mm/ sched/ ipc/ arch/ 的 unsafe 面，产 file:line 逐条登记表 +
密度总览，写入 docs/unsafe台账.md 的生成块（HTML 注释 marker 对包整表——
gen_abi.py 同纪律：marker 夹表体会塌 GFM 表渲染）。

用法:
  python3 tools/unsafe_audit.py            # 扫描 + 重产台账（分类/说明按内容锚从旧表继承）
  python3 tools/unsafe_audit.py --check    # 守卫模式（mkfat --check-fixture 同型）：内存重产
                                          # 与在册台账对拍，漂移 exit 1 + 重产提示
  python3 tools/unsafe_audit.py --json     # 机器可读全量清单（AI 测试友好化）

对拍语义（双向）:
  - 源码有出现点而表缺登记  = FAIL（行漂移/新增 unsafe 未入册）
  - 表有登记而源码无出现点  = FAIL（手改表/删代码未重产）
  密度/分类计数随登记表重算，漂移同 FAIL——密度趋势线 = 本文档的 git 史。

手改契约（登记是源码的真值，判读是人的真值）:
  - 「分类」「说明」两列可手改；重产按内容锚（路径+类型+说明列 ※ 前片段）继承
    旧判读——行漂移/注释改动不丢人工分类。
  - 说明列人工备注加在自动片段之后、以「※」分隔（锚只认 ※ 前部分）。
  - 行号/增删行勿手改——那是 --check 的执法面。

口径（详见 docs/unsafe台账.md §一）:
  - 出现点四类分开计：block / fn（含 unsafe extern "C" fn 定义）/ impl /
    extern（unsafe extern "C" {} 外部块）；类型签名位提及（fn 指针类型）只计数
    不登记——其实际调用点必有 unsafe 块在册，登记会双计。
  - 密度 = unsafe 跨度内非空非注行 / 扫描面 .rs 物理总行（跨度近似——块内安全
    子表达式、unsafe fn 整个函数体都计入，偏高估；对标 Android ~4% 行占比时
    注明口径差）。
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LEDGER = REPO / "docs" / "unsafe台账.md"
SCAN_DIRS = ("kernel", "mm", "sched", "ipc", "arch")
# 测试/基准面整体归「其它」——含测试内嵌的切换/页表探针，非生产路径
TEST_FILES = {"kernel/src/ktest.rs", "kernel/src/bench.rs"}

MARK_SUM = ("<!-- UNSAFE-AUDIT:SUMMARY:BEGIN -->",
            "<!-- UNSAFE-AUDIT:SUMMARY:END -->")
MARK_REG = ("<!-- UNSAFE-AUDIT:REGISTRY:BEGIN -->",
            "<!-- UNSAFE-AUDIT:REGISTRY:END -->")

CATEGORIES = ("页表", "上下文切换", "MMIO", "分配器", "硬件原子", "其它")
HAND_SEP = "※"  # 说明列：自动片段 ※ 人工备注（内容锚只认 ※ 前）

# ---- 分类规则（初判——逐块核对 2026-10-30 全量清单后成文） ----
# 优先级：TEST_FILES 强制「其它」→ FILE_DEFAULTS（同质文件整文件定性，内容
# 规则不越权——mmu.rs 的 tlbi/dsb asm 属页表维护而非设备面）→ CONTENT_RULES
# （只喂无文件定性的混合文件）→ 「其它」。改判优先走台账表列（手改被继承），
# 规则只喂新增。
FILE_DEFAULTS = {
    # 页表：PTE 读写/表树走查/TTBR0-ASID 激活/TLBI 维护（含写后 dsb 屏障）
    "arch/aarch64/src/mmu.rs": "页表",
    "kernel/src/vmar.rs": "页表",
    "kernel/src/aspace.rs": "页表",
    # 上下文切换：切换 asm/任务上下文装配/异常向量与帧保存（trap 向量表归此）
    "arch/aarch64/src/task.rs": "上下文切换",
    "arch/aarch64/src/trap.rs": "上下文切换",
    # MMIO：设备 MMIO + 系统寄存器/特权指令（mrs/msr/wfi/hvc/out/in…——按
    # 规则/01-内核编码.md 域铁律「MMIO/寄存器访问」簇归并，口径差异在台账 §一注明）
    "arch/aarch64/src/mmio.rs": "MMIO",
    "arch/aarch64/src/uart.rs": "MMIO",
    "arch/aarch64/src/gic.rs": "MMIO",
    "arch/aarch64/src/timer.rs": "MMIO",
    "arch/aarch64/src/fwcfg.rs": "MMIO",
    "arch/aarch64/src/smp.rs": "MMIO",
    # 分配器：SlabHeap/GlobalAlloc/freelist/Task Box 分配基元
    "kernel/src/heap.rs": "分配器",
    "sched/src/task.rs": "分配器",
    # 其余文件默认「其它」（裸指针解引用/静态可变槽/对象互钉/ABI 出入参拷贝/
    # 内存基元/内核对象引用计数/测试与遥测面……），混合文件走 CONTENT_RULES
}
# 内容规则（对原始跨度头部匹配——asm! 字符串内容在掩码文本里不可见，故用原文）
CONTENT_RULES = (
    # x86 syscall 快速入口寄存器（SYSCALL/SYSRET 基建）归上下文切换族
    (r"MSR_EFER|MSR_STAR|MSR_LSTAR", "上下文切换"),
    (r"\bIDT\b|\bTSS\b", "上下文切换"),
    (r"activate_if_changed|switch_away|context_switch|ctx_init|CTX_[AB]\b|RR\.pick_next",
     "上下文切换"),
    (r"trap::init", "上下文切换"),
    # svc 探针（内核任务从 EL1 发 syscall 的测试包装）不是寄存器访问
    (r'asm!\(\s*"svc', "其它"),
    # 特权指令/系统寄存器 asm（aarch64 msr/mrs/dsb/isb/tlbi/wfi/hvc/sev/smc；
    # x86 sti/cli/hlt/out/in/rdmsr/wrmsr/crN）
    (r'asm!\(\s*"(msr|mrs|dsb|isb|tlbi|wfi|sev|hvc|smc|sti|cli|hlt|out |in |rdmsr|wrmsr|mov %cr)',
     "MMIO"),
    (r"\boutb\b|\binb\b|\brdmsr\b|\bwrmsr\b", "MMIO"),
    (r"timer::|uart::|gic::|mmio::", "MMIO"),
    (r"mmu::", "页表"),
    (r"\bFRAMES\b", "分配器"),
)


# ---------------------------------------------------------------- 词法掩码
def mask_rust(text: str) -> str:
    """注释与字符串/字符字面量内容替换为空格（保换行与结构字符）——出现点
    定位与括号配平必须在掩码文本上做（asm!("msr …") 的括号在字符串里）。"""
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if c == "/" and i + 1 < n and text[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if text[j] == "/" and j + 1 < n and text[j + 1] == "*":
                    depth += 1
                    j += 2
                    continue
                if text[j] == "*" and j + 1 < n and text[j + 1] == "/":
                    depth -= 1
                    j += 2
                    continue
                j += 1
            for k in range(i, min(j, n)):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if c in "brc":
            j = i
            while j < n and text[j] in "brc":
                j += 1
            pref = text[i:j]
            if j < n and text[j] == '"':
                if "r" in pref:  # raw string r#"…"#
                    h = 0
                    while j + h < n and text[j + h] == "#":
                        h += 1
                    if h == 0:
                        e = text.find('"', j + 1)
                        e = n if e < 0 else e + 1
                    else:
                        e = text.find("#" * h + '"', j + h)
                        e = n if e < 0 else e + h + 1
                    for k in range(i, e):
                        if out[k] != "\n":
                            out[k] = " "
                    i = e
                    continue
                if "c" in pref:  # c"…" C 字符串
                    e = text.find('"', j + 1)
                    e = n if e < 0 else e + 1
                    for k in range(i, e):
                        if out[k] != "\n":
                            out[k] = " "
                    i = e
                    continue
            i = max(j, i + 1)
            continue
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"' or j == n - 1:
                    j += 1
                    break
                j += 1
            for k in range(i, min(j, n)):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if c == "'":
            m = re.match(r"'(\\.|[^'\\])'", text[i:])
            if m:  # 字符字面量（生命周期标注不匹配两字符形态，保留原样）
                for k in range(i, i + m.end()):
                    out[k] = " "
                i += m.end()
                continue
            i += 1
            continue
        i += 1
    return "".join(out)


def find_span(masked: str, start: int, kind: str) -> tuple[int, int] | None:
    """出现点跨度：block = unsafe 后首个 ( 或 {；fn/impl/extern = 跳过签名区
    （参数/返回类型/where）找体开 {。括号在掩码文本上配平。"""
    i = start
    if kind == "block":
        m = re.compile(r"[({]").search(masked, i)
        if not m:
            return None
        opener = m.start()
    else:
        opener = None
        while i < len(masked):
            ch = masked[i]
            if ch == "(":  # 跳过一个平衡括号组（参数列表/返回类型里的括号）
                d, j = 1, i + 1
                while j < len(masked) and d:
                    if masked[j] == "(":
                        d += 1
                    elif masked[j] == ")":
                        d -= 1
                    j += 1
                i = j
                continue
            if ch == "{":
                opener = i
                break
            i += 1
        if opener is None:
            return None
    stack = [masked[opener]]
    j = opener + 1
    while j < len(masked) and stack:
        c = masked[j]
        if c in "({[":
            stack.append(c)
        elif c in ")}]":
            if stack and {"}": "{", ")": "(", "]": "["}[c] == stack[-1]:
                stack.pop()
                if not stack:
                    return opener, j
        j += 1
    return opener, len(masked) - 1


def scan_file(path: Path, rel: str):
    """返回 (出现点列表, 签名位提及数, 文件总行数, 非空跨度行集合)。"""
    raw = path.read_text()
    masked = mask_rust(raw)
    masked_lines = masked.split("\n")
    nlines = len(masked_lines)

    def line_of(pos: int) -> int:
        return raw.count("\n", 0, pos) + 1

    occ, sig_count, unknown, line_set = [], 0, 0, set()
    for m in re.finditer(r"\bunsafe\b", masked):
        after = masked[m.end():]
        t = re.match(r"\s*(impl\b|fn\b|extern\b|[({])", after)
        if t is None:
            unknown += 1
            continue
        kw = t.group(1)
        rest = after[t.end():]
        if kw == "impl":
            kind = "impl"
        elif kw == "fn":
            kind = "fn" if re.match(r"\s*[A-Za-z_]", rest) else "sig"
        elif kw == "extern":
            # 掩码后 ABI 串成空白：extern 之后是「（空白化的 "C"）fn 名(」（fn
            # 定义/签名位提及）或「{」（unsafe extern 块）
            nxt = re.match(r"\s*[A-Za-z_]*\s*fn\b(.{0,40})", rest, re.S)
            if nxt is not None:
                kind = "fn" if re.match(r"\s*[A-Za-z_]", nxt.group(1)) else "sig"
            elif re.match(r"\s*\{", rest):
                kind = "extern"
            else:
                # 分型失败：宁可 loud 也不静默漏计
                print(f"unsafe_audit: ERROR {rel}:{line_of(m.start())}: unsafe extern "
                      f"后既非 fn 也非 {{——请修 tools/unsafe_audit.py 分型",
                      file=sys.stderr)
                raise SystemExit(2)
        else:
            kind = "block"
        if kind == "sig":
            sig_count += 1
            continue
        span = find_span(masked, m.end(), kind)
        head = re.sub(r"\s+", " ", raw[m.start():m.start() + 160]).strip()
        occ.append({"path": rel, "line": line_of(m.start()), "kind": kind,
                    "head": head})
        if span:
            s, e = span
            for ln in range(line_of(s), line_of(e) + 1):
                if ln <= nlines and masked_lines[ln - 1].strip():
                    line_set.add(ln)
    if unknown:
        print(f"unsafe_audit: WARN {rel}: {unknown} 处无法分型的 unsafe"
              f"（trait 声明位等）——请核对扫描器覆盖面", file=sys.stderr)
    return occ, sig_count, nlines, line_set


def classify(rel: str, head: str) -> str:
    if rel in TEST_FILES:
        return "其它"
    if rel in FILE_DEFAULTS:  # 同质文件整文件定性——内容规则不越权
        return FILE_DEFAULTS[rel]
    for pat, cat in CONTENT_RULES:
        if re.search(pat, head):
            return cat
    return "其它"


def esc(s: str) -> str:
    return s.replace("|", "\\|")


# ---------------------------------------------------------------- 台账模板
TEMPLATE = """# unsafe 台账（SEC-01 §4.5 件 22）

> 内核侧 unsafe 面单一事实源：`kernel/ mm/ sched/ ipc/ arch/` 的每个 unsafe
> 出现点逐条登记（file:line + 类型 + 用途分类），密度对标 Android ~4% 口径。
> 产物由 `tools/unsafe_audit.py` 生成（两个 marker 块内勿手改行号/增删行）；
> 改代码后重跑 `python3 tools/unsafe_audit.py` 再入册（abi-gen 同纪律），CI 以
> `--check` 执法（漂移 = FAIL）。密度趋势线 = 本文档的 git 史（每次重产的
> 密度块 diff 即趋势采样点）。

## 一、口径

- **扫描面**：`kernel/ mm/ sched/ ipc/ arch/` 全部 `.rs`（内核侧 crate）。
  `user/ drivers/ bootloader/` 暂不入表（挂后续扩面——扩面时改工具 SCAN_DIRS）。
- **出现点四类分开计**：`block`（unsafe 块）/ `fn`（unsafe fn，含
  `unsafe extern "C" fn` 定义）/ `impl`（unsafe impl）/ `extern`
  （`unsafe extern "C" {}` 外部块）。类型签名位提及（形如
  `Option<unsafe extern "C" fn(..)>` 的 fn 指针类型）**只计数不登记**——其实际
  调用点必有 unsafe 块在册，登记会双计。
- **六分类**（初判逐块核对于 2026-10-30 全量清单；改判直接改 §三 表的分类列，
  重产按内容锚继承）：
  - **页表**：PTE 读写/表树走查/TTBR0-ASID 激活/TLBI 维护（含写后 dsb 屏障）。
  - **上下文切换**：切换 asm/任务上下文装配/异常向量与帧保存（规则/01
    「上下文切换/寄存器保存」簇；trap 向量表与 x86 IDT/TSS/syscall 入口 MSR 归此）。
  - **MMIO**：设备 MMIO + **系统寄存器/特权指令**（mrs/msr/wfi/hvc/out/in/
    rdmsr…——按 规则/01「MMIO/寄存器访问」簇归并；纯设备 MMIO 与系统寄存器
    未拆两列，口径如实注记）。
  - **分配器**：SlabHeap/GlobalAlloc/freelist/帧分配器静态/Task Box 分配基元。
  - **硬件原子**：手写原子 asm（ldxr/stxr/casa/outline-atomics 面）——**现状
    0 处**（原子操作全走 `core::sync::atomic` 安全面）；类别保留占位。
  - **其它**：裸指针解引用/静态可变槽/对象互钉/ABI 出入参拷贝/内存基元/
    测试与基准面/遥测环静态——规则/01「外层包安全 API」的包裹层主体。
- **密度口径**：unsafe 跨度内**非空非注行** / 扫描面 `.rs` 物理总行。跨度近似
  （unsafe 块内安全子表达式、unsafe fn 整个函数体都计入）→ **偏高估**。
  对标 Android ~4%（Google 2025-11：unsafe 行占 Rust 总行比）时以此口径差为准
  ——两口径同向（行占比）但分母分子定义不同，趋势可比、绝对值慎比。
- **测试面剔除口径**：另报「剔除 ktest.rs/bench.rs」一行（生产面密度）。
- **说明列手改契约**：自动片段之后以「※」分隔可加人工备注；内容锚只认 ※ 前
  部分（行漂移不丢人工判读）。

## 二、密度总览（生成）

<!-- UNSAFE-AUDIT:SUMMARY:BEGIN -->
<!-- UNSAFE-AUDIT:SUMMARY:END -->

## 三、登记表（生成，逐出现点一行）

<!-- UNSAFE-AUDIT:REGISTRY:BEGIN -->
<!-- UNSAFE-AUDIT:REGISTRY:END -->

## 四、漏洞分类账维度（SEC-8 §5.1-2——没有维度就没有未来的曲线）

对拍物 = Android 2019 年先建类别化基线、2022 年画出 76%→35% 曲线的方法论
（`research/sec-notes/SEC-8-Rust与内存安全现状.md` §3.1/§5.1）。**从现在起**
issue/回归/审查 finding 一律带下列标签之一（可多标），哪怕长期记 0——维度
day-one 立住，「全 Rust」的安全叙事才有可出示的曲线起点。

| 标签 | 口径（什么进什么不进） | 现状计数 | 备注 |
|------|----------------------|---------|------|
| memory-safety | 走出 unsafe 契约的内存错（UAF/越界/悬垂/数据竞争/未初始化读）——含 C-ABI 桩面布局错（errno 档位/出参宽度/Stat 布局三案若再现即记此） | 0 | 三案均已在当刀闭环修复（见 §五），未流出为漏洞 |
| logic | 语义/时序错（竞速标记、饿死、协议状态机错序、off-by-one） | 0 | 台账只记「已确认的缺陷」；对抗审查抓出即修的记当刀复盘，不重复入账 |
| 协议 | 跨进程/跨端点 wire 面错（编解码不对称、畸形拒面缺口、会话错配） | 0 | 协议 crate host 测试族（psproto/netdproto 等）是第一防线 |
| 供应链 | 依赖投毒/typosquat/弃维护/advisory 命中 | 0 | 防线 = cargo-deny CI（件 23）+ Cargo.lock 全量入库 + uu 家族 =0.12.0 锁步 |
| 工具链 | 构建链/产物链错（elf2pe 槽位案、基准漂移案类） | 0 | 防线 = 双实现共识 fixture 守卫（mkfat --check-fixture 家族） |

> 计数更新纪律：确认缺陷当刀修完 → 该标签计数不加（修在合并前）；流出后才
> 入账并附 issue/复盘链接。本表手维护，不进生成块。

## 五、C-ABI 桩面同源审计（件 23 后半——unsafe 同级 review 面）

本表扫描面是内核侧 crate；BerryOS 的隐性 C 边界还有一块在 **berry-libc 桩与
`#[repr(C)]` 结构**（user 侧），列入 unsafe 同级审计口径：

- **三案实抓存档**（均已闭环，作为该面的「已知错误形态库」）：
  ① errno 桩档位案——`xattr` 族桩给 ENOSYS(38) 而 uucore 白名单只吞 95
  （桩的「正确 errno」由调用方错误分类代码定义）；
  ② 出参宽度案——handle 出参一律 `copy_out_u32`，曾 `copy_out_u64` 砸穿
  调用方 4 字节槽邻接局部（ABI §8 handle=u32）；
  ③ Stat 布局案——musl aarch64 asm-generic 字段序（st_mode@16）与 x86_64
  版同尺寸不同序，size 类消费者恒绿、mode 类首个消费者才显形；修法 = 布局
  定义处 `offset_of!` 全字段编译期钉板。
- **既有防线**：`Stat` 20 字段 offset_of! 钉板（fs.rs const assert 族）；
  get*_r 四桩签名对齐 libc crate 真原型；`tools/shim_audit.py` 符号面审计。
- **挂账**：桩函数签名对照表全量化（桩原型 vs libc crate 原型逐条对拍）——
  随 M-ECO A3 socket 桩批扩面时一并落。

## 六、维护流程

1. 改内核侧代码涉 unsafe 增删/移行 → `python3 tools/unsafe_audit.py` 重产
   → 台账 diff 随源入册（新出现点由工具规则给初判分类，人工复核 §三 表列）。
2. 改判分类/说明：直接编辑 §三 表格对应列（行号/增删行勿手改），下次重产按
   内容锚自动继承。
3. CI（ci.yml「unsafe 台账对拍」step）跑 `--check`：红 = 忘重产或手改越界；
   通过行顺打当前密度锚（趋势线在 CI run 史 + 本文档 git 史双面可读）。
4. 提交定序（多会话并行纪律）：台账基线 = 随行源码树（生成时的工作树快照），
   不是 HEAD——台账必须与其快照的内核源码同批（或其后）提交；先于源码单独
   入册（如快照含兄弟会话未提交 WIP）= CI `--check` 对提交树必红（表有登记
   而源码缺）。
5. 密度趋势：看本文档 `git log -p` 的 §二 diff。
"""


def split_row(line: str) -> list[str] | None:
    if not line.startswith("|"):
        return None
    parts = [p.strip() for p in re.split(r"(?<!\\)\|", line.strip().strip("|"))]
    return parts if len(parts) == 5 else None


def parse_ledger(text: str) -> dict[tuple[str, int, str], dict]:
    m = re.search(re.escape(MARK_REG[0]) + r"\n(.*?)" + re.escape(MARK_REG[1]),
                  text, re.S)
    if not m:
        return {}
    rows = {}
    for line in m.group(1).split("\n"):
        parts = split_row(line)
        if parts is None or parts[0] == "文件" or set(line) <= {"|", "-", " ", ":"}:
            continue
        try:
            ln = int(parts[1])
        except ValueError:
            continue
        rows[(parts[0], ln, parts[2])] = {
            "path": parts[0], "line": ln, "kind": parts[2],
            "cat": parts[3].replace("\\|", "|"),
            "note": parts[4].replace("\\|", "|"),
        }
    return rows


def anchor_base(path: str, kind: str, snippet: str) -> str:
    alnum = re.sub(r"[^A-Za-z0-9]", "", snippet)[:48]
    return f"{path}:{kind}:{alnum}"


def build_rows(occ: list[dict], old_rows: dict) -> list[dict]:
    """新出现点继承旧判读：内容锚 = 路径+类型+说明列 ※ 前片段（去非字母数字、
    截 48 字符）+ 同锚出现序号。行漂移/注释改动不丢人工分类。"""
    old_by_anchor: dict[str, dict] = {}
    seen: dict[str, int] = {}
    for r in sorted(old_rows.values(), key=lambda r: r["line"]):
        base = anchor_base(r["path"], r["kind"], r["note"].split(HAND_SEP)[0])
        seen[base] = seen.get(base, 0) + 1
        old_by_anchor[f"{base}#{seen[base]}"] = r
    seen2: dict[str, int] = {}
    out = []
    for o in occ:  # occ 已按 (path, line) 排序——同锚序号与旧表行序同向
        snippet = o["head"][:64].strip()  # 截断不得停在空白上——否则写/读往返不稳
        base = anchor_base(o["path"], o["kind"], snippet)
        seen2[base] = seen2.get(base, 0) + 1
        prev = old_by_anchor.get(f"{base}#{seen2[base]}")
        out.append({
            **o,
            "cat": prev["cat"] if prev else classify(o["path"], o["head"]),
            # note 内部形态恒为未转义原文（parse 侧已还原）；转义只在 render 出笔
            "note": prev["note"] if prev else snippet,
        })
    return out


def render(files_lines: dict[str, int], rows: list[dict], sig_total: int,
           dir_stats: dict, prod_pairs: int) -> tuple[str, str]:
    sum_lines = ["| 目录 | .rs 文件数 | 总行数 | unsafe 非空行 | 密度 |",
                 "|------|-----------|--------|--------------|------|"]
    tot_f = tot_l = tot_u = 0
    for d in SCAN_DIRS:
        f, l, u = dir_stats.get(d, (0, 0, set()))
        u = len(u)
        tot_f += f
        tot_l += l
        tot_u += u
        pct = f"{u / l * 100:.2f}%" if l else "—"
        sum_lines.append(f"| {d}/ | {f} | {l} | {u} | {pct} |")
    sum_lines.append(f"| **合计** | {tot_f} | {tot_l} | {tot_u} | "
                     f"**{tot_u / tot_l * 100:.2f}%** |")
    prod_l = tot_l - sum(files_lines.get(p, 0) for p in TEST_FILES)
    sum_lines.append("")
    sum_lines.append(f"- 剔除 ktest.rs/bench.rs（测试/基准面）：总行 {prod_l}，"
                     f"unsafe 非空行 {prod_pairs}，密度 "
                     f"{prod_pairs / prod_l * 100:.2f}%（生产面口径）")
    cats = {c: 0 for c in CATEGORIES}
    kinds = {"block": 0, "fn": 0, "impl": 0, "extern": 0}
    for r in rows:
        cats[r["cat"]] = cats.get(r["cat"], 0) + 1
        kinds[r["kind"]] += 1
    sum_lines.append("- 分类计数：" + " | ".join(f"{c} {cats.get(c, 0)}" for c in CATEGORIES))
    sum_lines.append("- 类型计数：" + " | ".join(f"{k} {v}" for k, v in kinds.items())
                     + f"（另有类型签名位提及 {sig_total} 处——只计不登记，防双计）")
    sum_lines.append("- 对标：Android ~4%（unsafe 行占 Rust 总行比，Google 2025-11）；"
                     "本表为跨度非空行口径（偏高估），见 §一")
    summary = "\n".join(sum_lines)

    reg_lines = ["| 文件 | 行 | 类型 | 分类 | 说明 |",
                 "|------|----|------|------|------|"]
    for r in rows:
        reg_lines.append(f'| {r["path"]} | {r["line"]} | {r["kind"]} | '
                         f'{r["cat"]} | {esc(r["note"])} |')  # 出笔才转义管道
    return summary, "\n".join(reg_lines)


def collect():
    """返回 (出现点列表, 签名位提及总数, 文件行数表, 目录统计, 生产面分子)。
    生产面分子 = 剔测试文件后的 (文件, 非空跨度行) 对数——文件级行集天然去重
    （同文件多个出现点的重叠行只计一次）。"""
    occ_all: list[dict] = []
    files_lines: dict[str, int] = {}
    dir_stats: dict[str, tuple[int, int, set]] = {}
    all_pairs: set = set()
    prod_pairs: set = set()
    sig_total = 0
    for d in SCAN_DIRS:
        root = REPO / d
        files = sorted(root.rglob("*.rs")) if root.is_dir() else []
        dir_lines = 0
        dir_unsafe: set = set()
        for f in files:
            rel = str(f.relative_to(REPO))
            occ, sig, nlines, line_set = scan_file(f, rel)
            sig_total += sig
            dir_lines += nlines
            files_lines[rel] = nlines
            occ_all.extend(occ)
            pairs = {(rel, ln) for ln in line_set}
            dir_unsafe |= pairs
            all_pairs |= pairs
            if rel not in TEST_FILES:
                prod_pairs |= pairs
        dir_stats[d] = (len(files), dir_lines, dir_unsafe)
    occ_all.sort(key=lambda o: (o["path"], o["line"]))
    return occ_all, sig_total, files_lines, dir_stats, all_pairs, prod_pairs


def regenerate(doc_text: str | None):
    occ_all, sig_total, files_lines, dir_stats, all_pairs, prod_pairs = collect()
    old_rows = parse_ledger(doc_text) if doc_text else {}
    rows = build_rows(occ_all, old_rows)
    summary, registry = render(files_lines, rows, sig_total, dir_stats,
                               len(prod_pairs))
    tot_l = sum(files_lines.values())
    prod_l = tot_l - sum(files_lines.get(p, 0) for p in TEST_FILES)
    return {MARK_SUM: summary, MARK_REG: registry}, {
        "occ": occ_all, "rows": rows, "sig": sig_total,
        "tot_lines": tot_l, "tot_unsafe": len(all_pairs),
        "prod_lines": prod_l, "prod_unsafe": len(prod_pairs),
    }


def splice(text: str, marks: tuple[str, str], content: str) -> str:
    pat = re.compile(re.escape(marks[0]) + r"\n.*?" + re.escape(marks[1]), re.S)
    repl = marks[0] + "\n" + content + "\n" + marks[1]
    return pat.sub(lambda _: repl, text, count=1) if pat.search(text) else text


def main() -> int:
    args = sys.argv[1:]
    check, as_json = "--check" in args, "--json" in args
    doc_text = LEDGER.read_text() if LEDGER.exists() else None
    parts, info = regenerate(doc_text)
    if as_json:
        print(json.dumps({
            "occurrences": [{k: o[k] for k in ("path", "line", "kind", "cat", "head")}
                            for o in info["rows"]],
            "sig_mentions": info["sig"],
        }, ensure_ascii=False, indent=1))
        return 0
    if check:
        cur = doc_text or ""
        ok = True
        for marks in (MARK_SUM, MARK_REG):
            m = re.search(re.escape(marks[0]) + r"\n(.*?)\n?" + re.escape(marks[1]),
                          cur, re.S)
            have = m.group(1).strip() if m else "<缺 marker 块>"
            want = parts[marks].strip()
            if have != want:
                ok = False
                name = "密度总览" if marks is MARK_SUM else "登记表"
                print(f"FAIL: unsafe 台账「{name}」与源码漂移（源码有表缺/表有源码缺/"
                      f"行漂移——mkfat --check-fixture 同型）。修法：")
                print("  python3 tools/unsafe_audit.py   # 重产后 diff 随源入册")
                hl, wl = have.split("\n"), want.split("\n")
                for i in range(max(len(hl), len(wl))):
                    h = hl[i] if i < len(hl) else "<无>"
                    w = wl[i] if i < len(wl) else "<无>"
                    if h != w:
                        print(f"  首差异 @{i + 1}: 在册={h[:110]}")
                        print(f"          重产={w[:110]}")
                        break
        if ok:
            print(f"unsafe 台账对拍 ok：{len(info['rows'])} 条在册"
                  f"（block/fn/impl/extern 逐条吻合；签名位提及 {info['sig']} 处只计不登记）")
            # 密度趋势锚（对抗审查 R1 补）：--check 通过时顺打当前密度一行——
            # CI 日志每跑留锚即成机器可读趋势线，零新检查面（漂移执法仍归 --check）。
            d_all = info["tot_unsafe"] / info["tot_lines"] * 100 if info["tot_lines"] else 0.0
            d_prod = (info["prod_unsafe"] / info["prod_lines"] * 100
                      if info["prod_lines"] else 0.0)
            print(f"unsafe 密度 {d_all:.2f}%（全量）/ {d_prod:.2f}%"
                  f"（除 ktest·bench 生产面），对标 Android ~4%（口径差见台账 §一）")
        return 0 if ok else 1
    text = doc_text if doc_text and re.search(re.escape(MARK_SUM[0]), doc_text) else TEMPLATE
    for marks in (MARK_SUM, MARK_REG):
        text = splice(text, marks, parts[marks])
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    LEDGER.write_text(text)
    cats = {}
    for r in info["rows"]:
        cats[r["cat"]] = cats.get(r["cat"], 0) + 1
    print(f"unsafe 台账已重产：{len(info['rows'])} 条登记 → {LEDGER.relative_to(REPO)}")
    print("分类计数：" + " | ".join(f"{c} {cats.get(c, 0)}" for c in CATEGORIES))
    return 0


if __name__ == "__main__":
    sys.exit(main())
