# E 线:第二轮遗漏审计——P0 修复验证与全测试回归

> 版本:v1.0(2026-11-16;文件名按任务书指派不带版本号,版本记于本行)|性质:第二轮遗漏审计 E 线任务 A——上轮四路审计 P0×3 修复件逐项验证 + 全 selftest/cargo test/smoke boot 回归,全部结论出自本轮实跑或直读,零「抄沿革自证」
> 锚(被验修复件):[qemu.rs](../../tools/berry/src/qemu.rs)(ce8a708ae vgpu 挂载)·[策略服务用户指南 v1.0](../../docs/策略服务用户指南-v1.0.md)(0x83 门控口径)·[berry-sys lib.rs](../../user/berry-sys/src/lib.rs)(wrapper 注释)·[bcts@1.toml](../../tools/bcts/frozen/bcts@1.toml)/[bcts-t2@1.toml](../../tools/bcts/frozen/bcts-t2@1.toml)(pr=1 双件);回归面锚:[bcts-run.sh](../../tools/bcts-run.sh)·justfile test-host 在册行(:788/:807-810/:815/:1042)·[smoke.sh](../../tools/smoke.sh) §分相门控(:511-519)
> 采集窗:2026-11-16;方法 = grep/直读核行号 + 命令逐件实跑(每项附命令与退出码);行号 = 本轮直读值
> 边界:零 git 操作;除本件外零仓内写(cargo 产物落 /tmp 隔离 target,smoke 串口日志落系统临时区);kernel/sched/arch/justfile/svcmgr/berry-ui/guifbd/docs/berry-guiproto/gui-wmd 零碰(berry-guiproto/gui-wmd = owner 占用面)
> 推断声明:仅 §5「内核 ELF 新鲜度」一处含推断成分(源文件 mtime 对拍法,随文标注);其余结论均有实跑产物或直读行号两源

---

## §0 总判(一屏版)

| # | 验证项 | 判 | 一句话 |
|---|---|---|---|
| 1 | P0-a qemu.rs vgpu | PASS | `-device virtio-gpu-device`(:273)先于 `-device ramfb`(:274),恒挂无条件,注释链完整 |
| 2 | P0-b 门控文档 | PASS | 指南五处门控口径 + 1.1 版记在席;berry-sys 两 wrapper 注释与签名全带 auth/RIGHT_MANAGE |
| 3 | P0-c BCTS@1 签发 | PASS | 双件 platform_revision=1;img_pr_check 对 level=1 实跑 PASS(exit 0);bcts@0 负面对照样红(exit 1)判别力自证 |
| 4 | 全 selftest 回归 | PASS(8 件) | berry_assemble/mkapp/mkimage/mkcpio(--selftest-split)/gensourceindex/bcts-run(33/33 腿)/shvarcheck/docs_guard 全绿 |
| 5 | cargo test 回归 | PASS(7 crate) | crypto 57/gpuproto 29/slackface 12/em 8/power 31/thermal 13/pm-integration 14,全 0 失败,计数与 justfile 在册数逐件对齐 |
| 6 | smoke boot 相 | PASS | SMOKE_PHASES=boot 实跑 exit 0,boot 标记 32/32 到齐,banner 形态过/ident 非 nogit,150s 帽内自收 |

---

## §1 P0 修复验证

### §1.1 P0-a:qemu.rs vgpu 挂载(ce8a708ae)

判:PASS。三面核:

1. **在位**:`tools/berry/src/qemu.rs:273` = `cmd.arg("-device").arg("virtio-gpu-device");`——全名形态(非 PCI 别名 `virtio-gpu`,注释 :266 引 05 号 §5.1 别名陷阱判例)。
2. **顺序**:`:274` = `cmd.arg("-device").arg("ramfb");`——vgpu(:273)先于 ramfb(:274),声明序 = console 绑定序,注释 :270-271 引 09 号 §4 实证(ramfb 在前则 VNC 面恒黑帧)。
3. **恒挂**:两行无任何条件包裹(:261-263 tablet 才是条件挂;:253-259 数据盘条件挂),与注释 :264「virtio-gpu 恒挂」口径一致;`:252` `-global virtio-mmio.force-legacy=false` 已先置(传输层 legacy 关,05 号 §1.2 定谳形态)。

传输属性注(:267-268)与 QEMU 11.0.3 实测记录一致(`force-legacy` 非该设备属性——本机 QEMU 恰为 11.0.3,环境注见 §5)。

### §1.2 P0-b:门控文档(策略服务用户指南 + berry-sys wrapper)

判:PASS。两件六点:

1. **指南正文**:`docs/策略服务用户指南-v1.0.md:7`(总述「每个 0x83 调用都携 auth——带 RIGHT_MANAGE 权的 Process handle」)、`:41`(「auth 能力门控」专段:三门控在表/类型 Process/权利 MANAGE,先于 op 分派,坏值 -EBADF/缺权 -EACCES,policydemo 双拒负断言面)、`:49`(示例结构注释)、`:84`(错误码速查 ACCESS_DENIED 行)、`:98`(op=2 STATUS 面门控,读面也在门后)——门控口径六处齐布。
2. **版记**:`:290` 1.1(2026-11-15)条目明记「门控口径对齐内核实态」「1.0 的『v1 无 handle 门控、auth 预留传 0』为漂移表述」——修复动作与缘由自证在档。
3. **wrapper 注释**:`user/berry-sys/src/lib.rs:567-571`(berry_policy_bind doc:门控时机/两错误码/持权者画像)、`:594-598`(berry_policy_status doc:同门控 + auth=0 裸调恒 -EBADF + 出参不被写语义)。
4. **签名面**:`:573`(bind 六参首参 auth: Handle)、`:614`(status 三参首参 auth: Handle)——原「硬编码 auth=0」形态已消除。
5. **消费面旁证**:`user/bins/policydemo/src/main.rs:65/:78` 双裸调负断言(BAD_HANDLE 期望)与门控语义自洽(指南 :41 所指「双拒恰 -EBADF = 门在位」)。
6. **留窗项(非本批缺陷)**:指南 :290 尾注自记同根因漂移「显示管线调度集成指南 §二示例仍 auth=0 裸调」——该文归显示线 owner,留窗修,与本批 P0-b 修复范围界定一致。

### §1.3 P0-c:BCTS@1 签发(pr=1 双件)

判:PASS。四面:

1. **双件在席 + pr=1**:`tools/bcts/frozen/bcts@1.toml:15` = `platform_revision = 1`(tier1_floor=90,:19);`tools/bcts/frozen/bcts-t2@1.toml:28` = `platform_revision = 1`(tier2_floor=31,:32)。两件 frozen_at 同点 `a2f9ff888…`(:16/:29)——「pr=1 双件一代」口径自证。
2. **img_pr_check 对 level=1 PASS(实跑)**:`BCTS_FROZEN=tools/bcts/frozen/bcts@1.toml tools/bcts-run.sh --image out/berryos.img --dry` → exit 0,首行「bcts: 版本声称位对账 ok:manifest platform_revision=1 == 冻结件 platform_revision」;清单回显 90 件(≥ floor 90)、bare 相 32 件(≥ bare 下限)。对账走 `tools/bcts-run.sh:1011`(`--dry` 不跳过,:1011 在 :1015 dry 分流之前),manifest 锚 = out/manifest.txt 4 条目全 `platform_revision=0000000001`。
3. **判别力负面对照(实跑)**:同镜像换 `bcts@0.toml`(pr=0)→ 「bcts: FAIL: 镜像 manifest platform_revision=1 != 回放清单 pr=0」exit 1——门非恒绿摆设,pr 不等真拒。
4. **T2 件与快照哈希**:BCTS_FROZEN=bcts-t2@1 `--tier 2 --dry` exit 0(清单回显 + 静态对账面完整);两 @1 件 `abi_snapshot_hash` = sha256:d47c0177… 与现役 `tools/abi.toml` 实测 shasum 逐字节一致——签发纪律(快照哈希随冻结点重钉)自证。

## §2 全 selftest 回归(逐件实跑)

命令形态:仓库根逐件裸跑,每件记退出码与末行。结果 8 件全绿:

| 件 | 命令 | rc | 末行摘 |
|---|---|---|---|
| berry_assemble | `python3 tools/berry_assemble.py --selftest` | 0 | berry-assemble: selftest ok |
| mkapp | `python3 tools/mkapp.py --selftest` | 0 | mkapp selftest: ok |
| mkimage | `python3 tools/mkimage.py --selftest` | 0 | mkimage: selftest ok |
| mkcpio | `python3 tools/mkcpio.py --selftest-split` | 0 | mkcpio: selftest-split ok |
| gensourceindex | `python3 tools/gensourceindex.py --selftest` | 0 | gensourceindex: selftest ok |
| bcts-run | `tools/bcts-run.sh --selftest` | 0 | 33/33 腿绿(T1 schema + 拒面合成 + 两态对拍 + T2 对账/咬合 + --image 联动;零 QEMU) |
| shvarcheck | `python3 tools/shvarcheck.py --selftest` | 0 | shvarcheck: selftest ok(吞名 7 形 + 免疫 6 形 + 花括号面) |
| docs_guard | `python3 tools/docs_guard.py` | 0 | docs_guard: PASS(0 警) |

注记:mkcpio 无裸 `--selftest` 旗标(实跑 rc=2 打用法文)——其唯一自检入口为 `--selftest-split`(mkcpio.py:676 分派,内部串跑 split 守卫 + selftest_manifest + selftest_berryver + selftest_sig_companion 四门一入口,:660-664)。任务书「mkcpio --selftest」写法与实件旗标名有此出入,以实件为准记档,不构成缺陷。

## §3 cargo test 回归(现空闲 crate,逐件实跑)

纪律:隔离 `CARGO_TARGET_DIR=/tmp/berry-auditE-target`(不碰仓内/全局 target);显式 `--target aarch64-apple-darwin`(根 `.cargo/config.toml` build.target 锁 aarch64-unknown-none,host 测试须覆盖——berry-thermal 头注同口径);manifest-path 形态照抄 justfile test-host 在册行(:788/:807-810/:815/:1042)。任务书列名 7 件(berry-power/berry-thermal 实际位于 user/bins/ 下),全跑:

| crate | 实测 | justfile 在册数 | rc |
|---|---|---|---|
| berry-crypto(user/Cargo.toml -p) | 57 passed / 0 failed | —(:1042 无计数注) | 0 |
| berry-gpuproto(user/Cargo.toml -p) | 29 passed / 0 failed | —(:788 无计数注) | 0 |
| berry-slackface | 12 passed / 0 failed | 12(:801) | 0 |
| berry-em | 8 passed / 0 failed | 8(:801) | 0 |
| berry-power(user/bins/) | 31 passed / 0 failed | 31(:801) | 0 |
| berry-thermal(user/bins/) | 13 passed / 0 failed | 13(:801) | 0 |
| pm-integration | 14 passed / 0 failed | 14(:811 注「host 14 测」) | 0 |

七件全绿且计数与 justfile 在册注释逐件对齐——无静默掉测、无漂移。berry-power/berry-thermal 的 bin 目标 `test = false`(bin 挂 wired required-features,guest 绑定面由 svcmgr selftest 盖),lib 测试面即上述计数,与 :807-808 门禁形态一致。

## §4 smoke boot 相(实跑)

命令:`SMOKE_PHASES=boot SMOKE_KEEP_LOG=1 timeout 150 /opt/homebrew/bin/bash tools/smoke.sh`(仓库根;QEMU 走 PATH 解析 /opt/homebrew/bin/qemu-system-aarch64)。

结果:**exit 0**(非 124,150s 帽内自收),逐门:

1. **前置门**:`berryos-size: assert-flip OK`(boot 低半 + 主体高半镜像,entry=0x400002c8);`abi-check: PASS(NR v1 38 号在册——内核/berry-sys/文档/syz/arch-wake 五面单源一致)`;`abi-check: G-ver-1 level 单调 HEAD level=1`——level=1 与 BCTS@1 pr=1 同值,P0-c 三方联动(PR≡level)闭合旁证。
2. **boot 相**:`smoke: boot 标记 32/32 到齐`;`smoke: SMOKE_PHASES=boot 止于 boot 相(分层 CI 轻面)`(:515-519 分相门控生效)。
3. **banner 形态(串口日志直读)**:`BerryOS boot 0.2.0 (6d317fca1-dirty rustc1.97.0 dbg)`——semver + 括号结构过 G-ver-2 门;ident 含 `-dirty` 不含 `nogit`,过 §6 发布链门(dirty = 工作树有未提交改动的本地构建常态,非违门形态);`ktest done` 在席。
4. **日志留档**:SMOKE_KEEP_LOG=1 串口日志留存于系统临时区 berryos-smoke.COTYyL(会话内证据件,不复制入仓)。

## §5 环境注与推断标注

- 宿主:macOS 27.2.1(Build 26B131,Apple Silicon);rustc 1.97.0(仓 rust-toolchain.toml 钉版,2d8144b78 2026-07-07);QEMU emulator version 11.0.3(/opt/homebrew);Python 3.14.6。
- 内核 ELF:`~/.cargo-target/aarch64-unknown-none/debug/berry-kernel`(全局 ~/.cargo/config.toml target-dir 覆盖态,paths.sh 动态解析),mtime 2026-11-15 21:38。**推断**:以「源文件 mtime 新于 ELF」对拍法核新鲜度——kernel/arch/mm/sched/ipc/fs/net/drivers/bootloader/abi.toml 域内零文件新于 ELF;新于 ELF 的 5 文件(gui-wmd ×2/berry-guiproto/userbins/bsh-priv/justfile)全在 owner 占用面或非内核构建输入,不入 bare boot 相——判定现行 ELF 对 boot 相即最新,未重跑 just build。若 owner 面后续落库,boot 相结论需随新 ELF 复跑。
- cargo 测试与 smoke 并行零冲突:前者 /tmp/berry-auditE-target(host 目标),后者读全局 target 内核 ELF(none 目标)。
- smoke QEMU 退出串「terminating on signal 15」= smoke.sh 成功后 cleanup 自杀 QEMU 的常态尾,非异常。

## §6 遗留观察(非 FAIL,记档供下轮)

1. **mkcpio 旗标名**:工具面唯一自检入口 `--selftest-split`,无裸 `--selftest`(§2 注记)——任务书/口径文书若引用需按实名。
2. **显示管线指南 auth=0 漂移留窗**:指南 :290 自记,归显示线 owner,本轮范围外(P0-b 六点已闭本批面)。
3. **justfile test-host 无 CI 接线**:D 线在册发现(test-host 零 CI),本轮 §3 实跑七 crate 全绿为其「本地绿」侧补证,CI 侧缺口仍开放。
