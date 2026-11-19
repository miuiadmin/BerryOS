# mkapp Rust 化二刀——manifest 门禁面 Rust 侧首建

Status: implemented

## Problem

彻底 Rust 化轨道 b 层 #4（mkapp，py ~2620 行，L 级最大件）刀序第四位。首刀（#522）已落 canonical 规范形重发射面；二刀要把 Python 现役 `check_manifest_text`（门禁第 1+2 道：结构/值域 + 三值链 + 规范形）在 Rust 侧首建，且不能引新依赖、不能动产线（五刀才换轨）。

## Decision

- **零依赖 TOML 子集解析器，不引 toml crate**：canonical() 全集本就是受限方言——基础串限可打印 ASCII 0x20..=0x7E 无转义、单元素数组、十进制 int 拒前导零/hex/下划线、无注释。解析器只需覆盖规范形，**非规范形在解析层即拒**，拒因直给行号与词法类目。与 Python 侧「tomllib 宽收 → canonical 比较兜底拒」分层不同、终态同拒，但报错面更锐（Python 兜底拒只有 mismatch 文案，Rust 给 `manifest.syntax.toml` + 行号）。
- **拒因三元组 `Reject{message, code, where, line}`** 与 Python `--json error.line` 错误域对齐（SDK-2 错误域指针）：64 拒面全谱与 Python `mkapp.py` 逐面对拍。
- **真分歧判例（已裁决收口，#526 REJECT 修复随刀）**：`#` 处置——原 Rust 解析层拒一切含 `#` 的行（含串内 `#`），Python tomllib 收串内 `#`。reviewer 对拍实证分叉后按「解析器给串内 `#` 豁免」路径裁决落地：`outside_hash` 双引号开关状态机定位串外 `#`（canonical 值串无转义故无 escaped-quote 面），串内 `#` = 值字符收、串外 `#`（行尾/整行注释）仍解析层拒钉 `manifest.syntax.toml`。黄金向量 13→14 accept 形 + `hash_in_string_exempt_outside_rejected` 钉板测试锁死；双侧 Python/Rust 对拍一致（ui.label "C# app"、blob.usage 含 # 收；行尾真注释拒）。
- **黄金向量双侧同步义务**：Rust 侧 13 accept 形由 Python `M.canonical()` 实产。Python canonical 改动必须同步重产向量，否则对拍假绿。
- **测试入口纪律**：mkapp Rust 件 host 测试必须显式 `--target host`——根 `.cargo/config.toml` 锁 `aarch64-unknown-none`，不带 target 报 384 错。

## Alternatives considered

引 toml crate（拒——禁新外部依赖，且其宽容解析面反成拒因噪声源）；复用 berry-manifest（拒——no_std 客机面 crate，host 门禁不应拖客机依赖面，分层不符）；等五刀产链换轨一次落全件（拒——2620 行 L 级件不分刀则风险集中，钉板五步纪律就是防这个）。
