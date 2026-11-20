# BSHRC 宿主写者与 PATH 端到端刀（berry bshrc 子命令族＋FAT12 就地写面）

Status: implemented

一句话说明：owner 令「彻底实施」续兑现 export 面刀挂账 #3——宿主 berry CLI 新增 `bshrc` 子命令族（set/get/list/rm），写面＝新模块 tools/berry/src/bshrc.rs 经 berry-fatfs path 依赖对既有 FAT12 数据盘就地写卷根 BSHRC.CFG（整盘内存编辑＋temp+fsync+rename 原子提交——Torn 前车封死；写读同 crate＝方言对拍按构造成立）；fail-closed 帽逐条对齐客机 rc_load 截断面；客机端零改；测试三层 345→373 全绿（mkfat dev-dep 整盘字节恒等对拍＋crashlog 独立读面 oracle＋真进程集成测五族）；PATH 端到端（宿主写→起机→bsh 装载→run which 命中）挂账低载窗（判据脚本在档）。

## 交付

- **D1 写者模块**：bshrc.rs（模型 rc_load 同款解析＋写面 W5 七步提交序＋原子换名＋帽五面 fail-closed＋env 通道帽投影软警＋run 注册表在用护栏三态〔device 拒/shot 保守拒/run 放行〕）；berry-fatfs version.workspace 跨工作区解析实测通过（风险销账——user/ 侧零改）。
- **D2 命令面**：Cmd::Bshrc 四动作＋dispatch（set 多对同批/get 裸值/list KEY=VALUE 行/--json 纯 JSON/退出码族 device(6)·env(3)·usage(2)）；resolve_disk 三级（--disk canonicalize＞BERRY_DATA_DISK＞default_data_disk）缺盘附清单指路；doctor bshrc_present 薄面。
- **D3 测试**：src 单测 23（含 mkfat 字节恒等＝写方言≡造盘方言、盘保真三面 diff 恰限定 BSHRC 面+SETTINGS/LAYOUT 内容零漂移、多簇旧件 CRLF 收编＋守恒差分、护栏三态）＋tests/bshrc.rs 集成 5（真进程边界全族＋拒形矩阵 7＋JSON 纯度）；基线 345→373。
- **审查修复（主线）**：usage 矩阵 set-空参用例环境解耦（--disk 注入）；BERRY_HOME 测试泄漏 RAII 治理（HomeGuard Drop 清 env＋临时目录——commands.rs「连环污染」纪律对齐）；README 护栏措辞补 BERRY_DATA_DISK 显式挂载残余；W4 帽口径注与根满文案三 note 记档。

## 裁决与判例

- **写方言≡读方言（构造性对拍）**：写者经 berry-fatfs（客机 fsmgr 同 crate）＋mkfat dev-dep 整盘字节恒等测试——比「写读两端实现互测」强一档（同一实现零对拍面）。
- **原子提交判例**：整盘内存编辑→同目录 temp+fsync+rename——宿主中断窗撕裂结构性不出现（FatErr::Torn 前车在册：任何撕裂中间态从不落在真名下）。
- **帽对齐律**：写端 fail-closed 帽（键 1..=32B/行 ≤126B/Entry ≤63/总量 ≤2048B 单簇/键值禁 CR·LF·NUL）逐条对齐 rc_load 截断面——「写得出但读端截断」的键值形结构性不可达（旧件遗留值除外——原样保留纪律，note 在档）。
- **在用护栏语义**：fsmgr eager 整读 FAT0＋根目录进 RAM——活跑中宿主写必被客机下次 commit 覆写；护栏扫 run 注册表 qemu_pid 探活（device/shot 拒、run 起核放行）；BERRY_DATA_DISK 显式挂载不可检＝文档化残余。

## 验证

- 宿主三层全绿：373 例（src 362+bin 1+tests 10）；canon 机查新文案 vs ROOTFS_MARKERS 59 项互非子串零交集；手跑烟测 rc 序 0/0/0/0/0/3 全合契约。
- e2e 两轮共盘（PATH=/apps 反证 miss＋PATH=/etc 正证命中 /etc/mtab）：脚本与判据在档（/tmp/bshrc-e2e.sh）；TCG 窗口敏感挂账低载窗复跑（判据独立可复执——宿主写面与客机读面的桥接由 berry-fatfs 同 crate＋mkfat 字节恒等＋crashlog 独立读三层构造性覆盖）。

## Related

- [BSHRC 宿主写者与 PATH 端到端刀实施设计](../../plans/2026-11-19-BSHRC宿主写者与PATH端到端刀实施设计.md)
- [2026-11-17-export内建与spawn-env面刀](2026-11-17-export内建与spawn-env面刀.md)（挂账 #3 收口）
- [存储布局与沙箱设计-v1.0](../../../docs/存储布局与沙箱设计-v1.0.md)（卷根 8.3 文件族——BSHRC.CFG 钦定位）
