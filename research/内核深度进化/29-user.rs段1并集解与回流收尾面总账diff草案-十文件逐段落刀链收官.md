# 内核深度进化 29 号：user.rs 段 1 并集解与回流收尾面总账 diff 草案——十文件逐段落刀链收官（29 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件把回流对拍总账落为十文件 38 段，22-28 号件已把 vmar/aspace/dpg/channel/ktest/syscall 六代码面逐段落刀——本件落最后一个代码冲突面 user.rs 段 1（:1797，merge-file 三方实跑恰 1 段零漂）并把 13 号施工序第 5 步的收尾面（justfile/members/白名单/07 口径/台账/Cargo.lock）升为逐刀总账，带出三个新定谳：①**段 1 并集解零语义耦合实证**——两线 `Vmar::map()` 签名逐字恒等（fa/vmo/vaddr/vmo_offset/len/perm 六参形），vdso `map_into` 消费点在 22 号 vmar 十段整取后自动绿；`aspace_ptr()` 取 `Arc::as_ptr` 堆稳定地址，登记后 Vmar move 进 Self 无悬垂；`BOOT_ASPACES` 全树无注销面 = boot 常驻进程永生语义，无泄漏面——13 号「并集」裁定从直觉升为三锚实证；②**asidface members 行 deletion-wins 静默丢弃**——fceb=1/ovh=1/base=0/合并树=0（四问核），members 行整段被静默取删侧，目录孤儿态（3 文件 + asid_tests 5 host 测）保留在树——第十一连判据（27 号）在 members 面的第 12 例；③**vmarface 入册义务升施工卡刀位**——11 号件预判的八触点两手工点（justfile 一行 + 白名单一笔）随本件收尾面总账定格，07 口径行 3283〔300 组〕随 vmarface +4 host 测滚新。至此 13 号施工序第 1-5 步全部逐段落刀完毕（22→29 号八件连链），回流施工只欠 owner 指令下达。

## 审计基线（2026-11-20 10:2x 直核）

- base 线 = `origin/wip/sched-1.1@166c83065`（#598 合流后）；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`（user.rs 2127 行；base 2117 / overhaul 2128）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1` → 树 `d56481499e`（28 号时点树 `ae7fef3cd` 随 base 前进漂移；十文件冲突单零漂——user.rs `<<<<<<<` 恰 1 处 :1797 与 13 号基线零漂）。
- 三方行数：ours(base) 2117 / fceb 2127 / theirs(overhaul) 2128——段 1 是 user.rs 唯一冲突段；其余面自动并区恒等。

## 主发现一：段 1 并集解零语义耦合实证（13 号裁定升三锚）

段 1（:1797-1803）ours 侧 = Q106 vDSO 数据页映射（5 行注释 + `crate::vdso::map_into(fa, &mut vmar)`），theirs 侧 = `crate::vmar::register_boot_aspace(asid, vmar.aspace_ptr())`。13 号裁定并集两句先后排（先 overhaul 登记后 base vDSO）。本件逐锚实证零耦合：

- **锚 1（map 签名恒等）**：两线 `Vmar::map()` 六参签名逐字相同（`&mut self, fa: &mut FrameAllocator, vmo: &mut Vmo, vaddr: usize, vmo_offset: usize, len: usize, perm: Perm`）——overhaul 侧仅是把实现改投 `self.aspace.map(fa, self.vmar_id, ...)`（区间树形），调用面零变化。vdso.rs `map_into` 消费点（`vmar.map(fa, vmo, VDSO_VA, 0, VDSO_PAGE, Perm::UserRo)`）在 22 号 vmar 十段整取后自动绿——23 号已核 `Perm::UserRo` 单点消费在 6 档扩容零穷尽面内。
- **锚 2（aspace_ptr 无悬垂）**：overhaul `Vmar { aspace: Arc<AddressSpace>, vmar_id, base, len }`——`aspace_ptr()` 取 `Arc::as_ptr`（堆稳定地址），登记后 `vmar` move 进 `Self { vmar: Some(vmar), .. }` 不改 Arc 堆地址；且 build 语境 Vmar 不再销毁（`_vmos` 三件随结构体持活）——无登记悬垂窗。
- **锚 3（BOOT_ASPACES 永生语义）**：`register_boot_aspace` 全树无注销对偶（grep `unregister` 零命中）——设计语义就是 boot 常驻 `UserProcess`（`current_pinned() == 0` 的测试/守护任务，全树仅 8 例）永生反查表；16 槽 `BOOT_ASPACES` 对 8 例有裕量，无泄漏面无越界面。

- 判定：段 1 解 = 并集两句（overhaul 登记句在前、base vDSO 句殿后），零手工调和面——与 13 号裁定一致，本件补实证闭合。
- 附核：段 1 窗外消费面全活——`alloc_asid()`（:1777）在 overhaul aspace.rs `:925` 供体活（24 号两段整取后自动承接）；`Vmar::new_root/ttbr0_pa`、`Vmo::create/commit/write` 两线同名同签名自动并区绿。

## 主发现二：asidface members 行 deletion-wins 静默丢弃——第十一连第 12 例

- 四问核：fceb members 有 `asidface`（:47）/ overhaul members 有（:48）/ base members 无（删除侧）/ 合并树无——git 三方合并静默取删侧，无冲突标记无红。判据与 27 号 F0-3/F0-4（ktest 定义体）、vmo/mm 供体链完全同族：**「fceb 有 → ovh 未改 → base 删」链 = 自动死且无红**。
- 与 27 号的差异：27 号五文件链是**代码供体**（丢了编译红），本例是 **members 行**（丢了只是 test-host 少跑 asidface 5 host 测——asidface 目录孤儿态：3 文件在树、`asid_tests.rs` 5 测自持、零在树消费、非 workspace member）。README 布局树 :84 已注记「二级合龙后目录孤儿化……去留归 owner 裁，W2 只登记不替裁」——本件维持该裁：**members 行不复活**（复活即重写 owner 已裁的孤儿化决定），asid_tests 5 测不进 test-host 口径。
- 判普补登：deletion-wins 的静默面不只代码供体——**members/白名单/挂测行三类登记面同判**（登记面丢失无编译红，只有口径账面漂）；回流施工单的复活义务清单须把登记面单独列栏（本件施工卡刀 ② 即其落位）。

## 主发现三：vmarface 入册义务定格（11 号八触点收尾）

11 号件预判 vmarface 回流八触点 = 六零手术 + 两手工（justfile 一行 + 白名单一笔）+ overhaul 线自身债代偿。本件随 base 前进复核后定格：

- **justfile（冲突 :719-723，全十文件唯一挂测行冲突）**：overhaul 侧两行（`cargo test -p berry-asidface` + `cargo test -p berry-vmarface`）vs base 侧零行。解 = **只插 vmarface 一行**（`cargo test -p berry-mm` 行 :718 后直插），asidface 行不复活（主发现二同裁）——11 号「不复活死行」方案实施化。
- **白名单**：合并树 `tools/sync-public.sh` 自动取 base 侧（overhaul 零改动）= 无 vmarface 笔——check-members 机器门在 members 并集（自动带活 vmarface）+ 白名单缺笔时必红。代偿刀 = 白名单补 `vmarface` 一笔（wsrcface :268 先登防红判例同款）。
- **07 口径行**：现值 3283〔300 组〕→ 回流施工批后 +4（vmarface 4 host 测实测 :631/:656/:689/:728）滚新；W1 深测坑纪律（单物理行巨文件 patch 后核 grep 计数）随刀。
- **Cargo.toml members / kernel deps / 台账 / wg / 布局树**：四触点零手术自动并（11 号判定随 base 前进零漂复核）；台账 8 段冲突随施工批重产即解（工具唯一真值源）；Cargo.lock 2 段机械冲突重锁即平。

## 施工卡（user.rs 面实施化 + 回流收尾面，六刀一批——13 号施工序第 5 步落位）

- **刀① 段 1 并集解**：:1797 两句先后排（register_boot_aspace 在前、vdso::map_into 殿后）。执法 grep：施工后 `git grep -n 'register_boot_aspace\|vdso::map_into' kernel/src/user.rs` 双句并存。
- **刀② justfile 挂测行**：berry-mm 行后插 `cargo test -p berry-vmarface --target {{host}}` 一行，asidface 死行不复活。执法：冲突窗解后 `grep -c 'berry-vmarface' justfile` = 1。
- **刀③ 白名单代偿**：`tools/sync-public.sh` WHITELIST 补 `vmarface`。执法：`--check-members` 门绿。
- **刀④ 07 口径滚新**：3283 → 实跑回填（预期 +4 = vmarface 4 测）；沿革节随批。
- **刀⑤ 台账重产 + Cargo.lock 重锁**：`python3 tools/unsafe_audit.py` 重产；任意 cargo 命令触发重锁。
- **刀⑥ 布局树注记**：`docs/规则/README.md` face 列表 +vmarface 行（11 号触点 8），face 计数与 members 闭合核对。
- 验证口径：`cargo build -p berry-kernel` 0 error；`just test-host` 全量（含 vmarface 4 测）；docs-guard/abi-check/wg --selftest 三门禁；check-docs --strict 0 告。
- 施工序位：13 号施工序第 5 步（第 1-4 步 = 22-28 号八件已全部落刀）——本件后回流施工单全部预制完毕，只欠 owner 回流指令。

## 与其他文档的关系

- 总账单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——本件 = 其施工序第 5 步（user.rs 并集 + justfile/members/台账/lock/入册收尾）的实施化收官；其「user.rs 1 段并集」裁定三锚实证闭合。
- 入册清单：[11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md](11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md)——八触点两手工点随本件定格为施工卡刀 ②③⑥。
- 判据链：[27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md](27-ktest.rs段1并集解与F0-3F0-4复活面diff草案-第十一连deletion-wins供体链整链静默丢弃定谳.md)（deletion-wins 判据）→ 本件（判普扩登记面：members/白名单/挂测行三类）；[22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md](22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md)（vmar 整取 = 锚 1 消费面前提）。
- 逐段链：22（vmar 十段）→ 23（PXN/UXN）→ 24（aspace 两段）→ 25（dpg 调和面 F）→ 26（channel 五段）→ 27（ktest 并集）→ 28（syscall 六段）→ **本件（user.rs + 收尾面）**——13 号十文件 38 段全段落刀完毕。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/base-overhaul origin/wip/sched-1.1`（树 `d56481499e`，base tip `166c83065`）；并集三锚 = `git show origin/wip/base-overhaul:kernel/src/vmar.rs`（:278 map 签名 / :257 aspace_ptr / :67 register_boot_aspace）× `git show origin/wip/sched-1.1:kernel/src/vdso.rs`（:57 map_into）；asidface 四问核 = `git show <rev>:Cargo.toml | grep -c asidface`（fceb 1/ovh 1/base 0/merged 0）；vmarface 测数 = overhaul lib.rs `#[test]` ×4。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百四十节（随本批——base 现占至六百三十九）。
