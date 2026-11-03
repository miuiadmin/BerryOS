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
