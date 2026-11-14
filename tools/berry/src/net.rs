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
}
