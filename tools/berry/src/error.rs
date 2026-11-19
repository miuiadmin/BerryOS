//! Typed errors with stable exit codes (design doc §附录 E).

use std::fmt;

pub type Result<T> = std::result::Result<T, BerryError>;

#[derive(Debug)]
pub struct BerryError {
    pub code: i32,
    pub message: String,
    pub hint: Option<String>,
}

impl BerryError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), hint: None }
    }

    pub fn env(message: impl Into<String>) -> Self {
        Self::new(3, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(4, message)
    }

    pub fn marker(message: impl Into<String>) -> Self {
        Self::new(5, message)
    }

    pub fn device(message: impl Into<String>) -> Self {
        Self::new(6, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(7, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(9, message)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn print(&self, json: bool) {
        if json {
            let v = serde_json::json!({
                "v": 1,
                "t": "error",
                "code": self.code,
                "message": self.message,
                "hint": self.hint,
            });
            println!("{v}");
        } else {
            eprintln!("berry: {}", self.message);
            if let Some(hint) = &self.hint {
                eprintln!("hint: {hint}");
            }
        }
    }
}

impl fmt::Display for BerryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BerryError {}

impl From<std::io::Error> for BerryError {
    fn from(e: std::io::Error) -> Self {
        Self::device(e.to_string())
    }
}

impl From<serde_json::Error> for BerryError {
    fn from(e: serde_json::Error) -> Self {
        Self::protocol(format!("JSON: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— W1-Q25 六刀：error.rs 零测面清偿。构造器码位钉板（附录 E 稳定退出码
    // 契约——这些数字是 CLI 面与脚本消费的 ABI 面，漂移即断用户）+ hint 链 +
    // trait 实现面。print() 两个分支走 stdout/stderr，非纯面不在此测。

    #[test]
    fn constructor_code_table() {
        // 附录 E 码位表逐格钉板：每族构造器 → 稳定码 + 默认无 hint。
        assert_eq!(BerryError::new(42, "m").code, 42);
        assert_eq!(BerryError::env("e").code, 3);
        assert_eq!(BerryError::timeout("t").code, 4);
        assert_eq!(BerryError::marker("k").code, 5);
        assert_eq!(BerryError::device("d").code, 6);
        assert_eq!(BerryError::protocol("p").code, 7);
        assert_eq!(BerryError::internal("i").code, 9);
    }

    #[test]
    fn constructor_message_and_default_hint() {
        let e = BerryError::protocol("bad frame");
        assert_eq!(e.message, "bad frame");
        assert!(e.hint.is_none(), "fresh constructor must carry no hint");
    }

    #[test]
    fn with_hint_chain_keeps_code_and_message() {
        // with_hint 按值消费按值返回（builder 链形态），只动 hint 不动其余域。
        let e = BerryError::env("BERRY_* unset").with_hint("export BERRY_SDK=...");
        assert_eq!(e.code, 3);
        assert_eq!(e.message, "BERRY_* unset");
        assert_eq!(e.hint.as_deref(), Some("export BERRY_SDK=..."));
    }

    #[test]
    fn display_is_message_only() {
        // Display 面 = 纯 message（hint 走 print 的第二行，不进 Display）——
        // `format!("{e}")` 的消费方（main 的 ? 传播链）拿到的就是这个串。
        let e = BerryError::timeout("wait marker").with_hint("ignored in Display");
        assert_eq!(format!("{e}"), "wait marker");
        assert_eq!(e.to_string(), "wait marker");
    }

    #[test]
    fn std_error_trait_object_usable() {
        // std::error::Error 空实现的存在性钉板：trait 对象上可用（Box<dyn Error>
        // 互操作面——? 传播到 main 的前提），source() 无底层因因为 None。
        let e = BerryError::internal("boom");
        let dyn_e: &dyn std::error::Error = &e;
        assert_eq!(dyn_e.to_string(), "boom");
        assert!(dyn_e.source().is_none());
    }

    #[test]
    fn from_io_error_maps_to_device_code() {
        // io::Error → 设备族（码 6）：文件/管道类宿主 I/O 失败的统一归宿。
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let e: BerryError = io_err.into();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("no such file"), "message keeps the io cause text");
        assert!(e.hint.is_none());
    }

    #[test]
    fn from_serde_json_error_maps_to_protocol_code() {
        // serde_json::Error → 协议族（码 7）且带 "JSON: " 前缀——QMP/VNC 帧
        // 解析失败的归类面（与 control.rs 坏 JSON 码 7 腿同族对拍）。
        let bad: serde_json::Error = serde_json::from_str::<serde_json::Value>("{oops").unwrap_err();
        let e: BerryError = bad.into();
        assert_eq!(e.code, 7);
        assert!(e.message.starts_with("JSON: "), "protocol message carries the JSON: prefix");
    }

    #[test]
    fn question_mark_propagation_through_result_alias() {
        // Result<T> 别名的 ? 传播链真跑：io::Error 经 From 自动升格进 BerryError。
        fn inner() -> std::io::Result<u32> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe gone"))
        }
        fn outer() -> Result<u32> {
            let v = inner()?;
            Ok(v)
        }
        let e = outer().unwrap_err();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("pipe gone"));
    }
}
