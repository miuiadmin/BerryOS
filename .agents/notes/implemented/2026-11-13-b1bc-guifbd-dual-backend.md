# B1-b+c:guifbd 双后端——virtio-gpu 2D 控制队列主链 + ramfb 红线

Status: implemented

## Problem
显示加速体系 B1 刀 guest 半主体（02 号 §3.2/§6）：guifbd 单 ramfb 后端升双后端——virtio-gpu 设备在则走 2D 控制队列（协议 berry-gpuproto〔B1-a 冻结〕+ 传输 berry-virtio），不在则现役 ramfb 行为逐字节不变。结构性前置缺口：guifbd 的 `SpawnSpec::mmio_vmo` 单窗槽恒被 fb 窗占用，virtio-mmio 窗无进入通道。

## Decision
四件：
1. **交接协议**（窗进子空间的路）：svcmgr 预拉 guifbd 后经 `ChildHandles::hv`（子 root Vmar——常驻不 reap 句柄长存）把 H_MMIO 后映射进子空间 `VIRTIO_MMIO_VA`，完成经 stdin 通道发单字节 'w'（失败 'n'）；guifbd 首 syscall = 读该字节——**读窗先于交接 = D-abort 死路，读字节是序保证**。交接破缺/旧 svcmgr（feeder EOF）→ 缺省安全落 ramfb（版本混布防御；旧 guifbd 不读 stdin,字节滞队无害）。
2. **后端分派**：'w' → guifbd 自扫 32 槽（vprobe 形态,did=16+ver=2）——找到起 vgpu（INIT_SEQ 四命令:GET_DISPLAY_INFO→CREATE_2D→ATTACH_BACKING→SET_SCANOUT,SET_SCANOUT 殿后是 QEMU require_backing 序执法）,没找到零输出走 ramfb（红线构造保证:无设备相唯一新行为 = 1 次 stdin 读,无可见差）。
3. **backing = fb 窗同一物理段**：资源 backing 恰为 GOP fb 连续 DRAM 段（`attach_entry` 单 MemEntry）——blit/蜂格/px() 读回路径零改动,ramfb 与 vgpu 双设备同屏一致；帧提交 = 全帧 TRANSFER_TO_HOST_2D+RESOURCE_FLUSH（v1 全帧起步,per-rect 同构 Linux virtgpu_plane.c 挂 v2——如实注）;传输统一走 INDIRECT 描述符表（`chain_put`,QEMU 侧 indirect_desc 默认 on〔S1 §2.1〕）;提交通道一次失败即停刷（`dead` 位——fb 窗同段 ramfb 扫描保屏活,坏设备不杀呈现链）。
4. **guifbd lib/bin 双面化**（gui-inputd 先例）:lib = 纯面（分派/链构造/段切/提交序,host `cargo test --lib` 7 测）;berry-libc 按目标门控;main.rs 薄壳 + guest.rs（981 行现役面零重排迁入）。

伴生：berry-virtio 增 `DESC_F_INDIRECT`/`FEAT_INDIRECT_DESC`/`mmio::DEVICE_ID_GPU`（值单源 berry-gpuproto 跨工作区 path 引常量,B1-a 头注预设形态）+ host 钉板 2 测;vprobe did=16 白名单行（S1 §6-1 前置项——不挂即挂 gpu 的 smoke 因 `VIRTIO ok` 缺席 FAIL）。

## Alternatives considered
- **svcmgr 自探（否）**:svcmgr 是 boot 手搭进程,无 self-vmar handle（自描述对只在 spawn_rootfs_elf 面给子）,读不了窗;内核探（否）:kernel 域 owner 占用禁碰,且 02 号「进程内后端」本意即 guifbd 自扫。
- **argv/fbinfo 第 7 域传探针结果（否）**:argv 不载 handle,窗仍无通道;fbinfo 写入需内核改。
- **第二 mmio_vmo 字段（否）**:SpawnSpec 加字段 = svcmgr 29 处 + bsh 8 处构造点全破,宽爆面。
- **H_MMIO 经 env gift（否）**:窗 VMO 无 TRANSFER 位（dup 只紧不松）,env 通道结构性载不了窗——mmio_vmo 的 map-不-take 机制是唯一窗通道。
- **per-page backing 表 v1 就做（延后）**:fb 窗连续段单 entry 已足,8KB 级表属 v2 动态分辨率（非连续 anon VMO backing）面;INDIRECT 作为统一传输面已实装实测。
- **vgpu 起链失败即 trap（否）**:现役 console 管随 guifbd 死——单行诊断 + ramfb 保活 + `FB ok vgpu` 缺失 = gpu 相 FAIL loud,红线更稳。

## Related
02 号 §3.2/§6（research/显示与加速体系设计/02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md）;05 号 S1 实录（三陷阱+force-legacy+indirect_desc 读数）;ADR #50（HDI 三口换实现不换契约——本刀零协议改动）/#61（env 8/8 满教训——进程内后端+交接协议正是其解）;B1-a note（协议 crate）;gui-inputd（lib/bin 双面先例）;vblk（virtqueue/协商/DMA 先例）。验证实录:双态 boot（A:`FB ok vgpu slot=31 pmode=1280x800`+VNC 800×600 白字黑底全帧收帧;B:标记集恰减 vgpu 行零漂移）+ smoke-image 全绿。
