//! TLS configuration loading for the VNC wave-5 planes: the VeNCrypt
//! client (`vnc check`), the https page server and the wss termination
//! proxy (`vnc web --tls`). All configurations come from a `berry vnc
//! certs` directory (ca-cert.pem / server-cert.pem / server-key.pem).
//!
//! Verification is strict by default: the client side trusts exactly the
//! directory's CA (chain + SAN against the dial host). `--insecure` is an
//! explicit escape hatch that accepts everything and must be paired with a
//! stderr warning at the call site. Error paths carry file names only —
//! private key material never appears in messages.

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

use crate::error::{BerryError, Result};

/// File names inside a `berry vnc certs` directory (wave-2 layout, probe
/// p1: reused verbatim as the trust/config source here).
pub const CA_CERT: &str = "ca-cert.pem";
pub const SERVER_CERT: &str = "server-cert.pem";
pub const SERVER_KEY: &str = "server-key.pem";

fn missing(dir: &Path, name: &str) -> BerryError {
    BerryError::new(
        2,
        format!("{} is missing {}", dir.display(), name),
    )
    .with_hint("generate a self-signed set with `berry vnc certs <dir>`")
}

fn bad_pem(path: &Path, err: &str) -> BerryError {
    // pki-types parse errors describe positions, never key material; the
    // file name rides along so the operator knows which artifact is bad.
    BerryError::protocol(format!("bad PEM in {}: {err}", path.display()))
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Strict client config: trust exactly `dir/ca-cert.pem` (chain + SAN).
/// The CA file must exist and hold at least one certificate, otherwise the
/// error is rc=2 with the `berry vnc certs` hint.
pub fn load_client_config(dir: &Path) -> Result<Arc<ClientConfig>> {
    let ca = dir.join(CA_CERT);
    if !ca.is_file() {
        return Err(missing(dir, CA_CERT));
    }
    let mut roots = RootCertStore::empty();
    let mut count = 0usize;
    for cert in CertificateDer::pem_file_iter(&ca).map_err(|e| bad_pem(&ca, &e.to_string()))? {
        let cert = cert.map_err(|e| bad_pem(&ca, &e.to_string()))?;
        roots.add(cert)
            .map_err(|e| bad_pem(&ca, &e.to_string()))?;
        count += 1;
    }
    if count == 0 {
        return Err(bad_pem(&ca, "no certificates found"));
    }
    let cfg = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| BerryError::internal(format!("TLS versions: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

/// Insecure client config: every certificate is accepted (danger verifier).
/// The call site must print the one-time warning before using it.
pub fn load_client_config_insecure() -> Arc<ClientConfig> {
    let verifier = Arc::new(AcceptAll(provider()));
    let mut cfg = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the safe default versions")
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    cfg.dangerous()
        .set_certificate_verifier(verifier);
    Arc::new(cfg)
}

/// Server config for the https page / wss proxy listener: the directory's
/// server-cert.pem + server-key.pem pair. No client-auth request (browsers
/// have no client certificate to offer — the exact wall that forces the
/// berry-side termination).
pub fn load_server_config(dir: &Path) -> Result<Arc<ServerConfig>> {
    let cert_path = dir.join(SERVER_CERT);
    let key_path = dir.join(SERVER_KEY);
    if !cert_path.is_file() {
        return Err(missing(dir, SERVER_CERT));
    }
    if !key_path.is_file() {
        return Err(missing(dir, SERVER_KEY));
    }
    let mut certs = Vec::new();
    for cert in CertificateDer::pem_file_iter(&cert_path)
        .map_err(|e| bad_pem(&cert_path, &e.to_string()))?
    {
        certs.push(cert.map_err(|e| bad_pem(&cert_path, &e.to_string()))?);
    }
    if certs.is_empty() {
        return Err(bad_pem(&cert_path, "no certificates found"));
    }
    let key = PrivateKeyDer::pem_file_iter(&key_path)
        .map_err(|e| bad_pem(&key_path, &e.to_string()))?
        .next()
        .transpose()
        .map_err(|e| bad_pem(&key_path, &e.to_string()))?
        .ok_or_else(|| bad_pem(&key_path, "no private key found"))?;
    let cfg = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| BerryError::internal(format!("TLS versions: {e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| BerryError::device(format!("{}: {e}", key_path.display())))?;
    Ok(Arc::new(cfg))
}

/// Dial host -> ServerName for the rustls client: an IP literal becomes
/// `IpAddress` (SAN IP verification), anything else `DnsName`. Brackets of
/// IPv6 forms (`[::1]`) are stripped first, matching `dial_host` output.
pub fn server_name(host: &str) -> Result<ServerName<'static>> {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = h.parse::<IpAddr>() {
        ServerName::try_from(ip)
            .map_err(|_| BerryError::protocol(format!("bad server name '{h}'")))
    } else {
        ServerName::try_from(h.to_string())
            .map_err(|_| {
                BerryError::protocol(format!("bad server name '{h}'"))
                    .with_hint("the TLS dial host must be an IP or a DNS name")
            })
    }
}

/// `--insecure` verifier: accepts any chain and any signature. The
/// provider's scheme list keeps handshake algorithm selection consistent
/// with the strict path.
#[derive(Debug)]
struct AcceptAll(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    /// A tiny certificate set generated once per test run (openssl
    /// subprocess) and shared via a OnceLock so every test across the
    /// crate (tls error paths, the vnc.rs VeNCrypt loopback, web.rs TLS
    /// serving) sees the real on-disk layout of `berry vnc certs`. The
    /// server cert chains to this CA and carries 127.0.0.1/localhost
    /// SANs.
    pub fn certs_dir() -> &'static Path {
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        gen(&DIR, "berry-tls-test")
    }

    /// A second, fully independent set (different CA keypair, identical
    /// layout): the wrong-CA negative test trusts this one while the
    /// peer serves a `certs_dir` certificate, pinning that strict
    /// verification actually rejects an unrelated issuer (F28) — if
    /// `load_client_config` ever degraded to trust-everything, that
    /// test fails.
    pub fn other_ca_dir() -> &'static Path {
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        gen(&DIR, "berry-tls-test-other")
    }

    /// Generate one self-contained certs dir under the temp dir. Both
    /// sets even share the CNs — name equality must not matter, only
    /// the keypair the server's chain actually verifies against.
    fn gen(cell: &'static OnceLock<PathBuf>, tag: &str) -> &'static Path {
        Path::new(cell.get_or_init(|| {
            let d = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            let openssl = if Path::new("/usr/bin/openssl").is_file() {
                "/usr/bin/openssl"
            } else {
                "openssl"
            };
            let steps: Vec<Vec<&str>> = vec![
                vec!["req", "-nodes", "-new", "-x509", "-newkey", "rsa:2048",
                    "-keyout", "ca-key.pem", "-out", super::CA_CERT, "-days", "2",
                    "-subj", "/CN=berry-test-ca",
                    "-addext", "basicConstraints=critical,CA:TRUE",
                    "-addext", "keyUsage=critical,keyCertSign,cRLSign"],
                vec!["req", "-nodes", "-new", "-newkey", "rsa:2048",
                    "-keyout", super::SERVER_KEY, "-out", "server.csr",
                    "-subj", "/CN=localhost"],
                vec!["x509", "-req", "-in", "server.csr", "-CA", super::CA_CERT,
                    "-CAkey", "ca-key.pem", "-CAcreateserial", "-out", super::SERVER_CERT,
                    "-days", "2",
                    "-extfile", "server.ext"],
            ];
            std::fs::write(d.join("server.ext"), "subjectAltName=IP:127.0.0.1,DNS:localhost\n")
                .unwrap();
            for args in &steps {
                let out = std::process::Command::new(openssl)
                    .args(args)
                    .current_dir(&d)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "openssl {} failed: {}",
                    args[0],
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            d
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::certs_dir;

    #[test]
    fn client_config_missing_ca_is_usage_error_with_hint() {
        let empty = std::env::temp_dir().join(format!("berry-tls-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let e = load_client_config(&empty).unwrap_err();
        assert_eq!(e.code, 2, "missing CA is a usage error (rc=2)");
        assert!(e.message.contains(CA_CERT));
        assert!(e.hint.unwrap().contains("berry vnc certs"));
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn client_config_bad_pem_names_the_file() {
        let d = std::env::temp_dir().join(format!("berry-tls-badpem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(CA_CERT), "this is not a pem certificate\n").unwrap();
        let e = load_client_config(&d).unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.message.contains(CA_CERT), "file name must ride along: {e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn server_config_missing_key_is_reported() {
        // A dir with the cert but no key: the key absence is the error.
        let e = load_server_config(certs_dir().parent().unwrap()).unwrap_err();
        assert!(e.message.contains(SERVER_KEY) || e.message.contains(SERVER_CERT));
    }

    #[test]
    fn configs_load_from_a_real_certs_dir() {
        let c = load_client_config(certs_dir()).unwrap();
        assert!(c.alpn_protocols.is_empty(), "no ALPN on the RFB/VeNCrypt path");
        let s = load_server_config(certs_dir()).unwrap();
        assert!(s.max_early_data_size == 0);
    }

    #[test]
    fn insecure_config_builds_without_panic() {
        let _ = load_client_config_insecure();
    }

    #[test]
    fn server_name_ip_and_dns_forms() {
        assert!(matches!(server_name("127.0.0.1").unwrap(), ServerName::IpAddress(_)));
        assert!(matches!(server_name("[::1]").unwrap(), ServerName::IpAddress(_)));
        assert!(matches!(server_name("localhost").unwrap(), ServerName::DnsName(_)));
        // Not a DNS name, not an IP: rejected with a hint.
        let e = server_name("not a name!").unwrap_err();
        assert_eq!(e.code, 7);
        assert!(e.hint.is_some());
    }
}

/// 十三刀补：server_name 括号剥离深面（中缀/前缀括号不动——trim 边
/// 契约与 is_loopback 同式；bare IPv6 直入 IpAddress 档）。
#[cfg(test)]
mod server_name_tests {
    use super::*;

    #[test]
    fn server_name_bracket_edge_forms() {
        // 前后缀括号成对剥（dial_host 输出形）。
        assert!(matches!(server_name("[fe80::1]").unwrap(), ServerName::IpAddress(_)));
        // 中缀括号不是剥离目标：整体当 DNS 名解析（非法名拒，code 7）。
        let e = server_name("host[name").unwrap_err();
        assert_eq!(e.code, 7);
        // bare IPv6 不带括号直接可解析（split 前的原始 host 形）。
        assert!(matches!(server_name("::1").unwrap(), ServerName::IpAddress(_)));
        // 尾缀孤立括号被宽松 trim 剥掉（is_loopback 同款 trim 边契约）：
        // "name]" → "name" 落 DnsName 档——钉实际行为防重构漂移。
        assert!(matches!(server_name("name]").unwrap(), ServerName::DnsName(_)));
        // 空串拒形。
        assert!(server_name("").is_err());
        // DNS 名保持 DnsName 档（IP 优先、非 IP 落 DNS 的分档律）。
        assert!(matches!(server_name("berry.local").unwrap(), ServerName::DnsName(_)));
    }
}
