---
name: berryos-dev
description: BerryOS 开发工作台规约——构建（EPERM 坑与隔离 target）、QEMU 手动 e2e 模板（拓扑契约与喂命令窗口）、分层验证链、并行会话实战纪律（同文件混装防护与 worktree rebase）、DSH workflow 规约。用户提到 构建/跑 QEMU/跑 smoke/跑测试/e2e/合入主线/workflow 失败/并行会话冲突 时使用。
---

# BerryOS 开发工作台规约

本会话（2026-11-15 至 11-16「启动产物与状态体系」十二批 + 七轮审计）实证提炼的可复用干法。每条都有实抓判例背书，不是想象规约。

## 步骤

### 一、构建——先避开三个环境坑

1. **EPERM 坑**：全局 `~/.cargo-target`（或 `~/.cargo/config.toml` 的 target-dir）下 rustc 子进程写 `.d` 文件稳定 `Operation not permitted`（shell 可写 rustc 被拒）。一切 cargo 构建带隔离目录：`CARGO_TARGET_DIR=$PWD/target-<tag>`（仓库已有 target-klog/target-util-snap 等同款先例；用后可删，勿提交）。
2. **路径固化坑**：`tools/paths.sh kernel` 经 `cargo metadata` 解析 target 目录——**尊重 CARGO_TARGET_DIR**（smoke/QEMU 手跑与构建用同一 env 即对齐）；但缓存的 build script 产物（out_dir 绝对路径）在换目录复跑时会指向已删目录出假红（genfbfont_guard 判例）——隔离目录即干净。
3. **berry CLI**：独立 workspace，裸 `cargo build/test` 会编到 no_std target——必须 `--target <host>`（host 取 `rustc -vV` 的 host 行）+ 隔离 CARGO_TARGET_DIR。bsh/settings 等 musl bin 构建必须带 `RUSTFLAGS="-C linker=tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096"`（照 justfile build-user 形态）——漏链接器会得到 cc 链接错。

### 二、QEMU 手动 e2e——拓扑契约是第一死因

最小 rootfs 形态模板（本会话多轮实证）：

```bash
CARGO_TARGET_DIR=$PWD/target-klog just build   # 或只 cargo build 建 kernel
KERNEL=target-klog/aarch64-unknown-none/debug/berry-kernel   # 用实构建目录，勿信 paths.sh 指的全局陈旧内核
(sleep <窗>; echo '命令'; sleep <尾>) | timeout <帽> qemu-system-aarch64 \
  -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs0,file=rootfs0.cpio -fw_cfg name=opt/berry/rootfs1,file=rootfs1.cpio \
  -fw_cfg name=opt/berry/noktest,string=1 \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=<盘>,cache.writeback=off -device virtio-blk-device,drive=drv0 \
  -device virtio-tablet-device,id=tab0 \
  2>&1 | tee <日志>
```

4. **拓扑契约四件**（缺一 park/boot 断）：rootfs 多卷（rootfs0/1 按在席卷拼）；**virtio-tablet 必挂**（gui-inputd 分类认领门要求指针类恰 1（2026-11-17 键盘刀后多设备合法：键盘类至多 1）——漏挂 tablet 即 `park` 挡断，实抓两次）；盘要 `mkfat.py` 现造 + `printf 'BERRYOSVBLK!' | dd` 印章；**`simdev=1` 键必须配第二路 `-serial`**（内核见键即 init uart1（0x09040000）——无设备在席时 `uart1_init` 写寄存器直接 D-abort halt，2026-11-17 实抓；smoke 形态恒配 `-serial unix:$SIM_SOCK`）。netdev/hostfwd/echod 辅助进程可省（vnet 静默缺席不挡链）。
5. **noktest 键**：`-fw_cfg name=opt/berry/noktest,string=1` 跳过 ktest/bench——绕开时序断言（LOADGEN A3 的 p50≤5ms 在宿主 load>8 时必 panic，环境面非代码面）；验证链路而非 ktest 时用它。
6. **喂命令窗口**：TCG 下 boot 链 3-10 分钟（宿主负载敏感），`sleep` 值按最近一轮日志的 `bsh>` 首现行号估；命令回显会被稳态矩阵洪流打散——判据用 `rg -o` 直接搜特征串，勿依赖行完整性。
7. **dmesg/探针类 e2e**：MARKER 探针与 dmesg 之间洪流可能把探针滚出 4KiB 尾窗——用 bsh 串接（`探针命令 && dmesg`）压间隔，或只依赖时序不敏感分量（已追尾行零再现、最新洪流特征在席）。

### 三、分层验证链（快到慢）

8. **秒级对拍**：`bash tools/fwcfg-check.sh`（fw_cfg 键表 21 键两侧一致）、`just abi-check`（ABI 五面）、`node .agents/tools/check-docs.mjs`（文档面）。
9. **分钟级**：berry CLI 测试 `cd tools/berry && CARGO_TARGET_DIR=/tmp/berry-t cargo test --release --target <host>`（373 例基线——遗漏轮笔误 1373 勘正）；`just test-host`（host 单测，rootface 已接入）。
10. **十分钟级**：`CARGO_TARGET_DIR=<同构建> SMOKE_PHASES=boot just smoke`（boot 相 32 标记，CI 轻面同款）；`just build` 全链（含 build-user/mkapp——manifest 注释必须纯 ASCII 且**禁任何注释**，mkapp 规范形门禁）。
11. **长跑**：rootfs 相/全量 smoke 只在低载窗跑（echod 18080 端口被并行 lane 独占是常态——A-VERIFY「停段于 FAT 族」同族环境面）。

### 四、并行会话实战纪律（同文件混装是头号事故源）

12. **commit 永远带显式路径** `git commit -m ... -- <路径清单>`——共享分支的 index 不属于自己（实抓：一次漏带把并行 lane 已暂存的 5 文件裹挟入库，soft reset 重做）。
13. **add 混合文件前看 diff 归属**：`git diff <文件> | rg "^@@"` 逐 hunk 辨归属——同文件里并行 lane 的 hunks 会随整文件 add 入库（跨文件撕裂实抓：消费点入库、定义没入库 = 基线不可构建）。你的 hunks 独立时可 hunk 手术：构造中间态文件 → `git hash-object -w` → `git update-index --cacheinfo`（index 态入库，工作树零扰动——并行 lane 的在途面完好保留其 diff）。
14. **恢复性接口摘抄**：并行半成品造成的基线破窗（如字段定义未入库）可用最小接口摘抄立即闭合（Cargo.toml members 修复、web.rs 三字段两个判例）——摘接口不摘功能面。
15. **共享分支同步撞 dirty**：merge/rebase 会拒碰 dirty 文件——用 **detached worktree 绕行**：`git worktree add --detach /tmp/x HEAD` → 原地 `git rebase -r origin/<分支>`（冲突逐个解；纯追加日志型文件用 union 合并保两侧）→ `git push origin HEAD:refs/heads/<分支>` → 删 worktree——主工作树 HEAD 与 dirty 零干扰。
16. **集成分支流程**：wip/sched-1.1 是集成分支——2026-11-16 已与 main 跨根收口合并（fork 点定谳法与全部判例见[收口笔记](../../notes/implemented/2026-11-16-跨根收口合并.md)、[防线笔记](../../notes/implemented/2026-11-16-跨根合并防线与判例.md)），此后两线共享历史走普通合并；`.githooks/` 三闸保留拦新的孤立根合并（逃生阀 `BERRYOS_ALLOW_CROSSROOT=1`）。**跨根/大歧文件合并铁律**：merge-file 三方自动区会静默丢静区内容（ktest 1800 行判例）——必须 HEAD 真源全量重建 + `git apply -3` 增量补丁 + multiset 两向审计，`git apply -3` 前先 `git add` 同步 index。
17. **--no-verify 的合规使用**：pre-commit 钩子的 check-docs --strict 会被并行 lane 随 merge 进来的断链拦停（非自己的债）——AGENTS 规则 8 明文「不是提交硬门槛」，核实告警确属他人后可 --no-verify 提交自己的部分。

### 五、DSH workflow 规约

18. **parallel() 系统性失败**：本环境 workflow 的 `parallel()` fan-out 高失败率（七轮里多轮全灭），单 `await agent()` 存活——脚本写「先并行、null 项串行 await 补跑」的自适应形态；串行兜底也全灭时换普通 `subagent` 工具通道（稳定）。
19. **agent 分工**：审计/核实类给 subagent（只读规格+行号级证据 schema）；实施类给详细规格+文件域隔离+「禁 git 提交」（主线统一验证提交）；agent 交付后**主线必复核**——实施轮三个 high 全是 agent 交付后主线实跑暴露的（验证形态单一、断言形态假设、路径坑）。
20. **模板字符串转义**：workflow script 是 JS 模板字符串——shell 语法的 `${1:-0}` 会被误解析（SyntaxError），写成文本拼接或转义；prompt 里 markdown 反引号同坑。

## 边界

- 本规约是经验密度不是真理全集——与 AGENTS.md/justfile 实际注释冲突时以后者为准（行号与命令形态随 lane 漂移，本文件只钉「坑与纪律」这类慢变事实）。
- 时序断言的「环境面」判定要有对照证据（同日低载窗全绿/同断言他形态过）才能归环境，勿用它当代码问题的挡箭牌。
- worktree/hash-object 等 git 手术动作破坏性高，执行前逐条核对路径与目标（AGENTS 并行纪律全条适用）。
