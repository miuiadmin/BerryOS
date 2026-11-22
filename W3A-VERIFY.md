# W3A-VERIFY —— 波3a 三路交付验收报告

Date: 2026-11-15
Owner: 波3a 验收者（对 SA/SB/SC 三路报告收口）
被验对象: `wip/ui-wave3` @ 验收基点 5302a6c64 + 验收中并行文档三笔（8c885820b/8820895a3/d541b5e4e，纯 .agents 文档面，零代码交叠）+ 验收修复三笔（见 §⑤/§⑥）

## ① 三路文件面 diff 复核

三路提交（全 9 笔，显式路径、均实测在案）：

| 路 | 提交 | 文件面 |
|---|---|---|
| SA 件⑦ appshell | f016aa99a7315 / 510f6ce95120 / a0642073d232 / 125a61ede1e8 | user/berry-appshell/**（新 crate 8 文件）+ launcher 消费四文件（desktop/frameloop/layout_io/main）+ icons 三文件 git mv（R100 两枚+R059 一枚）+ user/Cargo.{toml,lock} + launcher Cargo.{toml,lock} + justfile + tools/genoh*.py + W3A-IMPL-SA.md + 笔记 |
| SB 件③ wmd 面 | 6ef6e7b462a9 / b2b865a3737 / 1c1f1331b64 | user/berry-guiproto/src/lib.rs + user/bins/gui-wmd/{Cargo.toml,src/lib.rs(新),src/main.rs,src/table.rs} + justfile + 笔记 |
| SC 件⑤c TabBar | 5211d5df3e1 / 5302a6c644a1 | user/bins/berry-launcher/src/{desktop,frameloop,main}.rs + settings_io.rs(新) + syspages.rs(新) + W3A-IMPL-SC.md + 笔记 |

**交叠实况（如实记录，「严格文件面不相交」不成立）**：SA∩SC = launcher 三文件（desktop/frameloop/main.rs——SC 建立在 SA 抽取后的底座上，时序 SA 14:16 先/SC 14:46 后）；SA∩SB = justfile（test-host 追加行不同行：829 berry-appshell 行 vs 836 gui-wmd 行，互不覆写，两行今均在树）；SB∩SC = ∅。
**语义面不相交成立**：SB 承诺「零 launcher 文件触碰」实符（三笔面=guiproto/gui-wmd/justfile）；SC 承诺「全在 berry-launcher」实符（恰 5 文件）；SA 承诺「禁碰 guifbd/gui-wmd」实符；HEAD 双面共存实证（launcher 内 appshell 消费标记〔SHELL_HOST/bind/appshell::pump 等〕与 SC 标记〔syspages/settings_io/switch_sys_page/TABS_H_COMPACT 等〕grep 同在，无相互覆写）。夹缝单笔 6d80a0fd2（a11y 退位修）只动 appshell/scene.rs，属 SA 域前批修。
SB 报告称「改动清单在 W3A-IMPL-SB.md，三笔显式路径提交在案」——三笔在案属实，但**该清单文件本身未随批入库**（注二清树事故重写后遗落），验收代落账（786796321）。

## ①b 3b 暂缓面零实现 grep

- `softimed|softime`（user/+kernel/+drivers/ 全部 .rs）：仅注释提及（syspages.rs:13「Search 诚实降级（3b 暂缓面）」+ guiproto lib.rs:114 层注释），**零代码实现**。
- `IMMERSIVE|ime_wanted`（同域，大小写不敏感）：**全仓零命中**（连常量都未占——SB「本刀不占 guiproto 位」承诺实符）。
- 「合成」命中均为无关既有面（berry-tls/crypto/fatfs 测试合成数据、berry-gfx pass 文档、pump.rs「合成注入」= 件⑥a 已落的 OP_INPUT_INJECT 输入注入注释）；「沉浸」仅 pattern.rs 底色注释。
- SYSTEM/SERVICE 层：WM_LAYER_SYSTEM 仅策略面（gui-wmd hit-test/挖坑豁免，波2 既有）；WM_LAYER_SERVICE=2 常量+建窗收宽 = SB 本批协议/窗表面；OP_WM_SUBSCRIBE 仅协议侧（guiproto 编解码）+服务侧（gui-wmd），**客户端消费零实现**（guifbd 订阅未接，SB 移交面挂账如实）。
结论：3b 暂缓面（软键盘 UI/沉浸旗/IME 旗/SYSTEM 层视觉提交）**零实现，仅协议常量与注释**，实符。

## ② user 工作区全量 cargo test（host = aarch64-apple-darwin）

| 面 | 结果 |
|---|---|
| berry-ui（全量 lib+集成） | **243 passed / 0 failed**（修存量红后，见 §⑤；修前 241/2 红） |
| berry-appshell | **27 / 0**（=SA 报告数） |
| berry-guiproto | **52 / 0**（=SB 报告数） |
| gui-wmd lib | **14 / 0** 零警告（=SB 报告数） |
| gui-inputd（邻面） | **25 / 0**（=SB 邻面数） |
| tools/gui8-smoke.py --self-test | **SELFTEST PASS**（FAT12/canonical 109B/计数序/常态隔离/几何/QMP，零 QEMU） |
| launcher musl（berry-ld 链） | release 构建绿（798104B）+ `cargo test --no-run` 绿（宿主无 qemu-user 不可执行——既有注记不变）；appshell 3 告警 = SA 既有 |
| berry-appshell none 构建 / gui-wmd musl release | 绿 / 绿 339648B（=SB 报告数） |

注：host cargo 走全局共享 `~/.cargo-target`（sccache 配置）；关键红/绿均以隔离 `CARGO_TARGET_DIR` 复跑核实（排除共享投毒形态，见 §⑤/§③）。

## ③ gui8-smoke（真跑两 attempt，如实记录）

- **attempt 1（默认共享 target dir）：构建即死，非代码红**——`cargo build`（内核）复用共享缓存里他会话的 build-script 产物，链接脚本被烤成 `/private/tmp/pre-appshell-base/kernel/../tools/linker-aarch64.ld`（该 worktree 已被兄弟会话删除）→ rust-lld 找不到脚本 exit 101。铁证：`~/.cargo-target/aarch64-unknown-none/debug/build/berry-kernel-{5ec103bca78c2303,cdd4c76f0d75f1b3}/output` 首行即该绝对路径。**e58c94ccf 记载的「共享 target dir 投毒」形态原样复发**。
- **attempt 2（隔离 CARGO_TARGET_DIR=/tmp/w3a-smoke-target + GUI8_REBUILD=1 全链重编）：rig 正常运行，boot A 420s 未集齐标记 = 红**。串口实况（artifacts 保留 `/tmp/gui8-smoke.emclweul/boot-a.log`）：`BERRYBOOT ok`✓ → `ROOTFS ok src=handoff bytes=38039552`（新 rootfs 含 3a 全量代码+验收修复）✓ → `APP ok n=9`✓ → `LAUNCH v=1`✓ → `LAYOUTW ok`✓ → `LAYOUT ok n=2`✓ → **`memory allocation of 87432 bytes failed` + abort**（两实例同点死于 `UiMain::new` 堆分配）→ `LAUNCH ok`/`PAGE ok n=2` 永缺 → 超时。
- **定谳：基线既有红，非波3a 回归**——失败签名与 SA 验收补记（125a61ede）逐字同形（同为 LAYOUTW/LAYOUT 过链后 87432B 堆 abort；SA 另证净基点 b1646469d 独立 worktree 同跑同红且死得更早、SC 证严格超集推进）；根因 = wave4 七真应用图标常驻（appshell::icons `PX_CAP=225_000`px ≈ 879KB，纯抽取逐字携带的既有政策）压垮 launcher 1MiB 静态堆悬崖——**修复归 launcher/wave4 lane**（e58c94ccf 即同悬崖上一轮修复先例），3a 零行为变化不越权动它。本跑在机实证的 `LAYOUTW ok`/`LAYOUT ok n=2` 恰是 3a 的 conf/icons 链（appshell 抽取面）真机过链证据；a11y NodeFull 未再触发（6d80a0fd2 修有效）。
- **环境证据（任务书预期形态）**：验收全程 3-4 个兄弟 lane QEMU TCG 常驻（berry-live×2 起于 14:27/14:40 + berry-cli-cut@主树 14:52 等，CPU 74-98% 饱和）+ attempt 1 投毒碰撞。按任务书与 e58c94ccf 先例以 **host 断言 + 构建绿代位**（§② 全表）+ 本节在机部分标记为证；smoke 全绿补跑挂堆悬崖修复侧。

## ④ 设置页 BOOT.JSN 真改验证（host 侧模拟 FAT 读写形态）

gui8 同盘重启不可用（§③ 红），按任务书备选走 host 侧：对 disk.img 副本用 `tools/gui8-smoke.py Fat12Volume`（gui8 rig 同源 FAT12 读写器）实证（脚本 `/tmp/w3a-bootjsn-proof.py`，**BOOTJSN-PROOF PASS**）：

1. BOOT.JSN 缺席 → `add_file` 落 verbose canonical `{"boot":"verbose"}` → 重开卷回读**逐字节相等**；
2. 同簇原位 toggle：verbose→splash→verbose 两往，回读逐字节等于 canonical（dirent size/簇内容同改 = 设置页真改同形）；
3. SETTINGS.JSN canonical `{"anim":1,"theme":0,"volume":500}` 落盘回读逐字节相等；
4. 冷重开卷（= 同盘重启 host 等价形）BOOT.JSN 存活，根目录五条目共存不扰。

**连带发现并修正 SC 初稿差一**：canonical 实长 splash **17B**/verbose **18B**（SC 文档与 settings_io 头注写 16B/17B）；且 `boot_canonical_bytes` 测试两条长度断言（17/16）与同函数字节字面（18B/17B）自相矛盾——该测试 musl-only、host 不可执行仅编译验证，**两隐性红首跑必炸**（本机无 qemu-user 实证不能，以字节字面单源+python 对拍定谳）。修在 6b30aac79。

## ⑤ 红则修（记录）

1. **berry-ui widgets4 两测生而红**（波2批C 域，非 3a 回归——三路零碰 berry-ui，`git diff b1646469d..验收基点 -- user/berry-ui/` 为空）：隔离 target 复跑同红（排除投毒）+ b9788e248 出生点独立 worktree 同红（生而即红）；根因 = Select 数字传整框区带给居中共核，钉板 (b'2',6,16) 需 12px 左带；修 = 区带参数一行（535096164，笔记同批）；修后 243 绿。
2. **settings_io 长度差一**（§④）修 = 6b30aac79。
3. **gui8-smoke 红**（§③）：不修（wave4/launcher 域，归属与证据在案）。
4. 顺带记录：波2批C/D/E 提交信息所引「沿革三百四十三/四十四/四十五」在 docs/阶段沿革.md 无对应条目（号进提交信息未进文档）——挂账，不在验收批补写。

## ⑥ 全绿提交与哈希账

三路交付 9 笔（§① 表）在案；验收批 4 笔（显式路径分面）：53509616485ee646db1b5c8ec3e6acff428dd959（berry-ui 存量红修+笔记）/ 6b30aac79273dc4154417082a387b85e18d7176c（件⑤c canonical 修正×2 文件）/ 78679632188e71ee322c1b5ff3ada46aca2237dc（W3A-IMPL-SB 落账）/ 本报告。验收期并行文档三笔（8c885820b/8820895a3/d541b5e4e）与其余未跟踪物（.target-*/tools/.berry-verify/）未触碰。

## 结论

三路（SA appshell 抽取 / SB wmd 面 / SC TabBar 三页）**按报告交付属实，验收通过**：文件面语义不相交且序贯无覆写、3b 暂缓面零实现、host 测面全绿（243+27+52+14+25 + self-test）、launcher musl 链绿、BOOT.JSN 真改 host 实证、gui8-smoke 红为已归账的基线既有红（wave4 堆悬崖域，签名与 SA 记载逐字同形）且 3a 自身在机标记（LAYOUTW/LAYOUT ok）过链。验收修复三笔（一存量红+一差一+一落账遗落）已分面入库。
