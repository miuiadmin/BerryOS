//! Native device window: winit + softbuffer. The window owns presentation and
//! input mapping; it never knows whether frames came from VNC or shared memory.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::commands::{self, ScaleMode, ShellArgs};
use crate::control;
use crate::device::Device;
use crate::display::frame::{Frame, InputEvent, ScreenSource};
use crate::display::vnc::VncSource;
use crate::display::scale::{self, Fit};
use crate::error::{BerryError, Result};
use crate::input;
use crate::profile::Profile;

const TOOLBAR_H: u32 = 48;
const TOOLBAR_LABELS: [&str; 5] = ["SHOT", "RES", "BOOT", "OFF", "FULL"];
const TOOLBAR_TIPS: [&str; 5] = ["SCREENSHOT", "PROFILE", "REBOOT", "POWER", "FULLSCREEN"];

fn toolbar_buttons(width: u32) -> [(u32, u32); 5] {
    let bw = 60u32;
    let gap = 6u32;
    let total = 5 * bw + 4 * gap;
    let mut x = width.saturating_sub(total + 8);
    let mut out = [(0, 0); 5];
    for i in 0..5 {
        out[i] = (x, bw);
        x += bw + gap;
    }
    out
}

fn toolbar_hit(width: u32, height: u32, x: f64, y: f64) -> Option<usize> {
    let bar_top = height.saturating_sub(TOOLBAR_H);
    if y < bar_top as f64 {
        return None;
    }
    let by = bar_top + 8;
    for (i, (bx, bw)) in toolbar_buttons(width).iter().enumerate() {
        if x >= *bx as f64
            && x < (*bx + *bw) as f64
            && y >= by as f64
            && y < (by + 32) as f64
        {
            return Some(i);
        }
    }
    None
}

fn glyph(ch: char, row: usize) -> u8 {
    let bits: [u8; 7] = match ch {
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        _ => [0; 7],
    };
    bits[row]
}

fn draw_text(buf: &mut [u32], width: u32, height: u32, x: u32, y: u32, text: &str) {
    let fg = 0x00E8_ECF0;
    for (ci, ch) in text.chars().enumerate() {
        for row in 0..7 {
            let bits = glyph(ch, row);
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = x + ci as u32 * 12 + col * 2 + dx;
                            let py = y + row as u32 * 2 + dy;
                            if px < width && py < height {
                                buf[(py * width + px) as usize] = fg;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn menu_rect(width: u32, height: u32) -> (u32, u32, u32, u32) {
    let count = crate::profile::PROFILES.len() as u32;
    let w = 220u32;
    let h = count * 24 + 8;
    let x = width.saturating_sub(w + 8);
    let y = height.saturating_sub(TOOLBAR_H + h + 8);
    (x, y, w, h)
}

fn menu_hit(width: u32, height: u32, x: f64, y: f64) -> Option<usize> {
    let (mx, my, mw, mh) = menu_rect(width, height);
    if x < mx as f64 || x >= (mx + mw) as f64 || y < my as f64 || y >= (my + mh) as f64 {
        return None;
    }
    let row = ((y - my as f64) / 24.0) as usize;
    if row < crate::profile::PROFILES.len() {
        Some(row)
    } else {
        None
    }
}

fn draw_menu(buf: &mut [u32], width: u32, height: u32, current: &str) {
    let (mx, my, mw, mh) = menu_rect(width, height);
    for y in my..(my + mh).min(height) {
        for x in mx..(mx + mw).min(width) {
            buf[(y * width + x) as usize] = 0x0022_282E;
        }
    }
    for (i, p) in crate::profile::PROFILES.iter().enumerate() {
        let y = my + 4 + i as u32 * 24;
        if p.id == current {
            for yy in y..(y + 22).min(height) {
                for x in (mx + 2)..(mx + mw - 2).min(width) {
                    buf[(yy * width + x) as usize] = 0x0034_4A3A;
                }
            }
        }
        draw_text(buf, width, height, mx + 10, y + 5, &p.id.to_uppercase());
    }
}

fn draw_toolbar(buf: &mut [u32], width: u32, height: u32, armed: bool, hover: Option<usize>) {
    let bar_top = height.saturating_sub(TOOLBAR_H);
    let bar = 0x0018_1C20;
    for y in bar_top..height {
        let start = (y * width) as usize;
        let end = start + width as usize;
        if end <= buf.len() {
            buf[start..end].fill(bar);
        }
    }
    let by = bar_top + 8;
    for (i, (bx, bw)) in toolbar_buttons(width).iter().enumerate() {
        let btn = if i == 3 && armed {
            0x008B_2E2E
        } else if hover == Some(i) {
            0x003A_424A
        } else {
            0x002C_3238
        };
        for y in by..(by + 32).min(height) {
            for x in *bx..(*bx + *bw).min(width) {
                buf[(y * width + x) as usize] = btn;
            }
        }
        let label = TOOLBAR_LABELS[i];
        let tw = label.len() as u32 * 12;
        let tx = bx + (bw.saturating_sub(tw)) / 2;
        draw_text(buf, width, height, tx, by + 9, label);
    }
}

fn draw_tooltip(buf: &mut [u32], width: u32, height: u32, hover: usize) {
    let Some((bx, bw)) = toolbar_buttons(width).get(hover).copied() else {
        return;
    };
    let text = TOOLBAR_TIPS[hover];
    let tw = text.len() as u32 * 12;
    let bw_tip = tw + 10;
    let x = bx.saturating_add(bw / 2).saturating_sub(bw_tip / 2).min(width.saturating_sub(bw_tip));
    let y = height.saturating_sub(TOOLBAR_H + 26);
    for yy in y..(y + 20).min(height) {
        for xx in x..(x + bw_tip).min(width) {
            buf[(yy * width + xx) as usize] = 0x002A_3036;
        }
    }
    draw_text(buf, width, height, x + 5, y + 3, text);
}

pub fn run(args: ShellArgs) -> Result<()> {
    let profile = crate::profile::get(&args.profile)?;
    let image = args.image.clone();
    let accel = args.accel.clone();
    let memory = args.memory;
    let keep = args.keep;
    let session = if let Some(id) = &args.attach {
        let run = commands::find_run(Some(id))?;
        if !crate::control::is_up(&run) {
            return Err(BerryError::device(format!(
                "session {} has no control endpoint",
                run.display()
            ))
            .with_hint("run is stale; try `berry device cleanup`, then start a new session"));
        }
        let source = connect_vnc(&run)?;
        Session::Attach(Attached {
            run,
            source,
            last_poll: Instant::now(),
            last_reconnect: Instant::now(),
            down_since: None,
            dead: false,
        })
    } else {
        Session::Host(Device::start(
            args.image,
            profile.clone(),
            &args.accel,
            args.memory,
            args.display_mode,
            args.keep,
            "shell",
        )?)
    };
    let event_loop = EventLoop::new()
        .map_err(|e| BerryError::device(format!("event loop: {e}")))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(session, profile, args.scale, image, accel, memory, keep);
    if !args.timeout.is_zero() {
        app.deadline = Some(Instant::now() + args.timeout);
    }
    event_loop
        .run_app(&mut app)
        .map_err(|e| BerryError::device(format!("window: {e}")))?;
    app.session.stop();
    Ok(())
}

fn connect_vnc(run: &std::path::Path) -> Result<VncSource> {
    let ep = crate::commands::vnc_endpoint(run)?;
    VncSource::connect(&ep).map_err(|e| {
        BerryError::device(format!("attach VNC {}: {e}", ep.display()))
            .with_hint("run may be stale; try `berry device cleanup` then start again")
    })
}

enum Session {
    Host(Device),
    Attach(Attached),
}

struct Attached {
    run: std::path::PathBuf,
    source: VncSource,
    last_poll: Instant,
    last_reconnect: Instant,
    down_since: Option<Instant>,
    dead: bool,
}

impl Attached {
    /// Same-run host restart: control/VNC disappear briefly. Retry the VNC
    /// source after control is back instead of treating it as a dead session.
    fn try_reconnect(&mut self) {
        if self.last_reconnect.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_reconnect = Instant::now();
        if !control::is_up(&self.run) {
            return;
        }
        if let Ok(src) = connect_vnc(&self.run) {
            self.source = src;
            self.down_since = None;
        }
    }
}

impl Session {
    fn source_mut(&mut self) -> &mut dyn ScreenSource {
        match self {
            Self::Host(d) => d.source_mut(),
            Self::Attach(a) => &mut a.source,
        }
    }

    fn reset(&mut self) -> Result<()> {
        match self {
            Self::Host(d) => d.reset(),
            Self::Attach(a) => {
                control::request(&a.run, "power", serde_json::json!({"action":"reboot"}))?;
                Ok(())
            }
        }
    }

    fn stop_requested(&mut self) -> bool {
        match self {
            Self::Host(d) => d.stop_requested(),
            Self::Attach(a) => {
                if a.dead {
                    return true;
                }
                if a.last_poll.elapsed() < Duration::from_secs(1) {
                    return false;
                }
                a.last_poll = Instant::now();
                match control::request(&a.run, "status", serde_json::json!({})) {
                    Ok(v) => {
                        let state = v.get("state").and_then(|s| s.as_str()).unwrap_or("");
                        if state == "stopped" || state == "exited" {
                            a.dead = true;
                            true
                        } else if state == "rebooting" || state == "starting" {
                            a.down_since.get_or_insert_with(Instant::now);
                            a.try_reconnect();
                            false
                        } else {
                            if a.down_since.is_some() {
                                a.try_reconnect();
                            }
                            false
                        }
                    }
                    Err(_) => {
                        let saved = crate::run_dir::read_state(&a.run).unwrap_or_default();
                        if matches!(saved.as_str(), "stopped" | "stopping" | "failed") {
                            a.dead = true;
                            return true;
                        }
                        let since = *a.down_since.get_or_insert_with(Instant::now);
                        if since.elapsed() > Duration::from_secs(45) {
                            a.dead = true;
                            true
                        } else {
                            a.try_reconnect();
                            false
                        }
                    }
                }
            }
        }
    }

    fn stop(&mut self) {
        match self {
            Self::Host(d) => d.stop(),
            // Attach owns no QEMU: closing the window detaches, session stays.
            Self::Attach(_) => {}
        }
    }

    fn power_off(&mut self) {
        match self {
            Self::Host(d) => d.power_off(),
            Self::Attach(a) => {
                let _ = control::request(&a.run, "power", serde_json::json!({"action":"off"}));
                a.dead = true;
            }
        }
    }
}

struct App {
    session: Session,
    profile: Profile,
    scale: ScaleMode,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    last_frame: Option<Frame>,
    last_fit: Option<Fit>,
    cursor: Option<(f64, f64)>,
    mouse_down: bool,
    mods: ModifiersState,
    show_info: bool,
    title: String,
    deadline: Option<Instant>,
    quit: bool,
    power_armed: Option<Instant>,
    hover: Option<usize>,
    menu_open: bool,
    image: Option<PathBuf>,
    accel: String,
    memory: Option<u32>,
    keep: bool,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    fn new(
        session: Session,
        profile: Profile,
        scale: ScaleMode,
        image: Option<PathBuf>,
        accel: String,
        memory: Option<u32>,
        keep: bool,
    ) -> Self {
        Self {
            session,
            profile,
            scale,
            window: None,
            context: None,
            surface: None,
            last_frame: None,
            last_fit: None,
            cursor: None,
            mouse_down: false,
            mods: ModifiersState::empty(),
            show_info: false,
            title: format!("BerryOS — {}", profile.id),
            deadline: None,
            quit: false,
            power_armed: None,
            hover: None,
            menu_open: false,
            image,
            accel,
            memory,
            keep,
        }
    }

    fn poll_frame(&mut self) -> bool {
        match self.session.source_mut().try_next_frame() {
            Some(frame) => {
                self.last_frame = Some(frame);
                true
            }
            None => false,
        }
    }

    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let Some(frame) = self.last_frame.as_ref() else {
            return;
        };
        let integer = matches!(self.scale, ScaleMode::Integer(_));
        let guest_h = size.height.saturating_sub(TOOLBAR_H).max(1);
        let fit = scale::fit(frame.width, frame.height, size.width, guest_h, integer);
        self.last_fit = Some(fit);
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        scale::render(frame, &fit, size.width, size.height, &mut buffer, 0x0010_1418);
        draw_toolbar(
            &mut buffer,
            size.width,
            size.height,
            self.power_armed.is_some(),
            self.hover,
        );
        if !self.menu_open {
            if let Some(h) = self.hover {
                draw_tooltip(&mut buffer, size.width, size.height, h);
            }
        }
        if self.menu_open {
            draw_menu(&mut buffer, size.width, size.height, self.profile.id);
        }
        if buffer.present().is_err() {
            return;
        }
        let title = format!(
            "BerryOS — {} ({}x{}){}",
            self.profile.id,
            frame.width,
            frame.height,
            if self.show_info { " · VNC" } else { "" }
        );
        if title != self.title {
            self.title = title;
            window.set_title(&self.title);
        }
    }

    fn pointer(&mut self, px: f64, py: f64, buttons: u8) {
        let (Some(frame), Some(fit)) = (self.last_frame.as_ref(), self.last_fit.as_ref()) else {
            return;
        };
        if px < 0.0 || py < 0.0 {
            return;
        }
        if let Some(w) = &self.window {
            let size = w.inner_size();
            if py >= size.height.saturating_sub(TOOLBAR_H) as f64 {
                return;
            }
        }
        if let Some((x, y)) =
            scale::window_to_guest(px as u32, py as u32, fit, frame.width, frame.height)
        {
            let _ = self
                .session
                .source_mut()
                .send_input(InputEvent::Pointer { x, y, buttons });
        }
    }

    fn select_profile(&mut self, i: usize) {
        let Some(p) = crate::profile::PROFILES.get(i).copied() else {
            return;
        };
        self.menu_open = false;
        if p.id == self.profile.id {
            self.request_redraw();
            return;
        }
        match &mut self.session {
            Session::Host(dev) => {
                dev.stop();
                match Device::start(
                    self.image.clone(),
                    p,
                    &self.accel,
                    self.memory,
                    None,
                    self.keep,
                    "shell",
                ) {
                    Ok(dev) => self.session = Session::Host(dev),
                    Err(e) => {
                        eprintln!("resolution restart failed: {e}");
                        self.quit = true;
                        return;
                    }
                }
            }
            Session::Attach(a) => {
                let old_profile = self.profile.id;
                let mut applied = p;
                match crate::commands::restart_profile(&a.run, p.id) {
                    Ok(new_run) => match connect_vnc(&new_run) {
                        Ok(src) => {
                            a.run = new_run;
                            a.source = src;
                            a.dead = false;
                            a.last_poll = Instant::now();
                            a.last_reconnect = Instant::now();
                            a.down_since = None;
                        }
                        Err(e) => {
                            eprintln!(
                                "resolution reattach failed: {e}; rolling back to {old_profile}"
                            );
                            match crate::commands::restart_profile(&new_run, old_profile)
                                .and_then(|r| connect_vnc(&r).map(|src| (r, src)))
                            {
                                Ok((run, src)) => {
                                    a.run = run;
                                    a.source = src;
                                    a.dead = false;
                                    a.last_poll = Instant::now();
                                    applied = self.profile;
                                }
                                Err(e2) => {
                                    eprintln!("resolution rollback failed: {e2}");
                                    self.quit = true;
                                    return;
                                }
                            }
                        }
                    },
                    Err(e) => {
                        // New-first: the old host is still alive on failure.
                        eprintln!("resolution restart failed: {e}; old host kept");
                        a.dead = false;
                        self.menu_open = false;
                        self.request_redraw();
                        return;
                    }
                }
                self.profile = applied;
            }
        }
        self.last_frame = None;
        self.last_fit = None;
        self.power_armed = None;
        self.title = format!("BerryOS — {}", self.profile.id);
        if let Some(w) = &self.window {
            w.set_title(&self.title);
            w.request_redraw();
        }
    }

    fn toolbar_click(&mut self, x: f64, y: f64) -> bool {
        let Some(w) = &self.window else {
            return false;
        };
        let size = w.inner_size();
        if self.menu_open {
            if let Some(i) = menu_hit(size.width, size.height, x, y) {
                self.select_profile(i);
                return true;
            }
            if y < size.height.saturating_sub(TOOLBAR_H) as f64 {
                self.menu_open = false;
                self.request_redraw();
                return true;
            }
        }
        let Some(i) = toolbar_hit(size.width, size.height, x, y) else {
            return y >= size.height.saturating_sub(TOOLBAR_H) as f64;
        };
        if i != 3 {
            self.power_armed = None;
        }
        match i {
            0 => self.screenshot(),
            1 => {
                self.menu_open = !self.menu_open;
            }
            2 => self.reset(),
            3 => {
                let armed = self
                    .power_armed
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(3));
                if armed {
                    self.session.power_off();
                    self.quit = true;
                } else {
                    self.power_armed = Some(Instant::now());
                }
            }
            4 => {
                let full = w.fullscreen().is_some();
                w.set_fullscreen(if full {
                    None
                } else {
                    Some(Fullscreen::Borderless(None))
                });
            }
            _ => {}
        }
        self.request_redraw();
        true
    }

    fn key(&mut self, event: &winit::event::KeyEvent, pressed: bool) -> bool {
        // Host shortcuts are intercepted; everything else goes to the guest.
        if pressed {
            if let Key::Character(s) = &event.logical_key {
                let s = s.to_lowercase();
                if (self.mods.control_key() || self.mods.super_key()) && s == "s" {
                    self.screenshot();
                    return true;
                }
                if (self.mods.control_key() || self.mods.super_key()) && s == "r" {
                    self.reset();
                    return true;
                }
                if (self.mods.control_key() || self.mods.super_key()) && s == "q" {
                    self.quit = true;
                    return true;
                }
                if !self.mods.control_key() && !self.mods.super_key() && s == "i" {
                    self.show_info = !self.show_info;
                    self.request_redraw();
                    return true;
                }
                if !self.mods.control_key() && !self.mods.super_key() && s == "f" {
                    if let Some(w) = &self.window {
                        let full = w.fullscreen().is_some();
                        w.set_fullscreen(if full {
                            None
                        } else {
                            Some(Fullscreen::Borderless(None))
                        });
                    }
                    return true;
                }
            }
            if matches!(event.logical_key, Key::Named(NamedKey::Escape))
                && self.window.as_ref().is_some_and(|w| w.fullscreen().is_some())
            {
                if let Some(w) = &self.window {
                    w.set_fullscreen(None);
                }
                return true;
            }
        }
        if let Some(ks) = input::keysym(&event.logical_key) {
            let _ = self
                .session
                .source_mut()
                .send_input(InputEvent::Key { keysym: ks, down: pressed });
        }
        false
    }

    fn reset(&mut self) {
        if self.session.reset().is_ok() {
            self.last_frame = None;
            self.request_redraw();
            println!("reset: ok");
        }
    }

    fn screenshot(&mut self) {
        let Some(frame) = self.last_frame.as_ref() else {
            println!("screenshot: no frame yet");
            return;
        };
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let name = format!("berry-shot-{}-{}.png", d.as_secs(), d.subsec_millis());
        match crate::png::write_png(std::path::Path::new(&name), frame.width, frame.height, &frame.to_rgb()) {
            Ok(()) => println!("screenshot: {name} ({}x{})", frame.width, frame.height),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn send_touch(&mut self, x: f64, y: f64, pressed: bool) {
        self.pointer(x, y, if pressed { 1 } else { 0 });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut factor = match self.scale {
            ScaleMode::Integer(n) => n as f64,
            ScaleMode::Fit => {
                if self.profile.panel_h > 1200 {
                    0.55
                } else {
                    0.8
                }
            }
        };
        if let Some(m) = event_loop.primary_monitor() {
            let s = m.size();
            let max_w = s.width as f64 * 0.9 / self.profile.panel_w as f64;
            let max_h = s.height as f64 * 0.9 / self.profile.panel_h as f64;
            factor = factor.min(max_w).min(max_h).max(0.2);
        }
        let w = (self.profile.panel_w as f64 * factor).round().max(320.0) as u32;
        let h = (self.profile.panel_h as f64 * factor).round().max(240.0) as u32;
        let attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::PhysicalSize::new(w, h))
            .with_min_inner_size(winit::dpi::PhysicalSize::new(320, 240))
            .with_resizable(true);
        let Ok(window) = event_loop.create_window(attrs) else {
            eprintln!("berry: failed to create window");
            event_loop.exit();
            return;
        };
        let window = Arc::new(window);
        let Ok(context) = Context::new(window.clone()) else {
            eprintln!("berry: softbuffer context failed");
            event_loop.exit();
            return;
        };
        let Ok(surface) = Surface::new(&context, window.clone()) else {
            eprintln!("berry: softbuffer surface failed");
            event_loop.exit();
            return;
        };
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.session.stop();
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                let hover = self.window.as_ref().and_then(|w| {
                    let s = w.inner_size();
                    toolbar_hit(s.width, s.height, position.x, position.y)
                });
                if hover != self.hover {
                    self.hover = hover;
                    self.request_redraw();
                }
                if self.mouse_down {
                    self.pointer(position.x, position.y, 1);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                if self.hover.take().is_some() {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => {
                        if let Some((x, y)) = self.cursor {
                            if self.toolbar_click(x, y) {
                                self.mouse_down = false;
                                return;
                            }
                        }
                        self.mouse_down = true;
                        if let Some((x, y)) = self.cursor {
                            self.pointer(x, y, 1);
                        }
                    }
                    ElementState::Released => {
                        self.mouse_down = false;
                        if let Some((x, y)) = self.cursor {
                            self.pointer(x, y, 0);
                        }
                    }
                }
            }
            WindowEvent::Touch(t) => {
                let (x, y) = (t.location.x, t.location.y);
                match t.phase {
                    TouchPhase::Started => {
                        if !self.toolbar_click(x, y) {
                            self.send_touch(x, y, true);
                        }
                    }
                    TouchPhase::Moved => self.send_touch(x, y, true),
                    TouchPhase::Ended | TouchPhase::Cancelled => self.send_touch(x, y, false),
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state.is_pressed();
                self.key(&event, pressed);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit
            || self.session.stop_requested()
            || self.deadline.is_some_and(|d| Instant::now() >= d)
        {
            self.session.stop();
            event_loop.exit();
            return;
        }
        if self.poll_frame() {
            self.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

#[cfg(test)]
mod toolbar_tests {
    use super::*;

    #[test]
    fn hit_test_four_buttons() {
        let (w, h) = (480u32, 1000u32);
        for (i, (bx, bw)) in toolbar_buttons(w).iter().enumerate() {
            let x = (*bx + *bw / 2) as f64;
            let y = (h - TOOLBAR_H + 24) as f64;
            assert_eq!(toolbar_hit(w, h, x, y), Some(i), "button {i}");
        }
        assert_eq!(toolbar_hit(w, h, 10.0, 500.0), None);
    }

    #[test]
    fn profile_menu_hit_and_draw() {
        let (w, h) = (480u32, 1000u32);
        let (mx, my, mw, mh) = menu_rect(w, h);
        assert!(mw >= 200 && mh >= 5 * 24);
        for i in 0..crate::profile::PROFILES.len() {
            let x = (mx + mw / 2) as f64;
            let y = (my + 4 + i as u32 * 24 + 10) as f64;
            assert_eq!(menu_hit(w, h, x, y), Some(i));
        }
        assert_eq!(menu_hit(w, h, 10.0, 500.0), None);
        let mut buf = vec![0u32; w as usize * h as usize];
        draw_menu(&mut buf, w, h, "phone");
        assert_eq!(buf[(my * w + mx) as usize], 0x0022_282E);
    }

    #[test]
    fn hover_highlight_and_tooltip() {
        let (w, h) = (480usize, 1000usize);
        let mut buf = vec![0u32; w * h];
        draw_toolbar(&mut buf, w as u32, h as u32, false, Some(0));
        let (bx, _) = toolbar_buttons(w as u32)[0];
        assert_eq!(buf[((h - TOOLBAR_H as usize) + 8) * w + bx as usize], 0x003A_424A);
        draw_tooltip(&mut buf, w as u32, h as u32, 0);
        let ty = h - TOOLBAR_H as usize - 26;
        assert!(buf[ty * w..(ty + 1) * w].iter().any(|p| *p == 0x002A_3036));
    }

    #[test]
    fn draw_bar_and_text() {
        let (w, h) = (480usize, 1000usize);
        let mut buf = vec![0u32; w * h];
        draw_toolbar(&mut buf, w as u32, h as u32, false, None);
        let bar_top = h - TOOLBAR_H as usize;
        assert_eq!(buf[bar_top * w + 2], 0x0018_1C20);
        let (bx, bw) = toolbar_buttons(w as u32)[0];
        assert_eq!(buf[(bar_top + 8) * w + bx as usize], 0x002C_3238);
        // At least one text pixel differs from the button fill.
        let text_px = (bar_top + 9..bar_top + 23)
            .flat_map(|y| (bx as usize..(bx + bw) as usize).map(move |x| (x, y)))
            .any(|(x, y)| buf[y * w + x] == 0x00E8_ECF0);
        assert!(text_px);
    }
}
