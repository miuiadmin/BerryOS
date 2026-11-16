# 波3 件⑦ berry-appshell 抽取落地（Host 绑定缝/五模块形态/清树事故）

Status: implemented
Type: note
Date: 2026-11-15

为什么：波3 设计稿《2026-11-15-波3桌面三栏与系统窗实施设计》§6 把 launcher 五模块（main RPC 半程/frameloop 骨架/desktop 建树底座/icons/layout_io）的通用面抽成应用壳 crate `user/berry-appshell`（波4 七应用的消费底座——settings-app 样板头注「交互路由随 berry-appshell 件⑦」即等本刀）；执行序归[六波实施方案](../../plans/2026-11-15-体验大改造六波实施方案.md)步骤 4。本文记三项形态裁决与一次并行会话事故，供波4 应用接入与后续 crate 化先例取用。

## 裁决

- **berry-libc 不直连，Host 绑定表注入**：appshell 必须 host 可测（设计「每模块 host 单测」+ user 工作区 members 纪律），而 berry-libc 结构性不可 host `cargo test`（crt0 `global_asm!` ELF 专写法 + libc 同名符号对 host libSystem 插入——其 lib.rs「实况勘定」注记；user 成员 crate 依赖它零先例）。落法 = `Host { tag, diag, park, fs: Option<Fs> }` fn 指针表，消费者 boot 期 `bind()` 一次：诊断行 `"{tag}: {msg}"` 与抽取前 eprintln 字节恒等（launcher 绑 "launcher"）；`fs: None` = 冻结相缺席面（conf 按 Failed(EIO) 收口，冻结相零 RPC 门仍归调用方——`fs_h()==0` 早退语义不进 crate）。 berry-fsclient 的 `cfg(not(test))` 门只解 berry-sys 符号面（extern 声明 host 可编译），解不了 libc 面——两者叠用才是全缝。
- **pump 钩子序 = 操作序保真**：帧循环骨架（GETINFO 断言/跨代清账/帧 VMO 三连/SUBMIT→FRAME_CALLBACK→RELEASE/vote/input 泵/wait_many）整体入 crate，应用特化走 `HostHooks` 六回调（on_head/on_msgs/on_tick/on_presented/on_frame/extra_deadline）。保真锚：LAUNCH ok 类标记**不进骨架**（on_presented 回调归应用——否则任何 appshell 应用都会产 `LAUNCH ok`，smoke 标记 canon 污染）；on_presented 位次 = 原 drew 块内 RELEASE 后/vote 前；`PumpCfg.surface_id` 参数化（launcher=0xFFFF 系统面豁免，应用窗=wid）。
- ** OnceLock → ptr+len 原子对**：no_std crate 无 std::sync::OnceLock；icons 注册表/解码面两静态改 `AtomicPtr+AtomicUsize` 对（null=未装，len 先于 ptr Release 发布，读侧 Acquire 配对）——boot 单线程单写 + 首装定界幂等语义逐字保真，非引入并发面。
- **workspace 登记位**：纯函数/host 面 crate 进 `user/Cargo.toml` members（berry-fsclient 先例）；真消费 bin（launcher/settings-app 形态）仍是 musl 独立小工作区 path 依赖跨界——「库进 members、bin 走 escape」两轨不变。
- **生成器跟家走**：tools/genohstatus.py/genohwallpaper.py 的 OUT 随 status_icons.rs/wallpaper.rs 迁 `user/berry-appshell/src/`（selftest PIL 对拍 byte-equal 过链）；源资产 ohstatus/ohwallpaper 留 berry-launcher（宿主侧生成输入，无运行期耦合）。

## 坑

- **波4 会话清树（三度）**：本批未提交现场（含整 crate 目录）被波4 apps 会话的「工作树净」清树抹除三次（57fcbc464 提交信息自述「子代理越权 appshell 重构面已全数清除」——与 SA 任务书冲突的越权判定，编排层待裁）。处置 = SB 同款打法：逐件重写后**即时显式路径提交**（f016aa99a/510f6ce95）——已提交内容对 `git clean`/`reset --hard` 免疫。教训：多 lane 共工作树时，未提交现场不是资产，提交才是。
- **crate 名别名**：package `berry-appshell` 的代码内路径是 `berry_appshell`——消费侧须 `use berry_appshell as appshell;`（本刀首 build 的 15 连错根源）。
- **cfg(not(test)) 的模块顶 import 须同门**：import cfg'd-out 项的 `use` 行自身也要 `#[cfg(not(test))]`，否则 crate 自测编译即 unresolved import（依赖编译时 cfg(test) 恒关——launcher 消费不受影响，易漏）。

## 后续

设计 §6 新增面（wm 窗生命周期封装/Nav 页栈 + back 消费/ime_wanted/Dialog/Menu/Toast/APPDEMO 样例）非本刀——wmd 侧事件通道件已落（[波3 wmd 焦点笔记](2026-11-15-波3-wmd焦点与返回路由落地.md)），封装可接；改动清单/[W3A-IMPL-SA.md](../../research/w3a/2026-11-15-W3A-IMPL-SA.md)（工作树根）。
