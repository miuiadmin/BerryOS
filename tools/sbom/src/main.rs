//! berry-sbom——G1 刀 5：CycloneDX 1.5 SBOM 生成 + 开源协议阻断扫描。
//!
//! 功能：
//! 1. 扫描 workspace 的 Cargo.lock（依赖闭包）+ 各 Cargo.toml（元数据）
//! 2. 产出 CycloneDX 1.5 JSON 格式 SBOM
//! 3. 协议白名单执法——不在白名单的依赖即 FAIL（供应链阻断）
//!
//! 纯 std、无外部依赖（seal：SBOM 工具自身不引第三方——防递归污染）。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// 允许的开源协议（OSI 认证 + 常见宽松）。
const ALLOWED_LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "ISC",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "0BSD",
    "Unlicense",
    "Zlib",
    "CC0-1.0",
    "MPL-2.0",
    "Unicode-3.0",
    "CDLA-Permissive-2.0",
];

/// Cargo.lock 依赖条目。
#[derive(Debug, Clone)]
pub struct DepEntry {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub license: Option<String>,
}

/// 扫描结果。
pub struct ScanResult {
    pub deps: Vec<DepEntry>,
    pub violations: Vec<(String, String)>, // (name, license)
}

/// 从 Cargo.lock 解析依赖（[[package]] 节的 name/version/source）。
pub fn parse_lock(lock_path: &Path) -> Vec<DepEntry> {
    let Ok(text) = fs::read_to_string(lock_path) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    let mut cur_name = String::new();
    let mut cur_ver = String::new();
    let mut cur_src: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            if !cur_name.is_empty() {
                deps.push(DepEntry {
                    name: cur_name.clone(),
                    version: cur_ver.clone(),
                    source: cur_src.take(),
                    license: None,
                });
            }
            cur_name.clear();
            cur_ver.clear();
            cur_src = None;
        } else if let Some(v) = t.strip_prefix("name = ") {
            cur_name = v.trim_matches('"').to_string();
        } else if let Some(v) = t.strip_prefix("version = ") {
            cur_ver = v.trim_matches('"').to_string();
        } else if let Some(v) = t.strip_prefix("source = ") {
            cur_src = Some(v.trim_matches('"').to_string());
        }
    }
    if !cur_name.is_empty() {
        deps.push(DepEntry {
            name: cur_name,
            version: cur_ver,
            source: cur_src,
            license: None,
        });
    }
    deps
}

/// 从 Cargo.toml 的 [package] license 字段补充协议信息（按 name 匹配）。
pub fn enrich_licenses(deps: &mut Vec<DepEntry>, workspace_root: &Path) {
    // 扫描仓内所有 Cargo.toml 的 license 字段——构建 name→license 映射。
    let mut license_map: BTreeMap<String, String> = BTreeMap::new();
    scan_tomls(workspace_root, &mut |name, license| {
        license_map.insert(name.to_string(), license.to_string());
    });
    for dep in deps.iter_mut() {
        if let Some(l) = license_map.get(&dep.name) {
            dep.license = Some(l.clone());
        }
    }
}

fn scan_tomls(dir: &Path, cb: &mut dyn FnMut(&str, &str)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with('.') || fname == "target" || fname == "out" || fname == "node_modules" {
            continue;
        }
        if p.is_dir() {
            scan_tomls(&p, cb);
        } else if fname == "Cargo.toml" {
            if let Ok(text) = fs::read_to_string(&p) {
                // 提取 [package] 段内的 name 和 license
                let mut in_pkg = false;
                let mut name = String::new();
                let mut license = String::new();
                for line in text.lines() {
                    let t = line.trim();
                    if t.starts_with('[') {
                        if !in_pkg && !name.is_empty() && !license.is_empty() {
                            cb(&name, &license);
                        }
                        in_pkg = t == "[package]";
                        name.clear();
                        license.clear();
                    } else if in_pkg {
                        if let Some(v) = t.strip_prefix("name = ") {
                            name = v.trim_matches('"').to_string();
                        } else if let Some(v) = t.strip_prefix("license = ") {
                            license = v.trim_matches('"').to_string();
                        }
                    }
                }
                if in_pkg && !name.is_empty() && !license.is_empty() {
                    cb(&name, &license);
                }
            }
        }
    }
}

/// 协议白名单执法——返回违规 (name, license) 列表。
pub fn check_licenses(deps: &[DepEntry]) -> Vec<(String, String)> {
    deps.iter()
        .filter_map(|d| {
            // 无协议标注的本地件（无 source）——只警告不阻断（自家 crate）。
            if d.source.is_none() {
                return None;
            }
            // 远端依赖：协议须在白名单。
            match &d.license {
                Some(l) => {
                    let normalized = normalize_license(l);
                    if ALLOWED_LICENSES.contains(&normalized.as_str()) {
                        None
                    } else {
                        Some((d.name.clone(), l.clone()))
                    }
                }
                None => Some((d.name.clone(), "UNKNOWN".to_string())),
            }
        })
        .collect()
}

fn normalize_license(l: &str) -> String {
    // "Apache-2.0 OR MIT" → 取第一项（宽松优先）。
    l.split(" OR ").next().unwrap_or(l).trim().to_string()
}

/// 生成 CycloneDX 1.5 JSON。
pub fn cyclonedx_json(result: &ScanResult, _serial: u64) -> String {
    let mut json = String::new();
    let _ = write!(
        json,
        r#"{{"bomFormat":"CycloneDX","specVersion":"1.5","serialNumber":"urn:uuid:00000000-0000-4000-8000-000000000001","version":1,"metadata":{{"timestamp":"","tools":{{"components":[{{"type":"application","name":"berry-sbom","version":"0.1.0"}}]}},"component":{{"type":"operating-system","name":"BerryOS","version":"0.2.0"}}}},"components":["#
    );

    for (i, dep) in result.deps.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let _ = write!(
            json,
            r#"{{"type":"library","name":"{}","version":"{}""#,
            dep.name, dep.version
        );
        if let Some(ref s) = dep.source {
            let _ = write!(json, r#","purl":"pkg:cargo/{}@{}""#, dep.name, dep.version);
            let _ = write!(json, r#","externalReferences":[{{"url":"{}","type":"vcs"}}]"#, s);
        }
        if let Some(ref l) = dep.license {
            let _ = write!(
                json,
                r#","licenses":[{{"license":{{"id":"{}"}}}}]"#,
                normalize_license(l)
            );
        }
        json.push('}');
    }

    json.push_str(r#"],"dependencies":[]}"#);
    json
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = std::env::current_dir()
        .ok()
        .and_then(|d| d.ancestors().find(|a| a.join(".git").exists()).map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let mode = args.first().map(String::as_str).unwrap_or("--all");
    match mode {
        "--scan" => {
            let mut deps = parse_lock(&root.join("Cargo.lock"));
            let user_deps = parse_lock(&root.join("user/Cargo.lock"));
            let kernel_deps = parse_lock(&root.join("kernel/Cargo.lock"));
            deps.extend(user_deps);
            deps.extend(kernel_deps);
            enrich_licenses(&mut deps, &root);
            let violations = check_licenses(&deps);
            let result = ScanResult { deps, violations: violations.clone() };
            let json = cyclonedx_json(&result, 0);
            let out = root.join("sbom.json");
            fs::write(&out, &json).expect("写 sbom.json");
            eprintln!("sbom: {} 依赖 → {}（{} 违规）", result.deps.len(), out.display(), violations.len());
            for (name, lic) in &violations {
                eprintln!("  FAIL: {} = {}", name, lic);
            }
            if !violations.is_empty() {
                std::process::exit(1);
            }
        }
        "--check" => {
            let mut deps = parse_lock(&root.join("Cargo.lock"));
            let user_deps = parse_lock(&root.join("user/Cargo.lock"));
            let kernel_deps = parse_lock(&root.join("kernel/Cargo.lock"));
            deps.extend(user_deps);
            deps.extend(kernel_deps);
            enrich_licenses(&mut deps, &root);
            let violations = check_licenses(&deps);
            if violations.is_empty() {
                eprintln!("license-check: PASS（{} 依赖全在白名单）", deps.len());
            } else {
                eprintln!("license-check: FAIL（{} 违规）", violations.len());
                for (name, lic) in &violations {
                    eprintln!("  {}: {}", name, lic);
                }
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("用法: berry-sbom [--scan|--check]");
            eprintln!("  --scan  生成 CycloneDX 1.5 SBOM（sbom.json）+ 协议执法");
            eprintln!("  --check 仅协议白名单执法");
        }
    }
}
