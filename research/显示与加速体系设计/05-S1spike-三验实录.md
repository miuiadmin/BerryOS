# 05 号：S1 spike 三验实录——virtio-gpu 设备/传输/HVF 兼容（刀 3）

> 实录日期：2026-11-13。服务对象：02 号 §6 刀序 S1（三验合一，到期未跑已升格）——本文即 S1 的实测收口档。探针环境：Apple M3 Pro / 18GB / hw.pagesize=16384 / QEMU 11.0.3 Homebrew（`/opt/homebrew/bin/qemu-system-aarch64`，与 02 号 §来源索引本机实证同机同版本）。方法：全探针落 `/tmp/s1/`（脚本 `virtio-gpu-probe.sh` + 辅助 `vnc_probe.py`，实录重放版已固化）+ 现役产物（kernel = `~/.cargo-target/aarch64-unknown-none/debug/berry-kernel`（2026-11-13 04:54 构建，版本串 `0.2.0 (1eb45284-dirty … dbg)`）；rootfs = 仓库根 `rootfs0/1.cpio`（04:38）；UEFI 相 = `out/berryos.img`（03:57，`v0.2.0-176-g260b63aa-dirty`）+ `/opt/homebrew/share/qemu/edk2-aarch64-code.fd`）——**零仓内代码改动**（本文与 README 索引行外）。命令形态参照：`tools/smoke.sh` :598-605（rootfs 相）+ `tools/berry/src/qemu.rs` :133-150/:195-278（machine_args/spawn_image）。

---

## §0 结论速览：S1 判定 = **go**（三验全过，附 B1 输入清单）

- **验①设备在：过。** `-device virtio-gpu-device`（mmio 形）挂上 QEMU 建立无错（dumpdtb rc=0）；guest 侧串口实收 `vprobe: slot 28 did=16（未知设备类）`（TCG 相 serial.log :516）——**枚举对未知设备 id 非静默**：vprobe 打「未知设备类」行后 loud trap 只死自己（`drivers/bins/vprobe/src/main.rs` :89-92），boot 继续走完（VBLK ok slot=31 / NET ok slot=30 / BLKD ok / APPUI ok 全在）。四设备槽位排布 31→28（blk/net/tablet/gpu，倒序建总线，与 vprobe 头注 :10-12 互证）。附：FDT 探针证伪——virt 板 dtb 恒含 32 个 virtio_mmio 节点，带不带 gpu 零差异（§2.3），设备在席的真证据只在 mmio 寄存器面。
- **验②QEMU 显示侧：过。** `-display none -vnc 127.0.0.1:0` 下 QEMU 进程稳定（40s+ 观察窗 ×4 次实跑，全程零自行退出）、`qemu.log` 全程空（无 GPU 错误行）、VNC RFB 探活三问全应（banner `RFB 003.008` + 版本交换 + security 应答 `\x01\x01`，`/tmp/s1/vnc_probe.py`）。无 guest 驱动时 VNC 无帧属预期——本轮验的是传输面存活，与任务书口径一致。
- **验③HVF 兼容：过（双载体）。** kernel 直启相：HVF+gpu 与 HVF 无 gpu 对照**同位同形态 fatal**（§4.1，PC/FAR/EC/ISS 逐字节同）= gpu 零增量影响；fatal 本体是**既有覆盖缺口**（copy_to_user→未映射低地址，§5.5）。UEFI 相（AAVMF+ramfb+gpu 共存，02 号 §S1 ①原义形态）：**双 boot 全过**——`ktest done` ×2（两份 serial.log 同在 :86）+ 各 40s 稳定窗 + VNC ALIVE ×2 + qemu.log 空 ×2。
- **对 B1 刀的输入（§6 全文）**：① vprobe 必须先加 GPU=0x10 白名单行（否则挂 gpu 即 smoke FAIL——`VIRTIO ok` 因 trap 缺席）；② `-global virtio-mmio.force-legacy=false` 是硬前提（默认 legacy 下 gpu 槽 version 报 1，vprobe 先在 ver 断言死——§5.4 实测）；③ 设备名只用 `virtio-gpu-device`，`virtio-gpu` 是 PCI 别名陷阱（§5.1）；④ QEMU 设备侧 `indirect_desc` 默认 on——02 号 §3.2 坑①（ATTACH_BACKING 8KB 表走 INDIRECT）的宿主侧前提成立；⑤ 意外发现：HVF+kernel 直启+rootfs 相有既有 fatal（与 gpu 无关，§5.5），建议另立挂账刀。
- **未覆盖面（如实）**：02 号 §S1 ②原义的 D-Bus（`-display dbus,p2p=on` + ScanoutMap 共享图 fd/memfd 文件后备核实）本轮未动——任务书将传输验缩为 `-display none + -vnc` 增量可行性口径；D-Bus 面连同 memfd 核实仍挂账（02 号 §7 风险两条维持 [中]），排 B1-b 前置或 S1 补充刀。

## §1 探针命令全文（可重放，`/tmp/s1/virtio-gpu-probe.sh` 同源）

### 1.1 kernel 直启相（验①②③-载体一，仿 smoke.sh :598-605 + gpu 追加）

```bash
qemu-system-aarch64 -machine virt -accel <tcg|hvf> -cpu <cortex-a72|host> -m 128M \
  -kernel ~/.cargo-target/aarch64-unknown-none/debug/berry-kernel \
  -fw_cfg name=opt/berry/rootfs0,file=<repo>/rootfs0.cpio \
  -fw_cfg name=opt/berry/rootfs1,file=<repo>/rootfs1.cpio \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  -device virtio-gpu-device,id=gpu0 \
  -display none -vnc 127.0.0.1:0 \
  -monitor none -serial file:serial.log
```

盘 = `tools/mkfat.py` 现场造 + LBA0 印章 `BERRYOSVBLK!`（照 smoke.sh :570-571，vblk 自验对象）。

### 1.2 UEFI 相（验③-载体二 + 02 号 §S1 ①「AAVMF/ramfb 共存」原义，仿 qemu.rs spawn_image）

```bash
qemu-system-aarch64 -machine virt -accel hvf -cpu host -m 256M \
  -display none -vnc 127.0.0.1:0 -monitor none -serial file:serial.log \
  -drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -drive if=pflash,format=raw,file=vars.fd \
  -drive if=none,file=out/berryos.img,format=raw,id=bb,snapshot=on \
  -device qemu-xhci -device usb-storage,drive=bb \
  -global virtio-mmio.force-legacy=false \
  -device virtio-gpu-device,id=gpu0 -device ramfb \
  -fw_cfg name=opt/berry/display,string=desktop \
  -fw_cfg name=opt/berry/accel,string=hvf
```

vars = 64MiB 0xFF 现场造（`qemu::create_vars` 同法，`tools/berry/src/commands.rs` :206 的 64MiB 档）；`opt/berry/accel` 判别键必须注（§5.6）；`snapshot=on` 盘零写。

### 1.3 FDT/设备面探针

```bash
qemu-system-aarch64 -machine virt,dumpdtb=gpu.dtb -display none -monitor none \
  -global virtio-mmio.force-legacy=false \
  -netdev user,id=n0 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 -device virtio-gpu-device,id=gpu0
```

（dumpdtb 相挂 blk 会因缺 `drive=` 被拒——见 §5.2 陷阱；dtc 反编译对拍。）

## §2 验①设备在——实测

### 2.1 QEMU 侧：设备建立无错

- 定名（P0，`/tmp/s1/p0/device-help.txt`）：`-device help` 出 `name "virtio-gpu-device", bus virtio-bus`（mmio transport 形，正名）+ `name "virtio-gpu-pci", bus PCI, alias "virtio-gpu"`。
- dumpdtb 带 gpu rc=0、stderr 零行（P1）；`-device virtio-gpu-device,help` 关键默认：`edid=on` / `xres=1280` / `yres=800` / `max_outputs=1` / `blob=off` / **`indirect_desc=on`** / `hostmem=0`（`max_hostmem=268435456`）/ `x-disable-legacy-check=off`——02 号 §来源索引实测行（edid on/xres 1280/max_outputs 1/blob off）四项复核一致，新增 yres/indirect_desc/hostmem 三读数。

### 2.2 guest 侧：串口枚举行（最强证据）

TCG 相实收（`/tmp/s1/p2-tcg/serial.log`，行号为实录）：

```text
1  : 1BerryOS boot 0.2.0 (1eb45284-dirty rustc1.97.0 dbg)
5  : ROOTFS ok src=fwcfg bytes=34967552
119: VIRTIO vmo=0000000000010004          ← boot 授能 32 槽窗（kernel/src/main.rs :1449）
200: SSSSSSSVBLK ok cap=2048 slot=31      ← blk 落 slot 31（最高槽，首个 -device）
206: SSSWWWdNET ok ping=32 slot=30        ← net 落 slot 30
516: vprobe: slot 28 did=16（未知设备类）  ← gpu 落 slot 28：设备被枚举到，loud trap 只死自己
530: APPUI ok n=8                          ← boot 继续走完（vprobe 死不扰链）
```

判定链：vprobe 扫 32 槽读 mmio 头（`drivers/bins/vprobe/src/main.rs` :62-93），未知 did 走 :89-92 打行 + trap——**回答任务书预问「现役内核不识 gpu 也会枚举出未知 virtio 设备？」：会，且非静默**（打 `did=16` 行后才死，`VIRTIO ok` 标记因 trap 缺席 = smoke 兜底 FAIL 面，这正是 B1 输入①）。槽位几何：0x0A00_0000 + 28×0x200 = **0x0a003800**（`arch/aarch64/src/lib.rs` :74 VIRTIO_MMIO0_PA + `user/berry-exec/src/lib.rs` :166-168 SLOTS=32/STRIDE=0x200 与 QEMU 板几何互证）。

### 2.3 FDT 探针：证伪（重要修正）

带 gpu 与不带 gpu 的 dumpdtb 各 32 个 `virtio_mmio@` 节点（0x0a000000–0x0a003e00 步 0x200），`diff` 唯一差异 = `rng-seed`/`kaslr-seed`（每 boot 随机）——**virt 板 dtb 的 virtio_mmio 32 节点是静态常量，与挂载设备无关，FDT 对「gpu 在席」零鉴别力**。任务书预问的兜底口径「若静默跳过→验=QEMU 侧无错+mmio 区在 dtb」中，dtb 只证 32 槽窗几何在席（这个几何与仓内常量互证成立），设备在席证据必须走 mmio 寄存器面（§2.2 已到手）——B1 的探测面维持 vprobe 寄存器扫路线，勿投 FDT。

## §3 验②QEMU 显示侧——实测

- 进程稳定：TCG 相 1 次 + HVF kernel 相 2 次 + UEFI 相 2 次，每轮 40s 观察窗（`kill -0` 5s 粒轮询）零自行退出；TCG 相从 boot 到收口全程 >6 分钟在跑。
- 日志无 GPU 错：五轮 `qemu.log` 全部空文件（QEMU stderr/stdout 零输出，无 `virtio_gpu` 错误行）。
- VNC RFB 探活（`/tmp/s1/vnc_probe.py`，三问：connect → banner → 版本交换 + security 应答）：`banner=b'RFB 003.008\n'` + `security-reply=b'\x01\x01'` + ALIVE——kernel 相（无任何 guest 显示驱动）与 UEFI 相（桌面在跑）同应答，传输面存活。
- 无帧属预期的自证：kernel 相无 guifbd/GUI，VNC 仍协议应答——「server 在而帧面空」与「设备挂上不扰 boot」双证。

## §4 验③HVF 兼容——实测（双载体）

### 4.1 kernel 直启相：对照法钉死「与 gpu 无关的既有 fatal」

HVF + gpu（`p3-hvf-b1`）在 rootfs 相中段 fatal：

```text
225: policy: reject w=+200 -> clamp +64 (envelope ±64)   ← AI 调度 demo 合法输出（kernel/src/main.rs :4043），非错误
227: === EXCEPTION: sync (current-EL SPx) ===
      EC=0x25 (D-abort current)  ISS=0x47
      PC =0xffffff804004bfdc → nm 定位 = memcpy+0xa4（LR = syscall::copy_to_user+0x98）
      FAR=0x0000000000020fd8   ← 未映射低地址
264: -> fatal, halt
```

**去 gpu 对照**（`ctrl-hvf-nogpu`，唯一差异 = 无 `-device virtio-gpu-device`）：行 225/227/264 **同位同形态**（EXCEPTION 段 diff 仅 BENCHFA 计时/rt_ticks/x01/x08/x16/x17/x25 时间戳类读数，PC/FAR/EC/ISS/SP 逐字节同）——确定性既有故障，**gpu 零增量影响**。归因：内核 `copy_to_user`（memcpy）写未映射用户地址 0x20fd8 → EL1 D-abort → fatal；TCG 同命令全程绿（§2.2），故为 **HVF 加速面差异暴露的既有缺口**（smoke 全 TCG、berry run 全 UEFI——kernel 直启+rootfs 相+HVF 组合现役零覆盖）。挂账建议见 §5.5。

### 4.2 UEFI 相（AAVMF+ramfb+gpu 共存）双 boot：全过

| boot | 关键行 | 稳定窗 | VNC | qemu.log |
| --- | --- | --- | --- | --- |
| #1（p4-uefi-b1-retry） | serial.log :24 `BerryOS boot`、:86 `ktest done` | 40s 活 | ALIVE | 空 |
| #2（p4-uefi-b2，全新 vars） | 同上 :24/:86 | 40s 活 | ALIVE | 空 |

4K guest 页 × 16K 宿主页（HVF stage-2）兼容收口：镜像内核全 ktest 过（含 FP/q 断言族，FPSR 断言经 `opt/berry/accel` 键跳过——§5.6 判例自证）；`ramfb` 与 `virtio-gpu-device` 并挂 realize 无冲突、boot 无扰。**02 号 §S1 ③「HVF+virtio-gpu 双 boot 断言」以 UEFI 载体兑现**。

## §5 发现清单（排坑实录）

1. **设备名陷阱**：`virtio-gpu` 是 `virtio-gpu-pci` 的别名（`-device help` 原文 `alias "virtio-gpu"`）——virt 板上挂它设备会落 PCI 总线而非 virtio-mmio 32 槽窗（guest vprobe 扫不到、 berry-virtio mmio 路线全盲）。**正名 `virtio-gpu-device`**（bus virtio-bus）。B1/QEMU 命令面与文档一律用全名。
2. **dumpdtb 相的 blk 陷阱**：`-device virtio-blk-device` 不带 `drive=` 时 QEMU 拒绝（`drive property not set`）退出——FDT 探针的设备面用 net/tablet/gpu（无外部资源依赖）。
3. **FDT 零鉴别力**（§2.3）：virt 板 32 节点静态常量；设备在席证据只在 mmio 寄存器面。任务书「mmio 区在 dtb」预期降级为「窗几何互证」用途。
4. **legacy 陷阱（B1 硬前提）**：默认 `force-legacy=true` 下 gpu 设备 realize 无错（dumpdtb rc=0）但 guest 侧槽头 version 报 1——TCG 实测 `vprobe: slot 30 ver=1（占用槽须 2）`（`/tmp/s1/p5-legacy/serial.log` :581，ver 断言先于 did=16 断言触发）。`-global virtio-mmio.force-legacy=false` 必带（gui-inputd 判例同款，smoke.sh :560 注释同源；berry 桌面链 qemu.rs :239-241 已带）。
5. **HVF+kernel 直启+rootfs 相既有 fatal（意外发现，非 gpu 归因）**：§4.1 的 copy_to_user→0x20fd8 EL1 D-abort fatal，带不带 gpu 同位同形态。**现役覆盖缺口**（smoke 全 TCG / berry run 全 UEFI，kernel 直启+rootfs 相+HVF 三者组合无人跑过）+ 02 号 §4.3「残余风险类 = HVF 保存/恢复面缺陷」判例族的疑似新成员（FPSR 同族但不同症：FPSR 是 vmentry 寄存器面，本例更像 stage-2 可见性/缓存面或指令结果差异——需专门刀归因，本文只钉「与 virtio-gpu 无关」）。建议：另立挂账刀（复现命令 = §1.1 去 gpu 版，产物已存 `/tmp/s1/ctrl-hvf-nogpu/`）；S1 判定不受其影响（UEFI 载体全绿）。
6. **FPSR 判例键自证**：UEFI 相漏注 `-fw_cfg name=opt/berry/accel,string=hvf` 时镜像内核 `KERNEL PANIC——FP：MAIN 的 FPSR 未恢复`（ktest.rs :2875，`/tmp/s1/p4-uefi-b1/serial.log` :36-39）；补注后 ktest done。qemu.rs :250-265 判例与 fw_cfg 键机制双自证——探针手拼 QEMU 命令时的必带项清单又一条。
7. **QEMU 设备侧 INDIRECT 已开**：`indirect_desc` 默认 on（§2.1）——02 号 §3.2 坑①（ATTACH_BACKING 的 8KB mem_entry 表超单页 virtqueue，解法 ①INDIRECT）的**宿主侧前提实测成立**，guest 侧 berry-virtio 落地即用，无需 QEMU 配置动作。
8. **探针工程坑（给后来者）**：`tools/paths.sh` 的 `cargo metadata` 依赖 cwd 在仓库（在 /tmp 跑报「cargo 不可用」）——探针脚本 cd 仓库后解析或绝对路径兜底（重放版已固化）。

## §6 对 B1 刀的输入

1. **vprobe 白名单行先行（B1-a 前置或同刀）**：`drivers/bins/vprobe/src/main.rs` :84-92 加 `else if device_id == 0x10 {}` 分支（或 berry-virtio `mmio` 表 `DEVICE_ID_GPU: u32 = 16` 后 vprobe 认领，02 号 §3.1 已列 `lib.rs :230-237` 增一行）——否则任何挂 gpu 的 smoke run 因 `VIRTIO ok` 缺席而 FAIL（§2.2 实测 trap 链）。**回答任务书预问：guest 驱动要不要先加未知设备枚举行？——要，且它是 B1-a 的第一 commit**。
2. **QEMU 命令面三硬项**：`-device virtio-gpu-device`（全名，勿用别名）；`-global virtio-mmio.force-legacy=false`（§5.4）；HVF 档带 `-fw_cfg name=opt/berry/accel,string=hvf`（berry spawn_image 已带，手拼/CI 复刻时勿漏）。挂载顺序上 gpu 追加在既有设备后 = 落 slot 28（0x0a003800），既有设备槽位零漂移（tablet=29/net=30/blk=31 实测）——但勿绑死槽号（vprobe :10-12 纪律）。
3. **传输验残留**：D-Bus p2p + ScanoutMap/memfd 面未测（§0 未覆盖面）——排 B1-b（壳半）前置探针或 S1 补充半刀；VNC 增量矩形实收（β-0 载荷）也待 B1-a guest 驱动就位后才有帧面（本轮无 guest 驱动，VNC 无帧属预期，传输面存活已证）。
4. **多显示面 console 归属（新开放问题）**：UEFI 相 ramfb 与 gpu 并挂时 VNC（`-vnc :0`）绑哪个 console 未测（本轮 monitor 关闭，桌面 GUI 正常跑 = ramfb 面在用）——B1 排布显示设备顺序时需定 QEMU console 绑定序（推断：QEMU 按 `-device` 声明序绑 console 0，gpu 在 ramfb 前声明则 VNC 面向 gpu——需 B1-b 实测定谳）。
5. **kill 线不触发**：02 号 §6 S1 行的 kill 线（同脏区帧耗 > 软渲染 2×）是 B1 后性能关，本轮无帧面不适用；降档线（D-Bus Map 不可用）也未测——两条线原样保留给 B1。

## §7 产物与复现

- 探针脚本（实录重放版，含全部阶段函数 + 实录读数头注）：`/tmp/s1/virtio-gpu-probe.sh`；VNC 探活器：`/tmp/s1/vnc_probe.py`。
- 各阶段产物：`/tmp/s1/p0/`（设备帮助面）、`p1/`（两份 dtb/dts + diff）、`p2-tcg/`（TCG 全程 serial.log，did=16 在 :516）、`p3-hvf-b1/` 与 `ctrl-hvf-nogpu/`（fatal 对照双档）、`p4-uefi-b1/`（FPSR panic 档）+ `p4-uefi-b1-retry/` + `p4-uefi-b2/`（双 boot 过档）、`p5-legacy/`（legacy 陷阱档）。
- 复现：`bash /tmp/s1/virtio-gpu-probe.sh [p0|p1|p2|p3|p4|p5]`（全阶段约 40 分钟，TCG 相最慢；kernel/rootfs 产物用现役路径，重建后读数行号会漂）。

## 来源索引

**本地（file:line）**：`drivers/bins/vprobe/src/main.rs` :10-12（倒序建总线/勿绑槽号）、:62-93（32 槽扫描 + 未知 did loud trap）、:108-111（`VIRTIO ok` 单消息原子标记）；`drivers/berry-virtio/src/lib.rs` :200-238（mmio 寄存器表 + 设备 ID 族——B1 增 DEVICE_ID_GPU=16 落点）；`arch/aarch64/src/lib.rs` :74（VIRTIO_MMIO0_PA=0x0A00_0000）；`user/berry-exec/src/lib.rs` :153-168（VIRTIO_MMIO_VA/SLOTS=32/STRIDE=0x200）；`tools/smoke.sh` :558-561（force-legacy=false 注释）、:598-605（rootfs 相命令形态）、:637-643（VIRTIO ok 标记链）；`tools/berry/src/qemu.rs` :133-150（machine_args）、:195-278（spawn_image：pflash/vars/usb/ramfb/fw_cfg）、:239-241（force-legacy 注入）、:250-265（HVF FPSR 判例 + opt/berry/accel 键）、:335-343（create_vars）；`tools/berry/src/commands.rs` :206（vars 64MiB 档）；`kernel/src/main.rs` :4043（policy demo 行——非错误）、:1449（VIRTIO vmo= 行）；`kernel/src/ktest.rs` :2875（FPSR panic 断言位）；`research/显示与加速体系设计/02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md` §0/§2/§3.1/§3.2/§4.3/§6/§7（S1 定义/坑清单/判例族/kill 线）。

**本机实测（2026-11-13，QEMU 11.0.3 Homebrew / AAVMF edk2-stable202408-prebuilt）**：§2-§5 全部读数（产物目录见 §7）。

**外部**：无新增——本轮全仓内产物 + 本机实测，02 号外部索引（QEMU virtio-gpu 文档/uapi 头）未复核增补。
