# 38 号底稿：OEM 演练实弹——模拟「acme」从 product.toml 到 QEMU boot 定制镜像全链实录

> 版本：v1.0（2026-11-12；文件名按任务指定不带版本号，版本记于本行）｜性质：终极验收轮——31 号 §8 六站 dry-run 的**实弹化**：模拟一家 OEM「acme」从写 product.toml 到 QEMU boot 定制镜像走完全链，产出全链时间线 + 缺口清单分级 + 「组装不 fork」首次端到端实证（04 号提案）
> 锚：ADR #82（组装不 fork 三包四层）/ #77（SDK-2 签名链）/ #84（BCTS）；工具族母法 = `docs/产品配置层与槽位合同设计-v1.1.md`（§1 product schema/§4 九步管线）+ `docs/SDK-2-分发与签名链设计-v1.2.md` §三/§五 + `docs/设备侧验签与账本落盘设计-v1.0.md` §1.2/§1.3 + `docs/BCTS兼容性测试套件设计-v1.1.md` §3.2/§4.2；前序 = 31 号 §8（六站 dry-run）/ 37 号（四刀落地复核 + AAVMF /tmp 先例）
> 采集窗：2026-11-12 07:19–08:41 CST（OEM 链 07:22–07:31；BCTS Tier1 回放腿 08:05–08:41），主工作树 `wip/sched-1.1`，开窗 HEAD `06c4aa5`（轮十二落库提交）→ **窗内并行道一笔 docs-only 提交 `34529d1`（系统优化路线图，07:18:39——先于演练链起跑，零代码增量，仅 git describe 计数 142→143）**；另观察到并行道散件 `tools/bcts/frozen/bcts-t2@0-hdi-dynamic-map.md` 窗内新现（非本稿动作，未触碰）。开窗前 `git status` 逐查：零 M 件（四件在册散件 untracked 与本稿无关）。
> 方法：全链逐步实跑、逐步留证（命令 + 输出摘录 + 工件 sha256 + 串口日志）；演练件落 `/tmp/drill/`（键/包/镜像/日志），入仓参照物仅 `tools/drill/acme/product.toml` 一文件（头注「演练参照物」）；**平台侧零源码改动、零仓内写**（git 对拍：全程零 M，唯一仓内新增 = 上述参照物）；测试钥纪律 = fresh acme 钥对 /tmp 现产 + 平台 dev 钥模拟签（`tools/testkeys/dev/`，永不真签）；QEMU 全程 timeout 帽（420s 外套 + 360s 标记帽）。
> 环境绕道（37 号观察⑦延续，如实记）：①AAVMF 固件拷 `/tmp/drill/fw/`（QEMU 写态 open `/opt/homebrew` 被沙箱拒）；②平台内核构建 `CARGO_TARGET_DIR=/tmp/drill/platform/tgt` 隔离（`~/.cargo-target` 拒写）——两者均演练**前置**的平台侧准备，不计入 OEM 链计时。

---

## §0 总判（一屏版）

| # | 演练步 | 判 | 一句话 |
|---|---|---|---|
| 1 | OEM 定义：acme product.toml（preset 5/9 + priv_grants 2 app + issued_by=acme + boot params + branding + source 节） | ✅（两段式） | `--check` 过（`preset=5/9 size=96MiB split=32MiB source=on`）；**首检红一次**——`[product.source].instance` 产物存在性校验先咬（顺序摩擦，G4）；i18n zh-CN 八键组 = 如实拒（在册未收录，G6）；反回滚门 PR=2 拒/PR=1 过双钉 |
| 2 | 组装：`--dry-run` 计划面 | ✅ | rootfs 终表 55 条 = 基座 50 ⊕ preset 5；mkcpio/mkimage 参数集 + 源实例透传 + PR≡level 单源行 + kernel 锚回显全出；**计划不可直接执行**——app 条目路径钉死基座表（G1） |
| 3 | 签名链：keygen + mkapp ×5 + gensourceindex + admit | ✅ | acme 钥对 fresh 产；5 app 重打包 dev 钥签（lineage=acme 回声）+ 验签 5/5 + 换钥负钉；acme 官方源索引（signed + witnessed=5/5 + 八字段齐）；admit dev 档 TOFU 5 allow / product-open 5 reject unknown-src（真签轨道未开的机器证据） |
| 4 | 镜像：mkimage 全参 | ✅ | `acme-rich.img` 117,440,512B（96+16MiB）：四件套 + KERNEL.SIG + TRUSTPK + /SOURCE/ 两件套；manifest v1 entries=8 **platform_revision=1**（== abi.toml level）signed=on key_hash=acme.pk-sha256；确定性双跑字节同 |
| 5 | BCTS：定制镜像可行形态 | ⚠️ 缺口+替代 | runner 现绑标准镜像流（smoke.sh `-kernel`+fw_cfg，零镜像参数位）→ **G3 记 P0**；替代双腿 = 标准 Tier1 冻结回放绿（88/88）+ 定制镜像 QEMU boot 标记对拍（84/84 ×2 镜像互证） |
| 6 | QEMU boot：acme-rich.img EFI 链 | ✅ **演练终点** | **84/84 标记到齐**（APP ok n=**5**/APPUI ok n=**5**——preset 子集计数精确对拍）+ **`sig: verified` + `VBS ok slot=… len=64 meas=… sig=…`**（host 双对拍逐位一致）+ **5× `eco: app <ID> sig ok`**（装载守卫逐件验过）+ `eco: VER ok` + 金丝雀 + banner 形态；标准对照镜像同罩 84/84、sig/VBS 行=0 |
| 7 | 缺口清单 | ✅ §4 | P0×3 / P1×9 / P2×4——全链真实摩擦逐条分级，G1/G2（自有件与信任锚投放无键位）为「组装不 fork」从「链路通」到「产线成线」的主断点 |
| 8 | 计时 | ✅ §6 | **product.toml → 镜像就绪 ≈ 160s；→ boot 绿 = 515s（8min35s，其中 QEMU TCG boot 验证 320s）** |

**总判：全链走通。** 一家「拿到平台工件」的 OEM 用现役工具族（berry_assemble / berry-sign / mkapp / gensourceindex / mkcpio / mkimage）+ 纯数据投放（1 份清单 + 2 资产 + 5 自有 app 包 + 1 信任锚件 + 1 条目表 + 2 键文件），**零平台源码改动、零 fork**，8.5 分钟内产出可 boot 的定制镜像且签名链三环（boot 期内核验签 / 装载期 app 验签 / 官方源索引）全绿。代价如实：产线在两处断为手工缝针（OEM 自有 .bap 路径替换 + etc/trust-pk 锚投放——G1/G2），BCTS 认证面对定制镜像不可达（G3）——三件即「组装不 fork」从实证到量产的距离。

---

## §1 演练布景：OEM 拿到什么（平台工件包快照）

交付形态按 31 号 §8 第 0 站设定（工件 + 合同，非源码树）。演练前快照（sha256 全留 `/tmp/drill/logs/platform-artifacts.sha256`）：

| 平台工件 | 值 | 备注 |
|---|---|---|
| KERNEL.ELF | 6,402,720B，sha256 `a19b039187b5964d…e35d0d` | 隔离构建 `CARGO_TARGET_DIR=/tmp/drill/platform/tgt cargo build`（全链 3.79s，ident `v0.2.0-143-g34529d1-dirty`） |
| BERRYBOOT.EFI | 25,088B，sha256 `6132b8b30211ac…3ef7f54` | 轮十二验签版（链 berry-crypto，37 号 A/B +8,192B 的那件） |
| hello.txt | 28B，sha256 `e585ef27ba49d1c…73af1cae` | BerryBoot fs: 自验基准 |
| 标准 rootfs.cpio | 34,930,688B，sha256 `3c97c0e9808139…50947d85` | 平台基座全量（58 条目表 + 9 app）；.berryver `v0.2.0-142-g06c4aa5-dirty\|level 1\|v0.2.0`（07:12 窗构建） |
| 工具族 | berry_assemble / berry-sign / mkapp / gensourceindex / mkcpio / mkimage / bcts-run | selftest 全绿开局（berry-assemble/mkimage/gensourceindex/berry-sign 四件实跑 ok） |
| 合同面 | abi.toml `[platform].level = 1`；板表 qemu-virt；caps 闭集 console/fs/net | 装配反回滚门基准 |

OEM 侧投放物（acme 自有件，全在 `/tmp/drill/`）：`keys/acme.{sk,pk}`（fresh，pk `a8ad7d97…ab616`）、`acme/product.toml`、`acme/assets/`×2、`acme/apps/`×5（复制平台 staging 改 manifest [security] 三键）、`acme/etc/trust-pk`、`acme/acme-apps.toml`（源索引条目表）。入仓参照物：`tools/drill/acme/product.toml`（与演练原件逐字节同文，头注「演练参照物」+ 相对路径基准说明）。

---

## §2 全链时间线（逐步命令与输出摘录）

> 时间戳取文件 mtime + 命令内 `date +%s` 双源；完整输出存 `/tmp/drill/logs/`，串口日志存 `/tmp/drill/boot/`。

### 步① 07:22:10–07:22:41｜OEM 定义：product.toml + 首检（红）+ 词面/i18n/反回滚三探

**写清单**（抄 `--sample` 骨架改自有值）：`name="acme-rich"` / `vendor="acme"` / `board="qemu-virt"` / preset 5/9（SETTINGS、FILES、PHOTOS、CAMERA、BROWSER）/ priv_grants `SETTINGS=["console","fs"]`、`BROWSER=["net"]`、`issued_by="acme"` / boot.params `sched_slice_hz=64`+`intent_default="interactive"` / storage 96/32/p0 / branding 两件 / `[product.source] instance="sourceinst"` / kernel 锚 = 平台工件实 sha。

**首检红（顺序摩擦实咬）**：
```
$ python3 tools/berry_assemble.py --check /tmp/drill/acme/product.toml
berry-assemble: product.source.instance: 目录不存在 'sourceinst'（相对 product.toml 所在目录解析——schema §1.4 路径同口径）   [RC=1]
```
→ `[product.source]` 步⑦½ 深校验（`berry_assemble.py:266-296`——目录存在 + 两件套在席 + TLV 单解释器解析）把**签名链产物存在性**前压进清单校验：OEM 首检必红，流程被迫两段式（G4）。

**反回滚门双钉**：`--dry-run --platform-revision 2` → `声明 PR 2 超前平台 level 1 …新产品装旧平台拒`（RC=1）；PR=1 后续过。**i18n zh-CN 腿**（`product-i18n.toml` 加 `[product.defaults.locale]` 八键：languages=["zh-CN"]/timezone="Asia/Shanghai"/region="CN"/calendar="gregory"/first_day_of_week="mon"/hour_cycle="h23"/temperature_unit="celsius"）：
```
berry-assemble: product.defaults: 键 ['locale'] schema 在册、berry-assemble v0 未收录（product 键 add-only 随工具版本化——随 V13 后续刀扩；设计文档 §1.2-§1.5）   [RC=1]
```
→ 设计 §1.5 八键在册、工具未收录——诚实拒非静默（G6）。

### 步② 07:22:41 首跑 / 07:24:19 复跑转绿｜组装计划面（dry-run）

dry-run 首跑同咬 sourceinst（RC=1，同上——步③产物就位后复跑转绿，顺序摩擦 G4）；计划面全录 `/tmp/drill/logs/dryrun-plan.txt`，要点摘录：
```
berry-assemble: /tmp/drill/acme/product.toml OK（product=acme-rich board=qemu-virt preset=5/9 size=96MiB split=32MiB source=on）
berry-assemble: rootfs 条目 55 条 = 基座 50（bin/*+selftest/*）⊕ preset 5（app/*）——排序 = 整条 name=path（mkcpio CLI 现役语义，步④）
berry-assemble: mkcpio 参数集（步⑥）: python3 tools/mkcpio.py <out.cpio> <55 条 name=path·见上清单> --split 32
berry-assemble: mkimage 参数集（步⑦）: python3 tools/mkimage.py out/acme-rich.img --efi … --size-mib 96 --source-instance sourceinst
berry-assemble:   platform_revision 无注入位（mkimage 单源读 tools/abi.toml [platform].level=1 …）；装配反回滚门：声明 PR 1 ≤ 平台 level 1 过
berry-assemble: kernel 锚（步⑧）: KERNEL.ELF@sha256:a19b0391…e35d0d——v0 只验锚形，逐字节度量对账归全产线
berry-assemble: boot.params 覆盖 ['intent_default', 'sched_slice_hz']（Q2 override 语义——缺省归上游；序列化进 INITRD 段归全产线）
```
priv_grants 白名单对账在 `--check` 内完成（九型拒面执法，`berry_assemble.py:206-263`）：app_id ⊆ preset + issued_by lineage 域 + caps ⊆ 闭集。

### 步③ 07:22:46–07:23:59｜签名链四件：keygen → mkapp ×5 → gensourceindex → admit

**acme 钥对**（fresh /tmp，dev 模拟纪律）：
```
$ berry-sign keygen /tmp/drill/keys/acme.sk /tmp/drill/keys/acme.pk
keygen: /tmp/drill/keys/acme.pk pk=a8ad7d97474174bbb712940eb05b97b5c0c64f145b63812483769e4edaaab616
```

**preset 子集重打包 + dev 平台钥签**（OEM 自有件：复制平台 staging 5 app 目录，manifest `[security]` 三键改 `key_lineage="acme"`/`integrator="acme"`——integrator 声明与 priv_grants.issued_by 同域对账）：
```
$ python3 tools/mkapp.py --sign-key tools/testkeys/dev/dev.sk /tmp/drill/acme/baps/SETTINGS.bap /tmp/drill/acme/apps/SETTINGS
mkapp: …/SETTINGS.bap 61440B SETTINGS v0.1.0 console=false stack=8 intent=utility abi_revision=1 icons=1 views=1
sign: …/SETTINGS.bap.sig 113B lineage=acme gen=1 chain=0            （×5：SETTINGS/FILES/PHOTOS/CAMERA/BROWSER）
$ python3 tools/mkapp.py --verify-key tools/testkeys/dev/dev.pk --check …/SETTINGS.bap
mkapp: … OK … digests=ok / verify: … .bap.sig OK lineage=acme gen=1 chain=0 (BSIG-TOP 过)   （5/5 绿）
$ # 负钉：acme.pk 验 dev 签件 →
mkapp: berry-sign: 换钥拒: 信封当前钥无锚见证（可信 pk a8ad7d97…）   [RC=1]
```

**acme 官方源索引**（acme 钥自签源——`--source-id acme`）：
```
$ python3 tools/gensourceindex.py --rootfs-manifest /tmp/drill/acme/acme-apps.toml --user-dir /tmp/drill/acme \
    --out-dir /tmp/drill/acme/sourceinst --source-id acme --sign-key …/acme.sk --source-pk …/acme.pk
gensourceindex: …source-index.tlv 622B entries=5 index_version=1 witnessed=5 last_revision=1
gensourceindex: …source-keys.txt 252B source_id=acme sig_state=signed
$ python3 tools/gensourceindex.py --verify …/source-index.tlv
… OK entries=5 … 八字段 5/5 齐全 … 签名: 真验过（BSRC-TOP × berry-sign verify-raw）… 两件套对拍: … 一致
```

**host admit 双档**（装载裁决预演；账本 = 平台 berryos-dev lineage 锚 dev.pk 的最小合法件）：
```
$ berry-sign admit policy-dev.toml ledger.toml /tmp/drill/acme/baps
admit: BROWSER allow tofu-first-seen lineage=acme gen=1 / CAMERA·FILES·PHOTOS·SETTINGS allow verified …
admit: done apps=5 allow=5 reject=0 policy=dev
$ berry-sign admit policy-product-open.toml ledger.toml /tmp/drill/acme/baps
admit: BROWSER/CAMERA/FILES/PHOTOS/SETTINGS reject eco: app … unknown-src
admit: done apps=5 allow=0 reject=5 policy=product-open        [RC=1]
```
→ 产品档全拒 = **OEM lineage 未入信任账本、真签轨道未开**的机器实证（31 号 §8 第 2/3 站黄灯延续）；dev 档 TOFU 把 acme lineage 锚到信封自述 pk（=dev.pk）——lineage↔钥绑定未执法的可视化（G9）。

### 步④ 07:24:19–07:24:50｜定制 rootfs + 全参镜像

**mkcpio 定制 rootfs**（dry-run 计划 55 条 + 两处手工针脚：preset 5 条路径替换为 OEM 自有 .bap（G1）+ 追加 `etc/trust-pk`（G2）；`.sig` 伴生由 `sig_companions`（`mkcpio.py:226-247`）同名直映射**自动收编**，`.berryver` 烙印自动附加）：
```
$ (cd user && python3 ../tools/mkcpio.py /tmp/drill/acme/rootfs-acme.cpio <56 条 name=path> --split 32)
mkcpio: /tmp/drill/acme/rootfs-acme.cpio 34091008B x62 条目
mkcpio: .berryver v0.2.0-143-g34529d1-dirty|level 1|v0.2.0      ← 与平台工件 kernel 同窗 ident（VER 哨兵前提）
```
归档走查复核：app 条目恰 5 + 伴生 .sig 恰 5 + `etc/trust-pk` 在席 + `.berryver` 同窗 ✓（全档日志见 `/tmp/drill/logs/`）。

**mkimage 全参**（platform_revision 单源 / source-instance / sign-key / trust-pk / key-hash 全开）：
```
$ python3 tools/mkimage.py /tmp/drill/out/acme-rich.img \
    --efi bootloader/berryboot/out/BERRYBOOT.EFI --hello bootloader/berryboot/hello.txt \
    --kernel /tmp/drill/platform/kernel.elf --initrd /tmp/drill/acme/rootfs-acme.cpio \
    --size-mib 96 --source-instance /tmp/drill/acme/sourceinst \
    --sign-key /tmp/drill/keys/acme.sk --trust-pk /tmp/drill/keys/acme.pk \
    --key-hash d1984df498ce6f6f…fb155b5          # = sha256(acme.pk)
mkimage: ok /tmp/drill/out/acme-rich.img 117440512B (mbr+fat16+keymeta, 96+16MiB) … files=8 … source_instance=…
mkimage: manifest … entries=8 key_hash=d1984d… platform_revision=1 signed=on
```
manifest v1 八条目：四件套 + `/KERNEL.SIG`(64B) + `/TRUSTPK`(32B) + `/SOURCE/{SRCINDEX.TLV,SRCKEYS.TXT}`，**每条 platform_revision=0000000001 == abi.toml level**（三方联动第一方，宿主侧文件对拍过）。**确定性**：manifest 恢复重跑双镜像 sha256 恒等 `668d8e1366fe…39ea7`。product.toml kernel 锚 ↔ manifest KERNEL.ELF sha 逐字节相等（手工对账——工具不裁，G7）。

### 步⑤（BCTS 面）→ 见 §3；步⑥ 07:25:25–07:30:45｜QEMU boot = 演练终点

自建对拍器 `/tmp/drill/boot/boot-check.sh`（形态对读 smoke-image.sh：同 QEMU 拓扑/同标记面/同 banner-VER-金丝雀门；定制面新增 sig/VBS 行断言 + APP/APPUI 计数精确对拍 + 装载守卫逐 app 行；AAVMF 固件 = /tmp 副本 + blank vars.fd + timeout 帽）：
```
boot-check[acme]: QEMU 启动（AAVMF + /tmp/drill/out/acme-rich.img；expect APP n=5 sig=on）
boot-check[acme]: 标记 84/84 到齐（EFI 十段+接续 3+裸相 32+生态 39——APP/APPUI 计数对拍 n=5）
boot-check[acme]: 签名链行 = 5×sig ok + sig: verified + VBS ok slot=00000000441b0f18 len=64 meas=494c5c6e1270dab6 sig=64412698d2d0723f
boot-check[acme]: PASS（串口日志 /tmp/drill/boot/serial-acme.log）
```
串口关键行（`serial-acme.log` 实录）：
```
sig: verified                                    ← BerryBoot 验 KERNEL.SIG × TRUSTPK（acme 钥对）
VBS ok slot=00000000441b0f18 len=64 meas=494c5c6e1270dab6 sig=64412698d2d0723f   ← V10 槽填 + 内核消费
eco: app BROWSER sig ok / CAMERA sig ok / FILES sig ok / PHOTOS sig ok / SETTINGS sig ok   ← 装载守卫 ×5（dev.pk 锚）
APP ok n=5 / APPUI ok n=5                        ← preset 子集计数（对照标准 n=9/n=8）
eco: VER ok v0.2.0-143-g34529d1-dirty lvl 1      ← kernel↔rootfs 同窗三方联动绿
```
**host 双对拍（37 号先例复现）**：host SHA-512/256(KERNEL.ELF) 头 8B = `b6da70126e5c4c49`，host sign-raw 重算 KERNEL.SIG 头 8B = `3f72d0d298264164`；guest 行 meas/sig = 上述两值的**小端 u64 呈现**（`494c5c6e…`/`64412698…`）——逐位一致，镜像内↔宿主↔guest 三方闭合。FAT 抽取复核：镜像内 KERNEL.ELF sha256 == 平台工件 sha（`a19b0391…`）、KERNEL.SIG == 宿主重算字节、INITRD.BIN sha256 == 定制 rootfs（`beffb67c…`）。

**标准对照镜像**（同窗重建：平台 kernel + 平台全量 rootfs 58 条，无签四件套）：`boot-check[standard]: 标记 84/84 … PASS`——APP ok n=**9** / APPUI ok n=**8**（APPQUIET 无 [ui] 节 → ui face Absent 不计 decl，`svcmgr main.rs:2044-2060` 计数语义）、**sig:/VBS 行 = 0**（无签零扰动）、VER ok。两镜像标记面差 = **恰预期定制差**（±4 app + 签名链行）——「标准 Tier1 回放绿 + 定制镜像标记对拍」替代腿的另一半。

对照腿插曲（如实记，非平台缺陷）：首建对照镜像混窗组镜（fresh kernel `g34529d1` + 平台旧 rootfs `g06c4aa5`）→ `eco: VER mix vmo=…143-g34529d1… cpio=…142-g06c4aa5…` **哨兵精确咬中混窗构建**——版本管理标准 §1 三方联动的活体证据，同窗重建后转绿（G13）。

---

## §3 BCTS 面：侦察结论 + 替代双腿

**侦察（缺口坐实）**：`bcts-run.sh` 五步算法的复跑全相经 `tools/smoke.sh` 同入口（`bcts-run.sh:784-787`：`KERNEL=$(berryos_kernel debug)` + `SMOKE_KEEP_LOG=1 bash smoke.sh`），而 smoke.sh = `-kernel` ELF 直载 + fw_cfg rootfs **标准镜像流**（`smoke.sh:157`）；Tier 1 回放对象结构性不是 FAT 整盘镜像，**零镜像参数位**可指定制镜像——与 bcts-run 头注「v0 回放对象 = QEMU virt -kernel ELF 直载形态」自认一致。→ **G3（P0）：BCTS 认证面对 OEM 定制镜像不可达**（标记面可对拍、判定面无参数化入口）。

**替代双腿（任务书预案执行）**：
1. **标准 Tier1 冻结回放绿**：`BCTS_FROZEN=tools/bcts/frozen/bcts@0.toml CARGO_TARGET_DIR=/tmp/drill/platform/tgt tools/bcts-run.sh`（与官方 `just bcts` 同入口，回放 kernel = 演练平台工件同二进制）——**verdict = PASS（88/88；missing = 无）**，五步全绿（清单解析 + 空集守卫 + abi-check 门禁 PASS + 八跑全相绿 + 结果包）；结果包关键行：`mode=frozen` / `artifact_sha256=9a8794ea…` / `kernel_sha256=a19b0391…`（**= 演练平台工件 sha——回放对象与定制镜像同内核，对拍链闭合**）/ `serial_logs = boot-01…08`（已归档 `/tmp/drill/logs/bcts/`）。回放 wall-clock ≈ 35–40min（TCG 八跑 + 窗内并行道 QEMU 抢核拖慢，非 OEM 出镜像链路时间）。
2. **定制镜像 QEMU boot 标记对拍**：§2 步⑥ 双镜像 84/84 互证（acme vs standard，计数差恰预期）。

---

## §4 缺口清单（演练真正产出）——P0 挡 OEM / P1 提效 / P2 记录

> 每条 = 摩擦实状（演练何步何证据咬中）+ 归属。P0 = 不绕出不了合法定制镜像/认证；P1 = 能走通但要手工/要文档考古/语义未闭合；P2 = 记录在案。

| # | 级 | 缺口 | 演练实咬点 | 归属/去向 |
|---|---|---|---|---|
| G1 | **P0** | **OEM 三包输入无键位：自有件进不了组装计划**——preset 条目路径钉死基座表（`berry_assemble.py:536-540` 自认「v0 无 OEM 三包输入，新增/替换交换归 V13 后续刀」）；OEM 自签 .bap、自有 bin 件无法经工具面进 rootfs | 步④：dry-run 计划 55 条不能直接执行——5 条 app 路径手工替换为 `/tmp/drill/acme/baps/*.bap`（改写条目表脚本 12 行）；「组装不 fork」断在**计划可执行性** | V13 后续刀（#83 键位定形 + berry_assemble 升版）；新增 `[product.apps.override]` 类键位或 `--items` 注入位 |
| G2 | **P0** | **信任锚投放面无键位无文档**——运行时装载守卫锚 `etc/trust-pk`（`svcmgr main.rs:5750` 消费）不在基座表、不在 product.toml 键集、不在任何配方；与镜像侧 TRUSTPK 的关系（FAT 锚验内核 / rootfs 锚验 app）零 OEM 向文档 | 步④：`etc/trust-pk` 手工追加为 rootfs 条目（读 svcmgr 源码才知 32B raw + 路径约定）；锚内容选 dev.pk（= 签 .bap 的钥）属演练推断——**正式形 = §1.4 bootseed 随策略刀**（代码注记自认） | P3 策略刀（bootseed/锚发放）；berry_assemble 键位或 mkimage 侧配套 |
| G3 | **P0** | **BCTS runner 绑标准镜像流**——Tier 1 回放 = smoke.sh `-kernel`+fw_cfg（`bcts-run.sh:784-787`/`smoke.sh:157`），零镜像参数位；OEM 定制 FAT 镜像拿不到 BCTS 判定 | 步⑤侦察：无入口可指 `acme-rich.img`；替代双腿绿但**认证语义缺位**（OEM 提交平台裁决需「定制镜像单」而非「标准流单 + OEM 自拍对拍」） | #84 后续（bcts-run 镜像参数化 / smoke-image 流并入 BCTS 相位）；v0 头注自认「镜像侧版本声称位对账随组装镜像宿主启用」 |
| G4 | P1 | **清单校验依赖签名链产物先在席**——`--check`/`--dry-run` 对 `[product.source].instance` 做目录+两件套+TLV 深校验（`berry_assemble.py:279-295`），OEM 首检必红 | 步①：首检红 `目录不存在 'sourceinst'`；流程被迫两段式（定义→产索引→复检），或先写无 source 节清单再补 | berry_assemble 分级校验位（`--check-lite`/结构面与产物面分层）；或文档明示两段式流程 |
| G5 | P1 | **boot.params 只验不透传**——sched_slice_hz/intent_default 过词表但「序列化进 INITRD 段归全产线」（`berry_assemble.py:597-598` 自认） | 步②：计划面如实注「归全产线」；OEM 调度参数声明零运行时效果 | V13 全产线（INITRD 段序列化 + 消费端） |
| G6 | P1 | **i18n 键组 v0 未收录**——设计 §1.5 八键在册、berry_assemble DEFAULTS_DEFERRED（`:137`）拒收 | 步①：`[product.defaults.locale]` zh-CN 组 → 「schema 在册、v0 未收录」拒（诚实拒非静默）；若 v1 接触窗内来首个国际 OEM 则实际挡路（临期升 P0） | V13 后续刀收录八键 + `etc/locale.toml` 投放 + 设置应用读者（设计已定形） |
| G7 | P1 | **kernel 锚只验形不度量**——锚 sha 只做 64hex 形校验（`berry_assemble.py:406-409`），与 mkimage 实投件零对账（「逐字节度量对账归全产线步⑧」自认） | 步④：手工对账 product.toml 锚 == manifest KERNEL.ELF sha（相等 ✓）——若 OEM 抄错锚，工具链全程沉默直到 boot 才可能炸 | V13 步⑧（装配侧度量对账） |
| G8 | P1 | **mkimage manifest 固定名 manifest.txt**——`Path(out).with_name(MANIFEST_NAME)`（`mkimage.py:693`），同目录多镜像互踩 | 步④：standard 构建覆盖 acme manifest（发现后重跑恢复；镜像字节确定性恒等救场） | mkimage 侧 `--manifest-out` 或 manifest 名随镜像 stem |
| G9 | P1 | **lineage↔钥绑定未执法**——信封 lineage = manifest 回声，签名钥与 lineage 声明无机构绑定；admit TOFU 直接把 acme lineage 锚到 dev.pk | 步③：admit dev 腿 `allow tofu-first-seen lineage=acme`（锚 pk=dev.pk）；product-open 5×`unknown-src` 拒 = 现役唯一防线；演练 .bap 本身 = dev 钥签 + manifest 声明 acme（合法但语义悬空） | lineage 分配机构运营（P3 后真签面，#77 :222 oem-system 锚空位）；账本发放仪式 |
| G10 | P1 | **admit policy/ledger 文件 schema 无 CLI 自述**——`berry-sign admit` 用法行不展两件 schema；合法 policy（`schema=1`+`tier=…`）与 ledger（九键 lineage 节 + V4 恒空 + 非空守卫）全靠试错或读 p3sim.py | 步③：三连试错（`UnknownKey`→`NoLineage`→`V4NonEmpty`）才拼出合法件，耗 ~48s/占签名链段约三分之二 | berry-sign admit 用法面扩一行示例或 docs 面（命令速览）补 OEM 向条目 |
| G11 | P1 | **内核签名归属未定（推断最小案）**——mkimage `--sign-key` 由**组镜方**持钥签 KERNEL.ELF 并自携 TRUSTPK（`mkimage.py:111-127` 注记自认「随闸 C/Q3 立法、真机烧录锚随闸 E/Q1」）；演练 = acme 钥签平台内核（语义倒置：真链应平台签/OEM 锚烧录） | 步④：全参开走通（验签链绿），但「谁签内核」的合同缺位——OEM 自签自验的闭环信任价值 ≈ 0，真值在烧录锚 | 闸 C/Q3（签名数据格式立法）+ 闸 E/Q1（真机锚）；平台发版工件预签名形态 |
| G12 | P1 | **环境绕道未记档**——AAVMF 固件须拷 /tmp（QEMU 写态 open `/opt/homebrew` 被沙箱拒）+ 内核构建 CARGO_TARGET_DIR 隔离（`~/.cargo-target` 拒写） | 本演练前置步双绕道（37 号观察⑦同款，连续第二轮复现）；OEM/CI 视角 = 上手第一坑 | CI/开发文档一笔记档（命令速览或规则 08） |
| G13 | P2 | **混窗组镜 VER mix 咬 OEM**——OEM 若拿非成套平台工件（kernel 新 + rootfs 旧）组镜，`eco: VER mix` 即红 | 对照腿首建实咬（vmo=143-g34529d1 vs cpio=142-g06c4aa5）；哨兵工作正常 = 平台发版纪律建议：**平台工件按同窗成套交付**（kernel+rootfs 同 ident 打包），否则 OEM 首组必踩 | 版本管理标准 §1 交付面补条；发版工件成套打包 |
| G14 | P2 | **APPUI 计数语义无文档**——APP ok n=9 而 APPUI ok n=8（APPQUIET 无 [ui] → Absent 不计 decl，`svcmgr main.rs:2044-2060`）；定制对拍需预知各 app ui 形态 | 对照腿：对拍器初版硬编码 n=9 误红一次（演练侧缺陷，修正后绿）；OEM 自建对拍会踩同坑 | smoke-image/文档注记 app ui 形态表（或对拍器取前缀断言——官方 smoke-image 即前缀式） |
| G15 | P2 | **定制镜像验收面缺省集**——官方 smoke-image 的 kfile/init 字节对拍钉仓库根 rootfs.cpio（`smoke-image.sh:105-106` 入口快照、`:492` 对账），对定制镜像结构性不可复用（本演练自建对拍器）；RX 注入段/UTM 手动点验在定制对拍中省略 | 步⑥：自建 boot-check.sh（~90 行）覆盖 EFI 十段/接续/裸相/生态/sig/VBS/VER/金丝雀——**定制镜像验收无平台件**，OEM 各自造轮子 | G3 同刀（BCTS 参数化时吸收定制镜像相位）；或平台出 image 级对拍器参数位 |
| G16 | P2 | **条目表/清单双源漂移面**——OEM 需自持「源索引条目表」（acme-apps.toml）与「rootfs 条目表」（entries-acme.txt）两份数据 + product.toml preset 第三处列举，三处 app 集合手工保持一致 | 步③④：同一 5-app 集合写了三遍（product.toml preset / acme-apps.toml / entries 生成脚本以 preset 为源导出——已收敛一处，但 gensourceindex 侧仍独立一份） | V13 全产线（单源条目派生）；或 gensourceindex 支持 --preset 从 product.toml 读 |

**分级小结**：P0×3（G1 自有件输入 / G2 信任锚投放 / G3 BCTS 参数化）= 「组装不 fork」从链路通到量产线的三根针脚；P1×9 里 G4/G7/G8/G10/G12 是纯工具/文档提效（小刀），G5/G6 是已计划的滞后收编，G9/G11 是真签面时序（31 号黄灯延续，接触窗开启前置）；P2×4 记录。

---

## §5 「组装不 fork」实证结论（04 号提案首次端到端）

**命题**：OEM 不碰平台源码、不持平台仓，仅以数据与自有件投放产出定制镜像（ADR #82；04 §8.2 生死判据的反面验证）。

**实证账**：
1. **零源码改动**：全程 `git status` 零 M 件；仓内唯一新增 = `tools/drill/acme/product.toml` 参照物（54 行数据文件）。OEM 投放物全在仓外 `/tmp/drill/`：1 清单 + 2 资产 + 5 app 包（复制平台 staging 改 manifest）+ 1 锚件 + 1 条目表 + 2 键。
2. **工具族全消费**：berry_assemble（--check/--dry-run/--sample 骨架）+ berry-sign（keygen/sign/sign-raw/admit）+ mkapp（--sign-key/--verify-key/--check）+ gensourceindex（--sign-key/--verify）+ mkcpio（条目面+伴生收编+烙印）+ mkimage（四件套+验签两件+源实例+key-hash）——**七件全用上，零一件需要改平台代码才能走通**（G1/G2 是「绕过工具手工缝」，不是「改工具」）。
3. **三环签名链真绿**：boot 期（BerryBoot 验 KERNEL.SIG×TRUSTPK → V10 槽 → 内核 VBS 行，host 三方对拍逐位一致）+ 装载期（5× `eco: app … sig ok`，etc/trust-pk 锚）+ 分发面（源索引 signed/witnessed + admit 双档裁决语义正确）。
4. **fork 的物理面不存在**（31 号第 0 站判词延续）：本演练 OEM 全程零 clone/零 patch/零分支——唯一「深度依赖」是复制平台 app staging 改 manifest（SDK 投放形态的合理前置，等价于「拿 SDK 样例工程改配置」）。
5. **边界如实**：v0 产线在两处断为手工（G1/G2）+ 认证面不可达（G3）——「组装不 fork」今日成立的标准是**「能不 fork」**（链路全通、工具全在、缝针可缝），**还不是「无需懂内部」**（缝针位置需读 svcmgr/berry_assemble 源码才知道）。三根 P0 针脚缝上后，判据从「可行性实证」升「量产线在册」。

---

## §6 计时：OEM 拿到平台工件后多久出镜像（实测）

| 段 | 时刻/时长 | 内容 |
|---|---|---|
| T0 | 07:22:10 | product.toml 起笔（assets + 清单写毕 07:22:22） |
| 步①首检+三探 | +31s | 首检红（G4）+ 反回滚双钉 + i18n 拒腿 |
| 步③签名链 | +73s（07:22:46–07:23:59） | keygen 1s + mkapp×5+验签 25s + gensourceindex+verify 5s + **admit 三连试错 ~48s（G10——一次成型则 ~25s）** |
| 步②转绿 | +20s（07:23:59→07:24:19） | --check 绿 + dry-run 计划面 |
| 步④产件 | +31s（07:24:19–07:24:50） | mkcpio 6s + mkimage ~10s + manifest 对账 |
| **镜像就绪** | **T0+160s ≈ 2min40s** | acme-rich.img + manifest 落盘（**「拿到工件→出镜像」 headline 数**；admit 一次成型则 ≈ 135s） |
| 步⑥ QEMU boot | +320s（07:25:25–07:30:45） | TCG 全标记到齐 + 签名链行（84/84 + sig/VBS + 5×sig ok） |
| **演练终点** | **T0+515s = 8min35s** | product.toml → boot 绿全程（含 QEMU 验证相） |

口径注：①平台侧准备（内核隔离构建 3.79s/固件拷贝/工具 selftest）不计入——OEM 拿到的是工件不是树；②窗内演练基建（对拍器撰写 + 标准对照镜像两建两 boot + manifest 恢复，07:22–07:49 交错）与 OEM 链串行但**未计入**上表；③QEMU 320s 为 TCG 标记全链等待（官方 smoke-image 实测 ~115s 起步 + 生态 39 + 守卫 5 spawn），真机/加速面量级另计；④BCTS Tier1 回放（认证 lane）另跑不计入 OEM 出镜像时间。

---

## §7 边界与局限

- 本稿只动两文件：本报告 + `tools/drill/acme/product.toml`（参照物）；零 git 写操作；零平台源码/文档修复（G1-G16 全只报不修——演练纯度纪律）。
- 演练 = dev 钥模拟（fresh acme 钥对 + 平台 dev 钥），零真签、零仓内 testkeys 消费（dev.pk 只读复用与 /tmp 副本）。
- 并行道两事件如实记：窗内 HEAD `06c4aa5`→`34529d1`（docs-only，07:18:39——演练链起跑前，仅 describe 计数 142→143，内核代码零增量，kernel↔rootfs 同窗性经 VER ok 在机证明）；散件 `tools/bcts/frozen/bcts-t2@0-hdi-dynamic-map.md` 窗内新现（非本稿，未触碰）。
- 计时为单跑实测（非基准多次取中）；admit 试错 ~48s 已单列，去噪口径已给。
- boot 对拍器 RX 注入段省略（G15）；UTM 手动点验不在演练范围。

## 来源索引

- 仓内代码/配置：`tools/berry_assemble.py`（:97 板表/:121 caps 闭集/:137 deferred/:206-263 白名单/:266-296 source 深校验/:498-519 反回滚门/:522-553 条目解析/:536-540 v0 边界自认/:556-601 计划面/:594-598 锚形与 params 注记）；`tools/mkcpio.py`（:226-247 sig_companions）；`tools/mkapp.py`（:17-24 --sign-key/:1357-1373 发现序）；`tools/mkimage.py`（:92/:111-127/:124-125/:129-146/:152-178/:180-200/:202-219/:693）；`tools/gensourceindex.py`（:139-140/:661-720）；`tools/bcts-run.sh`（:784-787）；`tools/smoke.sh`（:157）；`tools/smoke-image.sh`（标记面 :166-236）；`user/bins/svcmgr/src/main.rs`（:1986-2060 APP/APPUI 批/:5735-5830 p3_sig_guard/:5746-5750 两锚）；`kernel/src/main.rs`（:291-322 VBS 行）；`user/berry-crypto/src/admit.rs`（:26-33 档表）；`user/berry-crypto/src/ledger.rs`（:398-406 V4 恒空）；`tools/abi.toml`（[platform].level=1）；`tools/rootfs-manifest.toml`（58 条基座表）。
- 演练件：`tools/drill/acme/product.toml`（入仓参照物）；`/tmp/drill/`（keys/acme/{sk,pk}、acme/{product.toml,product-i18n.toml,assets/,apps/,baps/,acme-apps.toml,etc/trust-pk,sourceinst/,rootfs-acme.cpio}、out/{acme-rich.img,manifest.txt,standard.img,rootfs-std.cpio}、boot/{boot-check.sh,serial-acme.log,serial-standard.log}、logs/（时间戳/计划面/条目表/admit 腿/i18n 拒腿/反回滚拒腿/工件 sha256/BCTS 结果）、platform/{kernel.elf,tgt}、fw/edk2-aarch64-code.fd）。
- 仓内文档：`docs/产品配置层与槽位合同设计-v1.1.md`（§1.2-§1.6/:89 locale 样例/:148-165 八键表/§4 九步）；`docs/设备侧验签与账本落盘设计-v1.0.md`（§1.2/§1.3/§4.1）；`docs/SDK-2-分发与签名链设计-v1.2.md`（§三/§五）；`docs/架构决策.md`（#77/#82/#84）。
- 研究稿：31 号（§8 六站 dry-run——本演练底本）；37 号（四刀复核 + AAVMF/CARGO_TARGET_DIR 绕道先例 + VBS 对拍先例 + P2a 发现序修）；04 号（§8.2 生死判据）；08 号（§3 组装工具链）；13 号（§5.1 V13 载体）。
- 实跑记录：本稿 §2 各表（命令+输出摘录+工件 sha）；关键哈希——acme-rich.img `668d8e1366fe…39ea7`（双跑恒等）、platform kernel `a19b0391…e35d0d`、acme.pk sha256 `d1984df4…fb155b5`、VBS 行 meas/sig host 对拍 `b6da70126e5c4c49`/`3f72d0d298264164`（LE 呈现逐位一致）。
