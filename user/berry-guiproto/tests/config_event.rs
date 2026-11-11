//! GUI R3 config event host 协议测试：poll/ack codec、golden、严格拒面。

use berry_guiproto::{
    encode_wm_config_ack_req, encode_wm_config_event_rep,
    encode_wm_config_poll_req, parse_wm_config_ack_req,
    parse_wm_config_event_rep, parse_wm_config_poll_req, WmConfigAckReq,
    WmConfigEvent, OP_WM_CONFIG_ACK, OP_WM_CONFIG_POLL, ST_BAD, ST_EAGAIN,
    ST_OK, WM_CONFIG_CAUSE_CREATED, WM_CONFIG_CAUSE_GEOMETRY,
    WM_CONFIG_CAUSE_OCCLUSION, WM_CONFIG_CAUSE_VISIBILITY,
};

fn sample() -> WmConfigEvent {
    WmConfigEvent {
        serial: 0x0102_0304,
        window_id: 0x0506,
        cause: WM_CONFIG_CAUSE_OCCLUSION,
        visible: true,
        raw_x: 1,
        raw_y: 2,
        raw_w: 800,
        raw_h: 600,
        visible_x: 1,
        visible_y: 32,
        visible_w: 800,
        visible_h: 568,
    }
}

#[test]
fn r3_config_event_roundtrip_and_golden() {
    let ev = sample();
    let mut buf = [0u8; 64];
    let n = encode_wm_config_event_rep(&mut buf, &ev).unwrap();
    assert_eq!(n, 32);
    let expected = [
        0x00, 0x03, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01,
        0x06, 0x05, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00,
        0x20, 0x03, 0x58, 0x02, 0x01, 0x00, 0x20, 0x00,
        0x20, 0x03, 0x38, 0x02, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(&buf[..n], &expected);
    assert_eq!(parse_wm_config_event_rep(&buf[..n]), Some(Ok(ev)));

    let mut req = [0u8; 8];
    let n = encode_wm_config_poll_req(&mut req);
    assert_eq!(n, 4);
    assert_eq!(&req[..n], &[OP_WM_CONFIG_POLL, 0, 0, 0]);
    assert_eq!(parse_wm_config_poll_req(&req[..n]), Some(()));
    assert_eq!(parse_wm_config_poll_req(&req[..3]), None);
    let mut bad_reserved = req;
    bad_reserved[1] = 1;
    assert_eq!(parse_wm_config_poll_req(&bad_reserved[..4]), None);
}

#[test]
fn r3_config_ack_roundtrip_and_rejects() {
    let req = WmConfigAckReq {
        window_id: 0x0506,
        serial: 0x0102_0304,
    };
    let mut buf = [0u8; 32];
    let n = encode_wm_config_ack_req(&mut buf, &req).unwrap();
    assert_eq!(n, 12);
    let expected = [
        OP_WM_CONFIG_ACK, 0, 0, 0, 0x06, 0x05, 0, 0,
        0x04, 0x03, 0x02, 0x01,
    ];
    assert_eq!(&buf[..n], &expected);
    assert_eq!(parse_wm_config_ack_req(&buf[..n]), Some(req));

    assert_eq!(encode_wm_config_ack_req(&mut buf, &WmConfigAckReq {
        window_id: 0, serial: 1,
    }), None);
    assert_eq!(encode_wm_config_ack_req(&mut buf, &WmConfigAckReq {
        window_id: 1, serial: 0,
    }), None);
    assert_eq!(parse_wm_config_ack_req(&buf[..11]), None);
    let mut bad_op = buf;
    bad_op[0] = OP_WM_CONFIG_POLL;
    assert_eq!(parse_wm_config_ack_req(&bad_op[..n]), None);
    let mut bad_reserved = buf;
    bad_reserved[1] = 1;
    assert_eq!(parse_wm_config_ack_req(&bad_reserved[..n]), None);
    let mut bad_tail = buf;
    bad_tail[6] = 1;
    assert_eq!(parse_wm_config_ack_req(&bad_tail[..n]), None);
    let mut trailing = [0u8; 13];
    trailing[..n].copy_from_slice(&buf[..n]);
    assert_eq!(parse_wm_config_ack_req(&trailing), None);
}

#[test]
fn r3_config_poll_error_and_value_reject_matrix() {
    assert_eq!(parse_wm_config_event_rep(&[ST_EAGAIN]), Some(Err(ST_EAGAIN)));
    assert_eq!(parse_wm_config_event_rep(&[ST_BAD]), Some(Err(ST_BAD)));
    assert_eq!(parse_wm_config_event_rep(&[ST_OK]), None);
    assert_eq!(parse_wm_config_event_rep(&[ST_BAD, 0xEE]), None);

    let mut wire = [0u8; 32];
    let ev = sample();
    encode_wm_config_event_rep(&mut wire, &ev).unwrap();
    let cases: [(usize, u8, &str); 6] = [
        (1, WM_CONFIG_CAUSE_CREATED, "cause zero is legal but replaced below"),
        (1, 4, "unknown cause"),
        (2, 2, "visible bool"),
        (3, 1, "reserved byte"),
        (10, 1, "reserved window tail"),
        (28, 1, "future scale reserved"),
    ];
    // 先钉合法 cause 值全集，再跑非法/保留位矩阵。
    for cause in [
        WM_CONFIG_CAUSE_CREATED,
        WM_CONFIG_CAUSE_GEOMETRY,
        WM_CONFIG_CAUSE_VISIBILITY,
        WM_CONFIG_CAUSE_OCCLUSION,
    ] {
        let mut legal = ev;
        legal.cause = cause;
        let mut buf = [0u8; 32];
        encode_wm_config_event_rep(&mut buf, &legal).unwrap();
        assert_eq!(parse_wm_config_event_rep(&buf), Some(Ok(legal)));
    }
    for (off, value, why) in cases {
        if off == 1 && value == WM_CONFIG_CAUSE_CREATED {
            continue;
        }
        let mut bad = wire;
        bad[off] = value;
        assert_eq!(parse_wm_config_event_rep(&bad), None, "{why}");
    }

    let mut zero_serial = ev;
    zero_serial.serial = 0;
    assert_eq!(encode_wm_config_event_rep(&mut wire, &zero_serial), None);
    let mut zero_window = ev;
    zero_window.window_id = 0;
    assert_eq!(encode_wm_config_event_rep(&mut wire, &zero_window), None);

    // parse 侧同样拒绝 wire 内 serial/window=0，防止只测 encode 的假绿。
    let mut wire_zero_serial = wire;
    wire_zero_serial[4..8].fill(0);
    assert_eq!(parse_wm_config_event_rep(&wire_zero_serial), None);
    let mut wire_zero_window = wire;
    wire_zero_window[8..10].fill(0);
    assert_eq!(parse_wm_config_event_rep(&wire_zero_window), None);
    assert_eq!(parse_wm_config_event_rep(&wire[..31]), None);
    let mut trailing = [0u8; 33];
    trailing[..32].copy_from_slice(&wire);
    assert_eq!(parse_wm_config_event_rep(&trailing), None);
    let mut reserved_tail = wire;
    reserved_tail[11] = 1;
    assert_eq!(parse_wm_config_event_rep(&reserved_tail), None);
    assert_eq!(parse_wm_config_event_rep(&[0xEE]), None);
}
