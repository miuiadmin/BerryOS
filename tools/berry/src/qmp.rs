//! QMP control plane: lifecycle, reset, screendump, key/pointer injection.

use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{BerryError, Result};
use crate::net::{Endpoint, Stream};

pub struct QmpClient {
    stream: BufReader<Stream>,
    next_id: u64,
}

impl QmpClient {
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let stream = endpoint.connect().map_err(|e| {
            BerryError::protocol(format!("QMP connect {}: {e}", endpoint.display()))
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut client = Self {
            stream: BufReader::new(stream),
            next_id: 1,
        };
        let greeting = client.read_line()?;
        let v: Value = serde_json::from_str(&greeting)?;
        if v.get("QMP").is_none() {
            return Err(BerryError::protocol("QMP: bad greeting"));
        }
        client.execute("qmp_capabilities", json!({}))?;
        Ok(client)
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        self.stream.read_line(&mut line).map_err(|e| {
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                BerryError::timeout("QMP: read timeout")
            } else {
                BerryError::protocol(format!("QMP read: {e}"))
            }
        })?;
        if line.is_empty() {
            return Err(BerryError::protocol("QMP: connection closed"));
        }
        Ok(line)
    }

    pub fn execute(&mut self, cmd: &str, args: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"execute": cmd, "arguments": args, "id": id});
        {
            let w = self.stream.get_mut();
            serde_json::to_writer(&mut *w, &msg)?;
            w.write_all(b"\n")?;
            w.flush()?;
        }
        loop {
            let line = self.read_line()?;
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(&line)?;
            if let Some(err) = v.get("error") {
                let class = err.get("class").and_then(Value::as_str).unwrap_or("error");
                let desc = err.get("desc").and_then(Value::as_str).unwrap_or("");
                return Err(BerryError::protocol(format!("QMP {cmd}: {class}: {desc}")));
            }
            let same_id = v.get("id").and_then(Value::as_u64) == Some(id);
            if same_id || (v.get("return").is_some() && v.get("id").is_none()) {
                return Ok(v.get("return").cloned().unwrap_or(Value::Null));
            }
            // else: asynchronous event; keep reading
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
        let w = self.stream.get_mut();
        serde_json::to_writer(&mut *w, &msg)?;
        w.write_all(b"\n")?;
        w.flush()?;
        Ok(())
    }
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
}
