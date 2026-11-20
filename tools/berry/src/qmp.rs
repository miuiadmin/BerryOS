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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
/// 才连，失联退 RFB msg5 保底）。失联（写失败）哑化后 ≥5s 节流重连
/// （RETRY_AFTER）——QMP 楔死期每拍输入至多挂一次 guard 级尝试，QMP
/// 恢复后注入自动复活。无排水面（S4 定谳撤除：try_clone 克隆共享 OFD，
/// 排水侧 set_nonblocking 会连带把阻塞写路径置非阻塞，~35 事件即哑化——
/// 排水污染实证）：正确性依据（对抗审查活体饱和探针实证，QEMU 11.0.3
/// monitor.c）：QMP 监视器把未写出的响应暂存**无界用户态 outbuf**
/// （monitor_flush_locked）且**永不因输出积压停读输入**（suspend 仅由
/// 请求队列准入触发），故本端不读响应不阻塞写路径（百万事件零超时实
/// 测）；残余代价 = QEMU 侧 RSS 按未冲刷响应量持续爬坡、断连即释放
/// （接受面，如实记档）。
pub struct QmpInjector {
    sock: Option<std::os::unix::net::UnixStream>,
    /// 重连参数：socket 路径与守护超时（哑化自愈试连复用）。
    path: PathBuf,
    guard: Duration,
    /// 下次允许试连的时刻；None = 无节流（建连在世或从未试连）。
    retry_at: Option<Instant>,
}

/// 哑化后重连节流窗：距上次失败 ≥5s 才再试一次。QMP 楔死期每拍输入
/// 一次 2s 级守护尝试过重（连打字都卡拍），5s 窗把尝试频率压到恢复
/// 探测级——QMP 恢复后至多 5s 输入复活。
const RETRY_AFTER: Duration = Duration::from_secs(5);

/// 连接生命周期取证（全局节流 5s 一行）：connect_guarded 各失败阶段与
/// 哑化/重连成败各打一行——楔死从静默变可见（2026-11-20 双形态实锤：
/// 起点握手失败整会话哑管道 + 会话中哑化仅事件驱动重连，此前全静默）。
fn life_note(stage: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static LAST_MS: AtomicU64 = AtomicU64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < 5_000 {
        return;
    }
    LAST_MS.store(now_ms, Ordering::Relaxed);
    eprintln!("berry: qmp-inject {stage}");
}

impl QmpInjector {
    /// 连接并协商 capabilities；对 greeting/回读与后续写全程设 `guard`
    /// 超时（持有方在 UI/代理线程连写——QEMU 卡死守护：超时即 None/
    /// 失联，调用方走降级面，线程不冻结）。任一步失败返回 None（调用
    /// 方自选降级面）。成功者自带哑化后节流重连自愈（见结构体注）。
    pub fn connect_guarded(path: &Path, guard: Duration) -> Option<Self> {
        use std::io::{BufRead, BufReader, Write};
        let mut s = match std::os::unix::net::UnixStream::connect(path) {
            Ok(s) => s,
            Err(_) => {
                life_note("connect 拒绝（监听窗竞速/路径缺席）");
                return None;
            }
        };
        s.set_read_timeout(Some(guard)).ok()?;
        s.set_write_timeout(Some(guard)).ok()?;
        let mut r = BufReader::new(s.try_clone().ok()?);
        let mut line = String::new();
        if r.read_line(&mut line).is_err() {
            life_note("greeting 超时（QEMU 主循环饱和?）");
            return None;
        }
        writeln!(s, "{{\"execute\":\"qmp_capabilities\"}}").ok()?;
        s.flush().ok()?;
        line.clear();
        r.read_line(&mut line).ok()?;
        if !line.contains("return") {
            life_note("caps 回读异常");
            return None;
        }
        Some(QmpInjector {
            sock: Some(s),
            path: path.to_path_buf(),
            guard,
            retry_at: None,
        })
    }

    /// 惰性形态（RamSource 原生路）：不建连，首个 pointer 事件才试连
    /// （守护超时语义同 connect_guarded）；失败进入 ≥5s 节流重试，不占
    /// UI 线程。
    pub fn lazy(path: &Path, guard: Duration) -> Self {
        QmpInjector {
            sock: None,
            path: path.to_path_buf(),
            guard,
            retry_at: None,
        }
    }

    /// 发一枚 PointerEvent（报文见 `pointer_msg`，与 LAN 拦截器旧实现
    /// 逐字节同式）。返回 false = 注入路哑化且未到重连窗（调用方走保
    /// 底）；写失败即置 sock=None 哑化并挂 ≥5s 重连窗，逾窗后下一次事
    /// 件试连一次——成功复活、失败续窗等下窗。
    pub fn pointer(&mut self, x: u16, y: u16, buttons: u8, gw: u32, gh: u32) -> bool {
        use std::io::{Read, Write};
        if self.sock.is_none() {
            if let Some(t) = self.retry_at {
                if Instant::now() < t {
                    return false;
                }
            }
            match Self::connect_guarded(&self.path, self.guard) {
                Some(fresh) => {
                    life_note("重连成功复活");
                    *self = fresh
                }
                None => {
                    life_note("重连失败（续 5s 窗）");
                    self.retry_at = Some(Instant::now() + RETRY_AFTER);
                    return false;
                }
            }
        }
        let Some(s) = self.sock.as_mut() else { return false };
        // 应答排水（根因修复 2026-11-19：QMP 每命令回一条 ~30B {"return":{}},
        // 不读则 macOS 内核发送缓冲 ~8KB 在 ~35 条后满 → write EAGAIN → 哑化
        // → 全部后续点击静默丢弃。用**超时读**清缓冲（try_clone 会共享 OFD
        // 把非阻塞泄漏到写路径——K3 撤排水的根因；本形直接在主柄上短超时
        // 读，读后恢复原超时，零 OFD 污染）。
        let orig_timeout = s.read_timeout().ok().flatten();
        s.set_read_timeout(Some(Duration::from_millis(1))).ok();
        let mut drain_buf = [0u8; 4096];
        loop {
            match s.read(&mut drain_buf) {
                Ok(0) | Err(_) => break, // EOF/超时/错码——排空或无数据
                Ok(_) => continue,       // 继续排
            }
        }
        if let Some(t) = orig_timeout {
            s.set_read_timeout(Some(t)).ok();
        }
        let msg = pointer_msg(x, y, buttons, gw, gh);
        if writeln!(s, "{msg}").is_err() || s.flush().is_err() {
            life_note("写失败哑化（挂 5s 重连窗）");
            self.sock = None;
            self.retry_at = Some(Instant::now() + RETRY_AFTER);
            return false;
        }
        true
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
    /// 网络级锚。服务器读完报文即关——注入器写侧失联自哑化（哑化后
    /// ≥5s 节流窗内后续 pointer=false 不试连；逾窗重连自愈另见
    /// injector_reconnect_throttle_and_revival）。
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
        let mut inj = QmpInjector::connect_guarded(&p2, Duration::from_millis(50))
            .expect("handshake must negotiate");
        // 服务器此刻阻塞在读上：首写必成功入队且被读到（守护超时只在
        // 写会阻塞时起效，此写即时完成）。
        assert!(inj.pointer(480, 500, 1, 960, 1000), "QMP 路存活");
        // 服务器已关：写失败 → 哑化挂 5s 窗 → 窗内 pointer=false。写超
        // 时守护防缓冲满阻塞死等（循环量 8× 于缓冲容量，EPIPE/超时必先
        // 到——收敛有界）。
        for _ in 0..512 {
            if !inj.pointer(1, 1, 0, 960, 1000) {
                break;
            }
        }
        assert!(!inj.pointer(1, 1, 0, 960, 1000), "失联哑化（节流窗内）");
        drop(inj);
        server.join().unwrap();
        let _ = std::fs::remove_file(&p2);
    }

    /// 重连节流钉板（K3 berry 韧性）：哑化后节流窗内事件不重试——新服
    /// 务端已在同路径健康在听，若试连必成功，pointer=false 即未试；
    /// 逾窗（retry_at 回拨模拟 ≥5s 流逝，窗值由 RETRY_AFTER 恒等断言
    /// 钉死）重试一次成功复活——「QMP 恢复后输入自动复活」的网络级锚。
    #[test]
    fn injector_reconnect_throttle_and_revival() {
        use std::io::{BufRead, BufReader, Read, Write};
        assert_eq!(RETRY_AFTER, Duration::from_secs(5), "节流窗=5s 钉板");
        let path = std::env::temp_dir().join(format!(
            "berry-qmp-throttle-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let guard = Duration::from_millis(500);
        // 服务端 A：握一次手、读一拍后关停——制造客户端写路径失联哑化。
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let pa = path.clone();
        let server_a = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(br#"{"QMP":{"version":{"qemu":{"micro":3}}},"capabilities":[]}}"#)
                .unwrap();
            s.write_all(b"\n").unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            s.write_all(br#"{"return":{},"id":1}"#).unwrap();
            s.write_all(b"\n").unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
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
        let mut inj = QmpInjector::connect_guarded(&pa, guard).expect("handshake");
        assert!(inj.pointer(1, 1, 0, 100, 100), "在世首拍");
        for _ in 0..512 {
            if !inj.pointer(1, 1, 0, 100, 100) {
                break;
            }
        }
        assert!(!inj.pointer(1, 1, 0, 100, 100), "失联哑化");
        server_a.join().unwrap();
        // 服务端 B：同路径新 QMP，健康在听——节流窗内的拍不得试连
        // （A 的 listener 已随线程结束关闭，但 socket 文件残留，须先清
        // 才能 bind——与既有 roundtrip 测试的清理先例同形）。
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server_b = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(br#"{"QMP":{"version":{"qemu":{"micro":3}}},"capabilities":[]}}"#)
                .unwrap();
            s.write_all(b"\n").unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"), "{line}");
            s.write_all(br#"{"return":{},"id":1}"#).unwrap();
            s.write_all(b"\n").unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("input-send-event"), "{line}");
            let _ = s.shutdown(std::net::Shutdown::Both);
        });
        assert!(
            !inj.pointer(1, 1, 0, 100, 100),
            "节流窗内不重试（B 在听，试连即 true）"
        );
        // 逾窗：回拨 retry_at 模拟 ≥5s 流逝。
        inj.retry_at = Some(Instant::now() - RETRY_AFTER);
        assert!(inj.pointer(1, 1, 0, 100, 100), "逾窗重连复活");
        drop(inj);
        server_b.join().unwrap();
        let _ = std::fs::remove_file(&path);
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
