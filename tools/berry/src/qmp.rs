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
use std::path::Path;
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
// 会话级指针注入器（I2 止血：原 web.rs LAN 拦截器抽出共用）
// ---------------------------------------------------------------------------

/// QMP 指针注入器：连 run/qmp-inject.sock（QMP unix 服务端，多客户端），
/// capabilities 一次，之后每个 PointerEvent 一条 input-send-event（abs
/// 值域 0..32767 + left btn）。两类持有者共用：LAN /ws 代理拦截器（每代理
/// 连接一个，连不上落纯字节管道）与 RamSource 原生指针主路（首个指针事件
/// 才连，失联退 RFB msg5 保底）。无排空线程——正确性依据（对抗审查活体
/// 饱和探针实证，QEMU 11.0.3 monitor.c）：QMP 监视器把未写出的响应暂存
/// **无界用户态 outbuf**（monitor_flush_locked）且**永不因输出积压停读
/// 输入**（suspend 仅由请求队列准入触发），故不读响应不阻塞写路径（百
/// 万事件零超时实测）；残余代价 = QEMU 侧 RSS 每事件 ~65B 缓存爬坡、断
/// 连即释放——pointer() 内的顺手有界排水（drain_pending）持续清空本端
/// 接收队列使 QEMU 得以及时冲刷，把爬坡压在内核缓冲量级。持有克隆的专
/// 职排空线程会让连接永不关闭 → fd 泄漏（既有设计取舍维持）。
pub struct QmpInjector {
    sock: Option<std::os::unix::net::UnixStream>,
    /// 排水用非阻塞克隆（connect 时 try_clone+置 O_NONBLOCK——只读丢弃，
    /// 不触碰阻塞写路径与其 2s 守护；克隆随 sock 同生命周期 drop）。
    drain: Option<std::os::unix::net::UnixStream>,
}

impl QmpInjector {
    /// 连接（无超时——LAN 拦截器旧形态原样，浏览器路行为零漂移）。
    pub fn connect(path: &Path) -> Option<Self> {
        Self::connect_guarded(path, None)
    }

    /// 连接并协商 capabilities；`guard` 在场时对 greeting/回读与后续写
    /// 全程设该超时（原生窗口路在 UI 线程连/写——QEMU 卡死守护：超时
    /// 即 None/失联，调用方走 RFB 保底，窗口不冻结）。任一步失败返回
    /// None（调用方自选降级面）。
    pub fn connect_guarded(path: &Path, guard: Option<Duration>) -> Option<Self> {
        use std::io::{BufRead, BufReader, Write};
        let mut s = std::os::unix::net::UnixStream::connect(path).ok()?;
        if let Some(d) = guard {
            s.set_read_timeout(Some(d)).ok()?;
            s.set_write_timeout(Some(d)).ok()?;
        }
        let mut r = BufReader::new(s.try_clone().ok()?);
        let mut line = String::new();
        r.read_line(&mut line).ok()?; // greeting
        writeln!(s, "{{\"execute\":\"qmp_capabilities\"}}").ok()?;
        s.flush().ok()?;
        line.clear();
        r.read_line(&mut line).ok()?;
        if !line.contains("return") {
            return None;
        }
        // 排水克隆（对抗审查卫生项）：非阻塞只读副本，pointer() 内顺手
        // 清空本端接收队列——QMP 每命令一条 {"return":{}} 同步响应，无人
        // 读则 QEMU 侧无界 outbuf 持续爬坡（见结构体注）；克隆失败不致命
        // （排水面退化缺席，正确性不受影响）。
        let drain = s.try_clone().ok().map(|d| {
            let _ = d.set_nonblocking(true);
            d
        });
        Some(QmpInjector { sock: Some(s), drain })
    }

    /// 发一枚 PointerEvent（报文见 `pointer_msg`，与 LAN 拦截器旧实现
    /// 逐字节同式）。返回 false = 注入路未连/已失联（调用方走保底）；
    /// 写失败即置 sock=None 永久哑化，与 LAN 拦截器旧语义一致。
    pub fn pointer(&mut self, x: u16, y: u16, buttons: u8, gw: u32, gh: u32) -> bool {
        use std::io::Write;
        self.drain_pending();
        let Some(s) = self.sock.as_mut() else { return false };
        let msg = pointer_msg(x, y, buttons, gw, gh);
        if writeln!(s, "{msg}").is_err() || s.flush().is_err() {
            self.sock = None;
            return false;
        }
        true
    }

    /// 有界顺手排水（≤64KiB/调用，非阻塞——WouldBlock 即止）：清空本端
    /// 接收队列里的同步响应/异步事件残留，让 QEMU 得以及时把 outbuf 冲
    /// 进内核（见结构体注的 RSS 爬坡抑制）。读错误不置 sock=None（排水
    /// 面失败与写路径失联是两回事——克隆与主柄同底 fd，ECONNRESET 等终
    /// 态错会随后在写路径如实报出）。
    fn drain_pending(&mut self) {
        use std::io::Read;
        let Some(d) = self.drain.as_mut() else { return };
        let mut buf = [0u8; 4096];
        let mut budget = 16; // 16×4KiB = 64KiB 帽
        loop {
            match d.read(&mut buf) {
                Ok(0) => return,
                Ok(_) => {
                    budget -= 1;
                    if budget == 0 {
                        return;
                    }
                }
                Err(_) => return, // WouldBlock/临时面——下拍再来
            }
        }
    }
}

/// PointerEvent → input-send-event 报文：客机像素坐标缩放到 abs 值域
/// 0..32767（分母取 max(1) 防零除），btn 只表达 left 位。行内续行转义
/// 使各事件无缝拼接，报文自带一个换行（写侧 writeln 再补一个——QMP 的
/// 逐行 JSON 解析忽略空行，与既有浏览器路线上字节一致）。
fn pointer_msg(x: u16, y: u16, buttons: u8, gw: u32, gh: u32) -> String {
    let sx = (u64::from(x) * 32767 / u64::from(gw.max(1))).min(32767);
    let sy = (u64::from(y) * 32767 / u64::from(gh.max(1))).min(32767);
    let down = buttons & 1 != 0;
    format!(
        "{{\"execute\":\"input-send-event\",\"arguments\":{{\"events\":[\
         {{\"type\":\"abs\",\"data\":{{\"axis\":\"x\",\"value\":{sx}}}}},\
         {{\"type\":\"abs\",\"data\":{{\"axis\":\"y\",\"value\":{sy}}}}},\
         {{\"type\":\"btn\",\"data\":{{\"down\":{down},\"button\":\"left\"}}}}]}}}}\n"
    )
}

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
    /// 事件无缝拼接与自带换行——与 LAN 拦截器旧实现逐字节同式。
    #[test]
    fn injector_pointer_msg_shape() {
        let m = pointer_msg(480, 500, 1, 960, 1000);
        assert_eq!(
            m,
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[\
             {\"type\":\"abs\",\"data\":{\"axis\":\"x\",\"value\":16383}},\
             {\"type\":\"abs\",\"data\":{\"axis\":\"y\",\"value\":16383}},\
             {\"type\":\"btn\",\"data\":{\"down\":true,\"button\":\"left\"}}]}}\n"
        );
    }

    /// 边沿：满幅/零轴钳位与松键 up 沿（x=gw 恰 32767，y=0 恰 0）。
    #[test]
    fn injector_pointer_msg_edges() {
        let m = pointer_msg(1000, 0, 0, 1000, 500);
        assert!(
            m.contains("\"axis\":\"x\",\"value\":32767")
                && m.contains("\"axis\":\"y\",\"value\":0")
                && m.contains("\"down\":false"),
            "{m}"
        );
    }

    /// 注入器全链路（假 unix QMP 服务端，零 QEMU 秒级自测）：greeting →
    /// qmp_capabilities → pointer 报文落线——迁出共用后握手与报文面的
    /// 网络级锚。服务器读完报文即关——注入器写侧失联自哑化（后续
    /// pointer=false）一并钉住。
    #[test]
    fn injector_handshake_and_pointer_roundtrip() {
        use std::io::{BufRead, BufReader, Read, Write};
        let path = std::env::temp_dir().join(format!(
            "berry-qmp-inj-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let p2 = path.clone();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(br#"{"QMP":{"version":{"qemu":{"micro":3}},"capabilities":[]}}"#)
                .unwrap();
            s.write_all(b"\n").unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"), "{line}");
            s.write_all(br#"{"return":{},"id":1}"#).unwrap();
            s.write_all(b"\n").unwrap();
            // 读 pointer 报文并钉形状，然后关停（此时客户端首写必已
            // 入队——服务器还没关，写不可能先于本读返回失败）。
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("input-send-event"), "{line}");
            assert!(line.contains("\"axis\":\"x\",\"value\":16383"), "{line}");
            assert!(line.contains("\"down\":true"), "{line}");
            let _ = s.shutdown(std::net::Shutdown::Both);
            let mut sink = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink.extend_from_slice(&buf[..n]),
                }
            }
        });
        let mut inj = QmpInjector::connect_guarded(&p2, Some(Duration::from_millis(50)))
            .expect("handshake must negotiate");
        // 服务器此刻阻塞在读上：首写必成功入队且被读到（守护超时只在
        // 写会阻塞时起效，此写即时完成）。
        assert!(inj.pointer(480, 500, 1, 960, 1000), "QMP 路存活");
        // 服务器已关：写失败 → 哑化 → 永久 false。写超时守护防缓冲满
        // 阻塞死等（循环量 8× 于缓冲容量，EPIPE/超时必先到——收敛有界）。
        for _ in 0..512 {
            if !inj.pointer(1, 1, 0, 960, 1000) {
                break;
            }
        }
        assert!(!inj.pointer(1, 1, 0, 960, 1000), "失联后永久哑化");
        drop(inj);
        server.join().unwrap();
        let _ = std::fs::remove_file(&p2);
    }

    /// Network-level pump round-trip against a scripted fake QMP server on
    /// an ephemeral loopback TCP listener: greeting -> qmp_capabilities
    /// negotiation -> async VNC event dispatched to the sink -> id-routed
    /// execute reply. Zero QEMU; this is the only automated net over the
    /// reader-thread routing (a regression there silently drops audit
    /// events).
    #[test]
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
