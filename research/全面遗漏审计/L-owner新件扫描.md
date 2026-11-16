# L——owner 新件扫描（全面遗漏审计第四轮 B 线·语义级）

> 版本:1.0 ｜ 日期:2026-11-16 ｜ 定位:第四轮语义级审计 B 线——git log 最近 10 笔（HEAD=18388fb54 起）中 owner 提交的「需感知/集成面」四问扫描:①新增 crate/服务/API ②修改的共享文件（可能影响我们的功能）③新 ADR/设计文档 ④是否覆盖/回退我们的改动。方法:git 只读命令（log/show/diff）拉变更清单 + HEAD 直读核行号,两源落判;推断标「推断」;全仓只读零 git 写,本文件为唯一产出件（README 登记行未随批补——任务书「只写产出件」单件口径,留主会话随批登记）。

## 〇、结论速览

| 检查面 | 总判 |
|--------|------|
| ①新增 crate/服务/API | merge 63fbca1c9 净变更 192 文件（+13817/-2729）:根目录 face crate ×3 新入册（fwcfgface/psface/wakeface,判定律自 arch HAL 外迁单源）+ 既有 face 模块扩充 ×3（dpgface/chanface/vslotface）+ 波3 桌面三栏落件（berry-appshell 七模块 + 7 系统应用 + gui-wmd 策略核）+ TCP 二刀协议面（TCP_DATA_MAX 128→250 + CHAN_MSG_MAX=256 新常量 + 编译期几何断言族）;无新 ABI 号（最高仍 0x88,tools/abi.toml:752） |
| ②共享文件 | 根 Cargo.toml（members 33 项,merge 曾双写由 fbe1d6665 自愈）/justfile（test-host 口径 1949→2030）/[docs/规则/07-测试与冒烟.md](../../docs/规则/07-测试与冒烟.md)（口径同步）/[docs/规则/README.md](../../docs/规则/README.md)（face 列表 26→32）/kernel 四源与 arch 三源（判定律外迁留壳）/ABI 五面（0x84 desc）/[sync-public.sh](../../tools/sync-public.sh)（白名单补账）——详见 §三 |
| ③ADR/设计文档 | **零新 ADR**（[docs/架构决策.md](../../docs/架构决策.md) 在 81057ef95..HEAD 零改动,G 线「#86 三占」仍开）;调度器API参考 v1.8→v1.9 升版（225f3bbef）;shell/ABI/存储三设计正文随刀;.agents 新增 notes 5 + research 6;窗口外近邻 6e1697170 清偿笔带来**显示与加速体系设计 v1.1 方向翻案（desktop 竖屏 1080×1920）**——B 线需感知 |
| ④覆盖/回退我们的改动 | **零覆盖零回退**:审计产出/、qemu.rs、策略指南、bcts@1 双件近 10 笔零触碰;berray-sys 仅 telemetry 注释区被动,policy wrapper 区完整在席（行号漂移 +20,见 §五）;TCP「恢复自 tag」无他人改动可冲（区间零提交实证）;owner 沿革撞号已两刀让号收口（登记面冲突,非代码回退） |
| 附:新发现（上报面） | **根目录 W3A-\*.md 5 份双写**（缺头块副本与 .agents/research/w3a/ 同文在册）+ **tools/.berry-verify/ 73 个构建缓存文件入库**——均随 63fbca1c9 误入,HEAD 仍在册,建议 owner 清 |

## 一、窗口与 owner 判定

- 最近 10 笔（HEAD 起）:owner（miui/admin,同一邮箱 33443359+miuiadmin）8 笔 = 18388fb54（TCP 两刀）/fbe1d6665（五轮审计修复）/cb726accd（终审 docs）/63fbca1c9（merge wip/sched-1.1）/c9466fc7e（沿革撞号第七刀）/225f3bbef（API 参考 v1.9）/50ad8adef（mm W3-Q42,admin）/16a00e162（布局树补登）;berryos-bot 2 笔（610119c7c dpgface #331、539b8acf6 chanface #323,净增量重放管道件）不计 owner。
- 63fbca1c9 检查面取「相对第一父净变更」:第二父链 19 笔（owner 12 + bot 5 + autofix 1 + 重放 1）,其中窗口外深位还有 owner 侧 b9e7d4b64（波3 桌面三栏）/881138e20（fwcfgface）/33921d43a（psface）/9777dc867（wakeface,已见于 log 第 14 笔）/0b131ef1e（appshell/softime 接线）/cad5bd521（bcts 探针族）/9e2462d2d（CI dispatch）/72df73560（unsafe 台账重产）——均经该 merge 入主线,计为「owner 最新合流面」一并覆盖（推断:按任务书「最新合流」语义,merge 净 diff 已含其全部落件,不逐笔单列）。
- 第一父 81057ef95（启动产物实施批）亦为 owner（miui）,在 10 笔窗口外一位,其 loadgen/0x84 实装由窗口内 fbe1d6665 修复收口,本件只记交互不展开。

## 二、新增面——问 1:新增了什么 crate/服务/API

### 2.1 新 crate ×3（根目录 face 族,包名 berry-\*）

| crate（目录/包名） | 来源笔 | 单源内容 | HAL 消费点（留壳形） |
|---|---|---|---|
| fwcfgface（berry-fwcfgface） | 881138e20（W3-Q86,#307）经 merge | fw_cfg 目录行走/顺序读推进/ramfb 域门（find_file_in/WordStream/SeqState） | [arch/aarch64/src/fwcfg.rs](../../arch/aarch64/src/fwcfg.rs):82/:86/:90/:99/:131/:133 |
| psface（berry-psface） | 33921d43a（W3-Q72,#362）经 merge | PSCI 版本门/探测路由门/FEATURES 侦察 | [arch/aarch64/src/psci.rs](../../arch/aarch64/src/psci.rs):19（pub use 再导出形） |
| wakeface（berry-wakeface） | 9777dc867（W3-Q66,#365）经 merge | 唤醒归因链（IrqMap/src_of/restore_valid/slot_index,九源集） | [arch/aarch64/src/wake.rs](../../arch/aarch64/src/wake.rs):38/:82/:95 |

三件均入根 [Cargo.toml](../../Cargo.toml) members（psface :24;fwcfgface/wakeface 尾插 :37-38）且挂 justfile test-host（wakeface :655-660;psface :750-757,host 测 16+9 腿）。命名与 rootface/susface/aokface 同族（目录无前缀/包名 berry-\*,布局树在册,[docs/规则/README.md](../../docs/规则/README.md):83-84,face 列表 26→32、入库窗 ..16 由 16a00e162 补登）。

### 2.2 既有 face crate 模块扩充 ×3 + mm 单源 ×1

- dpgface/src/fastpath.rs（+tests.rs）:dpg P0-1 fastpath 唤醒直切判序钉板（610119c7c,W3-Q59,替 #252）;chanface/src/freeze.rs:channel 冻结面配对判 + 队列序守卫（539b8acf6,W3-Q49——bot 件,列此为面完整性）;vslotface/src/droot.rs（+tests.rs）:vmar asid→root 登记槽判序（ce6d0ac5d 经 merge,W3-Q60）。
- [mm/src/frame.rs](../../mm/src/frame.rs):reserve/release 区间算术单源 clip_to_window + wg 债随刀（50ad8adef,W3-Q42,替 #229——「净增量重放」形:原 crate 化 PR 重放为单源函数,不新增 crate）。

### 2.3 新服务/应用:波3 桌面三栏与系统窗（b9e7d4b64 经 merge,#356 替 #325）

- **7 个系统应用**（独立 musl 小工作区,各带 Cargo.toml/Cargo.lock/main.rs,不入 user/Cargo.toml members）:user/bins/{browser,camera,contacts,files,mms,photos,settings}-app。
- **berry-appshell 抽取**:新模块 src/{conf,host,overlay,pump,rpc,scene}.rs + icons/status_icons/wallpaper 三件自 berry-launcher R 迁入（R057/R100×2）——launcher 五模块纯抽取的应用壳 crate,首消费者 berry-launcher,行为零变化。
- **berry-launcher**:settings_io.rs/syspages.rs 新（系统页装配）;**gui-wmd**:src/lib.rs 新（窗口表/焦点维护/OP_WM_BACK 500ms 兜底策略核,双面 crate,mock 喂真 wire 帧）。
- 接线:justfile test-host 挂 appshell（:895,workspace 形）/gui-wmd（:902）/berry-softime（:929,W1 补接线）。

### 2.4 协议/内核 API 面（18388fb54 TCP 两刀）

- [user/berry-netdproto/src/lib.rs](../../user/berry-netdproto/src/lib.rs):新常量 CHAN_MSG_MAX=256（:89,kernel chanface 单源同值镜像 + 断言族机器钉）;TCP_DATA_MAX 128→250（:95,语义从「MSS 通告值」翻为「RPC 单 op 载荷帽,与 MSS 解耦」,:34 注释随改）;inline 几何断言族四条编译期 const assert（帧形 ≤ 通道帽,超帽即编译期炸）。
- drivers/bins/vnet/src/tcp.rs（+400 行）:TX_SLOTS=4 环/GBN/snd_wnd 记账;user/berry-tcp/src/lib.rs +190 行配套;berry-tcp/tests/state.rs +256 行（F77 三测增量）。终门自证:berry-tcp 53/53 + netdproto 43/43 + netproto 24/24 + harness 19/19。
- **无新 ABI 号**:tools/abi.toml 号段最高仍 0x88（:752）,0x85 attest 仍预留未实现（:724-728）;0x84 telemetry_open klog 快照域翻真（81057ef95 实装 + fbe1d6665 desc 补全三拒面五面再生,:715）。

### 2.5 新工具与 CI

- tools/selftest_guard.py（新,selftest 接线门,55 件扫描 0 漏接线）+ bcts 探针族三件挂 test-host 前置（dns-stub/parkprobe/hostfwdprobe,cad5bd521;justfile dns-stub 腿 :1170）。
- CI 两道（ci.yml/ci-light.yml）补 workflow_dispatch 手动兜底（9e2462d2d,#325 事件黑洞判例）;ci.yml 增 ksproto/mmap/signal 三件 host 测试 fail-fast 位（:145-168,「justfile test-host 未接线 = 静默腐烂」对抗审查实抓判例）。

## 三、共享文件修改面——问 2:可能影响我们的功能

| 共享文件 | 触笔 | 对我们的影响 |
|---|---|---|
| 根 [Cargo.toml](../../Cargo.toml) | merge（members 增 3）→ fbe1d6665 恢复 | merge 曾产生 members 双写（单行串 + 数组并存）,fbe1d6665 删单行形恢复数组（现 33 项,:3-38）——凡按 members 对账的门已自适应;非回退,属 merge 冲突残留自愈 |
| [justfile](../../justfile) | merge | test-host 新增接线 ≥8 处（§2.1-2.3 行号）;**口径 1949→2030**——我们回归报告引用的测试计数基准已变 |
| [docs/规则/07-测试与冒烟.md](../../docs/规则/07-测试与冒烟.md) | merge | test-host 注释行口径同步 2030（206 组）+ 探针族/selftest_guard/sync-public 对账门挂前置描述——回归纪律的文档基准随迁 |
| [docs/规则/README.md](../../docs/规则/README.md) | 16a00e162/merge | 布局树 face 列表 26→32 + 入库窗 ..16（:83-84）——新 crate 台账以布局树为家,索引面无义务（G 线 §1.1 判例维持） |
| kernel/src/{channel,dpg,vmar}.rs、sched/src/sched.rs、mm/src/frame.rs、arch/aarch64/src/{fwcfg,psci,wake}.rs | merge/50ad8adef | 内核判定律外迁 face 后源文件留壳消费——我们读内核判定逻辑时**单源已搬家**,直读旧位只见壳与 SAFETY 注（推断:后续审计/引用须以 face crate 为判定律家） |
| ABI 五面（kernel/src/{abi_gen,syscall}.rs、[tools/abi.syz](../../tools/abi.syz)/[abi.toml](../../tools/abi.toml)、[user/berry-sys/src/abi_gen.rs](../../user/berry-sys/src/abi_gen.rs)+lib.rs） | fbe1d6665 | 0x84 desc 三拒面五面再生（同步五文件）;**berry-sys lib.rs 仅 telemetry 注释区（:517-530 域）被动,policy wrapper 区完整**（现 :587-:614,E 号件原行号 :567-:571/:594-:598 已漂 +20,回引以符号名定位） |
| [tools/sync-public.sh](../../tools/sync-public.sh) | merge（bot #368+owner 补账） | 白名单补账 fwcfgface/susface/psface/wakeface（:217-226）——**新 crate 入册第五面**（索引/沿革/布局树/白名单/justfile 五面齐,G 线四件套口径可扩） |
| [docs/unsafe台账.md](../../docs/unsafe台账.md) | 72df73560/19d441753（autofix） | 台账 998→1009 随源重产——unsafe 计数基准变化 |
| user/berry-libc（alloc）、berry-ui/{lib,paint,pattern}、guifbd/guest.rs、berry-guiproto | merge（波3 批） | UI 面——E 线已标注 owner 占用面（berry-guiproto/gui-wmd）,我们不碰,仅感知 |
| [docs/阶段沿革.md](../../docs/阶段沿革.md) | 四笔 owner docs 触 | 撞号收口两刀（第六刀 59d11164c 五组让号 375-379、第七刀 c9466fc7e 两组让号 470/471）——沿革号段再次扩容,引用沿革号须以 HEAD 实数为准 |
| 根/用户 [Cargo.lock](../../Cargo.lock) ×2、user/Cargo.toml（appshell/softime 位次重排） | merge | 锁文件随新 crate/新依赖滚新;user members 无净增（7 应用为独立工作区） |

## 四、ADR/设计文档——问 3:需要我们了解的决策

1. **零新 ADR**:docs/架构决策.md 在 81057ef95..HEAD 零改动（git log 实证 0 笔）——G 线「预留号 #86 三占」（彻底Rust化计划 vs 显示与加速设计 vs berry-gpuproto 注释）**仍开**,本轮 owner 大合流未处置。
2. **调度器API参考 v1.9**（225f3bbef,R085 自 v1.8 改名）:timer slack 三 fn 补登（Q72 同族第十四例）,索引行随迁——API 参考现行版为 v1.9（[docs/规则/README.md](../../docs/规则/README.md):84 亦引 v1.9）。
3. **设计正文三处随刀**（fbe1d6665）:shell设计 v1.0 登记 dmesg;系统调用ABI设计 v1.0 记 0x84 语义;存储布局与沙箱设计 v1.0 记 manifest 规范形（注释删）。
4. **.agents 新件 11 份**（经 merge 入册）:notes/implemented 5 份（波3 系列 4 + CI-workflow-dispatch 手动兜底 1）;research 6 份（w3a/ 五件 + boot事件细化刀6侦察 1）——波3 的实施设计在 .agents/plans/2026-11-15-波3桌面三栏与系统窗实施设计.md（user/Cargo.toml 注释引）。
5. **窗口外近邻重大决策（推断:按「最新合流需感知」语义纳入）**:6e1697170（base 直推,63fbca1c9^1 祖先,merge-base 实证）带来**显示与加速体系设计 v1.0→v1.1 方向翻案——desktop 竖屏 1080×1920**（[docs/索引.md](../../docs/索引.md):133:「v1.1 方向翻案」+「B1-a 协议 crate（berry-gpuproto）已落」）——B 线（显示桌面）上轮结论中按 v1.0 裁的桌面形态面需按 v1.1 重读;同笔清偿 G 线两遗留（gpuproto 索引行 :133 已在册;显示设计旧名引用随 v1.1 升版收口）。

## 五、覆盖/回退检查——问 4:有没有覆盖/回退我们的改动

「我们的改动」操作口径 = 上轮审计产出 + E 线 P0 修复锚件 + E 线标注的我方面。逐件核:

| 我方件 | 近 10 笔触碰 | 判 |
|---|---|---|
| research/全面遗漏审计/*（A-G 产出） | 零触碰（git log 本目录仅 dc74ec763/123f4b580 两笔,均窗口外） | ✓ 未覆盖 |
| [tools/berry/src/qemu.rs](../../tools/berry/src/qemu.rs)（E P0-a vgpu 锚） | 零触碰（merge 净变更与三笔窗口内提交均未及） | ✓ 未覆盖 |
| [docs/策略服务用户指南-v1.0.md](../../docs/策略服务用户指南-v1.0.md) + tools/bcts/frozen/bcts@1 双件（E P0-b/c 锚） | 零触碰 | ✓ 未覆盖 |
| [user/berry-sys/src/lib.rs](../../user/berry-sys/src/lib.rs)（E P0-b wrapper 验证面） | fbe1d6665 仅 telemetry klog 注释区;policy wrapper（berry_policy_bind :612/berry_policy_status 域 :587-:599）语义未动,仅行号漂移 | ✓ 未覆盖（行号过期注记见 §三） |
| TCP 面（drivers/bins/vnet、berry-tcp、berry-netdproto） | 18388fb54「恢复自 tag tcp-knives-1-2-backup（4e638bd6b,stash 快照）」——git log 4e638bd6b..18388fb54^ 该五文件**零提交**（实证）,恢复无他人改动可冲,属 owner 自身 WIP 落账 | ✓ 无冲刷 |
| 我方管道件（bot 净增量重放 #331/#323/#365/#307/#368 等） | 经 63fbca1c9 正常并入主线,无反向 revert 笔 | ✓ 无回退 |

登记面两起冲突均已收口（非代码回退）:owner 大合流笔撞 11-07 keeper 沿革号 → 第七刀让号 470/471（c9466fc7e）;W2 五组撞号 → 第六刀让号 375-379（59d11164c 经 merge）。merge 尾巴（根 Cargo.toml members 双写）已由 fbe1d6665 次笔自愈。

**总判:零覆盖、零回退。**

## 六、需我们动作/感知的清单

1. **上报 owner 清理两处误入库**（随 63fbca1c9,HEAD 仍在册）:①根目录 W3A-IMPL-SA/SB/SC/REVIEW/VERIFY.md 5 份——[.agents/research/w3a/](../../.agents/research/w3a/) 同文副本但缺 Type/Status 头块,违「一事实一归属」（Curia 规则 1/5）;②tools/.berry-verify/ 73 个构建缓存文件（CACHEDIR.TAG/fingerprint/build-lock）——构建 target 泄漏入库（git ls-files 实数 73）。
2. **E 号件行号漂移注记**:berry-sys policy wrapper 区 :567-:571/:594-:598/:573/:614 → 现 :587-:591/:599/:612（推断:漂移来自 fbe1d6665 telemetry 0x84 扩注的上方增行）;E 文件回引时以符号名（berry_policy_bind/berry_policy_status）定位。
3. **G 线遗留状态刷新**:gpuproto 索引行已清偿（索引.md:133 行内注记,6e1697170）;#86 三占仍开（本轮零 ADR 动作）;「test-host 八件 CI 零引用」**维持**——ci.yml 中 G 线八件名 grep 8 处命中全为 berry-signal 前缀误匹配（G 线 §1.1 同款判例）,但 D 线「test-host 主体零 CI」口径需注例外:ksproto/mmap/signal 三件 + 探针族已接 CI fail-fast（ci.yml:145-168）。
4. **回归基准刷新**:test-host 口径 1949→2030（justfile/规则07 双源）;cargo 166 测零败为 cb726accd 终门口径,本轮 TCP 门为 berry-tcp 53/53 等（18388fb54 自证）。
5. **显示线重读触发**:显示与加速体系设计 v1.1 方向翻案（竖屏 1080×1920）落地在册——B 线后续对拍须换 v1.1 基线（推断:上轮 B 线按 v1.0 裁的桌面形态/窗口 1:1 面结论需复核）。
6. **新 crate 入册五面口径固化**:本轮新 face 三件走齐「members+布局树+沿革+sync-public 白名单+justfile test-host」五面——建议后续新件审计按五面对拍（G 线四件套口径扩一面）。
