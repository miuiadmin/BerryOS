//! Per-run directories + a tiny serial tail (marker watcher).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{BerryError, Result};

pub fn home() -> PathBuf {
    if let Ok(p) = std::env::var("BERRY_HOME") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        if let Ok(p) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(p).join("BerryOS");
        }
    }
    if let Ok(p) = std::env::var("HOME") {
        return PathBuf::from(p).join(".berry");
    }
    PathBuf::from(".berry")
}

fn stamp() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{:09}", d.as_secs(), d.subsec_nanos())
}

pub struct RunDir {
    path: PathBuf,
    keep: bool,
}

impl RunDir {
    pub fn create(label: &str, keep: bool) -> Result<Self> {
        let root = home().join("run");
        fs::create_dir_all(&root)?;
        let id = format!("{}-{}-{}", stamp(), std::process::id(), label);
        let path = root.join(&id);
        fs::create_dir_all(&path)?;
        let meta = serde_json::json!({
            "v": 1,
            "label": label,
            "pid": std::process::id(),
            "started": stamp(),
        });
        fs::write(path.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;
        // M2 第 10 步：run 历史索引——逐 run 追加单行 JSON（O_APPEND 单行
        // 写，多进程并发安全）；字段不复制 meta.json，只引用 run 目录。
        append_index(&serde_json::json!({
            "v": 1, "t": "run.start", "run": id, "label": label,
        }));
        Ok(Self { path, keep })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn keep(&mut self) {
        self.keep = true;
    }

    /// M1 审计补刀：失败现场保留 + stderr 打印路径（无上限堆积的出口
    /// 提示——失败目录留了、在哪、知道能手动清；成功路径与显式 --keep
    /// 走 keep() 静默）。
    pub fn keep_fail(&mut self) {
        self.keep = true;
        eprintln!("berry: failure run kept: {}", self.path.display());
    }
}

impl Drop for RunDir {
    fn drop(&mut self) {
        // end 行先记后删（宿主崩溃时 end 必漏——索引容忍不全，枚举以扫
        // 目录兜底，见 index.jsonl 头注）。
        append_index(&serde_json::json!({
            "v": 1, "t": "run.end",
            "run": self.path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default(),
            "kept": self.keep,
        }));
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// M2 第 10 步：~/.berry/run/index.jsonl 历史索引（O_APPEND + 单次
/// write_all 整行——O_APPEND 只保证单次 write(2) 原子，writeln! 对未缓冲
/// File 是多次 write，两进程并发时行会字符级交错〔审计实抓〕；宿主崩溃
/// 必漏 end 行——消费方枚举以扫目录兜底；索引当前纯留痕（零读方，清
/// 理/历史查询消费留待后续刀），不是权威清单）。
fn append_index(line: &serde_json::Value) {
    let path = home().join("run").join("index.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        // 先拼完整行（含换行）再一次 write_all——常规本地文件上单次
        // write 即完成，O_APPEND 保证各写不覆盖；writeln! 的必然多段
        // write（并发交错面）已消除。
        let _ = f.write_all(format!("{line}\n").as_bytes());
    }
}

/// Update the session state file (C1d; survives host crash for recovery).
pub fn write_state(run: &Path, state: &str) {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let v = serde_json::json!({
        "v": 1,
        "state": state,
        "pid": std::process::id(),
        "updated": d.as_millis() as u64,
    });
    let _ = fs::write(run.join("state.json"), serde_json::to_vec_pretty(&v).unwrap_or_default());
}

/// Read the last persisted state (None = no state file).
pub fn read_state(run: &Path) -> Option<String> {
    let b = fs::read(run.join("state.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&b).ok()?;
    v.get("state").and_then(|s| s.as_str()).map(str::to_string)
}

pub struct Tail {
    path: PathBuf,
    file: Option<File>,
    offset: u64,
    all: Vec<u8>,
}

impl Tail {
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: None,
            offset: 0,
            all: Vec::new(),
        }
    }

    pub fn read_new(&mut self) -> Vec<u8> {
        if self.file.is_none() {
            self.file = File::open(&self.path).ok();
            if self.file.is_some() {
                self.offset = 0;
            }
        }
        let Some(f) = self.file.as_mut() else {
            return Vec::new();
        };
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset += buf.len() as u64;
        if !buf.is_empty() {
            self.all.extend_from_slice(&buf);
            if self.all.len() > 4 << 20 {
                let cut = self.all.len() - (2 << 20);
                self.all.drain(..cut);
            }
        }
        buf
    }

    pub fn contains(&self, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        String::from_utf8_lossy(&self.all).contains(needle)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.all).to_string()
    }

    pub fn tail_text(&self, max: usize) -> String {
        let s = self.text();
        if s.len() <= max {
            s
        } else {
            s[s.len() - max..].to_string()
        }
    }
}

pub fn wait_for_file(path: &Path, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(BerryError::timeout(format!(
        "timed out waiting for {}",
        path.display()
    )))
}

// ---- 四轮审计（D4）：RunDir/index 生命周期护栏——write_all 整行格式与
// ---- create/drop 两行契约此前零护栏（writeln! 并发交错是三轮实抓回归面）。
// ---- env（BERRY_HOME）是进程级全局：两测试经 ENV_LOCK 串行防竞争。
#[cfg(test)]
mod run_dir_tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn index_lifecycle_two_lines() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("berry-run-test-{}", std::process::id()));
        std::env::set_var("BERRY_HOME", &tmp);
        std::fs::remove_dir_all(&tmp).ok();
        {
            let mut run = RunDir::create("t", false).unwrap();
            run.keep();
        } // Drop：先记 run.end（kept=true）再按 keep 免删
        let idx = std::fs::read_to_string(tmp.join("run").join("index.jsonl")).unwrap();
        let lines: Vec<&str> = idx.lines().collect();
        assert_eq!(lines.len(), 2, "index 两行契约: {idx}");
        assert!(lines[0].contains("\"t\":\"run.start\""));
        assert!(lines[0].contains("\"label\":\"t\""));
        assert!(lines[1].contains("\"t\":\"run.end\""));
        assert!(lines[1].contains("\"kept\":true"));
        assert!(idx.ends_with('\n'), "单行整写含尾换行");
        std::env::remove_var("BERRY_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn drop_without_keep_removes_dir() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("berry-run-test2-{}", std::process::id()));
        std::env::set_var("BERRY_HOME", &tmp);
        std::fs::remove_dir_all(&tmp).ok();
        let path;
        {
            let run = RunDir::create("t2", false).unwrap();
            path = run.path().to_path_buf();
        }
        assert!(!path.exists(), "非 keep 的 Drop 删目录");
        std::env::remove_var("BERRY_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
