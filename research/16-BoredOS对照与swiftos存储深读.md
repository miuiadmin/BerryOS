# BoredOS 对照与 swift-os 存储源码深读（第五层）

> 调研日期 2026-11-08 · 方法：BoredOS（GPL-3.0，**只对照理解不搬码**）与 swift-os（Apache-2.0，可借语义）完整 clone 源码实读；对照基准 = BerryOS G4 demand paging / VMO-pager 路线与 P0-b 存储吸收议程 · 姊妹篇：12–15 号
>
> 本文为 16 号专题。**过程事故记录（方法节自警）**：并行 clone 的 shell 优先级事故——`cd dir && clone A & clone B &` 中 B 在原 cwd 执行，swift-os 曾误落 berryos 仓根（未跟踪态即发现，验证后移出，`git status` 复核零残留）；教训 = 后台并行 clone 一律 `(cd dir && clone) &` 子壳形态或显式目标路径。
> 编号互注（2026-11-09）：本件与 research/调研报告/16-新生代OS设计初衷.md 同号不同系（主目录专题 vs 子目录进化史）——引用带全路径消歧。

## 1. BoredOS——C 系现代 Unix 的内存面量级与形态（对照理解）

- **量级锚点**：现代 Unix 内存子系统全套（pmm/vmm+vma/mmu/slab/pagecache/radix_tree）≈ **4,055 行 C**（vmm 784 + mmu 785 + slab 782 + pmm 472 + pagecache 411 + radix 347 + vma 248）。
- **形态 = Linux 缩微**：内核 pagecache + radix tree 索引（`mem/pagecache.c`/`radix_tree.c`）——与 BerryOS 的 **VMO + 用户态 pager** 路线是两种范式：BoredOS 把页缓存放内核（file-backed 缺页内核内闭环），BerryOS 把 pager 放用户态服务（内核只做机制面）。对照价值：两条路线的复杂度落点不同——BoredOS 的复杂度在内核数据结构，BerryOS 在 IPC/pager 协议。
- **栈自动扩展守卫**（`vmm_handle_page_fault`）：VMA 未命中且下邻 VMA 是栈 → 检查 `rsp-128` 红区（SysV ABI 红区）+ `MAX_STACK_SIZE` 8MiB 帽 → 升写锁扩展。**红区守卫**（向下探栈的合法窗口）是 BerryOS 用户栈扩展面（v1 RAM overlay 尚无）将来可对照的细节。
- **COW**：fork 时 `mmu_clone_user_cow` + `VMA_FLAG_COW` + `PT_COW`（软件位 bit 9）；BerryOS v1 无 fork，此面为 v2+ 对照。

## 2. swift-os——存储三层与签名更新的源码级设计（可借语义，Apache-2.0）

### 2.1 datafs「现代极简主义」（`kernel/fs/datafs.swift` 头注实读）

- 4KiB 块；**固定 inode 表**（128B/inode：类型/父 inode/内联名/size/mode/mtime/单索引块）；**目录无数据块——子项 = parent 指向我的 inode**，lookup/readdir 即扫小 inode 表；单间接索引 1024 指针 → 单文件 4MiB 帽（后续里程碑加二级间接）；位图**内存持有、变更写穿**（SQLite 页翻新可回收空间）。
- **多卷化**：全局态收进 `DfsVolume` 记录、API 按卷槽索引——slot 0 = 持久 /data 盘，后槽绑额外可写盘（Hetzner Volume 实例）。
- **与 BerryOS 同构互证**：「目录即 inode 扫描」= BerryOS 存储设计的「**树即登记簿**（ls /apps = 安装表，无第二本账）」——两个独立项目收敛到同一「扁平登记表优于层级目录结构」判断；datafs 的 4MiB 单文件帽与 BerryOS FAT 8.3/16 项根目录同为「先做对再做全」的量级取舍。

### 2.2 签名双槽更新的信任边界（`kernel/fs/swosboot.swift`/`updatestore.swift`——喂 P0-b）

- 双槽各持**完整 Ed25519 签名镜像**；内核验签后挂活跃槽镜像，验签失败拒挂。
- **精妙的信任边界（原文实读）**：manifest **只 CRC32 保护、不签名**——内核只持**公钥**（不能自签）；manifest 的角色被严格限定为「**在自认证的签名镜像之间选择**」，不是信任锚——攻击者能改 manifest 造成的最坏结果 = 换选另一个**仍有效签名**的旧槽（可用性/DoS 面），**永不**是执行未签名代码。
- **对 P0-b 的直接输入**（存储 §8.5 议程的签名 base 落地形态）：签名对象 = **镜像本体**而非元数据清单；选择器（指针/manifest）不需要被保护到密码学强度，只要被选者的真伪由签名自证——这比「全链签名」省一层密钥管理且信任面更小。BerryOS `sys/boot` 双目录 + 指针（ADR #47）叠加此形态：指针非信任锚、槽镜像自认证、验签失败拒挂。

## 3. 汇总：给 BerryOS 的三件输入

| 输入 | 来源 | 去处 |
|---|---|---|
| 签名镜像双槽 + 选择器非信任锚 | swift-os swosboot（Apache-2.0，语义可借） | P0-b 议程落地形态（`存储布局与沙箱设计` §8.5 增补） |
| datafs 扁平登记表/量级取舍互证 | swift-os datafs | 设计互证记录（无需改码——BerryOS 已同构） |
| 红区守卫的栈扩展细节 | BoredOS vmm（GPL，只对照） | v2+ 用户栈扩展面的对照清单 |

## 参考

- BoredOS: `mem/{vmm,pagecache,radix_tree,mmu,slab,pmm,vma}.c`（GPL-3.0——仅对照理解）
- swift-os: `kernel/fs/{datafs,swosboot,updatestore}.swift`、`kernel/net/*`（Apache-2.0）
- BerryOS 对照：G4 草案（VMO-pager）、`存储布局与沙箱设计` §7/§8.5、ADR #47
