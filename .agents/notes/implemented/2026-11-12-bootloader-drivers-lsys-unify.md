# Agent Note: bootloader/drivers L-SYS 统一 0.2.0 + 规则/14 版本管理域文件入位（版本治理步骤 2+3）

Status: implemented

## Problem

版本治理迁移步骤 1（user/ workspace 41 crate 继承统一，[2026-11-12-user-workspace-version-inherit](2026-11-12-user-workspace-version-inherit.md)）收口后，L-SYS 仍有两处漂移：bootloader/（berryboot app/berry-efi + berrydump）与 drivers/（5 member）全部字面 0.1.0，与根/user 的 0.2.0 差一个版本；同时治理方案的文档规范步（三轨体系入 docs/规则/）未落——版本纪律散在标准 v1.0（五轨道）与治理方案 v1.0（三轨/workspace 统一）两文，规则/ 域文件家族没有版本域入口，动版本号前无路由可查。

## Decision

- **berryboot/berrydump 走字面版本，不走 workspace 继承**：两处是 EFI 静态库小 workspace（各 1–2 member），加 [workspace.package] 继承制收益趋零；berry-efi 更被 berrydump 跨工作区 path 依赖，字面值对任何消费方零歧义。任务书「无 workspace 继承」按此落——member 字面 version = "0.2.0"，升版逐 crate 手同步（合计 3 件）。
- **drivers/ 复用步骤 1 先例**：drivers/Cargo.toml 新增 [workspace.package] version = "0.2.0"，5 member（berry-virtio + bins/ 四件）改 version.workspace = true——drivers member 无外部跨工作区消费者（只出不进），继承制零风险；只迁 version，edition 不随刀。
- **berrydump 随批入刀**（任务书只点名 berryboot）：步骤标题是「bootloader/ 版本统一」，留 berrydump 0.1.0 即留漂移；其唯一依赖 berry-efi path 无版本钉，升值零连锁。
- **规则/14 用三轨口径而非五轨道口径**：域文件是铁律蒸馏层不是标准副本——五轨道全文留标准 v1.0（Type: standard，ADR #85），14 号文件落三轨（L-ABI/L-SYS/L-APP）+ L-SYS 同值 + 升版门 + 消费面对拍 + 禁手设版本五条，深读指回标准与治理方案（一事实一归属）；三轨与五轨道的映射在文内一句话钉死。
- **禁手设版本的例外台账如实写全**：任务书只列 hexyl/uu，但实勘 user/bins/ 未收编独立小工作区 21 件 + tools/berry 现仍 0.1.0——例外只写 hexyl/uu 会让规则落地即被现状证伪；写成「未收编独立小工作区台账在案、只减不增、收编随后续刀」。

## Alternatives considered

- **berryboot 也上 [workspace.package] 继承**：否决——跨工作区消费（berrydump 引 berry-efi）下继承解析依赖 berryboot 自身 workspace 根，字面版本把这层歧义整个消掉，且任务书明示无继承。
- **顺手收编 user/bins/ 21 件独立小工作区**：否决——超出步骤 2 范围、涉 user/ 域并行会话面（berry-ksproto 是兄弟 WIP 明确不碰），台账化挂后续刀。
- **规则/14 直接抄五轨道总表**：否决——与标准 v1.0 双写，标准演进即漂移；域文件只钉铁律 + 指针。

## Consequences

- L-SYS 全仓同值 0.2.0：五 workspace（根 27 + user 41 + berryboot 2 + berrydump 1 + drivers 5 共 76 包）cargo metadata 全 0.2.0；berryboot/berrydump check（none target）与 drivers 全员 check（musl target）绿。
- berryboot/Cargo.lock 随批刷（berry-crypto/berry-loader/berry-manifest 0.1.0→0.2.0——步骤 1 后首刷）；berrydump/drivers lock 同理。
- docs_guard G10 既有红（上笔治理方案提交漏升索引计数 15→16/117→118）随批机械修复复绿——非本刀引入但挡验收门。
- 遗留台账：user/bins/ 独立小工作区 21 件 + tools/berry + berry-ksproto（兄弟 WIP）仍 0.1.0；治理方案迁移路径余 uname 闭环步与 1.0 三同步门。

## Related

- 步骤 1 前篇：[2026-11-12-user-workspace-version-inherit](2026-11-12-user-workspace-version-inherit.md)
- 立法正身：[docs/版本管理标准v1.0.md](../../../docs/版本管理标准v1.0.md)（ADR #85 五轨道）· 域文件：[docs/规则/14-版本管理.md](../../../docs/规则/14-版本管理.md) · 方案：[docs/全局版本号治理方案-v1.0.md](../../../docs/全局版本号治理方案-v1.0.md)
