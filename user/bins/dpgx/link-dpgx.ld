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
 */
ENTRY(_start)

PHDRS {
  load PT_LOAD FLAGS(7); /* R+W+X */
  tls  PT_TLS;
}

SECTIONS {
  . = 0x200000;
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
