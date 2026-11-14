//! Terminal QR for `berry vnc web --lan` pairing (wave 10): the phone
//! scans the code, opens the page on the same Wi-Fi, and types the VNC
//! password into the page prompt. Content is ALWAYS the full page URL
//! with the `#password=` fragment stripped (the wave-9 restart
//! condition: the password never enters a scannable artifact) — this
//! module never sees the fragment because the caller strips it first.
//!
//! Encoding is Nayuki's qrcodegen (the wave-10 dependency decision:
//! single-file pure-Rust MIT crate, zero transitive dependencies, API
//! of four functions) at ECC Medium — screen scans carry moiré and
//! polarity-inversion margins, M is steadier than L and smaller than Q;
//! a ~101-character page URL lands at v6/41x41 with no rendering
//! pressure. Rendering folds two module rows into one physical line of
//! half-block characters (`█`/`▀`/`▄`/space) with the spec-minimum
//! 4-module quiet zone and a box-drawing border. Polarity note: blocks
//! render dark modules in the terminal FOREGROUND, so a dark-background
//! terminal shows an inverted matrix — most decoders (macOS Vision
//! included, verified on the wave-10 samples) read inverted codes; this
//! is a documented boundary, not an adaptive feature.

use qrcodegen::{QrCode, QrCodeEcc};

/// Quiet zone in modules — the QR spec minimum (ISO/IEC 18004 §2.2).
pub const QUIET_ZONE: i32 = 4;

/// Encode `text` (ECC Medium) and render the terminal block. `None`
/// covers every failure mode: `DataTooLong` from the encoder (not
/// reachable with page URLs — v40/Medium holds ~2.3KB) is surfaced by
/// the caller as a one-line warning, never a service failure.
pub fn render_qr(text: &str) -> Option<String> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    Some(render_matrix(&qr))
}

/// Render an encoded matrix: 4-module quiet zone, two module rows per
/// physical line via half blocks (an odd total pads the last line with
/// a light row), one box-drawing border ring. Pure — same matrix, same
/// string.
pub(crate) fn render_matrix(qr: &QrCode) -> String {
    let total = qr.size() + 2 * QUIET_ZONE;
    let mut out = String::new();
    let bar = "═".repeat(total as usize);
    out.push_str(&format!("╔{bar}╗\n"));
    let mut y = 0;
    while y < total {
        let mut line = String::with_capacity(total as usize);
        for x in 0..total {
            let top = module(qr, x, y);
            // An odd total folds a nonexistent bottom row as light.
            let bottom = if y + 1 < total { module(qr, x, y + 1) } else { false };
            line.push(half_block(top, bottom));
        }
        out.push_str(&format!("║{line}║\n"));
        y += 2;
    }
    out.push_str(&format!("╚{bar}╝\n"));
    out
}

/// Module state at (x, y) of the quiet-zone-padded matrix; outside the
/// encoded area the module is light (the quiet zone itself).
fn module(qr: &QrCode, x: i32, y: i32) -> bool {
    let (qx, qy) = (x - QUIET_ZONE, y - QUIET_ZONE);
    if qx < 0 || qy < 0 || qx >= qr.size() || qy >= qr.size() {
        return false;
    }
    qr.get_module(qx, qy)
}

/// Half-block fold of two module rows (true = dark = foreground).
fn half_block(top: bool, bottom: bool) -> char {
    match (top, bottom) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact LAN page URL shape `vnc web --lan` prints (fragment
    /// already stripped — the QR content discipline).
    const LAN_URL: &str = "http://192.168.1.105:8123/?autoconnect=true&path=ws%3A%2F%2F192.168.1.105%3A8123%2Fws&resize=scale";

    /// Golden vectors (probe f1, decoded back by macOS Vision): the
    /// README example pins v1/21x21, the LAN URL shape pins its version
    /// and size. Both ECC Medium. The version pins keep a dependency
    /// bump (or accidental ECC change) from silently reshaping the code.
    #[test]
    fn golden_versions() {
        let hello = QrCode::encode_text("Hello, world!", QrCodeEcc::Medium).unwrap();
        assert_eq!(hello.version().value(), 1);
        assert_eq!(hello.size(), 21);
        let url = QrCode::encode_text(LAN_URL, QrCodeEcc::Medium).unwrap();
        assert_eq!(url.version().value(), 6);
        assert_eq!(url.size(), 41);
    }

    /// U49: rendering geometry — every physical line is the padded width
    /// (total + 2 border chars), the block carries the border ring, the
    /// quiet zone is all light (first folded row sits entirely inside
    /// it), and the line count folds two module rows per line with the
    /// odd-total pad.
    #[test]
    fn render_geometry_and_quiet_zone() {
        for text in ["Hello, world!", LAN_URL] {
            let qr = QrCode::encode_text(text, QrCodeEcc::Medium).unwrap();
            let total = qr.size() + 2 * QUIET_ZONE;
            let block = render_matrix(&qr);
            let lines: Vec<&str> = block.lines().collect();
            let content_rows = (total as usize + 1) / 2;
            assert_eq!(lines.len(), content_rows + 2, "border ring + folded rows");
            for l in &lines {
                let width = l.chars().count();
                assert_eq!(width as i32, total + 2, "line width {width}");
            }
            assert!(lines[0].starts_with('╔') && lines[0].ends_with('╗'));
            assert!(lines[last(&lines)].starts_with('╚') && lines[last(&lines)].ends_with('╝'));
            // First folded row: module rows 0..2 of the padded matrix —
            // fully quiet zone, hence all-light between the borders.
            let first = lines[1];
            let inner: String = first.chars().skip(1).take(total as usize).collect();
            assert!(inner.chars().all(|c| c == ' '), "quiet zone must be light: {inner:?}");
            // The finder pattern is dark at QR (0,0): the first content
            // line ABOVE it belongs to the quiet zone; row QUIET_ZONE
            // folds QR rows 0+1 and must contain dark blocks (top-left
            // finder border is all dark on its first module row).
            let finder_line = lines[1 + (QUIET_ZONE as usize) / 2];
            assert!(finder_line.contains('▀') || finder_line.contains('█'));
        }
    }

    /// U49: the fold mapping itself — all four half-block states.
    #[test]
    fn half_block_fold_states() {
        assert_eq!(half_block(true, true), '█');
        assert_eq!(half_block(true, false), '▀');
        assert_eq!(half_block(false, true), '▄');
        assert_eq!(half_block(false, false), ' ');
    }

    /// U49: unencodable input (v40/Medium tops out around 2.3KB) and
    /// the empty string both behave — None for the former, a v1 block
    /// for the latter (an empty payload is a valid QR).
    #[test]
    fn encode_limits() {
        assert!(render_qr(&"x".repeat(3000)).is_none());
        let block = render_qr("").unwrap();
        let lines: Vec<&str> = block.lines().collect();
        // v1: 21 + 8 quiet = 29 wide, (29+1)/2 = 15 folded rows.
        assert_eq!(lines.len(), 17);
        assert_eq!(lines[0].chars().count(), 31);
    }

    fn last(v: &[&str]) -> usize {
        v.len() - 1
    }
}
