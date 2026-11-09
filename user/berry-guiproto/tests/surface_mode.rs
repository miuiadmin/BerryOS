//! GUI P0 Surface Mode host 协议测试：三路径 round-trip、字节钉板、
//! 不变量与 IME composition 拒面。

use berry_guiproto::surface::*;
use berry_guiproto::{
    OP_WM_SURFACE_ESCAPE_SEED, OP_WM_SURFACE_MODE, ST_BAD, ST_DENIED, ST_UNSUPPORTED,
};

fn req(mode: u8, input: u8, ime: u8, a11y: u8) -> SurfaceModeReq {
    SurfaceModeReq {
        surface_id: 1,
        role: SURFACE_ROLE_APP,
        mode,
        input_classes: input,
        ime_mode: ime,
        a11y_mode: a11y,
    }
}

fn info(mode: u8, input: u8, ime: u8, a11y: u8, scale: u16) -> SurfaceModeInfo {
    let escape = mode != SURFACE_MODE_RETAINED;
    SurfaceModeInfo {
        surface_id: 1,
        role: SURFACE_ROLE_APP,
        mode,
        input_classes: input,
        ime_mode: ime,
        a11y_mode: a11y,
        contract_flags: SURFACE_F_FOCUS
            | SURFACE_F_RATE
            | SURFACE_F_CLEANUP
            | SURFACE_F_A11Y
            | if escape { SURFACE_F_ESCAPE } else { 0 },
        scale_permille: scale,
        safe_left: 0,
        safe_top: 32,
        safe_right: 0,
        safe_bottom: 16,
    }
}

#[test]
fn surface_escape_seed_control_frame_is_strict() {
    let seed = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
    let mut buf = [0u8; SURFACE_ESCAPE_SEED_REQ_LEN];
    assert_eq!(
        encode_wm_surface_escape_seed_req(&mut buf, &seed),
        Some(SURFACE_ESCAPE_SEED_REQ_LEN)
    );
    assert_eq!(buf[0], OP_WM_SURFACE_ESCAPE_SEED);
    assert_eq!(&buf[1..9], &seed);
    assert_eq!(parse_wm_surface_escape_seed_req(&buf), Some(seed));
    assert_eq!(parse_wm_surface_escape_seed_req(&buf[..8]), None);
    assert_eq!(parse_wm_surface_escape_seed_req(&{
        let mut b = buf.to_vec();
        b.push(0);
        b
    }), None);
    let mut bad_op = buf;
    bad_op[0] = OP_WM_SURFACE_MODE;
    assert_eq!(parse_wm_surface_escape_seed_req(&bad_op), None);
}

#[test]
fn production_policy_denies_escape_and_unsupported_capabilities() {
    let canvas = req(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_TOUCH | INPUT_CLASS_MOUSE,
        IME_MODE_DISABLED,
        A11Y_MODE_HYBRID,
    );
    assert_eq!(
        surface_mode_production_policy(canvas, false, None, 1080, 1920),
        Err(ST_DENIED)
    );
    let granted = surface_mode_production_policy(canvas, true, None, 1080, 1920)
        .expect("authorized canvas grant");
    assert_eq!(
        granted.contract_flags,
        SURFACE_F_FOCUS
            | SURFACE_F_RATE
            | SURFACE_F_CLEANUP
            | SURFACE_F_A11Y
            | SURFACE_F_ESCAPE
    );
    assert_eq!(granted.scale_permille, 1000);
    assert_eq!((granted.safe_left, granted.safe_top, granted.safe_right, granted.safe_bottom), (0, 0, 0, 0));

    let mut keyboard = canvas;
    keyboard.input_classes |= INPUT_CLASS_KEYBOARD;
    assert_eq!(
        surface_mode_production_policy(keyboard, true, None, 1080, 1920),
        Err(ST_UNSUPPORTED)
    );
    let mut ime = canvas;
    ime.ime_mode = IME_MODE_HOST;
    assert_eq!(
        surface_mode_production_policy(ime, true, None, 1080, 1920),
        Err(ST_UNSUPPORTED)
    );
    let mut system = canvas;
    system.role = SURFACE_ROLE_SYSTEM;
    assert_eq!(
        surface_mode_production_policy(system, true, None, 1080, 1920),
        Err(ST_DENIED)
    );
    assert_eq!(
        surface_mode_production_policy(canvas, true, None, 0, 1920),
        Err(ST_BAD)
    );
}

#[test]
fn production_policy_allows_escape_renegotiation_but_not_downgrade() {
    let canvas = req(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_TOUCH | INPUT_CLASS_MOUSE,
        IME_MODE_DISABLED,
        A11Y_MODE_HYBRID,
    );
    let first = surface_mode_production_policy(canvas, true, None, 1080, 1920).unwrap();
    let immersive = req(
        SURFACE_MODE_IMMERSIVE,
        INPUT_CLASS_TOUCH,
        IME_MODE_DISABLED,
        A11Y_MODE_REGIONS,
    );
    assert!(surface_mode_production_policy(immersive, true, Some(first), 1080, 1920).is_ok());
    let retained = req(
        SURFACE_MODE_RETAINED,
        INPUT_CLASS_TOUCH,
        IME_MODE_DISABLED,
        A11Y_MODE_TREE,
    );
    assert_eq!(
        surface_mode_production_policy(retained, true, Some(first), 1080, 1920),
        Err(ST_DENIED)
    );
}

#[test]
fn surface_mode_three_paths_roundtrip_and_pin_bytes() {
    let cases = [
        (
            "normal",
            req(
                SURFACE_MODE_RETAINED,
                INPUT_CLASS_TOUCH,
                IME_MODE_HOST,
                A11Y_MODE_TREE,
            ),
            info(
                SURFACE_MODE_RETAINED,
                INPUT_CLASS_TOUCH,
                IME_MODE_HOST,
                A11Y_MODE_TREE,
                3_000,
            ),
        ),
        (
            "professional",
            req(
                SURFACE_MODE_CANVAS,
                INPUT_CLASS_TOUCH | INPUT_CLASS_MOUSE | INPUT_CLASS_KEYBOARD,
                IME_MODE_HOST,
                A11Y_MODE_HYBRID,
            ),
            info(
                SURFACE_MODE_CANVAS,
                INPUT_CLASS_TOUCH | INPUT_CLASS_MOUSE | INPUT_CLASS_KEYBOARD,
                IME_MODE_HOST,
                A11Y_MODE_HYBRID,
                1_500,
            ),
        ),
        (
            "game",
            req(
                SURFACE_MODE_IMMERSIVE,
                INPUT_CLASS_KEYBOARD | INPUT_CLASS_GAMEPAD,
                IME_MODE_DISABLED,
                A11Y_MODE_REGIONS,
            ),
            info(
                SURFACE_MODE_IMMERSIVE,
                INPUT_CLASS_KEYBOARD | INPUT_CLASS_GAMEPAD,
                IME_MODE_DISABLED,
                A11Y_MODE_REGIONS,
                1_000,
            ),
        ),
    ];

    for (name, request, granted) in cases {
        assert!(surface_mode_req_app_valid(&request), "{name} request");
        let mut buf = [0u8; SURFACE_MODE_REQ_LEN];
        let n = encode_wm_surface_mode_req(&mut buf, &request).unwrap();
        assert_eq!(n, SURFACE_MODE_REQ_LEN);
        assert_eq!(buf[0], OP_WM_SURFACE_MODE);
        assert_eq!(parse_wm_surface_mode_req(&buf[..n]), Some(request));

        let mut buf = [0u8; SURFACE_MODE_REP_LEN];
        let n = encode_wm_surface_mode_rep(&mut buf, &granted).unwrap();
        assert_eq!(n, SURFACE_MODE_REP_LEN);
        assert_eq!(parse_wm_surface_mode_rep(&buf[..n]), Some(Ok(granted)));
        assert!(surface_mode_grant_matches(&request, &granted), "{name} grant");
        assert!(surface_mode_info_fits(&granted, 1080, 1920), "{name} fit");
    }

    // 普通路径字节钉板：P0 请求不携带特权，也不自报 safe area/scale。
    let mut buf = [0u8; SURFACE_MODE_REQ_LEN];
    let n = encode_wm_surface_mode_req(
        &mut buf,
        &req(
            SURFACE_MODE_RETAINED,
            INPUT_CLASS_TOUCH,
            IME_MODE_HOST,
            A11Y_MODE_TREE,
        ),
    )
    .unwrap();
    assert_eq!(
        &buf[..n],
        &[OP_WM_SURFACE_MODE, 0, 0, INPUT_CLASS_TOUCH, IME_MODE_HOST, A11Y_MODE_TREE, 1, 0]
    );

    // 授予帧：scale/safe area 只来自 host。
    let normal = info(
        SURFACE_MODE_RETAINED,
        INPUT_CLASS_TOUCH,
        IME_MODE_HOST,
        A11Y_MODE_TREE,
        3_000,
    );
    let mut buf = [0u8; SURFACE_MODE_REP_LEN];
    let n = encode_wm_surface_mode_rep(&mut buf, &normal).unwrap();
    let expected = [
        0, 0, 0, INPUT_CLASS_TOUCH, IME_MODE_HOST, A11Y_MODE_TREE,
        SURFACE_F_FOCUS | SURFACE_F_RATE | SURFACE_F_CLEANUP | SURFACE_F_A11Y,
        0, 0xB8, 0x0B, 0, 0, 32, 0, 0, 0, 16, 0, 1, 0,
    ];
    assert_eq!(&buf[..n], &expected);
}

#[test]
fn surface_mode_contract_rejects_role_mode_sidecar_and_flags() {
    let good = req(
        SURFACE_MODE_RETAINED,
        INPUT_CLASS_TOUCH,
        IME_MODE_HOST,
        A11Y_MODE_TREE,
    );
    assert!(surface_mode_req_valid(&good));
    assert!(!surface_mode_req_app_valid(&SurfaceModeReq {
        role: SURFACE_ROLE_SYSTEM,
        ..good
    }));

    let bad_requests = [
        SurfaceModeReq { surface_id: 0, ..good },
        SurfaceModeReq { role: 2, ..good },
        SurfaceModeReq { mode: 3, ..good },
        SurfaceModeReq { input_classes: 0, ..good },
        SurfaceModeReq {
            input_classes: INPUT_CLASS_KNOWN | 0x80,
            ..good
        },
        SurfaceModeReq { ime_mode: 2, ..good },
        SurfaceModeReq { a11y_mode: 3, ..good },
        // Retained 不能把语义树降级成区域/混合。
        SurfaceModeReq { a11y_mode: A11Y_MODE_REGIONS, ..good },
        // Canvas 至少要有区域语义；不能裸树。
        req(SURFACE_MODE_CANVAS, INPUT_CLASS_MOUSE, IME_MODE_HOST, A11Y_MODE_TREE),
        // Immersive 不能伪装 retained 树。
        req(
            SURFACE_MODE_IMMERSIVE,
            INPUT_CLASS_GAMEPAD,
            IME_MODE_DISABLED,
            A11Y_MODE_TREE,
        ),
        // System 角色不得申请 escape hatch。
        SurfaceModeReq {
            surface_id: 1,
            role: SURFACE_ROLE_SYSTEM,
            mode: SURFACE_MODE_IMMERSIVE,
            input_classes: INPUT_CLASS_TOUCH,
            ime_mode: IME_MODE_DISABLED,
            a11y_mode: A11Y_MODE_REGIONS,
        },
    ];
    for bad in bad_requests {
        assert!(!surface_mode_req_valid(&bad));
        let mut buf = [0u8; SURFACE_MODE_REQ_LEN];
        assert_eq!(encode_wm_surface_mode_req(&mut buf, &bad), None);
    }

    let good_info = info(
        SURFACE_MODE_RETAINED,
        INPUT_CLASS_TOUCH,
        IME_MODE_HOST,
        A11Y_MODE_TREE,
        1_000,
    );
    let bad_infos = [
        SurfaceModeInfo { surface_id: 0, ..good_info },
        SurfaceModeInfo { role: 2, ..good_info },
        SurfaceModeInfo { mode: 3, ..good_info },
        SurfaceModeInfo { input_classes: 0, ..good_info },
        SurfaceModeInfo { ime_mode: 2, ..good_info },
        SurfaceModeInfo { a11y_mode: 3, ..good_info },
        SurfaceModeInfo {
            contract_flags: SURFACE_FLAG_KNOWN | 0x80,
            ..good_info
        },
        SurfaceModeInfo {
            contract_flags: SURFACE_F_FOCUS | SURFACE_F_RATE | SURFACE_F_CLEANUP,
            ..good_info
        },
        SurfaceModeInfo {
            contract_flags: good_info.contract_flags | SURFACE_F_ESCAPE,
            ..good_info
        },
        SurfaceModeInfo { scale_permille: 499, ..good_info },
        SurfaceModeInfo { scale_permille: 4_001, ..good_info },
    ];
    for bad in bad_infos {
        assert!(!surface_mode_info_valid(&bad));
        let mut buf = [0u8; SURFACE_MODE_REP_LEN];
        assert_eq!(encode_wm_surface_mode_rep(&mut buf, &bad), None);
    }

    // host 不能把 Canvas 请求静默降级成 Retained；拒绝必须走 1B 错误。
    let canvas_req = req(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_MOUSE,
        IME_MODE_HOST,
        A11Y_MODE_HYBRID,
    );
    assert!(!surface_mode_grant_matches(
        &canvas_req,
        &info(
            SURFACE_MODE_RETAINED,
            INPUT_CLASS_MOUSE,
            IME_MODE_HOST,
            A11Y_MODE_TREE,
            1_000,
        )
    ));
    let mut changed_surface = good_info;
    changed_surface.surface_id = 2;
    assert!(!surface_mode_grant_matches(&good, &changed_surface));

    // Canvas/Immersive 缺 escape 授权也拒绝；三者公共语义位不可少。
    let canvas = info(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_MOUSE,
        IME_MODE_HOST,
        A11Y_MODE_HYBRID,
        1_000,
    );
    let no_escape = SurfaceModeInfo {
        contract_flags: canvas.contract_flags & !SURFACE_F_ESCAPE,
        ..canvas
    };
    assert!(!surface_mode_info_valid(&no_escape));
}

#[test]
fn surface_mode_safe_area_and_wire_reject_faces() {
    let mut granted = info(
        SURFACE_MODE_IMMERSIVE,
        INPUT_CLASS_GAMEPAD,
        IME_MODE_DISABLED,
        A11Y_MODE_REGIONS,
        1_000,
    );
    granted.safe_left = 500;
    granted.safe_right = 580;
    granted.safe_top = 0;
    granted.safe_bottom = 0;
    assert!(!surface_mode_info_fits(&granted, 1080, 1920));
    granted.safe_right = 579;
    assert!(surface_mode_info_fits(&granted, 1080, 1920));
    assert!(!surface_mode_info_fits(&granted, 0, 100));
    assert!(!surface_mode_info_fits(&granted, 100, 0));

    let request = req(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_MOUSE,
        IME_MODE_HOST,
        A11Y_MODE_HYBRID,
    );
    let mut buf = [0u8; SURFACE_MODE_REQ_LEN];
    let n = encode_wm_surface_mode_req(&mut buf, &request).unwrap();
    let mut short_req = [0u8; SURFACE_MODE_REQ_LEN + 1];
    short_req[..n].copy_from_slice(&buf[..n]);
    assert_eq!(parse_wm_surface_mode_req(&short_req[..n - 1]), None);
    assert_eq!(parse_wm_surface_mode_req(&short_req[..n + 1]), None);
    let wrong_op = {
        let mut f = buf;
        f[0] = OP_WM_SURFACE_MODE + 1;
        f
    };
    assert_eq!(parse_wm_surface_mode_req(&wrong_op), None);

    let mut rep = [0u8; SURFACE_MODE_REP_LEN];
    let n = encode_wm_surface_mode_rep(
        &mut rep,
        &info(
            SURFACE_MODE_CANVAS,
            INPUT_CLASS_MOUSE,
            IME_MODE_HOST,
            A11Y_MODE_HYBRID,
            1_000,
        ),
    )
    .unwrap();
    let mut short_rep = [0u8; SURFACE_MODE_REP_LEN + 1];
    short_rep[..n].copy_from_slice(&rep[..n]);
    assert_eq!(parse_wm_surface_mode_rep(&short_rep[..n - 1]), None);
    assert_eq!(parse_wm_surface_mode_rep(&short_rep[..n + 1]), None);
    assert_eq!(parse_wm_surface_mode_rep(&[ST_BAD]), Some(Err(ST_BAD)));
    assert_eq!(parse_wm_surface_mode_rep(&[ST_DENIED]), Some(Err(ST_DENIED)));
    assert_eq!(parse_wm_surface_mode_rep(&[ST_UNSUPPORTED]), Some(Err(ST_UNSUPPORTED)));
    assert_eq!(parse_wm_surface_mode_rep(&[0]), None);
    assert_eq!(encode_wm_surface_mode_req(&mut [], &request), None);
    assert_eq!(
        encode_wm_surface_mode_rep(
            &mut [],
            &info(
                SURFACE_MODE_CANVAS,
                INPUT_CLASS_MOUSE,
                IME_MODE_HOST,
                A11Y_MODE_HYBRID,
                1_000,
            )
        ),
        None
    );
}

#[test]
fn ime_composition_roundtrip_utf8_cursor_and_rejects() {
    let text = "Berry莓".as_bytes();
    assert_eq!(text.len(), 8);
    let mut ev = ImeComposition {
        revision: 9,
        phase: IME_PHASE_UPDATE,
        purpose: IME_PURPOSE_TEXT,
        cursor_cp: 6,
        text_len: text.len() as u8,
        text: [0; IME_TEXT_MAX],
    };
    ev.text[..text.len()].copy_from_slice(text);
    let mut buf = [0u8; IME_COMPOSITION_LEN];
    let n = encode_ime_composition(&mut buf, &ev).unwrap();
    assert_eq!(n, IME_COMPOSITION_LEN);
    assert_eq!(parse_ime_composition(&buf[..n]), Some(ev));
    assert_eq!(&buf[12..20], text);
    assert!(buf[20..].iter().all(|&b| b == 0));

    let bad = [
        ImeComposition { revision: 0, ..ev },
        ImeComposition { phase: 5, ..ev },
        ImeComposition { purpose: 4, ..ev },
        ImeComposition { cursor_cp: 7, ..ev },
        ImeComposition {
            text_len: IME_TEXT_MAX as u8 + 1,
            ..ev
        },
        ImeComposition {
            phase: IME_PHASE_HIDE,
            ..ev
        },
    ];
    for bad_ev in bad {
        assert!(!encode_ime_composition(&mut buf, &bad_ev).is_some());
    }

    let mut dirty_hide = ImeComposition {
        revision: 3,
        phase: IME_PHASE_HIDE,
        purpose: IME_PURPOSE_TEXT,
        cursor_cp: 0,
        text_len: 0,
        text: [0; IME_TEXT_MAX],
    };
    dirty_hide.text[0] = 1;
    assert_eq!(encode_ime_composition(&mut buf, &dirty_hide), None);

    // 非法 UTF-8：连续首字节后缺 continuation。
    let invalid = [0xC3, 0x41];
    let mut invalid_ev = ImeComposition {
        revision: 2,
        phase: IME_PHASE_COMMIT,
        purpose: IME_PURPOSE_TEXT,
        cursor_cp: 2,
        text_len: invalid.len() as u8,
        text: [0; IME_TEXT_MAX],
    };
    invalid_ev.text[..invalid.len()].copy_from_slice(&invalid);
    assert_eq!(encode_ime_composition(&mut buf, &invalid_ev), None);

    // 手工坏帧：截断、尾随、保留域、unknown status-like padding 全拒。
    let n = encode_ime_composition(&mut buf, &ev).unwrap();
    let mut short_ime = [0u8; IME_COMPOSITION_LEN + 1];
    short_ime[..n].copy_from_slice(&buf[..n]);
    assert_eq!(parse_ime_composition(&short_ime[..n - 1]), None);
    assert_eq!(parse_ime_composition(&short_ime[..n + 1]), None);
    let mut trailing = buf;
    trailing[20] = 1;
    assert_eq!(parse_ime_composition(&trailing), None);
    let mut reserved = buf;
    reserved[9] = 1;
    assert_eq!(parse_ime_composition(&reserved), None);
}

#[test]
fn a11y_region_sidecar_roundtrip_and_rejects() {
    let label = "地图莓".as_bytes();
    assert_eq!(label.len(), 9);
    let mut region = A11yRegion {
        node_id: 7,
        parent_id: 3,
        x: 12,
        y: 34,
        w: 1080,
        h: 1920,
        role: A11Y_ROLE_GAME_REGION,
        actions: A11Y_ACTION_FOCUS | A11Y_ACTION_ACTIVATE,
        state: A11Y_STATE_VISIBLE | A11Y_STATE_FOCUSED,
        label_len: label.len() as u8,
        label: [0; A11Y_LABEL_MAX],
    };
    region.label[..label.len()].copy_from_slice(label);

    let mut buf = [0u8; A11Y_REGION_LEN];
    let n = encode_a11y_region(&mut buf, &region).unwrap();
    assert_eq!(n, A11Y_REGION_LEN);
    assert_eq!(parse_a11y_region(&buf[..n]), Some(region));
    assert_eq!(&buf[20..29], label);
    assert!(buf[29..].iter().all(|&b| b == 0));

    let bad = [
        A11yRegion { node_id: 0, ..region },
        A11yRegion {
            parent_id: region.node_id,
            ..region
        },
        A11yRegion { w: 0, ..region },
        A11yRegion { h: 0, ..region },
        A11yRegion {
            role: A11Y_ROLE_GAME_REGION + 1,
            ..region
        },
        A11yRegion {
            actions: A11Y_ACTION_KNOWN | 0x80,
            ..region
        },
        A11yRegion {
            state: A11Y_STATE_KNOWN | 0x80,
            ..region
        },
        A11yRegion {
            label_len: A11Y_LABEL_MAX as u8 + 1,
            ..region
        },
    ];
    for bad_region in bad {
        assert_eq!(encode_a11y_region(&mut buf, &bad_region), None);
    }

    let mut dirty_tail = region;
    dirty_tail.label[label.len()] = 1;
    assert_eq!(encode_a11y_region(&mut buf, &dirty_tail), None);

    let invalid = [0xFF, 0x41];
    let mut invalid_label = region;
    invalid_label.label[..invalid.len()].copy_from_slice(&invalid);
    invalid_label.label_len = invalid.len() as u8;
    assert_eq!(encode_a11y_region(&mut buf, &invalid_label), None);

    let n = encode_a11y_region(&mut buf, &region).unwrap();
    let mut long = [0u8; A11Y_REGION_LEN + 1];
    long[..n].copy_from_slice(&buf[..n]);
    assert_eq!(parse_a11y_region(&long[..n - 1]), None);
    assert_eq!(parse_a11y_region(&long[..n + 1]), None);
    let mut trailing = buf;
    trailing[29] = 1;
    assert_eq!(parse_a11y_region(&trailing), None);
}

#[test]
fn p0_sidecar_goldens_caps_reserved_and_sweeps() {
    // 帧帽与算术（父 crate「改帽必炸」纪律的可见镜像）。
    assert_eq!(SURFACE_MODE_REQ_LEN, 8);
    assert_eq!(SURFACE_MODE_REP_LEN, 20);
    assert_eq!(IME_TEXT_MAX, 64);
    assert_eq!(IME_COMPOSITION_LEN, 76);
    assert_eq!(A11Y_LABEL_MAX, 32);
    assert_eq!(A11Y_REGION_LEN, 52);
    assert!(SURFACE_MODE_REQ_LEN <= 256);
    assert!(SURFACE_MODE_REP_LEN <= 256);
    assert!(IME_COMPOSITION_LEN <= 256);
    assert!(A11Y_REGION_LEN <= 256);

    // IME 全帧 golden：不同字节域使用区分度明显的非对称值，防 BE/错位。
    let ime_text = [0x41, 0xE8, 0x8E, 0x93];
    let ime = ImeComposition {
        revision: 0x0102_0304,
        phase: IME_PHASE_UPDATE,
        purpose: IME_PURPOSE_MULTILINE,
        cursor_cp: 2,
        text_len: ime_text.len() as u8,
        text: [0; IME_TEXT_MAX],
    };
    let mut ime_value = ime;
    ime_value.text[..ime_text.len()].copy_from_slice(&ime_text);
    let mut ime_buf = [0u8; IME_COMPOSITION_LEN];
    let ime_n = encode_ime_composition(&mut ime_buf, &ime_value).unwrap();
    let mut ime_want = [0u8; IME_COMPOSITION_LEN];
    ime_want[..4].copy_from_slice(&[4, 3, 2, 1]);
    ime_want[4] = IME_PHASE_UPDATE;
    ime_want[5] = IME_PURPOSE_MULTILINE;
    ime_want[6..8].copy_from_slice(&[2, 0]);
    ime_want[8] = 4;
    ime_want[12..16].copy_from_slice(&ime_text);
    assert_eq!(&ime_buf[..ime_n], &ime_want);
    assert_eq!(parse_ime_composition(&ime_buf), Some(ime_value));

    // a11y 全帧 golden：id/parent/几何均用不同常数钉 LE 与错位。
    let a11y_label = [0xE8, 0x8E, 0x93];
    let mut region = A11yRegion {
        node_id: 0x0102_0304,
        parent_id: 0x0A0B_0C0D,
        x: 0x0102,
        y: 0x0304,
        w: 0x0506,
        h: 0x0708,
        role: A11Y_ROLE_GAME_REGION,
        actions: A11Y_ACTION_KNOWN,
        state: A11Y_STATE_KNOWN,
        label_len: a11y_label.len() as u8,
        label: [0; A11Y_LABEL_MAX],
    };
    region.label[..a11y_label.len()].copy_from_slice(&a11y_label);
    let mut a11y_buf = [0u8; A11Y_REGION_LEN];
    let a11y_n = encode_a11y_region(&mut a11y_buf, &region).unwrap();
    let mut a11y_want = [0u8; A11Y_REGION_LEN];
    a11y_want[..4].copy_from_slice(&[4, 3, 2, 1]);
    a11y_want[4..8].copy_from_slice(&[0x0D, 0x0C, 0x0B, 0x0A]);
    a11y_want[8..10].copy_from_slice(&[2, 1]);
    a11y_want[10..12].copy_from_slice(&[4, 3]);
    a11y_want[12..14].copy_from_slice(&[6, 5]);
    a11y_want[14..16].copy_from_slice(&[8, 7]);
    a11y_want[16] = A11Y_ROLE_GAME_REGION;
    a11y_want[17] = A11Y_ACTION_KNOWN;
    a11y_want[18] = A11Y_STATE_KNOWN;
    a11y_want[19] = 3;
    a11y_want[20..23].copy_from_slice(&a11y_label);
    assert_eq!(&a11y_buf[..a11y_n], &a11y_want);
    assert_eq!(parse_a11y_region(&a11y_buf), Some(region));

    // WM 成功授予 off7 是前向兼容保留位：编码恒 0、解析不校验。
    let granted = info(
        SURFACE_MODE_CANVAS,
        INPUT_CLASS_MOUSE,
        IME_MODE_HOST,
        A11Y_MODE_HYBRID,
        1_000,
    );
    let mut rep = [0u8; SURFACE_MODE_REP_LEN];
    let rep_n = encode_wm_surface_mode_rep(&mut rep, &granted).unwrap();
    let mut wild_reserved = rep;
    wild_reserved[7] = 0xEE;
    assert_eq!(
        parse_wm_surface_mode_rep(&wild_reserved[..rep_n]),
        Some(Ok(granted))
    );
    assert_eq!(parse_wm_surface_mode_rep(&[ST_BAD, 0]), None);

    // IME off10..12 保留 u16 必须为 0。
    let mut ime_reserved = ime_buf;
    ime_reserved[10] = 1;
    assert_eq!(parse_ime_composition(&ime_reserved), None);

    // 四种 P0 定长帧逐 cut 截断全拒，+1 尾随全拒。
    let request = req(
        SURFACE_MODE_IMMERSIVE,
        INPUT_CLASS_GAMEPAD,
        IME_MODE_DISABLED,
        A11Y_MODE_REGIONS,
    );
    let mut request_buf = [0u8; SURFACE_MODE_REQ_LEN];
    let request_n = encode_wm_surface_mode_req(&mut request_buf, &request).unwrap();
    let frames: [&[u8]; 4] = [
        &request_buf[..request_n],
        &rep[..rep_n],
        &ime_buf[..ime_n],
        &a11y_buf[..a11y_n],
    ];
    for frame in frames {
        for cut in 0..frame.len() {
            match frame.len() {
                8 => {
                    assert_eq!(parse_wm_surface_mode_req(&frame[..cut]), None);
                }
                20 => {
                    assert!(parse_wm_surface_mode_rep(&frame[..cut]).is_none());
                }
                76 => {
                    assert_eq!(parse_ime_composition(&frame[..cut]), None);
                }
                52 => {
                    assert_eq!(parse_a11y_region(&frame[..cut]), None);
                }
                _ => unreachable!(),
            }
        }
    }
    let mut long_request = [0u8; SURFACE_MODE_REQ_LEN + 1];
    long_request[..request_n].copy_from_slice(&request_buf);
    assert_eq!(parse_wm_surface_mode_req(&long_request), None);
    let mut long_rep = [0u8; SURFACE_MODE_REP_LEN + 1];
    long_rep[..rep_n].copy_from_slice(&rep);
    assert!(parse_wm_surface_mode_rep(&long_rep).is_none());
    let mut long_ime = [0u8; IME_COMPOSITION_LEN + 1];
    long_ime[..ime_n].copy_from_slice(&ime_buf);
    assert_eq!(parse_ime_composition(&long_ime), None);
    let mut long_a11y = [0u8; A11Y_REGION_LEN + 1];
    long_a11y[..a11y_n].copy_from_slice(&a11y_buf);
    assert_eq!(parse_a11y_region(&long_a11y), None);
}

#[test]
fn p0_utf8_adversarial_and_wire_value_rejects() {
    let invalid = [
        &[0xC0, 0x80][..],
        &[0xC1, 0xBF],
        &[0xE0, 0x80, 0x80],
        &[0xE0, 0x9F, 0xBF],
        &[0xF0, 0x80, 0x80, 0x80],
        &[0xF0, 0x8F, 0xBF, 0xBF],
        &[0xED, 0xA0, 0x80],
        &[0xF4, 0x90, 0x80, 0x80],
        &[0xF5, 0x80, 0x80, 0x80],
        &[0xF7, 0xBF, 0xBF, 0xBF],
        &[0xFE],
        &[0xFF],
        &[0x80],
        &[0xBF],
        &[0xE8, 0x8E],
    ];
    for bytes in invalid {
        let mut ime = ImeComposition {
            revision: 1,
            phase: IME_PHASE_UPDATE,
            purpose: IME_PURPOSE_TEXT,
            cursor_cp: 0,
            text_len: bytes.len() as u8,
            text: [0; IME_TEXT_MAX],
        };
        ime.text[..bytes.len()].copy_from_slice(bytes);
        assert!(encode_ime_composition(&mut [0; IME_COMPOSITION_LEN], &ime).is_none());
        let mut wire = [0u8; IME_COMPOSITION_LEN];
        wire[..4].copy_from_slice(&1u32.to_le_bytes());
        wire[4] = IME_PHASE_UPDATE;
        wire[8] = bytes.len() as u8;
        wire[12..12 + bytes.len()].copy_from_slice(bytes);
        assert_eq!(parse_ime_composition(&wire), None);

        let mut region = A11yRegion {
            node_id: 1,
            parent_id: 0,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            role: A11Y_ROLE_TEXT,
            actions: 0,
            state: 0,
            label_len: bytes.len() as u8,
            label: [0; A11Y_LABEL_MAX],
        };
        region.label[..bytes.len()].copy_from_slice(bytes);
        assert!(encode_a11y_region(&mut [0; A11Y_REGION_LEN], &region).is_none());
        let mut wire = [0u8; A11Y_REGION_LEN];
        wire[..4].copy_from_slice(&1u32.to_le_bytes());
        wire[12] = 1;
        wire[14] = 1;
        wire[19] = bytes.len() as u8;
        wire[20..20 + bytes.len()].copy_from_slice(bytes);
        assert_eq!(parse_a11y_region(&wire), None);
    }

    // Parse 侧值域拒绝：直接 patch wire，绕过 encode-none 已覆盖的路径。
    let mut ime = ImeComposition {
        revision: 1,
        phase: IME_PHASE_UPDATE,
        purpose: IME_PURPOSE_TEXT,
        cursor_cp: 0,
        text_len: 1,
        text: [0; IME_TEXT_MAX],
    };
    ime.text[0] = b'A';
    let mut wire = [0u8; IME_COMPOSITION_LEN];
    let n = encode_ime_composition(&mut wire, &ime).unwrap();
    for (off, value, why) in [
        (0, 0, "revision zero"),
        (4, 5, "phase"),
        (5, 4, "purpose"),
        (6, 2, "cursor"),
        (8, 65, "len"),
    ] {
        let mut bad = wire;
        bad[off] = value;
        assert_eq!(parse_ime_composition(&bad[..n]), None, "{why}");
    }

    let mut region = A11yRegion {
        node_id: 1,
        parent_id: 0,
        x: 0,
        y: 0,
        w: 1,
        h: 1,
        role: A11Y_ROLE_TEXT,
        actions: 0,
        state: 0,
        label_len: 1,
        label: [0; A11Y_LABEL_MAX],
    };
    region.label[0] = b'A';
    let mut wire = [0u8; A11Y_REGION_LEN];
    let n = encode_a11y_region(&mut wire, &region).unwrap();
    for (off, value, why) in [
        (0, 0, "node zero"),
        (4, 1, "self parent"),
        (12, 0, "width"),
        (14, 0, "height"),
        (16, 8, "role"),
        (17, 0x80, "actions"),
        (18, 0x80, "state"),
        (19, 33, "label len"),
    ] {
        let mut bad = wire;
        bad[off] = value;
        assert_eq!(parse_a11y_region(&bad[..n]), None, "{why}");
    }
}

#[test]
fn p0_safe_height_and_ime_phases_roundtrip() {
    let mut tall = info(
        SURFACE_MODE_RETAINED,
        INPUT_CLASS_TOUCH,
        IME_MODE_HOST,
        A11Y_MODE_TREE,
        1_000,
    );
    tall.safe_left = 0;
    tall.safe_right = 0;
    tall.safe_top = 960;
    tall.safe_bottom = 960;
    assert!(!surface_mode_info_fits(&tall, 1080, 1920));
    tall.safe_bottom = 959;
    assert!(surface_mode_info_fits(&tall, 1080, 1920));

    for phase in [
        IME_PHASE_HIDE,
        IME_PHASE_SHOW,
        IME_PHASE_UPDATE,
        IME_PHASE_COMMIT,
        IME_PHASE_CANCEL,
    ] {
        let empty = phase == IME_PHASE_HIDE;
        let text = if empty { &[][..] } else { b"A" };
        let mut ev = ImeComposition {
            revision: 7,
            phase,
            purpose: IME_PURPOSE_TEXT,
            cursor_cp: text.len() as u16,
            text_len: text.len() as u8,
            text: [0; IME_TEXT_MAX],
        };
        ev.text[..text.len()].copy_from_slice(text);
        let mut buf = [0u8; IME_COMPOSITION_LEN];
        let n = encode_ime_composition(&mut buf, &ev).unwrap();
        assert_eq!(parse_ime_composition(&buf[..n]), Some(ev));
    }
}
