//! Shared-RAM scanout: QEMU `memory-backend-file,share=on` exposes guest RAM
//! as a host file; the guest framebuffer is just an offset inside it. This is
//! the v0.5 data plane — no VNC frame encoding, no per-frame guest commit,
//! and it sees exactly what the guest wrote (the same bytes the display
//! controller scans out).
//!
//! Geometry/PA come from the kernel's boot line:
//!   `FB ok 480x1000 fmt=1 stride=1920 fb=0000000043d00000`
//! The line is printed once at boot; `berry shell` waits for it.
//!
//! I2 止血两件：读面两段式（先只散读 512 采样点各 4B 算签名，变化才
//! 全帧读——静态屏免 60Hz 全帧 pread 底噪）；原生指针主路走 QMP
//! input-send-event（QEMU 11 的 VNC 路对裸 RFB msg5 丢绝对坐标），
//! 失联才退 RFB msg5 保底。

use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::display::frame::{Caps, Frame, InputEvent, ScreenSource, TransportKind};
use crate::display::vnc::VncInput;
use crate::error::{BerryError, Result};
use crate::qmp::QmpInjector;

const RAM_BASE: u64 = 0x4000_0000;

#[derive(Clone, Copy, Debug)]
pub struct FbInfo {
    pub fb_pa: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

impl FbInfo {
    /// Parse the kernel's `FB ok <w>x<h> fmt=<f> stride=<s> fb=0x...` line.
    pub fn parse(line: &str) -> Option<Self> {
        let mut w = None;
        let mut h = None;
        let mut fmt = None;
        let mut stride = None;
        let mut fb = None;
        for tok in line.split_whitespace() {
            if let Some(v) = tok.strip_prefix("fmt=") {
                fmt = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("stride=") {
                stride = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("fb=") {
                let v = v.strip_prefix("0x").unwrap_or(v);
                fb = u64::from_str_radix(v, 16).ok();
            } else if let Some((a, b)) = tok.split_once('x') {
                if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                    w = Some(a);
                    h = Some(b);
                }
            }
        }
        Some(Self {
            fb_pa: fb?,
            width: w?,
            height: h?,
            stride: stride?,
            format: fmt?,
        })
    }
}

/// 可定位精确读面（pread 语义）：真源 = `File`；测试源 = 内存面 + 读
/// 分类计数（两段式行为的自证锚）。trait 私有，经 `Box<dyn ReadAt>` 装进
/// `RamScan`——不扩公共 API 面。
trait ReadAt {
    fn read_exact_at(&self, buf: &mut [u8], off: u64) -> io::Result<()>;
}

impl ReadAt for File {
    fn read_exact_at(&self, buf: &mut [u8], off: u64) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            FileExt::read_exact_at(self, buf, off)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            let mut done = 0;
            while done < buf.len() {
                let n = self.seek_read(&mut buf[done..], off + done as u64)?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "ram eof"));
                }
                done += n;
            }
            Ok(())
        }
    }
}

/// 签名种子与折叠素数（与旧单段式逐位一致——签名语义零漂移）。
const SIG_SEED: u64 = 0xcbf2_9ce4_8422_2325;
/// 采样点数（旧单段式同参数：512 点等步长）。
const SIG_POINTS: usize = 512;

/// FNV 折叠单步。
fn sig_fold(sig: &mut u64, v: u32) {
    *sig ^= u64::from(v);
    *sig = sig.wrapping_mul(0x100_0000_01b3);
}

/// fb 窗口两段式扫描器：stage-1 只散读采样点各 4B 算签名（静态屏每轮
/// ~2KB 读量），签名变才 stage-2 全帧读 + BGRA→RGBA 重排。采样点集与旧
/// 单段式（全帧读后同式采样）同源——检测语义逐点零漂移。与 RamSource
/// 分离以可单测（无需活 VNC 连接）。
struct RamScan {
    reader: Box<dyn ReadAt>,
    offset: u64,
    raw: Vec<u8>,
    /// 采样点字节偏移（构造时按窗口长度一次定死：0 起等步长、窗口尾不足
    /// 4B 处停——与旧签名循环同界；几何不变，无每轮分配）。
    sample_offs: Vec<usize>,
    last_sig: u64,
    seq: u64,
}

impl RamScan {
    fn new(reader: Box<dyn ReadAt>, offset: u64, need: usize) -> Self {
        let step = (need / 4 / SIG_POINTS).max(1);
        let mut sample_offs = Vec::new();
        let mut i = 0usize;
        while i + 3 < need {
            sample_offs.push(i);
            i += step * 4;
        }
        Self {
            reader,
            offset,
            raw: vec![0; need],
            sample_offs,
            last_sig: 0,
            seq: 0,
        }
    }

    /// stage-1：散读采样点各 4B 算签名（每点一次小 pread——1080p 级窗口
    /// 步长 ~16KB，聚合成跨点大段只会把读量放大回全帧；实测 512 次小读
    /// 本身已快于一次全帧读，字节量砍 ~99%）。
    fn probe_sig(&self) -> io::Result<u64> {
        let mut sig = SIG_SEED;
        let mut word = [0u8; 4];
        for &off in &self.sample_offs {
            self.reader.read_exact_at(&mut word, self.offset + off as u64)?;
            sig_fold(&mut sig, u32::from_le_bytes(word));
        }
        Ok(sig)
    }

    /// 全帧缓冲上的同点集签名（stage-2 后重算——last_sig 恒描述所发帧的
    /// 实读字节，不受 stage-1 与全帧读之间的客机写入影响）。
    fn buf_sig(&self) -> u64 {
        let mut sig = SIG_SEED;
        for &off in &self.sample_offs {
            let b: [u8; 4] = self.raw[off..off + 4].try_into().unwrap();
            sig_fold(&mut sig, u32::from_le_bytes(b));
        }
        sig
    }

    fn try_next_frame(&mut self, info: &FbInfo) -> Option<Frame> {
        // 两段式：先采样签名，未变即 None（零全帧读）。
        let probe = self.probe_sig().ok()?;
        if probe == self.last_sig {
            return None;
        }
        self.reader.read_exact_at(&mut self.raw, self.offset).ok()?;
        self.last_sig = self.buf_sig();
        self.seq += 1;
        let mut frame = Frame::new(info.width, info.height);
        frame.seq = self.seq;
        let w = info.width as usize;
        let h = info.height as usize;
        let stride = info.stride as usize;
        for y in 0..h {
            let row = y * stride;
            for x in 0..w {
                let o = row + x * 4;
                let b0 = self.raw[o] as u32;
                let b1 = self.raw[o + 1] as u32;
                let b2 = self.raw[o + 2] as u32;
                frame.pixels[y * w + x] = match info.format {
                    0 => (b0 << 16) | (b1 << 8) | b2,       // RGBX
                    1 => (b2 << 16) | (b1 << 8) | b0,       // BGRX
                    _ => 0,
                };
            }
        }
        Some(frame)
    }
}

/// 原生指针注入通道状态（QMP 主路，惰性）。
enum Inject {
    /// 无注入面（非 unix 端点形态）。
    Off,
    /// 首个指针事件才连。
    Pending(std::path::PathBuf),
    /// 已连：QMP 路活着。
    Live(QmpInjector),
    /// 连不上/中途断：本源生命周期内不再重试，走 RFB msg5 保底
    /// （restart_in_place / 换 profile 都换新源，自然重试）。
    Dead,
}

pub struct RamSource {
    scan: RamScan,
    info: FbInfo,
    vnc: VncInput,
    inject: Inject,
}

impl RamSource {
    pub fn open(
        path: &Path,
        info: FbInfo,
        vnc: VncInput,
        inject_sock: Option<&Path>,
    ) -> Result<Self> {
        if info.width == 0 || info.height == 0 || info.stride < info.width * 4 {
            return Err(BerryError::protocol("RAM: bad fb geometry"));
        }
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if info.fb_pa < RAM_BASE {
            return Err(BerryError::protocol("RAM: fb PA below RAM base"));
        }
        let offset = info.fb_pa - RAM_BASE;
        let need = info.stride as u64 * info.height as u64;
        if offset + need > len {
            return Err(BerryError::protocol(format!(
                "RAM: fb window {offset}+{need} outside {len}"
            )));
        }
        Ok(Self {
            scan: RamScan::new(Box::new(file), offset, need as usize),
            info,
            vnc,
            inject: match inject_sock {
                Some(p) => Inject::Pending(p.to_path_buf()),
                None => Inject::Off,
            },
        })
    }

    /// QMP 注入一步：Pending 首用即连（连不上 → Dead）；Live 发 abs+btn
    /// （写失败注入器自哑化）。true = 已走 QMP，false = 调用方走 RFB msg5
    /// 保底。缩放分母用 fb 实几何（InputEvent 指针坐标即该域）。连接带
    /// 2s 守护超时——本调用在 UI 线程，QEMU 卡死不得冻结窗口。
    fn qmp_pointer(&mut self, x: u16, y: u16, buttons: u8) -> bool {
        if let Inject::Pending(p) = &self.inject {
            let next = match QmpInjector::connect_guarded(p, Some(Duration::from_secs(2))) {
                Some(inj) => Inject::Live(inj),
                None => Inject::Dead,
            };
            self.inject = next;
        }
        match &mut self.inject {
            Inject::Live(inj) => inj.pointer(x, y, buttons, self.info.width, self.info.height),
            _ => false,
        }
    }
}

impl ScreenSource for RamSource {
    fn caps(&self) -> Caps {
        Caps {
            version: 1,
            max_w: 4096,
            max_h: 4096,
            damage: false,
            multi_touch: 0,
            cursor: false,
            kind: TransportKind::Shm,
        }
    }

    fn try_next_frame(&mut self) -> Option<Frame> {
        self.scan.try_next_frame(&self.info)
    }

    fn send_input(&mut self, event: InputEvent) -> Result<()> {
        match event {
            // 指针主路 QMP input-send-event（S1 层二判例：QEMU 11 的 VNC
            // pointer 路丢绝对坐标——vs->absolute 为假、queue_abs 不入队；
            // input-send-event 实测可达 virtio-tablet），失联才退 RFB
            // msg5。键路维持 RFB。
            InputEvent::Pointer { x, y, buttons } => {
                if self.qmp_pointer(x, y, buttons) {
                    return Ok(());
                }
                self.vnc.pointer(x, y, buttons)
            }
            InputEvent::Key { keysym, down } => self.vnc.key(keysym, down),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fb_line() {
        let i = FbInfo::parse(
            "FB ok 480x1000 fmt=1 stride=1920 fb=0000000043d00000",
        )
        .unwrap();
        assert_eq!(i.width, 480);
        assert_eq!(i.height, 1000);
        assert_eq!(i.stride, 1920);
        assert_eq!(i.fb_pa, 0x43d0_0000);
    }

    /// 读分类计数（Rc 共享给测试柄）：fb 窗口全长读 = stage-2 全帧读，
    /// 其余 = stage-1 散读。
    #[derive(Default)]
    struct ReadLog {
        full: std::cell::Cell<usize>,
        small: std::cell::Cell<usize>,
    }

    /// 内存面 Mock（Rc<RefCell> 数据允许建扫描器后再改帧内容）。
    struct MockRam {
        data: std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
        fb_off: usize,
        fb_len: usize,
        log: std::rc::Rc<ReadLog>,
    }

    impl ReadAt for MockRam {
        fn read_exact_at(&self, buf: &mut [u8], off: u64) -> io::Result<()> {
            let o = off as usize;
            let end = o.checked_add(buf.len()).unwrap_or(usize::MAX);
            let data = self.data.borrow();
            if end > data.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "mock eof"));
            }
            buf.copy_from_slice(&data[o..end]);
            drop(data);
            let is_full = o == self.fb_off && buf.len() == self.fb_len;
            let c = if is_full { &self.log.full } else { &self.log.small };
            c.set(c.get() + 1);
            Ok(())
        }
    }

    /// 64x16 BGRX、stride 256（窗口 4096B、fb 起偏 0x10）：采样步长 2px
    /// → 8B，恰 512 点（0、8 … 4088）——on/off 采样点都充足。
    fn setup() -> (
        RamScan,
        std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
        std::rc::Rc<ReadLog>,
        FbInfo,
    ) {
        let info = FbInfo {
            fb_pa: RAM_BASE + 0x10,
            width: 64,
            height: 16,
            stride: 256,
            format: 1,
        };
        let (fb_off, fb_len) = (0x10usize, 4096usize);
        let data = std::rc::Rc::new(std::cell::RefCell::new(vec![0u8; fb_off + fb_len]));
        let log = std::rc::Rc::new(ReadLog::default());
        let mock = MockRam {
            data: data.clone(),
            fb_off,
            fb_len,
            log: log.clone(),
        };
        let scan = RamScan::new(Box::new(mock), fb_off as u64, fb_len);
        (scan, data, log, info)
    }

    /// 静态帧零全帧读：首帧一次全帧读 + 发帧；此后每轮只散读采样点、
    /// 不发帧也不全帧读。第二轮散读数恰为采样点数——隐含钉死 stage-1
    /// 与全帧签名（last_sig）同点集（错位即假阳性全帧读，本测必红）。
    #[test]
    fn two_stage_static_frame_skips_full_read() {
        let (mut scan, _data, log, info) = setup();
        assert!(scan.try_next_frame(&info).is_some(), "首帧必发");
        assert_eq!(log.full.get(), 1, "首帧恰一次全帧读");
        let probe_reads = scan.sample_offs.len();
        assert!(probe_reads > 0);
        assert!(scan.try_next_frame(&info).is_none(), "静态帧不发帧");
        assert_eq!(log.full.get(), 1, "静态帧零全帧读");
        assert_eq!(
            log.small.get(),
            probe_reads * 2,
            "两轮各恰一轮采样散读"
        );
    }

    /// 采样点变化 → 全帧读 + 发帧；顺带钉 BGRX 重排（b2<<16|b1<<8|b0）。
    #[test]
    fn two_stage_changed_sample_point_full_read() {
        let (mut scan, data, log, info) = setup();
        assert!(scan.try_next_frame(&info).is_some());
        // 采样点 8（像素 2 首字节）翻转。
        data.borrow_mut()[0x10 + 8] = 0xB0;
        let f = scan.try_next_frame(&info).expect("采样点变化必发帧");
        assert_eq!(log.full.get(), 2, "变化帧恰再来一次全帧读");
        assert_eq!(f.pixels[2], 0x0000_00B0, "BGRX 首字节落蓝通道");
        assert_eq!(f.seq, 2);
    }

    /// 非采样点变化不检出：512 点采样对零星小改动的盲区是既有取舍
    /// （检出率 ~2KB/窗口），两段式与旧单段式同盲区——语义零漂移钉板。
    #[test]
    fn two_stage_off_sample_change_still_missed() {
        let (mut scan, data, log, info) = setup();
        assert!(scan.try_next_frame(&info).is_some());
        // 偏移 4 不在采样集（步长 8B：0、8、16 …）。
        data.borrow_mut()[0x10 + 4] = 0xFF;
        assert!(scan.try_next_frame(&info).is_none(), "盲区改动不发帧");
        assert_eq!(log.full.get(), 1, "盲区改动零全帧读");
    }
}
