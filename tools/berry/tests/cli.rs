//! CLI 集成测试（cbox 刀 3 补面，轮 2 审计 [19]）——经真实进程边界钉
//! `berry` 子命令出口语义：解析失败 = 非零退出（宿主脚本面靠退出码判
//! 健康度，不能只靠 stderr 文本）；`--json` 契约 = stdout 纯 JSON 行、
//! human 诊断（拒行注记）走 stderr；`--pc` 0x 前缀与裸十六进制同值。
//!
//! 盘件走 `tools/mkfat.py` 真产物（src/crashlog.rs 测试同款——确定性、
//! 零 fixture 入库；mkfat 缺席即失败，不静默跳过）。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn berry(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_berry"))
        .args(args)
        .output()
        .expect("berry 子进程启动失败")
}

/// 测试围栏目录（pid 命名——测试线程并行同 pid 但本文件各测各用子目录）。
fn fence(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "berry-cli-test-{}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 造盘：mkfat --put 注入 CBOX.LOG 后补 16 扇区零前缀（主路盘形态，与
/// crashlog 单元测试同款——vol@LBA16 发现路径）。
fn mkfat_disk(dir: &Path, ledger: &[u8]) -> PathBuf {
    let lfile = dir.join("ledger.log");
    let img = dir.join("disk.img");
    std::fs::write(&lfile, ledger).unwrap();
    let mk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mkfat.py");
    let out = Command::new("python3")
        .arg(mk)
        .arg(&img)
        .args(["--put", &format!("{}:/CBOX.LOG", lfile.display())])
        .output()
        .expect("python3 起 mkfat 失败");
    assert!(
        out.status.success(),
        "mkfat 造盘失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut d = vec![0u8; 16 * 512];
    d.extend_from_slice(&std::fs::read(&img).unwrap());
    std::fs::write(&img, &d).unwrap();
    img
}

/// 一条好账（EL0 崩溃）+ 一条坏行——拒行注记面fixture。
fn ledger_good_and_bad() -> Vec<u8> {
    let mut r = berry_cbox::Rec::EMPTY;
    r.seq = 1;
    r.kind = 1; // KIND_EL0（crashlog 域值；此处只作任意合法 kind 用）
    r.name[..9].copy_from_slice(b"user_task");
    r.pc = 0x1004;
    r.lr = 0x4000_900c;
    r.vct = 42;
    let mut line = [0u8; berry_cbox::LINE_MAX];
    let mut buf = Vec::new();
    let n = berry_cbox::line_encode(&r, 1, &mut line).unwrap();
    buf.extend_from_slice(&line[..n]);
    buf.push(b'\n');
    buf.extend_from_slice(b"CBOX v1 seq=x kind=el0\n"); // 解析必拒
    buf
}

/// 最小带符号 ELF64（单符号 `main@0x1000`）。同构 `src/symbolize.rs` 的
/// `testutil::build_elf` 最小版——cfg(test) 对集成测试不可见，就地落一份
/// 只服务 CLI 出口语义（解析面断言单源在 symbolize 单元测试）。
fn one_sym_elf() -> Vec<u8> {
    let mut strtab = vec![0u8];
    strtab.extend_from_slice(b"main\0");
    let mut symtab = Vec::new();
    symtab.extend_from_slice(&0u32.to_le_bytes()); // 空名（null sym）
    symtab.push(0x12);
    symtab.push(0);
    symtab.extend_from_slice(&1u16.to_le_bytes()); // shndx=1（.text）
    symtab.extend_from_slice(&0x1000u64.to_le_bytes());
    symtab.extend_from_slice(&0x10u64.to_le_bytes());
    symtab.extend_from_slice(&1u32.to_le_bytes()); // "main" @strtab 1
    symtab.push(0x12);
    symtab.push(0);
    symtab.extend_from_slice(&1u16.to_le_bytes());
    symtab.extend_from_slice(&0x1000u64.to_le_bytes());
    symtab.extend_from_slice(&0x10u64.to_le_bytes());
    let ehsz = 0x40usize;
    let sym_off = ehsz;
    let sym_sz = symtab.len();
    let str_off = sym_off + sym_sz;
    let str_sz = strtab.len();
    let shoff = (str_off + str_sz + 15) & !15;
    let shnum = 4usize;
    let mut d = vec![0u8; shoff + shnum * 64];
    d[..4].copy_from_slice(b"\x7fELF");
    d[4] = 2;
    d[5] = 1;
    d[6] = 1;
    d[0x28..0x30].copy_from_slice(&(shoff as u64).to_le_bytes());
    d[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes());
    d[0x3c..0x3e].copy_from_slice(&(shnum as u16).to_le_bytes());
    d[sym_off..str_off].copy_from_slice(&symtab);
    d[str_off..str_off + str_sz].copy_from_slice(&strtab);
    let mut sh = |i: usize, typ: u32, flags: u64, off: usize, size: usize, link: u32| {
        let o = shoff + i * 64;
        d[o + 4..o + 8].copy_from_slice(&typ.to_le_bytes());
        d[o + 8..o + 0x10].copy_from_slice(&flags.to_le_bytes());
        d[o + 0x18..o + 0x20].copy_from_slice(&(off as u64).to_le_bytes());
        d[o + 0x20..o + 0x28].copy_from_slice(&(size as u64).to_le_bytes());
        d[o + 0x28..o + 0x2c].copy_from_slice(&link.to_le_bytes());
    };
    sh(1, 1, 0x2, ehsz, 0x100, 0); // .text：SHF_ALLOC
    sh(2, 2, 0, sym_off, sym_sz, 3); // SHT_SYMTAB → strtab
    sh(3, 3, 0, str_off, str_sz, 0); // SHT_STRTAB
    d
}

/// `symbolize --pc`：坏十六进制 = 非零退出；0x 前缀/裸十六进制同值。
#[test]
fn symbolize_pc_parse_and_prefix_parity() {
    let dir = fence("pc");
    let elf = dir.join("one.elf");
    std::fs::write(&elf, one_sym_elf()).unwrap();
    let elf_arg = elf.display().to_string();

    // ① 非十六进制 pc：非零退出 + stderr 有诊断（退出码是宿主脚本契约面）。
    let bad = berry(&["symbolize", "--pc", "zz", "--elf", &elf_arg]);
    assert!(!bad.status.success(), "坏 pc 必须非零退出");
    assert!(!bad.stderr.is_empty());

    // ② 0x 前缀与裸十六进制：stdout 逐字节相等且命中符号。
    let a = berry(&["symbolize", "--pc", "0x1004", "--elf", &elf_arg]);
    let b = berry(&["symbolize", "--pc", "1004", "--elf", &elf_arg]);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(a.stdout, b.stdout, "0x 前缀与裸十六进制必须同值");
    let text = String::from_utf8_lossy(&a.stdout);
    assert!(text.contains("main"), "须命中符号：{text}");

    std::fs::remove_dir_all(&dir).ok();
}

/// `--json crash-log`：stdout 纯 JSONL（每行可解析）、拒行注记走
/// stderr（human 诊断不混机器面）；全毁账本 = 非零退出。
#[test]
fn crash_log_json_face_and_exit_codes() {
    let dir = fence("json");
    let img = mkfat_disk(&dir, &ledger_good_and_bad());
    let img_arg = img.display().to_string();

    // ① JSON 面：拒行注记在 stderr、stdout 每行独立 JSON。
    let out = berry(&["--json", "crash-log", "--disk", &img_arg]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("拒 1 坏行"), "拒行注记须走 stderr：{err}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("拒"), "stdout 不得混 human 注记：{text}");
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "一指纹一行 JSONL：{text}");
    let v: serde_json::Value = serde_json::from_str(lines[0])
        .unwrap_or_else(|e| panic!("stdout 须纯 JSON：{e}；行={}", lines[0]));
    assert_eq!(v["t"], "crash-fp");
    assert_eq!(v["task"], "user_task");

    // ② human 面对照：拒行注记进报告头（stdout）。
    let human = berry(&["crash-log", "--disk", &img_arg]);
    assert!(human.status.success());
    assert!(String::from_utf8_lossy(&human.stdout).contains("拒 1 坏行"));

    // ③ 全毁形态（非空账本零行可解析）：非零退出——与空账本/健康不
    // 可同形。
    let wreck_dir = dir.join("wreck");
    std::fs::create_dir_all(&wreck_dir).unwrap();
    let wreck = mkfat_disk(&wreck_dir, b"CBOX v1 seq=x\n");
    let out = berry(&["crash-log", "--disk", &wreck.display().to_string()]);
    assert!(!out.status.success(), "全毁账本必须非零退出");

    std::fs::remove_dir_all(&dir).ok();
}
