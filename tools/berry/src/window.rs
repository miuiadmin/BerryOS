//! Native device window: winit + softbuffer. The window owns presentation and
//! input mapping; it never knows whether frames came from VNC or shared memory.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::commands::{ScaleMode, ShellArgs};
use crate::device::Device;
use crate::display::frame::{Frame, InputEvent};
use crate::display::scale::{self, Fit};
use crate::error::{BerryError, Result};
use crate::input;
use crate::profile::Profile;

pub fn run(args: ShellArgs) -> Result<()> {
    let profile = crate::profile::get(&args.profile)?;
    let device = Device::start(
        args.image,
        profile,
        &args.accel,
        args.memory,
        args.display_mode,
        args.keep,
        "shell",
    )?;
    let event_loop = EventLoop::new()
        .map_err(|e| BerryError::device(format!("event loop: {e}")))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(device, profile, args.scale);
    if !args.timeout.is_zero() {
        app.deadline = Some(Instant::now() + args.timeout);
    }
    event_loop
        .run_app(&mut app)
        .map_err(|e| BerryError::device(format!("window: {e}")))?;
    app.device.stop();
    Ok(())
}

struct App {
    device: Device,
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
}

impl App {
    fn new(device: Device, profile: Profile, scale: ScaleMode) -> Self {
        Self {
            device,
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
        }
    }

    fn poll_frame(&mut self) -> bool {
        match self.device.source_mut().try_next_frame() {
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
        let fit = scale::fit(frame.width, frame.height, size.width, size.height, integer);
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
        if let Some((x, y)) =
            scale::window_to_guest(px as u32, py as u32, fit, frame.width, frame.height)
        {
            let _ = self
                .device
                .source_mut()
                .send_input(InputEvent::Pointer { x, y, buttons });
        }
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
                .device
                .source_mut()
                .send_input(InputEvent::Key { keysym: ks, down: pressed });
        }
        false
    }

    fn reset(&mut self) {
        if self.device.reset().is_ok() {
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
                self.device.stop();
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                if self.mouse_down {
                    self.pointer(position.x, position.y, 1);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => {
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
                    TouchPhase::Started => self.send_touch(x, y, true),
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
            || self.device.stop_requested()
            || self.deadline.is_some_and(|d| Instant::now() >= d)
        {
            self.device.stop();
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
