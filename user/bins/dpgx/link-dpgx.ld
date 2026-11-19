/* X-4：dpgx 单 PT_LOAD 特形链接脚本——berry-exec congruent 门首个真消费者。
 *
 * 现役 musl 族 berry-ld 产物 = 分段多 k（X-4 宿主清点：4-k/3-k 分段，
 * 全走 eager 兜底）——本脚本收单 LOAD：
 *   - 全 file-backed 输出节连排（text/rodata/data/tdata）→ k =
 *     file_off − vaddr 单值（≈ −0x1ff000，页倍数）；
 *   - `. = ALIGN(4096)` 钉在 .tdata 尾（无 TLS 时空过）→ filesz 端
 *     恰页齐 → demand_plan (f) 的 bss 文件域窗 [fe, fs_top) 为空 =
 *     SHT/shstrtab 尾字节永不入 bss 页；bss/tbss 全落零尾 zvmo
 *     （0x23 src=0 零页臂）。
 *   - tbss 紧随 tdata（TLS image 连续性）；若 tdata 存在则 pad 到页
 *     （零填充 = tbss 语义等价）。
 * RWX 混装单段 = v1 用户页可执行常态（规则 03 §A14 注），NX 档留 v2。
 *
 * X-4e 修（E 臂首跑全灭根因）：PHDRS 手写块不带 FILEHDR PHDRS 时，lld
 * 把 ELF 头排出所有 LOAD（首段 off=0x1000 起）→ 头不在映像内 →
 * `__ehdr_start` 被解析为 0 → crt0 berry_tls_setup 的 .data 引用槽
 * 存 0 → find_ptls 读 [0+0x20]（e_phoff）→ G1.5 kill 胎 → dpgx_foot
 * w1 早退。阳性对照：stdhello（默认布局）首 LOAD off=0 含头。
 * 修 = FILEHDR PHDRS 把头钉进单 LOAD 头部（k=−0x200000 仍页倍数，
 * filesz 端页齐/tbss 零尾/单 k 全不变式保住；filesz 总量不变——头
 * 占的 0xb0 被 tdata 尾 ALIGN 重吸收）；.text 起点随 SIZEOF_HEADERS
 * 平移。
 */
ENTRY(_start)

PHDRS {
  load PT_LOAD FLAGS(7) FILEHDR PHDRS; /* R+W+X，含 ELF 头 + phdr 表 */
  tls  PT_TLS;
}

SECTIONS {
  . = 0x200000 + SIZEOF_HEADERS;
  .text : { *(.text .text.* .init .fini) } :load
  .rodata : {
    *(.rodata .rodata.* .srodata .srodata.*)
    *(.eh_frame .eh_frame_hdr .init_array .fini_array .data.rel.ro)
  } :load
  .data : { *(.data .data.* .sdata .sdata.* .got .got.* .igot.plt) } :load
  /* TLS image（存在时）：tdata pad 页齐 → filesz 端页齐不变式保住。 */
  .tdata : { *(.tdata .tdata.*); . = ALIGN(4096); } :load :tls
  .tbss : { *(.tbss .tbss.*) } :tls
  .bss (NOLOAD) : { *(.bss .bss.* COMMON) } :load
  /DISCARD/ : { *(.comment) *(.note .note.*) }
}
