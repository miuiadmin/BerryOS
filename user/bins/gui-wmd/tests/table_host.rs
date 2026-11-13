//! RUN-K1 gui-wmd host harness v1：不启 QEMU、不造假 channel，直接驱动
//! lib 面权威 `WmTable`。I/O 由确定性 recorder 注入；本测试只证明表策略
//! 与状态转移，present relay/compositor 仍由 GUI8 真链负责。

use berry_guiproto as gp;
use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Global recorder 是进程级静态；测试以锁串行，避免跨表回复互相覆盖。
static TEST_LOCK: Mutex<()> = Mutex::new(());
use gui_wmd::table::{WmIo, WmTable};

const LAST_CAP: usize = 320;

static LAST: [AtomicU8; LAST_CAP] = [const { AtomicU8::new(0) }; LAST_CAP];
static LAST_LEN: AtomicUsize = AtomicUsize::new(0);
static CLOCK: AtomicU64 = AtomicU64::new(1);
static WRAP_DIAG: AtomicUsize = AtomicUsize::new(0);
static CLOSED: AtomicUsize = AtomicUsize::new(0);
static TRAPPED: AtomicUsize = AtomicUsize::new(0);

struct TestIo;

impl WmIo for TestIo {
    fn now_ns(&self) -> u64 {
        CLOCK.fetch_add(1, Ordering::SeqCst)
    }

    fn send(&self, _channel: berry_sys::Handle, reply: &[u8]) {
        let len = reply.len().min(LAST_CAP);
        for (dst, src) in LAST[..len].iter().zip(reply[..len].iter()) {
            dst.store(*src, Ordering::SeqCst);
        }
        LAST_LEN.store(len, Ordering::SeqCst);
    }

    fn status(&self, channel: berry_sys::Handle, status: u8) {
        self.send(channel, &[status]);
    }

    fn close_handles(&self, _handles: &[berry_sys::Handle]) {
        CLOSED.fetch_add(_handles.len(), Ordering::SeqCst);
    }

    fn diag_wrap_clear(&self) {
        WRAP_DIAG.fetch_add(1, Ordering::SeqCst);
    }

    fn trap(&self) -> ! {
        TRAPPED.fetch_add(1, Ordering::SeqCst);
        panic!("WmTable reached fatal trap in host harness");
    }
}

static TEST_IO: TestIo = TestIo;

fn reset_io() {
    LAST_LEN.store(0, Ordering::SeqCst);
    CLOCK.store(1, Ordering::SeqCst);
    WRAP_DIAG.store(0, Ordering::SeqCst);
    CLOSED.store(0, Ordering::SeqCst);
    TRAPPED.store(0, Ordering::SeqCst);
}

fn last() -> Vec<u8> {
    let len = LAST_LEN.load(Ordering::SeqCst);
    LAST[..len].iter().map(|b| b.load(Ordering::SeqCst)).collect()
}

fn table() -> WmTable {
    reset_io();
    WmTable::new(&TEST_IO, 800, 600)
}

fn wm(
    t: &mut WmTable,
    owner: u16,
    client_type: u8,
    msg: &[u8],
) -> Vec<u8> {
    t.handle_wm(77, owner, client_type, msg, &[]);
    last()
}

fn create(
    t: &mut WmTable,
    owner: u16,
    client_type: u8,
    layer: u8,
    flags: u8,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> u16 {
    let req = gp::WmCreateReq {
        layer,
        flags,
        parent: 0,
        group: 0,
        x,
        y,
        w,
        h,
    };
    let mut buf = [0; 64];
    let n = gp::encode_wm_create_req(&mut buf, &req).unwrap();
    let reply = wm(t, owner, client_type, &buf[..n]);
    gp::parse_wm_create_ack(&reply).unwrap()
}

fn destroy(t: &mut WmTable, owner: u16, id: u16) -> u8 {
    let mut buf = [0; 64];
    let n = gp::encode_wm_destroy_req(&mut buf, id);
    let reply = wm(t, owner, gp::WM_CLIENT_APP, &buf[..n]);
    reply[0]
}

fn config_poll(t: &mut WmTable, owner: u16) -> gp::WmConfigEvent {
    let mut buf = [0; 64];
    let n = gp::encode_wm_config_poll_req(&mut buf);
    let reply = wm(t, owner, gp::WM_CLIENT_APP, &buf[..n]);
    gp::parse_wm_config_event_rep(&reply).unwrap().ok().unwrap()
}

fn config_ack(t: &mut WmTable, owner: u16, id: u16, serial: u32) -> u8 {
    let mut buf = [0; 64];
    let n = gp::encode_wm_config_ack_req(
        &mut buf,
        &gp::WmConfigAckReq { window_id: id, serial },
    )
    .unwrap();
    let reply = wm(t, owner, gp::WM_CLIENT_APP, &buf[..n]);
    reply[0]
}

#[test]
fn host_matrix_pins_create_focus_owner_and_destroy_owned() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let a = create(&mut t, 1, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 0, 0, 320, 240);
    let b = create(&mut t, 2, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 320, 0, 320, 240);
    assert_ne!(a, b);
    assert_eq!(t.focused(), Some((2, b)));

    assert!(t.focus_from_pointer(a));
    assert_eq!(t.focused(), Some((1, a)));
    assert!(!t.focus_from_pointer(a)); // 已聚焦 no-op

    let mut buf = [0; 64];
    let n = gp::encode_wm_destroy_req(&mut buf, b);
    let denied = wm(&mut t, 1, gp::WM_CLIENT_APP, &buf[..n]);
    assert_eq!(denied[0], gp::ST_DENIED);

    assert_eq!(destroy(&mut t, 2, b), gp::ST_OK);
    assert_eq!(t.focused(), Some((1, a)));
    assert_eq!(t.destroy_owned(1), 1);
    assert_eq!(t.focused(), None);
}

#[test]
fn host_matrix_pins_config_serial_and_surface_gate() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let id = create(&mut t, 9, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 10, 20, 300, 200);
    assert_eq!(t.config_ready(9, id), Some(false));
    assert_eq!(t.surface_geometry(9, id), None);

    let event = config_poll(&mut t, 9);
    assert_eq!(event.window_id, id);
    assert_eq!(event.cause, gp::WM_CONFIG_CAUSE_CREATED);
    assert_eq!(event.serial, t.version());
    assert_eq!(config_ack(&mut t, 9, id, event.serial + 1), gp::ST_DENIED);
    assert_eq!(config_ack(&mut t, 9, id, event.serial), gp::ST_OK);
    assert_eq!(t.config_ready(9, id), Some(true));

    let req = gp::WmSetReq {
        mask: gp::WM_SET_F_GEOM,
        window_id: id,
        x: 10,
        y: 20,
        w: 240,
        h: 160,
        vis: gp::WM_VIS_SHOW,
        z: 0,
    };
    let mut buf = [0; 64];
    let n = gp::encode_wm_set_req(&mut buf, &req).unwrap();
    assert_eq!(wm(&mut t, 9, gp::WM_CLIENT_APP, &buf[..n])[0], gp::ST_OK);
    assert_eq!(t.config_ready(9, id), Some(false));

    let changed = config_poll(&mut t, 9);
    assert_eq!(changed.cause, gp::WM_CONFIG_CAUSE_GEOMETRY);
    assert_eq!(changed.serial, t.version());
    assert_eq!(config_ack(&mut t, 9, id, changed.serial), gp::ST_OK);
    let geometry = t.surface_geometry(9, id).unwrap();
    assert_eq!(geometry.raw_bounds(), (10, 20, 240, 160));
}

#[test]
fn host_matrix_pins_avoid_sys_occlusion_geometry() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let system = create(
        &mut t,
        1,
        gp::WM_CLIENT_SYSTEM,
        gp::WM_LAYER_SYSTEM,
        0,
        0,
        0,
        800,
        24,
    );
    let app = create(
        &mut t,
        2,
        gp::WM_CLIENT_APP,
        gp::WM_LAYER_APP,
        gp::WM_CREATE_F_AVOID_SYS,
        0,
        0,
        800,
        600,
    );

    for owner_and_id in [(1, system), (2, app)] {
        let event = config_poll(&mut t, owner_and_id.0);
        assert_eq!(config_ack(&mut t, owner_and_id.0, owner_and_id.1, event.serial), gp::ST_OK);
    }
    let geometry = t.surface_geometry(2, app).unwrap();
    assert_eq!(geometry.raw_bounds(), (0, 0, 800, 600));
    assert_eq!(geometry.visible_local(), berry_guiproto::Rect { x: 0, y: 24, w: 800, h: 576 });
    assert_eq!(geometry.occlusion_insets(), (0, 24, 0, 0));
}

#[test]
fn host_matrix_pins_table_wait_snapshot_and_forget_parked() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let id = create(&mut t, 4, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 0, 0, 100, 100);
    let version = t.version();
    let event = config_poll(&mut t, 4);
    assert_eq!(config_ack(&mut t, 4, id, event.serial), gp::ST_OK);

    // ACK 已写 1B OK；清 recorder 后同 version WAIT 应 park 而非立即应答。
    LAST_LEN.store(0, Ordering::SeqCst);
    t.wait(99, version);
    assert_eq!(LAST_LEN.load(Ordering::SeqCst), 0);

    // 表变更向 parked 通道应答 snapshot；destroy_owned 本身不再发 1B 状态。
    assert_eq!(t.destroy_owned(4), 1);
    let snapshot = gp::parse_wm_snapshot_rep(&last()).unwrap().ok().unwrap();
    assert!(snapshot.version != version);
    assert_eq!(snapshot.count, 0);

    // 后到 WAIT 覆盖单车位；forget 后再 bump 不应写已关闭 channel。
    let id2 = create(&mut t, 5, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 0, 0, 80, 60);
    let v2 = t.version();
    t.wait(99, v2);
    t.forget_parked(99);
    LAST_LEN.store(0, Ordering::SeqCst);
    assert_eq!(destroy(&mut t, 5, id2), gp::ST_OK);
    assert_eq!(LAST_LEN.load(Ordering::SeqCst), 1); // 只见 destroy 的 OK
}

#[test]
fn host_matrix_pins_hide_show_refocus_and_config_cause() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let a = create(&mut t, 1, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 0, 0, 100, 100);
    let b = create(&mut t, 1, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 110, 0, 100, 100);
    assert_eq!(t.focused(), Some((1, b)));
    for id in [a, b] {
        let initial = config_poll(&mut t, 1);
        assert_eq!(initial.cause, gp::WM_CONFIG_CAUSE_CREATED);
        assert_eq!(config_ack(&mut t, 1, id, initial.serial), gp::ST_OK);
    }

    let hide = gp::WmSetReq {
        mask: gp::WM_SET_F_VIS,
        window_id: b,
        x: 110,
        y: 0,
        w: 100,
        h: 100,
        vis: gp::WM_VIS_HIDE,
        z: 0,
    };
    let mut buf = [0; 64];
    let n = gp::encode_wm_set_req(&mut buf, &hide).unwrap();
    assert_eq!(wm(&mut t, 1, gp::WM_CLIENT_APP, &buf[..n])[0], gp::ST_OK);
    assert_eq!(t.focused(), Some((1, a)));
    assert_eq!(t.surface_geometry(1, b), None);
    let hidden = config_poll(&mut t, 1);
    assert_eq!(hidden.window_id, b);
    assert_eq!(hidden.cause, gp::WM_CONFIG_CAUSE_VISIBILITY);
    assert!(!hidden.visible);
    assert_eq!(config_ack(&mut t, 1, b, hidden.serial), gp::ST_OK);

    let show = gp::WmSetReq { vis: gp::WM_VIS_SHOW, ..hide };
    let n = gp::encode_wm_set_req(&mut buf, &show).unwrap();
    assert_eq!(wm(&mut t, 1, gp::WM_CLIENT_APP, &buf[..n])[0], gp::ST_OK);
    assert_eq!(t.focused(), Some((1, b)));
    let shown = config_poll(&mut t, 1);
    assert_eq!(shown.window_id, b);
    assert_eq!(shown.cause, gp::WM_CONFIG_CAUSE_VISIBILITY);
    assert!(shown.visible);
    assert_eq!(config_ack(&mut t, 1, b, shown.serial), gp::ST_OK);
    assert!(t.surface_geometry(1, b).is_some());
}

#[test]
fn host_matrix_pins_compositor_destination_clipping() {
    use gui_wmd::table::composite_clip;

    let inside = composite_clip((10, 20), gp::Rect { x: 5, y: 6, w: 30, h: 40 }, 800, 600).unwrap();
    assert_eq!(inside.dst, gp::Rect { x: 15, y: 26, w: 30, h: 40 });
    assert_eq!(inside.src, gp::Rect { x: 5, y: 6, w: 30, h: 40 });

    let right = composite_clip((790, 20), gp::Rect { x: 0, y: 0, w: 100, h: 40 }, 800, 600).unwrap();
    assert_eq!(right.dst, gp::Rect { x: 790, y: 20, w: 10, h: 40 });
    assert_eq!(right.src, gp::Rect { x: 0, y: 0, w: 10, h: 40 });

    let bottom = composite_clip((0, 590), gp::Rect { x: 0, y: 0, w: 100, h: 40 }, 800, 600).unwrap();
    assert_eq!(bottom.dst, gp::Rect { x: 0, y: 590, w: 100, h: 10 });
    assert_eq!(bottom.src, gp::Rect { x: 0, y: 0, w: 100, h: 10 });

    assert!(composite_clip((800, 0), gp::Rect { x: 0, y: 0, w: 10, h: 10 }, 800, 600).is_none());
}

#[test]
fn system_navigation_back_home_and_task_own_app_visibility() {
    let _serial = TEST_LOCK.lock().unwrap();
    let mut t = table();
    let a = create(&mut t, 1, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 0, 0, 100, 100);
    let b = create(&mut t, 2, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 110, 0, 100, 100);
    let c = create(&mut t, 3, gp::WM_CLIENT_APP, gp::WM_LAYER_APP, 0, 220, 0, 100, 100);
    assert_eq!(t.focused(), Some((3, c)));
    for (owner, id) in [(1, a), (2, b), (3, c)] {
        let event = config_poll(&mut t, owner);
        assert_eq!(config_ack(&mut t, owner, id, event.serial), gp::ST_OK);
    }

    assert_eq!(t.navigation_back(), Some(3));
    assert_eq!(t.focused(), None);
    assert!(t.surface_geometry(3, c).is_none());

    assert_eq!(t.navigation_home(), 3);
    assert_eq!(t.navigation_task(), Some(1));
    assert_eq!(t.focused(), Some((1, a)));
    assert_eq!(t.navigation_task(), Some(2));
    assert_eq!(t.focused(), Some((2, b)));
    let shown = config_poll(&mut t, 2);
    assert!(shown.visible);
    assert_eq!(config_ack(&mut t, 2, shown.window_id, shown.serial), gp::ST_OK);
    assert!(t.surface_geometry(1, a).is_none());
    assert!(t.surface_geometry(2, b).is_some());

    assert_eq!(t.navigation_home(), 3);
    assert_eq!(t.focused(), None);
    assert!(t.surface_geometry(2, b).is_none());

    assert_eq!(t.navigation_task(), Some(1));
    assert_eq!(t.focused(), Some((1, a)));
    let changed = config_poll(&mut t, 1);
    assert_eq!(changed.cause, gp::WM_CONFIG_CAUSE_VISIBILITY);
    assert!(changed.visible);
    assert_eq!(config_ack(&mut t, 1, changed.window_id, changed.serial), gp::ST_OK);
}
