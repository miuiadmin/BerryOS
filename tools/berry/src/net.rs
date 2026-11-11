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
    pub fn qemu_vnc(&self) -> String {
        match self {
            #[cfg(unix)]
            Endpoint::Unix(p) => format!("unix:{}", p.display()),
            Endpoint::Tcp(a) => format!("tcp:{a}"),
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
}

impl Stream {
    pub fn try_clone(&self) -> io::Result<Stream> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => Ok(Stream::Unix(s.try_clone()?)),
            Stream::Tcp(s) => Ok(Stream::Tcp(s.try_clone()?)),
        }
    }

    pub fn set_read_timeout(&self, d: Option<std::time::Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(d),
            Stream::Tcp(s) => s.set_read_timeout(d),
        }
    }

    pub fn set_write_timeout(&self, d: Option<std::time::Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_write_timeout(d),
            Stream::Tcp(s) => s.set_write_timeout(d),
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
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}
