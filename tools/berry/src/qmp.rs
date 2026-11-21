//! QMP control plane: lifecycle, reset, screendump, key/pointer injection,
//! and an asynchronous event pump (wave 2: VNC client audit events).
//!
//! Structure: a dedicated reader thread owns the receive half and routes
//! every incoming JSON line — command replies go to the pending map keyed by
//! id, asynchronous events (e.g. VNC_CONNECTED) go to an optional sink that
//! the session host installs. The caller-facing contract of `execute`
//! (command name in QMP error text, error shapes, 10s timeout semantics) is
//! unchanged from the wave-1 single-threaded read loop.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{BerryError, Result};
use crate::net::{Endpoint, Stream};

/// Async event sink: receives (event name, full event object).
pub type EventSink = Arc<dyn Fn(&str, &Value) + Send + Sync>;

struct Pending {
    cmd: String,
    tx: std::sync::mpsc::Sender<Result<Value>>,
}

#[derive(Default)]
struct ReaderShared {
    pending: Mutex<HashMap<u64, Pending>>,
    sink: Mutex<Option<EventSink>>,
}

impl ReaderShared {
    fn take_earliest(&self) -> Option<Pending> {
        let mut m = self.pending.lock().unwrap();
        let min = m.keys().copied().min()?;
        m.remove(&min)
    }

    fn fail_all(&self, msg: String) {
        let mut m = self.pending.lock().unwrap();
        for (_, p) in m.drain() {
            let _ = p.tx.send(Err(BerryError::protocol(msg.clone())));
        }
    }
}

/// What one incoming QMP line is (pure, unit-tested).
enum Incoming<'a> {
    /// Asynchronous event with its name.
    Event(&'a str),
    /// Command response; `error` true when it carries an "error" object.
    Response { id: Option<u64>, error: bool },
    /// Neither (greetings, unknown noise).
    Other,
}

fn classify(v: &Value) -> Incoming<'_> {
    if let Some(name) = v.get("event").and_then(Value::as_str) {
        return Incoming::Event(name);
    }
    if v.get("return").is_some() || v.get("error").is_some() {
        return Incoming::Response {
            id: v.get("id").and_then(Value::as_u64),
            error: v.get("error").is_some(),
        };
    }
    Incoming::Other
}

fn finish(p: Pending, v: &Value) {
    let out = match v.get("error") {
        Some(err) => {
            let class = err.get("class").and_then(Value::as_str).unwrap_or("error");
            let desc = err.get("desc").and_then(Value::as_str).unwrap_or("");
            Err(BerryError::protocol(format!("QMP {}: {}: {}", p.cmd, class, desc)))
        }
        None => Ok(v.get("return").cloned().unwrap_or(Value::Null)),
    };
    let _ = p.tx.send(out);
}

fn read_loop(reader: &mut BufReader<Stream>, shared: &ReaderShared) {
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                // QEMU exited or the socket broke: unblock every waiter.
                shared.fail_all("QMP: connection closed".to_string());
                return;
            }
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                shared.fail_all(format!("JSON: {e}"));
                return;
            }
        };
        match classify(&v) {
            Incoming::Event(name) => {
                let sink = shared.sink.lock().unwrap().clone();
                if let Some(f) = sink {
                    f(name, &v);
                }
                // No sink installed: drop the event (wave-1 semantics).
            }
            Incoming::Response { id: Some(id), error } => {
                let p = shared.pending.lock().unwrap().remove(&id);
                if let Some(p) = p {
                    finish(p, &v);
                } else if error {
                    // Late error for a caller that already timed out.
                }
            }
            Incoming::Response { id: None, .. } => {
                // No id on a return/error: complete the earliest pending
                // (same fallback as the wave-1 loop).
                if let Some(p) = shared.take_earliest() {
                    finish(p, &v);
                }
            }
            Incoming::Other => {}
        }
    }
}

pub struct QmpClient {
    writer: Stream,
    shared: Arc<ReaderShared>,
    next_id: u64,
}

impl QmpClient {
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let stream = endpoint.connect().map_err(|e| {
            BerryError::protocol(format!("QMP connect {}: {e}", endpoint.display()))
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let writer = stream.try_clone()?;
        let shared = Arc::new(ReaderShared::default());
        // Reader thread: forwards the greeting, then routes lines forever.
        let (greet_tx, greet_rx) = std::sync::mpsc::channel::<String>();
        let rd_shared = shared.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(n) if n > 0 => {}
                _ => {
                    // Fail fast instead of letting connect() time out.
                    let _ = greet_tx.send(String::new());
                    return;
                }
            }
            if greet_tx.send(line).is_err() {
                return;
            }
            read_loop(&mut reader, &rd_shared);
        });
        let greeting = greet_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| {
                // Shutdown before failing: the client is never constructed,
                // so its Drop (which does this) will not run — without this
                // the reader thread blocks forever on its socket clone
                // (thread + fd leak on every greeting timeout).
                let _ = writer.shutdown();
                BerryError::timeout("QMP: read timeout")
            })?;
        if greeting.is_empty() {
            let _ = writer.shutdown();
            return Err(BerryError::protocol("QMP: connection closed"));
        }
        let v: Value = match serde_json::from_str(&greeting) {
            Ok(v) => v,
            Err(e) => {
                let _ = writer.shutdown();
                return Err(e.into());
            }
        };
        if v.get("QMP").is_none() {
            let _ = writer.shutdown();
            return Err(BerryError::protocol("QMP: bad greeting"));
        }
        let mut client = Self {
            writer,
            shared,
            next_id: 1,
        };
        if let Err(e) = client.execute("qmp_capabilities", json!({})) {
            let _ = client.writer.shutdown();
            return Err(e);
        }
        // The socket read timeout existed for the synchronous connect phase
        // (wave-1 semantics). The reader thread must now block indefinitely:
        // an idle QMP connection is healthy, and a WouldBlock there would
        // wrongly look like a closed connection and kill the event pump.
        // `execute` enforces its own 10s per-command timeout via the channel.
        client.writer.set_read_timeout(None)?;
        Ok(client)
    }

    /// Install the asynchronous event sink (VNC client audit, wave 2). Only
    /// one sink is kept; installing replaces the previous one.
    pub fn set_event_sink(&self, sink: EventSink) {
        *self.shared.sink.lock().unwrap() = Some(sink);
    }

    pub fn execute(&mut self, cmd: &str, args: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"execute": cmd, "arguments": args, "id": id});
        let (tx, rx) = std::sync::mpsc::channel::<Result<Value>>();
        self.shared
            .pending
            .lock()
            .unwrap()
            .insert(id, Pending { cmd: cmd.to_string(), tx });
        let write = (|| -> std::io::Result<()> {
            let w = &mut self.writer;
            serde_json::to_writer(&mut *w, &msg)?;
            w.write_all(b"\n")?;
            w.flush()
        })();
        if let Err(e) = write {
            self.shared.pending.lock().unwrap().remove(&id);
            return Err(BerryError::device(format!("QMP write: {e}")));
        }
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(res) => res,
            Err(_) => {
                self.shared.pending.lock().unwrap().remove(&id);
                Err(BerryError::timeout("QMP: read timeout"))
            }
        }
    }

    pub fn screendump(&mut self, path: &str) -> Result<()> {
        self.execute("screendump", json!({"filename": path}))?;
        Ok(())
    }

    pub fn send_key(&mut self, qcode: &str) -> Result<()> {
        self.execute(
            "send-key",
            json!({"keys":[{"type":"qcode","data":qcode}]}),
        )?;
        Ok(())
    }

    /// Absolute pointer injection, 0..32767 axis domain (virtio-tablet).
    pub fn pointer_abs(&mut self, x: u16, y: u16, down: bool) -> Result<()> {
        let mut events = vec![
            json!({"type":"abs","data":{"axis":"x","value":x as i64}}),
            json!({"type":"abs","data":{"axis":"y","value":y as i64}}),
        ];
        if down {
            events.push(json!({"type":"btn","data":{"down":true,"button":"left"}}));
        } else {
            events.push(json!({"type":"btn","data":{"down":false,"button":"left"}}));
        }
        self.execute("input-send-event", json!({"events":events}))?;
        Ok(())
    }

    pub fn tap_abs(&mut self, x: u16, y: u16) -> Result<()> {
        self.execute(
            "input-send-event",
            json!({"events":[
                {"type":"abs","data":{"axis":"x","value":x as i64}},
                {"type":"abs","data":{"axis":"y","value":y as i64}},
                {"type":"btn","data":{"down":true,"button":"left"}},
                {"type":"btn","data":{"down":false,"button":"left"}}
            ]}),
        )?;
        Ok(())
    }

    pub fn query_status(&mut self) -> Result<Value> {
        self.execute("query-status", json!({}))
    }

    /// Inject the VNC password at runtime (design: never in argv/logs). QEMU
    /// routes this to the first declared `-vnc` server (id=default), which is
    /// the external endpoint; error descs from QMP do not echo the password.
    pub fn set_vnc_password(&mut self, pw: &str) -> Result<()> {
        self.execute("set_password", json!({"protocol": "vnc", "password": pw}))?;
        Ok(())
    }

    /// Schedule VNC password expiry (probe p5 schema: `time` is `never` or a
    /// positive number of seconds as a string; QEMU validates the value).
    pub fn expire_vnc_password(&mut self, time: &str) -> Result<()> {
        self.execute("expire_password", json!({"protocol": "vnc", "time": time}))?;
        Ok(())
    }

    /// All VNC servers/clients. Unlike `query-vnc` (single default object,
    /// probe f4), this returns an array whose entries carry `id`, `auth`,
    /// `server[]`, and `clients[]`, distinguishing unix/tcp/websocket.
    pub fn query_vnc_servers(&mut self) -> Result<Value> {
        self.execute("query-vnc-servers", json!({}))
    }

    pub fn system_reset(&mut self) -> Result<()> {
        self.execute("system_reset", json!({}))?;
        Ok(())
    }

    pub fn system_powerdown(&mut self) -> Result<()> {
        self.execute("system_powerdown", json!({}))?;
        Ok(())
    }

    pub fn quit(&mut self) -> Result<()> {
        // QEMU may close the socket before replying; both outcomes are success.
        let _ = self.execute("quit", json!({}));
        Ok(())
    }

    /// Fire-and-forget quit for teardown paths: never blocks on a reply.
    pub fn quit_now(&mut self) -> Result<()> {
        let msg = json!({"execute": "quit", "id": self.next_id});
        self.next_id += 1;
        let w = &mut self.writer;
        serde_json::to_writer(&mut *w, &msg)?;
        w.write_all(b"\n")?;
        w.flush()?;
        Ok(())
    }
}

impl Drop for QmpClient {
    fn drop(&mut self) {
        // Unblock the reader thread: shutdown hits the shared socket, so its
        // blocking read returns and the thread exits instead of lingering.
        self.writer.shutdown();
    }
}

// ---------------------------------------------------------------------------
// QMP 指针注入器（QmpInjector 家族）已随拦截器退役刀整族删除
// （2026-11-20：判例驳斥后 web/ram 两路改走 VNC 直通，零消费者；
// 归档锚=.agents/research/证据快照/2026-11-20-VNC直通绝对坐标判例驳斥.md）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qmp_error_shape() {
        let v: Value = serde_json::from_str(
            r#"{"error":{"class":"GenericError","desc":"bad"}}"#,
        )
        .unwrap();
        assert_eq!(v["error"]["class"], "GenericError");
    }

    #[test]
    fn line_classification() {
        let ev: Value = serde_json::from_str(
            r#"{"timestamp":{"seconds":1},"event":"VNC_CONNECTED","data":{}}"#,
        )
        .unwrap();
        assert!(matches!(classify(&ev), Incoming::Event("VNC_CONNECTED")));

        let resp: Value =
            serde_json::from_str(r#"{"return":{},"id":7}"#).unwrap();
        assert!(matches!(
            classify(&resp),
            Incoming::Response { id: Some(7), error: false }
        ));

        let err: Value = serde_json::from_str(
            r#"{"error":{"class":"GenericError","desc":"x"},"id":9}"#,
        )
        .unwrap();
        assert!(matches!(
            classify(&err),
            Incoming::Response { id: Some(9), error: true }
        ));

        // return without id: legacy "complete earliest pending" fallback.
        let anon: Value = serde_json::from_str(r#"{"return":{}}"#).unwrap();
        assert!(matches!(
            classify(&anon),
            Incoming::Response { id: None, error: false }
        ));

        let greeting: Value =
            serde_json::from_str(r#"{"QMP":{"version":{"qemu":{}}}}"#).unwrap();
        assert!(matches!(classify(&greeting), Incoming::Other));
    }

    #[test]
    fn response_error_message_carries_cmd() {
        // The reader formats QMP errors with the pending command name — the
        // execute() contract callers depend on (wave 1 preserved).
        let (tx, rx) = std::sync::mpsc::channel();
        let p = Pending { cmd: "set_password".into(), tx };
        let v: Value = serde_json::from_str(
            r#"{"error":{"class":"GenericError","desc":"parameter connected must be keep"}}"#,
        )
        .unwrap();
        finish(p, &v);
        let err = rx.recv().unwrap().unwrap_err();
        assert_eq!(
            err.message,
            "QMP set_password: GenericError: parameter connected must be keep"
        );
    }

    /// 注入报文钉板（迁出 web.rs 的零漂移锚）：中点坐标缩放、left 按下沿、
    /// 事件无缝拼接与自带换行——与 LAN 拦    #[test]
    fn connect_handshake_event_pump_and_execute_roundtrip() {
        use std::io::{BufRead, BufReader, Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(br#"{"QMP":{"version":{"qemu":{"micro":3}},"capabilities":[]}}"#)
                .unwrap();
            s.write_all(b"\n").unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            // 1st request: qmp_capabilities (sent by connect itself).
            r.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"), "{line}");
            s.write_all(br#"{"return":{},"id":1}"#).unwrap();
            s.write_all(b"\n").unwrap();
            // 2nd request: answer with an async event first, then the reply —
            // the pump must route both in one read sweep.
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("query-status"), "{line}");
            s.write_all(
                br#"{"timestamp":{"seconds":1},"event":"VNC_CONNECTED","data":{"server":{"auth":"vnc"},"client":{"host":"127.0.0.1","service":"5900","websocket":true}}}"#,
            )
            .unwrap();
            s.write_all(b"\n").unwrap();
            s.write_all(br#"{"return":{"status":"running","running":true},"id":2}"#)
                .unwrap();
            s.write_all(b"\n").unwrap();
            // Hold the socket until the client drops (EOF ends this read).
            let mut sink = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink.extend_from_slice(&buf[..n]),
                }
            }
        });
        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let ev_sink = events.clone();
        let mut c = QmpClient::connect(&Endpoint::Tcp(format!("127.0.0.1:{port}")))
            .expect("handshake must negotiate against the scripted greeting");
        c.set_event_sink(Arc::new(move |name, v| {
            ev_sink.lock().unwrap().push((name.to_string(), v.clone()));
        }));
        let st = c.execute("query-status", json!({})).expect("execute reply");
        assert_eq!(st["status"], "running");
        assert_eq!(st["running"], true);
        // The event line was written before the reply; the reader processes
        // lines in order, so by the time execute returned it is dispatched.
        let got = events.lock().unwrap().clone();
        assert_eq!(got.len(), 1, "sink must have received exactly the event");
        assert_eq!(got[0].0, "VNC_CONNECTED");
        assert_eq!(got[0].1["event"], "VNC_CONNECTED");
        assert_eq!(got[0].1["data"]["server"]["auth"], "vnc");
        assert_eq!(got[0].1["data"]["client"]["websocket"], true);
        // Drop shuts the socket; the server script then exits cleanly.
        drop(c);
        server.join().unwrap();
    }
}
