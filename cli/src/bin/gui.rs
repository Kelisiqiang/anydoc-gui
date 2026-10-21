// AnyDoc GUI 宿主 —— 静态资源服务 + 文件写入 API
//
// 架构：
//   浏览器端跑 WASM 内核做转换（不卡界面、进度真实）
//   写盘走本机的 /api/write（浏览器无法直接写任意路径）
//
// 之所以不用 Tauri / Electron：anydoc 官方 WASM 内核有 6.7MB，
// 塞不进单 exe；而 Tauri 交叉编译 Windows 需要完整 MSVC 工具链。
// 「本地服务 + 系统浏览器」是体积最小、跨平台最好的方案。

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ─────────── 主入口 ───────────
fn main() {
    let dir = resource_dir();
    if !dir.join("index.html").exists() {
        eprintln!("❌ 找不到 GUI 资源目录: {}", dir.display());
        eprintln!("   请确保 gui/ 目录与 anydoc-gui.exe 在同一文件夹。");
        std::process::exit(1);
    }

    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(17811);

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("❌ 端口 {port} 绑定失败: {e}");
            eprintln!("   换个端口试试: anydoc-gui.exe 17812");
            std::process::exit(1);
        }
    };

    let url = format!("http://127.0.0.1:{}/", port);
    println!("🚀 AnyDoc GUI");
    println!("   资源  {}/", dir.display());
    println!("   地址  {}", url);
    println!("   关闭此窗口即退出。\n");

    open_browser(&url);

    let root = Arc::new(dir);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let r = root.clone();
                // 每连接一线程：WASM 有 6.7MB，并发加载时不能串行阻塞
                std::thread::spawn(move || {
                    if let Err(e) = handle(s, &r) {
                        eprintln!("请求处理失败: {e}");
                    }
                });
            }
            Err(e) => eprintln!("连接错误: {e}"),
        }
    }
}

// ─────────── 浏览器打开 ───────────
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

// ─────────── 资源目录 ───────────
fn resource_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("gui");
            if cand.exists() {
                return cand;
            }
            if dir.join("index.html").exists() {
                return dir.to_path_buf();
            }
        }
    }
    PathBuf::from("gui")
}

// ─────────── 请求处理 ───────────
fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_nodelay(true).ok();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    // 解析请求行: METHOD PATH HTTP/1.1
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();

    // 读取 headers
    let mut content_length = 0usize;
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h)?;
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    match (method.as_str(), path.as_str()) {
        ("GET", "/api/health") => {
            let body = format!(
                r#"{{"ok":true,"version":"{}","kernel":"anydoc"}}"#,
                env!("CARGO_PKG_VERSION")
            );
            respond_json(&mut stream, 200, &body)?;
        }
        ("POST", "/api/pickdir") => {
            let picked = pick_directory();
            let body = match picked {
                Some(p) => format!(r#"{{"path":{}}}"#, json_str(&p)),
                None => r#"{"path":""}"#.to_string(),
            };
            respond_json(&mut stream, 200, &body)?;
        }
        ("POST", "/api/write") => {
            let mut buf = vec![0u8; content_length];
            if content_length > 0 {
                reader.read_exact(&mut buf)?;
            }
            let body = String::from_utf8_lossy(&buf).to_string();
            match write_file(&body) {
                Ok(p) => {
                    let b = format!(r#"{{"ok":true,"path":{}}}"#, json_str(&p));
                    respond_json(&mut stream, 200, &b)?;
                }
                Err(e) => {
                    let b = format!(r#"{{"ok":false,"error":{}}}"#, json_str(&e));
                    respond_json(&mut stream, 500, &b)?;
                }
            }
        }
        ("GET", _) => {
            serve_static(&mut stream, root, &path)?;
        }
        _ => {
            respond_text(&mut stream, 405, "Method Not Allowed")?;
        }
    }

    Ok(())
}

// ─────────── 静态资源 ───────────
fn serve_static(stream: &mut TcpStream, root: &Path, path: &str) -> std::io::Result<()> {
    let rel = if path == "/" { "index.html" } else { &path[1..] };
    let file = root.join(rel);

    // 防目录穿越
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let safe = match file.canonicalize() {
        Ok(c) if c.starts_with(&root_canon) => c,
        _ => root_canon.join("index.html"),
    };

    match fs::read(&safe) {
        Ok(data) => {
            let mime = match safe.extension().and_then(|s| s.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
                Some("wasm") => "application/wasm",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                mime,
                data.len()
            );
            stream.write_all(header.as_bytes())?;
            stream.write_all(&data)?;
        }
        Err(_) => {
            let body = b"404 Not Found";
            let header = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes())?;
            stream.write_all(body)?;
        }
    }
    stream.flush()
}

// ─────────── 目录选择 ───────────
fn pick_directory() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // 用 PowerShell 弹窗选择目录
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms;
$f = New-Object System.Windows.Forms.FolderBrowserDialog;
$f.Description = '选择输出目录';
if ($f.ShowDialog() -eq 'OK') { Write-Output $f.SelectedPath }
"#;
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows：退回到当前目录
        std::env::current_dir().ok().map(|p| p.display().to_string())
    }
}

// ─────────── 写文件 ───────────
fn write_file(body: &str) -> Result<String, String> {
    // 极简 JSON 解析：只取三个字符串字段
    let dir = extract_json_str(body, "dir").ok_or("缺少 dir")?;
    let path = extract_json_str(body, "path").ok_or("缺少 path")?;
    let content = extract_json_str(body, "content").ok_or("缺少 content")?;

    let base = Path::new(&dir);
    if !base.exists() {
        fs::create_dir_all(base).map_err(|e| format!("创建目录失败: {e}"))?;
    }

    // 防路径穿越：先做纯字符串校验，确保路径未越出 base
    for part in Path::new(&path).components() {
        match part {
            std::path::Component::ParentDir => return Err("非法路径".into()),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("非法路径".into())
            }
            _ => {}
        }
    }

    // 防路径穿越：最终路径必须在 base 之内
    let base_canon = base.canonicalize().map_err(|e| format!("{e}"))?;
    let target = base.join(&path);

    // 父目录可能还不存在（选目录时常带子目录结构），先创建再规范化
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let parent_canon = target
        .parent()
        .ok_or("路径无效")?
        .canonicalize()
        .map_err(|e| format!("{e}"))?;
    if !parent_canon.starts_with(&base_canon) {
        return Err("非法路径".into());
    }
    let file_name = target.file_name().ok_or("路径无效")?;
    let target_canon = parent_canon.join(file_name);

    if let Some(parent) = target_canon.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }

    fs::write(&target_canon, content.as_bytes()).map_err(|e| format!("写入失败: {e}"))?;
    Ok(target_canon.display().to_string())
}

// ─────────── JSON 辅助 ───────────
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 从 JSON 字符串中提取 "key":"value"（值里的转义按 \n \t \" \\ 处理）
fn extract_json_str(src: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let start = src.find(&pat)? + pat.len();
    let rest = &src[start..];
    let rest = rest.trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some(out),
            b'\\' if i + 1 < bytes.len() => {
                i += 1;
                match bytes[i] {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'u' => {
                        if i + 4 < bytes.len() {
                            if let Ok(c) = u32::from_str_radix(&rest[i + 1..i + 5], 16) {
                                if let Some(ch) = char::from_u32(c) {
                                    out.push(ch);
                                }
                            }
                            i += 4;
                        }
                    }
                    c => out.push(c as char),
                }
                i += 1;
            }
            _ => {
                // 复制完整 UTF-8 字符
                let ch = rest[i..].chars().next()?;
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    None
}

// ─────────── 响应 ───────────
fn respond_json(stream: &mut TcpStream, code: u16, body: &str) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        code,
        reason,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn respond_text(stream: &mut TcpStream, code: u16, body: &str) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        code,
        reason,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}
