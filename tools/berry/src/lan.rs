//! LAN address resolution and the `vnc web --lan` admission policy
//! (wave 10). The URL (and QR) host must be an address a phone on the
//! same Wi-Fi can dial, which on this machine is the interface IPv4 —
//! NOT a `.local` mDNS name (Android's mDNS support is unreliable,
//! probe f2) and NOT the UDP-connect-to-8.8.8.8 idiom (a VPN fake-ip
//! default route returns 198.18.0.1 here — empirically rejected, probe
//! f2). Resolution shells out to fast platform utilities, the same
//! discipline as `vnc certs` shelling out to openssl: zero new
//! dependencies, missing/crashing tools are plain chain failures that
//! fall through to the 127.0.0.1 fallback plus a one-time warning
//! (`--host` remains the explicit override).
//!
//! The admission matrix lives here as a pure function so the security
//! posture (wave-1 non-loopback policy applied to LAN binding: no
//! passwordless plaintext RFB on the LAN) is unit-pinned, not scattered
//! through the command.

use crate::error::{BerryError, Result};
use crate::vnc_cfg::{is_loopback, ListenInfo};

/// Resolved host for the `--lan` URL/QR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanHost {
    /// A usable LAN address (interface IPv4 or an explicit `--host`).
    Addr(String),
    /// Every platform probe failed: fall back to 127.0.0.1 and warn
    /// once — the server still binds 0.0.0.0, the operator can dial the
    /// machine's real address manually or restart with `--host`.
    FallbackLoopback,
}

/// Pure field picker for `hostname -I` style output (Linux): the first
/// field that parses as IPv4 and is neither loopback (127/8) nor
/// link-local (169.254/16). Garbage fields (scopes, IPv6, words) are
/// skipped; no candidate -> None.
pub fn pick_lan_ipv4(fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find(|f| match f.parse::<std::net::Ipv4Addr>() {
            Ok(ip) => !ip.is_loopback() && !ip.is_link_local(),
            Err(_) => false,
        })
        .map(|s| s.to_string())
}

/// Pure macOS interface-list filter: keep `en<N>` names only, sorted by
/// the numeric suffix (en2 before en10) so enumeration order is
/// deterministic across `ifconfig -l` runs.
pub fn macos_en_candidates(ifaces: &[&str]) -> Vec<String> {
    let mut v: Vec<(u32, &str)> = ifaces
        .iter()
        .filter_map(|f| {
            let n = f.strip_prefix("en")?;
            Some((n.parse::<u32>().ok()?, *f))
        })
        .collect();
    v.sort();
    v.into_iter().map(|(_, s)| s.to_string()).collect()
}

/// Validate an explicit `--host` override: non-empty, no whitespace, no
/// path separators — the value goes verbatim into a URL (bare IPv6 gets
/// brackets later via the `addr_host` semantics). Violations are rc=2
/// usage errors, aligned with `parse_max_conns`.
pub fn validate_host_override(host: &str) -> Result<String> {
    if host.is_empty()
        || host.chars().any(|c| c.is_whitespace())
        || host.contains('/')
        || host.contains('\\')
    {
        return Err(BerryError::new(2, format!("invalid --host value '{host}'"))
            .with_hint("--host takes a bare address like 192.168.1.105 (no scheme, path, or spaces)"));
    }
    Ok(host.to_string())
}

/// Resolve the LAN host for the URL/QR. Explicit `--host` wins verbatim
/// (already validated); otherwise the platform chain; total failure
/// falls back to loopback (the caller warns).
pub fn resolve_lan_host(explicit: Option<&str>) -> LanHost {
    if let Some(h) = explicit {
        return LanHost::Addr(h.to_string());
    }
    let found = if cfg!(target_os = "macos") {
        lan_ipv4_macos()
    } else if cfg!(target_os = "linux") {
        lan_ipv4_linux()
    } else if cfg!(target_os = "windows") {
        lan_ipv4_windows()
    } else {
        None
    };
    match found {
        Some(a) => LanHost::Addr(a),
        None => LanHost::FallbackLoopback,
    }
}

/// macOS chain: `ipconfig getifaddr en0` (single line, zero parsing),
/// else enumerate `en*` from `ifconfig -l` (sorted) and probe each.
fn lan_ipv4_macos() -> Option<String> {
    if let Some(a) = probe_macos_iface("en0") {
        return Some(a);
    }
    let out = std::process::Command::new("ifconfig")
        .arg("-l")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let list = String::from_utf8_lossy(&out.stdout).to_string();
    let fields: Vec<&str> = list.split_whitespace().collect();
    for ifc in macos_en_candidates(&fields) {
        if let Some(a) = probe_macos_iface(&ifc) {
            return Some(a);
        }
    }
    None
}

/// One `ipconfig getifaddr <if>` probe: success + a single IPv4 that is
/// neither loopback nor link-local.
fn probe_macos_iface(iface: &str) -> Option<String> {
    let out = std::process::Command::new("ipconfig")
        .args(["getifaddr", iface])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match s.parse::<std::net::Ipv4Addr>() {
        Ok(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(s),
        _ => None,
    }
}

/// Linux chain: `hostname -I` fields through the shared pure picker.
fn lan_ipv4_linux() -> Option<String> {
    let out = std::process::Command::new("hostname")
        .arg("-I")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let list = String::from_utf8_lossy(&out.stdout).to_string();
    let fields: Vec<&str> = list.split_whitespace().collect();
    pick_lan_ipv4(&fields)
}

/// Windows chain: PowerShell `Get-NetIPAddress` (ipconfig output is
/// localization-prone), first non-loopback non-link-local IPv4. The
/// result is re-validated in Rust before use.
fn lan_ipv4_windows() -> Option<String> {
    let script = "(Get-NetIPAddress -AddressFamily IPv4 | \
                  Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } | \
                  Select-Object -First 1).IPAddress";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let first = s.lines().next().unwrap_or("").trim().to_string();
    match first.parse::<std::net::Ipv4Addr>() {
        Ok(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(first),
        _ => None,
    }
}

/// Admission for `vnc web --lan` (pure; the command turns `Reject` into
/// rc=2 + hint, `Allow` into the serving path with its warnings).
#[derive(Debug, PartialEq, Eq)]
pub enum LanAdmission {
    /// No `--lan`: loopback mode, behavior unchanged by wave 10.
    Loopback,
    /// rc=2 rejection with operator-facing message and hint.
    Reject { msg: String, hint: String },
    /// Serving allowed. `warn_extra`: the run's VNC bind is already
    /// non-loopback, so QEMU's ws port is directly exposed on the LAN —
    /// the berry proxy is not the only entrance (extra warning row).
    Allow { warn_extra: bool },
}

/// The security matrix (plan contract 7; wave 11 admits TLS runs):
///
/// - no `--lan` -> loopback, unchanged;
/// - explicit `--tls` -> reject (mutually exclusive with `--lan`);
/// - run without a password -> reject (LAN binding is a non-loopback
///   exposure; wave-1 policy: no open RFB on the LAN — TLS runs
///   included, their page leg is still plaintext http);
/// - everything else -> allow (TLS runs since wave 11: the page stays
///   http and the `/ws` proxy's upstream leg speaks rustls wss; the
///   command layer picks the upstream form), with the extra exposure
///   warning when the run bind is non-loopback.
pub fn lan_admission(lan: bool, listen: &ListenInfo, tls_flag: bool, has_pass: bool) -> LanAdmission {
    if !lan {
        return LanAdmission::Loopback;
    }
    if tls_flag {
        return LanAdmission::Reject {
            msg: "--lan and --tls are mutually exclusive".to_string(),
            // F53: flag-level mutex only — the https-page form is what
            // --tls names; TLS runs themselves take --lan directly
            // since wave 11 (only the proxy's upstream leg speaks wss).
            hint: "the explicit --tls flag is the loopback https-page form; a TLS run uses --lan directly since wave 11 (only the /ws proxy's upstream leg speaks wss)".to_string(),
        };
    }
    if !has_pass {
        return LanAdmission::Reject {
            msg: "--lan refuses a passwordless run (a LAN bind is a non-loopback exposure)".to_string(),
            hint: "restart with --vnc-pass to set one (see also berry vnc passwd)".to_string(),
        };
    }
    LanAdmission::Allow {
        warn_extra: !is_loopback(&listen.host),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U47: the shared IPv4 field picker.
    #[test]
    fn pick_lan_ipv4_cases() {
        // First legal candidate wins, skipping loopback and link-local.
        assert_eq!(
            pick_lan_ipv4(&["127.0.0.1", "169.254.7.7", "192.168.1.105", "10.0.0.5"]),
            Some("192.168.1.105".to_string())
        );
        assert_eq!(pick_lan_ipv4(&["10.0.0.5"]), Some("10.0.0.5".to_string()));
        assert_eq!(pick_lan_ipv4(&["172.16.2.3", "junk"]), Some("172.16.2.3".to_string()));
        // All-loopback, all-link-local, IPv6-only, garbage, empty: None.
        assert_eq!(pick_lan_ipv4(&["127.0.0.1", "127.8.8.8"]), None);
        assert_eq!(pick_lan_ipv4(&["169.254.1.2"]), None);
        assert_eq!(pick_lan_ipv4(&["fe80::1", "2001:db8::2"]), None);
        assert_eq!(pick_lan_ipv4(&["not-an-ip", ""]), None);
        assert_eq!(pick_lan_ipv4(&[]), None);
    }

    /// U47: macOS interface enumeration — filter and numeric sort.
    #[test]
    fn macos_en_candidates_sorting() {
        assert_eq!(
            macos_en_candidates(&["lo0", "en0", "utun4", "en10", "en2", "awdl0", "bridge100", "en"]),
            vec!["en0", "en2", "en10"]
        );
        // "en" without digits and "eno1"-style names are not probes.
        assert!(macos_en_candidates(&["eno1", "enX"]).is_empty());
        assert!(macos_en_candidates(&[]).is_empty());
        // Deterministic: same input, same order.
        let a = macos_en_candidates(&["en3", "en1", "en0"]);
        let b = macos_en_candidates(&["en0", "en1", "en3"]);
        assert_eq!(a, b);
    }

    /// U47: `--host` validation — verbatim acceptance, structural
    /// rejection (empty, whitespace, path separators).
    #[test]
    fn host_override_validation() {
        assert_eq!(validate_host_override("192.168.1.105").unwrap(), "192.168.1.105");
        assert_eq!(validate_host_override("[::1]").unwrap(), "[::1]");
        assert_eq!(validate_host_override("fe80::a").unwrap(), "fe80::a");
        for bad in ["", " ", "a b", "192.168.1.105/x", "a\\b", "host\tname"] {
            let err = validate_host_override(bad).unwrap_err();
            assert_eq!(err.code, 2, "bad={bad:?}");
            assert!(err.hint.unwrap_or_default().contains("--host"), "bad={bad:?}");
        }
    }

    /// U47: fallback semantics of the resolver with an explicit host —
    /// verbatim, no platform probe involved.
    #[test]
    fn resolve_lan_host_explicit_wins() {
        assert_eq!(
            resolve_lan_host(Some("10.1.2.3")),
            LanHost::Addr("10.1.2.3".to_string())
        );
        assert_eq!(resolve_lan_host(None), resolve_lan_host(None));
    }

    /// U48: the full admission matrix, six rows.
    #[test]
    fn admission_matrix() {
        let loop_listen = ListenInfo {
            host: "127.0.0.1".to_string(),
            port: 5901,
            ws: Some(6001),
            tls: false,
        };
        let wide_listen = ListenInfo {
            host: "0.0.0.0".to_string(),
            port: 5901,
            ws: Some(6001),
            tls: false,
        };
        let tls_listen = ListenInfo { tls: true, ..loop_listen.clone() };
        // Row 1: no --lan -> loopback mode regardless of everything.
        for l in [&loop_listen, &wide_listen, &tls_listen] {
            assert_eq!(lan_admission(false, l, true, false), LanAdmission::Loopback);
        }
        // Row 2 (wave 11): TLS run + password + --lan -> allow — the
        // page stays http and the /ws proxy's upstream leg speaks wss
        // (warn_extra follows the bind, exactly like plaintext rows).
        assert_eq!(
            lan_admission(true, &tls_listen, false, true),
            LanAdmission::Allow { warn_extra: false }
        );
        // Row 2b: TLS run without a password stays refused (the LAN leg
        // is plaintext http; the password is the only gate).
        match lan_admission(true, &tls_listen, false, false) {
            LanAdmission::Reject { msg, hint } => {
                assert!(msg.contains("passwordless"), "msg: {msg}");
                assert!(hint.contains("--vnc-pass"), "hint: {hint}");
            }
            other => panic!("passwordless --lan must reject, got {other:?}"),
        }
        // Row 3: --tls flag + --lan -> reject with the mutex hint.
        match lan_admission(true, &loop_listen, true, true) {
            LanAdmission::Reject { msg, hint } => {
                assert!(msg.contains("mutually exclusive"), "msg: {msg}");
                assert!(hint.contains("--lan"), "hint: {hint}");
            }
            other => panic!("--tls + --lan must reject, got {other:?}"),
        }
        // Row 4: plaintext, no password -> reject; hint names --vnc-pass.
        match lan_admission(true, &loop_listen, false, false) {
            LanAdmission::Reject { msg, hint } => {
                assert!(msg.contains("passwordless"), "msg: {msg}");
                assert!(hint.contains("--vnc-pass"), "hint: {hint}");
            }
            other => panic!("passwordless --lan must reject, got {other:?}"),
        }
        // Row 5: plaintext + password + loopback bind -> allow, quiet.
        assert_eq!(
            lan_admission(true, &loop_listen, false, true),
            LanAdmission::Allow { warn_extra: false }
        );
        // Row 6: plaintext + password + non-loopback bind -> allow with
        // the extra exposure warning.
        assert_eq!(
            lan_admission(true, &wide_listen, false, true),
            LanAdmission::Allow { warn_extra: true }
        );
    }
}
