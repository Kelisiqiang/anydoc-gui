// AnyDoc GUI 启动器 —— 在系统浏览器中打开本地 UI
//
// 为什么这样设计：
//   anydoc 的解析内核有 WASM 官方构建（@firecrawl/anydoc-wasm, MIT），
//   但它有 6.7MB，无法塞进单 exe。
//   因此 GUI 采用「本地 HTTP 服务 + 系统浏览器」形态：
//   exe 启动服务、打开浏览器，转换全部在浏览器本地完成（WASM），文件不上传。
//
// 优点：无需交叉编译 GUI 框架、体积小、跨平台、零依赖
// 缺点：不是原生窗口体验

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

/// 极简 HTTP 服务器：只为提供内嵌的 GUI 静态资源
fn serve(root: &Path, port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);

        // 解析路径
        let path = req
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/")
            .to_string();

        let rel = if path == "/" { "index.html" } else { &path[1..] };
        let file = root.join(rel);

        // 防目录穿越
        let canonical = file.canonicalize().unwrap_or_else(|_| root.join("index.html"));
        let safe = if canonical.starts_with(root.canonicalize().unwrap_or_else(|_| root.to_path_buf())) {
            canonical
        } else {
            root.join("index.html")
        };

        let (mime, body) = match std::fs::read(&safe) {
            Ok(data) => {
                let m = match safe.extension().and_then(|s| s.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("css") => "text/css; charset=utf-8",
                    Some("js") => "application/javascript; charset=utf-8",
                    Some("wasm") => "application/wasm",
                    _ => "application/octet-stream",
                };
                (m, data)
            }
            Err(_) => (
                "text/plain; charset=utf-8",
                b"404 Not Found".to_vec(),
            ),
        };

        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            mime,
            body.len()
        );

        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(&body);
        let _ = stream.flush();
    }

    Ok(())
}

/// 资源目录：优先找 exe 同级的 gui/ 子目录
fn resource_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("gui");
            if cand.exists() {
                return cand;
            }
            // 也支持同级直接放置
            if dir.join("index.html").exists() {
                return dir.to_path_buf();
            }
        }
    }
    PathBuf::from("gui")
}

fn main() {
    let dir = resource_dir();

    if !dir.join("index.html").exists() {
        eprintln!("❌ 找不到 GUI 资源目录: {}", dir.display());
        eprintln!("   请确保 gui/ 目录与 anydoc-gui.exe 在同一文件夹下。");
        std::process::exit(1);
    }

    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(17811);

    let url = format!("http://127.0.0.1:{}/", port);

    println!("🚀 AnyDoc GUI");
    println!("   资源: {}", dir.display());
    println!("   地址: {}", url);
    println!("   关闭此窗口即退出服务。\n");

    // 打开系统浏览器
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }

    if let Err(e) = serve(&dir, port) {
        eprintln!("❌ 服务启动失败: {}", e);
        eprintln!("   端口可能被占用，请换一个：anydoc-gui.exe 17812");
        std::process::exit(1);
    }
}