# VER-01 版本管理体系调研与提案——业界版本号矩阵 × BerryOS 现状与分期落法（2026-11）

> 版本：**1.0**（v1.0）｜2026-11-08｜文件名 `VER-01-版本管理体系调研与提案-v1.0.md`
> **定位**：回答「我们有版本管理吗、全套吗」——业界 OS 版本号体系的结构是什么、BerryOS 现状盘点、缺口矩阵与分期提案。消费面：ADR 版本总纲候选、ADR #82 OEM 定制的配对面、SDK-2 文档面。
> **方法**：workflow 双路（A=业界网源深研 ‖ B=本地盘点，B 失败由主会话直核补齐）+ 主会话锚点验证（git tag/Cargo/boot banner/manifest 消费面直查）。
> **已知边界**：①Android 三页 devsite 正文截断——build id 构成与 VINTF 配对细节仅标题级佐证（GKI 分支命名表完整取得）；②Fuchsia API level 页全 404（站改版）——师承同门的最相关机制无一手，不作转述；③Apple 用维基（非一手）。若提案依赖这三点，需补抓。

## 问题

别人家的 OS 有「很细致的各种版本号」——内核版本、平台版本、API level、安全补丁级、build id 各管各的。BerryOS 有吗？全套吗？缺什么、该什么时候补？

## 结论（先给）

**BerryOS 有工业级的「项目过程版本」（文档双标 v主.次 / ADR 71+ / G 关卡 / 沿革 174 条 / 判例 146 则），但「系统产物版本」五轨道缺三整两半**——业界共识是五轨道模型（兼容承诺只挂单轨、跨轨由配对矩阵裁决），我们只有两条半轨有雏形、无闭环。这不是落后——pre-1.0 项目正常如此——但 **ADR #82 OEM 组装总纲刚落、SDK-2 在途，版本配对面是下一个被需要的地基**，本提案给分期落法：现在/1.0 前/v2 三档。

## 证据一：业界五轨道模型（A 路）

| 平台 | 版本号清单（一个 OS 同时维护的号） | 关键设计点 |
|---|---|---|
| **Linux**（https://www.kernel.org/releases.html） | mainline（9–10 周一版）→ stable（周更）→ LTS（现行 6.18/6.12/6.6/6.1/5.15/5.10，EOL 2026–2028） | **主版本号无兼容语义**（官方 FAQ：4.x→5.x 仅因「点后数字太大」）；兼容承诺只由「同一 LTS 分支」承载；发行版后缀（uname -r）是独立轨道上游不背书 |
| **Android** | 平台 API level + 代号 + build id + SPL + **内核独立轨**（GKI 分支 androidN-kernelX.Y：android12-5.10 … android17-6.18 完整在册）+ vendor HAL/SPL 矩阵 | GKI：内核兼容锚定 **KMI 冻结**而非版本号本身，内核可单独 OTA；出厂 = 平台 × 内核分支 × HAL 三方配对（VINTF 矩阵裁决，正文截断仅标题级佐证） |
| **Windows**（https://learn.microsoft.com/en-us/windows/release-health/） | 功能版本 YYH1/H2（26H2/25H2/22H2）+ 版本内月度质量更新 + 每版本 known/resolved issues 三件套 | 功能版本与质量更新分离；IT 按版本号查健康状态定部署窗口 |
| **Apple**（维基非一手） | 商品版本 macOS（Tahoe=26，年度+代号）‖ Darwin 内核版本（27.0.0）**双轨并行** | 内核轨道管系统接口、商品版管 SDK 目标；Darwin 9 起 iPhone OS 分叉、20 起对应 macOS 11+ |
| **SemVer**（https://semver.org/） | MAJOR/MINOR/PATCH；**1.0.0 = 公共 API 冻结起点**；+build 元数据明文「不影响优先级」 | 平台 OS 层几乎不用 SemVer（节奏不匹配）——它只管生态包轨道 |

**归纳（业界共识形态）**：①**内核轨道**（独立生命周期，Linux LTS / GKI+KMI）②**平台 API 轨道**（API level / 商品版，管应用 ABI）③**安全补丁轨道**（SPL / 月更，正交于功能版）④**构建标识**（无兼容语义，唯一定位一次产出，崩溃归因用）⑤**生态包轨道**（SemVer，包管理器消费）。**一次系统发布 = 五元组各取一值配对；兼容承诺只挂单条轨道，跨轨道组合合法性由配对矩阵裁决。**

## 证据二：BerryOS 现状盘点（主会话直核，锚在文）

| 轨道 | 现状 | 锚 | 判 |
|---|---|---|---|
| 内核/ABI | abi.toml 单源 nr 三十七在册 + stage v1/v2/v3 分期 + 五面生成（abi_gen/文档/syz/wake_gen/schema）+ abi-check 执法 | `tools/abi.toml`；`docs/规则/03-系统调用.md` | **半缺**：syscall 契约分期在册，但无内核版本身份（`rg kernel_version|uname kernel/` 零命中）、boot banner 无版本串（`main.rs:228` 仅 "BerryOS boot\n"）、tag 仅 G0/G1 两个里程碑标 |
| 平台 API | SDK_CURRENT 常量 + compat min 装载期检查（ADR #55 三值域） | `user/bins/svcmgr/src/main.rs:56,5516` | **半缺**：是 API level 雏形（布尔门槛式），但非单调 level 数、不进镜像标识、无 level 与 ABI stage 的联动表 |
| 安全补丁 | 无 | — | **缺**（pre-1.0 可接受；BCTS 承诺面立法〔ADR #82〕落地后必须有对应位） |
| 构建标识 | 无 build id、无 git hash 烙印；smoke kfile 字节对拍是隐性绑定（测试纪律非可查询版本） | `tools/smoke-image.sh` kfile 对拍 | **缺**：崩溃归因/镜像对账只能靠字节 diff |
| 生态包 | .bap manifest 有 version 字段但**零消费**（`rg man.version svcmgr` 空）；Cargo version 全 0.1.0 摆设 | `user/bins/*/manifest.toml`；`kernel/Cargo.toml` | **半缺**：格式在、语义无 |
| （项目过程） | 文档 v主.次 双标 110 件 / ADR 71+ / G0–G5 / 沿革 174 条 / 判例 146 | `docs/索引.md`；`docs/阶段沿革.md` | **有且强**——但这是过程版本，不是产物版本，两者勿混 |

## 提案：三档分期落法

**现在就该有（一刀可落，零争议）**
1. **构建标识**：boot banner 升级 `BerryOS <semver> (<git-short-hash> <dirty?>)\n`——版本常量经 build.rs 从 git 注入（Cargo version 顺手从 0.1.0 抬起真值）；镜像/boot 二进制/崩溃 dump 从此可归因。消费面：smoke 日志头、kfile 对拍显式化。
2. **tag 纪律**：每个 G 关 / 每收官里程碑打 `v0.x` tag（现 G0/G1 已是事实起点）——成本零、语义立。
3. **manifest version 消费闭环**：mkapp 门禁把 `version` 从摆设字段升为记录位（打印入 eco 行）——格式已在那儿，缺的只是一行消费。

**1.0 前（SDK 冻结窗）**
4. **平台 API level 单调化**：SDK_CURRENT 布尔门槛 → 单调整数 level（abi.toml 单源新位），.bap 记 `min_level`，装载期检查照旧——对齐 Android API level 形态而更简（无多 level 并存）。
5. **ABI 轨道身份**：ABI 文档抬头挂 `ABI_LEVEL`，与 syscall 号表冻结承诺联动（37/41/45 三数已是事实版本语义，补一个可引用的名字）。
6. **SPL 雏形位**：BCTS 承诺面（ADR #82）预留 `security_level` 字段——先占位后启用。

**v2（OEM/多设备时）**
7. **配对矩阵**：OEM 三包四层（#82）按五元组配对（VINTF 同构）——内核轨/平台轨/安全位/构建 id/包 SemVer 各取一值，BCTS 只测承诺面不变量扩版本配对断言。
8. **LTS 轨道裁决**：届时按生态压力定（Linux 模式 vs 单轨滚动——v1 无 fork、单仓单链，默认单轨）。

## 适用边界

- Android VINTF 细节/Fuchsia in-tree API level/Apple 一手为缺口三点——提案若进入 ADR 立法，先补抓（尤其 Fuchsia：与师承同门，其「API level 随提交递增 + compatibility testing」形态对 BerryOS 单仓节奏可能是最合身参照）。
- 本提案不落码不立 ADR——版本总纲 ADR 候选位：#83 起（#82 已占用）。
