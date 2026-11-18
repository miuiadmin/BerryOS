//! Local transport endpoints: Unix sockets on POSIX, loopback TCP elsewhere.
//! QMP and VNC share this so the rest of the tool never branches on OS.

use std::io::{self, Read, Write};

#[derive(Clone, Debug)]
pub enum Endpoint {
    #[cfg(unix)]
    Unix(std::path::PathBuf),
    Tcp(String),
}

impl Endpoint {
    /// Value for QEMU's `-vnc`. TCP endpoints must use display-number syntax
    /// (`host:{port-5900}`): probe f2 (QEMU 11.0.3) showed `-vnc` rejects the
    /// `tcp:` prefix and treats a single-colon number as a display number
    /// (real port = number + 5900), so literal ports below 5900 are not
    /// expressible and yield an error instead of a silent misbind.
    pub fn qemu_vnc(&self) -> std::result::Result<String, crate::error::BerryError> {
        match self {
            #[cfg(unix)]
            Endpoint::Unix(p) => Ok(format!("unix:{}", p.display())),
            Endpoint::Tcp(a) => {
                let (host, port) = split_host_port(a)?;
                let display = crate::vnc_cfg::port_to_display(port)?;
                Ok(format!("{host}:{display}"))
            }
        }
    }

    pub fn qemu_qmp(&self) -> String {
        match self {
            #[cfg(unix)]
            Endpoint::Unix(p) => format!("unix:{},server=on,wait=off", p.display()),
            Endpoint::Tcp(a) => format!("tcp:{a},server=on,wait=off"),
        }
    }

    pub fn connect(&self) -> io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Endpoint::Unix(p) => {
                let s = std::os::unix::net::UnixStream::connect(p)?;
                Ok(Stream::Unix(s))
            }
            Endpoint::Tcp(a) => {
                let s = std::net::TcpStream::connect(a)?;
                s.set_nodelay(true).ok();
                Ok(Stream::Tcp(s))
            }
        }
    }

    pub fn display(&self) -> String {
        match self {
            #[cfg(unix)]
            Endpoint::Unix(p) => p.display().to_string(),
            Endpoint::Tcp(a) => a.clone(),
        }
    }
}

pub enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    Tcp(std::net::TcpStream),
    /// TLS-wrapped TCP (VNC wave 5: VeNCrypt client session inside
    /// `VncSource`). One rustls session per connection: `try_clone` is not
    /// supported (no dup of the session state machine); shutdown talks to
    /// the socket directly so a blocked read unblocks and the reader thread
    /// exits.
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>),
}

/// Split a `host:port` address string; IPv6 forms may be bracketed
/// (`[::1]:5971`) or bare (`:::5971` is not accepted by std parsers). The
/// host text is preserved verbatim (brackets included) so QEMU argv output
/// matches the accepted `-vnc` spelling.
pub(crate) fn split_host_port(a: &str) -> std::result::Result<(String, u16), crate::error::BerryError> {
    let bad = || crate::error::BerryError::protocol(format!("bad address '{a}'"));
    if let Some(rest) = a.strip_prefix('[') {
        // "[host]:port"
        let (host, tail) = rest.split_once(']').ok_or_else(bad)?;
        let port = tail.strip_prefix(':').ok_or_else(bad)?;
        let port: u16 = port.parse().map_err(|_| bad())?;
        return Ok((format!("[{host}]"), port));
    }
    let (host, port) = a.rsplit_once(':').ok_or_else(bad)?;
    if host.is_empty() {
        // 空主机（":5900"）在 QEMU -vnc 语义 = 全接口监听——静默错绑面，
        // 按「error instead of silent misbind」拒（同 5899 下界形）。
        return Err(bad());
    }
    let port: u16 = port.parse().map_err(|_| bad())?;
    Ok((host.to_string(), port))
}

impl Stream {
    pub fn try_clone(&self) -> io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => Ok(Stream::Unix(s.try_clone()?)),
            Stream::Tcp(s) => Ok(Stream::Tcp(s.try_clone()?)),
            // One session state machine cannot be duplicated; callers that
            // need a second handle must re-dial or keep a single Arc<Mutex>.
            Stream::Tls(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "TLS stream cannot be cloned",
            )),
        }
    }

    pub fn set_read_timeout(&self, d: Option<std::time::Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(d),
            Stream::Tcp(s) => s.set_read_timeout(d),
            Stream::Tls(s) => s.sock.set_read_timeout(d),
        }
    }

    pub fn set_write_timeout(&self, d: Option<std::time::Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_write_timeout(d),
            Stream::Tcp(s) => s.set_write_timeout(d),
            Stream::Tls(s) => s.sock.set_write_timeout(d),
        }
    }

    pub fn shutdown(&self) {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            Stream::Tcp(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            Stream::Tls(s) => {
                let _ = s.sock.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
            Stream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
            Stream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_vnc_tcp_uses_display_number() {
        // Probe f2: -vnc takes display numbers, so 127.0.0.1:5971 must be
        // written as 127.0.0.1:71 (real port = number + 5900).
        assert_eq!(
            Endpoint::Tcp("127.0.0.1:5971".into()).qemu_vnc().unwrap(),
            "127.0.0.1:71"
        );
        assert_eq!(
            Endpoint::Tcp("[::1]:5900".into()).qemu_vnc().unwrap(),
            "[::1]:0"
        );
        // Ports below 5900 cannot be expressed; error instead of misbind.
        assert!(Endpoint::Tcp("127.0.0.1:5899".into()).qemu_vnc().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn qemu_vnc_unix_unchanged() {
        assert_eq!(
            Endpoint::Unix("/tmp/x.sock".into()).qemu_vnc().unwrap(),
            "unix:/tmp/x.sock"
        );
    }

    /// split_host_port 边界全家桶：括号 IPv6 保留原拼写（QEMU argv 对读
    /// 契约）、非括号 rsplit_once 形、各坏形全拒（错误而非静默错绑）。
    #[test]
    fn split_host_port_boundaries() {
        // 括号 IPv6：host 原样保留含括号——qemu_vnc 输出与 -vnc 接受的
        // 拼写一致（display() 同源）。
        assert_eq!(
            split_host_port("[::1]:5971").unwrap(),
            ("[::1]".to_string(), 5971)
        );
        assert_eq!(
            split_host_port("[fe80::1%25lo0]:5900").unwrap(),
            ("[fe80::1%25lo0]".to_string(), 5900)
        );
        // IPv4 + rsplit_once 形（右冒号优先——主机含冒号时取最右）。
        assert_eq!(
            split_host_port("127.0.0.1:5971").unwrap(),
            ("127.0.0.1".to_string(), 5971)
        );
        // 非 IPv4 主机名。
        assert_eq!(
            split_host_port("localhost:5901").unwrap(),
            ("localhost".to_string(), 5901)
        );
        // 括号 IPv6 括号面坏形。
        for bad in [
            "localhost",           // 无端口分隔
            ":5900",               // 空主机（QEMU 语义 = 全接口监听，拒）
            "localhost:",          // 空端口
            "localhost:http",      // 非数字端口
            "localhost:65536",     // u16 越界
            "[::1",                // 括号未闭合
            "[::1]5900",           // ] 后缺 :
            "[::1]:x",             // 括号形非数字端口
        ] {
            assert!(
                split_host_port(bad).is_err(),
                "split_host_port({bad:?}) should reject"
            );
        }
        // 端口 0 在解析层合法（u16 域内）——不可表达性由 qemu_vnc 层拒
        //（<5900 下界），分层各司其职。
        assert_eq!(
            split_host_port("localhost:0").unwrap(),
            ("localhost".to_string(), 0)
        );
        assert!(Endpoint::Tcp("localhost:0".into()).qemu_vnc().is_err());
    }

    /// qemu_qmp 端点拼写：TCP 形 server=on,wait=off 后缀与 Unix 形同构。
    #[test]
    fn qemu_qmp_endpoint_spelling() {
        assert_eq!(
            Endpoint::Tcp("127.0.0.1:4444".into()).qemu_qmp(),
            "tcp:127.0.0.1:4444,server=on,wait=off"
        );
        #[cfg(unix)]
        assert_eq!(
            Endpoint::Unix("/tmp/qmp.sock".into()).qemu_qmp(),
            "unix:/tmp/qmp.sock,server=on,wait=off"
        );
    }

    /// display() 人类可读形：TCP 原串、Unix 原路径。
    #[test]
    fn endpoint_display() {
        assert_eq!(Endpoint::Tcp("10.0.0.2:5901".into()).display(), "10.0.0.2:5901");
        #[cfg(unix)]
        assert_eq!(
            Endpoint::Unix("/tmp/v.sock".into()).display(),
            "/tmp/v.sock"
        );
    }
}
