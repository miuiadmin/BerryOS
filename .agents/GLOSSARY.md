# GLOSSARY — 术语表（种子）

本表统一工作区术语：**一概念一词**，用新词前先查、写进正文前先登记。条目格式：`**术语（英文）**：定义。_Avoid_: 易混叫法`；`_Avoid_` 只作命名指引，机器不查。

## 制度

**Curia（枢机院）**：本套文档管理制度的总称——两根布局、10 类文档、四句口诀与轻量自检脚本；定位为可移植到任何项目的文档管理方案。_Avoid_: 文档平台、知识库系统

**两根（two roots）**：受管文档的封顶布局——`.agents/` 装过程与过程产物，`docs/` 装外部事实、自由知识与现行标准。_Avoid_: 双目录方案

**四句口诀**：放哪的最后裁决——没做的→`plans/`、做完的→`research/`、为什么→`notes/`、过时的→翻状态或归档。_Avoid_: 目录规范

**家（home）**：类型对应的固定归属目录——`.agents/` 下五个（`plans/`、`research/`、`notes/`、`skills/`、`archive/`）；`docs/` 是 `standard` 的家。_Avoid_: 分类目录

**类型化文档（typed doc）**：带 `Type:` / `Status:` / `Updated:` 三行头块、按 10 类之一定义、住对应家或 `docs/` 的文档；`docs/` 只允许 `standard`，目录说明文件、笔记、`skill` 与 `docs/` 自由知识不适用。_Avoid_: 正式文档

**头块（header block）**：文档开头的固定字段行块——类型化文档三行固定（`standard` 另加 `Version:`），笔记两行（标题与 `Status:`），`skill` 用 frontmatter；`docs/` 自由知识无头块。_Avoid_: frontmatter（只有技能用）

**标准（standard）**：`docs/` 下带 `Type: standard`、`Version:` 与 `Status:` 的现行契约——设计规范、UI 设计系统、API/数据契约、编码约定、写作与排版约定；状态取 `current` / `historical` / `superseded`。_Avoid_: 规格（`spec` 住 `plans/`）、普通说明

**docs 自由知识（free-form docs）**：`docs/` 下没有 `Type:` 的文档——外部事实、接入说明、架构导览与 FAQ；无状态机，当前文件即真相。_Avoid_: 把所有 `docs/` 文档都叫无头文档

**标准候选（standard candidate）**：`--suggest-standards` 按文件名与头块线索列出的、可考虑升成 `docs/` `standard` 的自由知识；只是提示，升格由人确认，检查器不改写。_Avoid_: 准标准、待定标准

**契约版本（contract version）**：模块设计文档与 `docs/` 标准头块里的 `Version: MAJOR.MINOR.PATCH`——记这份文档自己的契约演进；与包版本、取代链各管一件事。_Avoid_: 文档版本、发布版本

**笔记（note）**：`notes/` 下的决策因果记录，头块两行，取值与所在 lifecycle 目录一致。_Avoid_: 日记、流水账

**桥（bridge）**：把 `.agents/` 暴露给各 CLI 的入口（`CLAUDE.md`、`.claude/skills`、`.codex/skills`）——`init` 在目标路径空闲时建符号链接，被占用时保留原物并打印手工命令。_Avoid_: 适配层、同步脚本

**工作区健康（health）**：`--health` 的只读体检——报告载荷齐全度、三条桥状态、`AGENTS.md` 合并态与 docs 标准/候选计数；不改文件，正常退出 0。_Avoid_: 自检报告、状态页

**本地补丁基线（local patch baseline）**：本地修改检查器前记录的 create-curia 版本与检查器 sha256（前 8 位），升级时用来区分上游改动与本地补丁。_Avoid_: 校验和、指纹

**技能（skill）**：`skills/<name>/SKILL.md`——按需加载的工作流；frontmatter 的 `description` 决定触发时机。_Avoid_: 提示词模板、脚本

**归档四步**：移动、加 `Archived:` 行、重写自身相对链接、修入链；第 3、4 步缺一步都会留下断链或静默错链。_Avoid_: 备份、冷存储

**取代（superseded）**：文档被取代后的终态；必须带 `Superseded-by:` 指向取代者，不得迁出。_Avoid_: 废弃、作废

**存量树只读纳管**：采用本规范时存量文档保持原样、规则不回溯、下次实质修改时自然补齐。_Avoid_: 迁移、批量修复

**原始语料豁免（raw marker）**：目录内放空标记文件 `.agents-raw` 即整目录免检，用于外部镜像等批量采集物。_Avoid_: 白名单、忽略目录

## 写作

**slop 清单**：写作自查的九类脏东西（重复规则、状态注解、推理记录等），见 [prose-standard](./skills/prose-standard/SKILL.md)。_Avoid_: 检查表

**三档示例法**：校准写作规则的方法——每条规则配「过度裁剪 / 平衡 / 过度展开」对照。_Avoid_: 正反例

## 图形与客户端 SDK

**场景图合成器（berry-compositor）**：统一执掌分层场景图、每客户端独占表面会话、离屏交换链合成与焦点/手势输入路由的单进程系统服务，合流并取代过渡期分离的 `guifbd` 与 `gui-wmd`。_Avoid_: 帧缓冲中继器、带裁剪服务

**表面交换链（Swapchain）**：按窗口实际目标像素宽高分配的 2–3 槽只读共享 VMO 轮转缓冲与显式 `Acquire/Release Fence` 同步机制。_Avoid_: 全屏单帧 VMO、行弹舱拷贝

**虚拟像素（vp）**：与物理屏幕密度解耦的唯一 UI 逻辑度量单位（`px = round(vp * density_q8 / 256)`），所有组件内在几何、间距、圆角与字号均以 `vp` 声明。_Avoid_: 物理像素硬编码、手乘密度常数

**预测性返回（Predictive Back）**：按「收起软键盘 → 关闭系统浮层 → 应用内导航栈跟手侧滑回退 → 根页面窗口平滑缩回桌面图标并转入后台冻结」执行的四级有序返回协议。_Avoid_: 外部直接 `DESTROY` 强拆窗口

**快照速启（QuickBoot Snapshot）**：宿主模拟器基于镜像内容哈希缓存桌面就绪基准 VM 状态、实现 `< 500ms` 瞬时亮屏唤醒并同步实时时钟的启动机制。_Avoid_: 每次冷跑固件自检
