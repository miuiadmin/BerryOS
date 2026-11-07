//! UITEST —— GUI F1 的 AppRoom 真链探针。
//!
//! 本 app 故意不写像素、不包含 GUI wire 编解码、不映射 VMO。它只驱动
//! `berry-room` 完成 env → WM session → 窗口 → 零页 frame → 输入 →
//! destroy → exit，作为后续 UI SDK 迁移的宿主契约样本。

#![no_std]
#![no_main]

use berry_room::{AppRoom, RoomConfig};

fn append_decimal(out: &mut [u8], mut value: u32) -> usize {
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    if value == 0 {
        digits[0] = b'0';
        count = 1;
    }
    while value != 0 && count < digits.len() {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    let mut written = 0usize;
    while count > 0 && written < out.len() {
        count -= 1;
        out[written] = digits[count];
        written += 1;
    }
    written
}

fn geometry_line(width: usize, height: usize) -> ([u8; 40], usize) {
    let mut line = [0u8; 40];
    let head = b"UITESTWIN ok w=";
    line[..head.len()].copy_from_slice(head);
    let mut n = head.len();
    n += append_decimal(&mut line[n..], width as u32);
    let sep = b" h=";
    line[n..n + sep.len()].copy_from_slice(sep);
    n += sep.len();
    n += append_decimal(&mut line[n..], height as u32);
    line[n] = b'\n';
    (line, n + 1)
}

fn input_line(x: u32, y: u32) -> ([u8; 48], usize) {
    let mut line = [0u8; 48];
    let head = b"UITESTINPUT ok x=";
    line[..head.len()].copy_from_slice(head);
    let mut n = head.len();
    n += append_decimal(&mut line[n..], x);
    let sep = b" y=";
    line[n..n + sep.len()].copy_from_slice(sep);
    n += sep.len();
    n += append_decimal(&mut line[n..], y);
    line[n] = b'\n';
    (line, n + 1)
}

fn fail(room: &AppRoom, what: &str) -> ! {
    let mut line = [0u8; 64];
    let head = b"uitest: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: u32) -> ! {
    let config = RoomConfig {
        argv0: b"bin/uitest",
        uses_fs: false,
    };
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => {
            // env 失败时没有可靠 console；非 0 退出让标记缺失兜底。
            berry_room::panic_exit(134)
        }
    };
    room.print(b"UITEST up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(e) => fail(&room, e.as_str()),
    };
    room.print(b"UITESTROOM ok\n");
    let (line, line_len) = geometry_line(width, height);
    room.print(&line[..line_len]);

    if room.submit(1).is_err() {
        fail(&room, "submit");
    }
    room.print(b"UITESTFRAME ok\n");

    if room.boot_probe() {
        room.print(b"UITESTPROBE ok\n");
    } else {
        let event = match room.wait_pointer_up() {
            Ok(event) => event,
            Err(e) => fail(&room, e.as_str()),
        };
        let (input, len) = input_line(event.x, event.y);
        room.print(&input[..len]);
    }

    if room.destroy().is_err() {
        fail(&room, "destroy");
    }
    room.print(b"UITESTDESTROY ok\n");
    room.print(b"UITESTEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // no_std release panic=abort 家族约定：退出码 134。
    berry_room::panic_exit(134)
}
