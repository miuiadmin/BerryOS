# Agent Note: G16 双源漂移收口——gensourceindex --preset 单源派生（app 条目直接出自 product.toml）

Status: implemented

## Problem

OEM 走官方源要自持「源索引条目表」（38 号演练的 acme-apps.toml），与 rootfs 条目表、product.toml preset 三处列举同一 app 集合，手工保持一致（38 号 §4 G16：演练实咬 = 同一 5-app 集合写了三遍；entries 生成脚本后来收敛到以 preset 为源导出，但 gensourceindex 侧仍独立一份输入面）——三写漂移正是定制碎片化的种子形态。

## Decision

`gensourceindex.py --preset <product.toml>`：源索引 app 条目**直接派生自产品清单**——复用 `berry_assemble.load_product`（结构 + 产物面全量执法）⊕ `resolve_entries`（preset 筛选、override 路径替换、构建树存在性、launcher 终表对账），本工具只做 `app/*.bap` 前缀过滤 + 路径归一，零第二产品解析器；两工具函数级惰性互导（berry_assemble 深校验 source 节时 import 本文件），无模块环。派生集 ≡ 产品终表 app 集（preset ⊕ override），「源索引条目表」自持面就此消解；拒面 = berry_assemble Reject 全谱转 die 文案原样透传（产品清单坏 / 条目不在基座 / 构建树缺席等）。

## Alternatives considered

- **`--entries-from-rootfs`（条目派生自 rootfs-manifest app 条目）**：被拒——rootfs 表是平台基座全集（9 app），产品裁剪面在 preset；派生自它要么索引全量 app（语义错）要么仍需第二处裁剪清单，双源只消一写。
- **`--cross-check`（product 清单与既有条目表两表对拍、漂移即拒）**：被拒——检测不消解：OEM 仍维护两份数据，对拍腿本身要养，且新漂移只在跑门时炸而非结构上不可能。
- **维持现状（条目表自持）**：38 号已判 P2 缺口在册，非选项。

## Consequences

- selftest ⑫ 四腿：override 绝对路径替换（基座未选 app 不进）/ 双 preset 相对路径归一 / Reject 拒面透传 / 空 preset loud 拒；CLI 实跑（drill3 产品，5 preset）派生 5/5 条目 + `--verify` 过——同一 app 集合从此只写 product.toml 一处。
- 无 `--preset` 时 `load_app_entries`（rootfs-manifest 路）行为零漂移（向后兼容）；`--rootfs-manifest` 在 `--preset` 在席时改任基座表供路径解析。
- G16 终账行翻闭归复核轮报告（41/42 号 §4 表续），本笔记只记取舍。

## Related

- 实施面：`tools/gensourceindex.py`（load_preset_entries + do_generate 分叉 + selftest ⑫）；对向复用 `tools/berry_assemble.py`。
- 基线：`research/厂商定制化深研/38-OEM演练实弹报告.md` §4 G16 行（修法建议原文即「gensourceindex 支持 --preset 从 product.toml 读」）；`42-实施轮十六复核.md` §4 G16 未闭行。
