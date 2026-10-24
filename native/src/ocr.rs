// 云端 OCR：把「含扫描页的 PDF」交给 Firecrawl Parse 转 Markdown
//
// ── 为什么需要这个 ──
// 官方 anydoc 的 PDF 内核是 pdf-inspector，它**本地不做 OCR**：
// 遇到扫描页 / 图片页会直接报 `needsOcr`，整份文档都失败。
// 官方自己给的解法是 `anydoc scan.pdf --ocr hosted` —— 把整份文档交给
// Firecrawl Parse 云端处理（官方 README 明确写了：本地不 OCR，
// 用 `ocr: 'hosted'` 才会发到云端；而且**不用注册就能用**）。
//
// 官方 Node 版调用的是：
//   POST https://api.firecrawl.dev/v2/parse
//   multipart/form-data
//     options = {"parsers":[{"type":"pdf","mode":"auto"}],"origin":"anydoc@x.y.z"}
//     file    = 原始 PDF 字节
//   响应 = {"success":true,"data":{"markdown":"..."}}
//   Authorization: Bearer <FIRECRAWL_API_KEY>（可选，不填也能用，只是限额更低）
//
// 这里复刻同一条通路，让 GUI 也能把扫描件转出来。
//
// 为什么用 ureq 而不是 WinHTTP：WinHTTP 发带 body 的请求时会自动带
// `Expect: 100-continue`，而这个接口不认这套握手（会互相等待、直接死锁）；
// ureq 行为可控，且自带根证书、不依赖系统证书库。

use std::sync::OnceLock;
use std::time::Duration;

use ureq::tls::{TlsConfig, TlsProvider};

const DEFAULT_URL: &str = "https://api.firecrawl.dev/v2/parse";
const BOUNDARY: &str = "----AnyDocBoundary7MA4YWxkTrZu0gW";
const TIMEOUT: Duration = Duration::from_secs(300);

/// 共享的 HTTP 客户端（TLS 用 rustls，自带根证书）
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            // 让 4xx/5xx 也正常返回，方便读错误正文
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::Rustls)
                    .build(),
            )
            .build()
            .into()
    })
}

/// 拼 multipart/form-data 请求体
fn build_body(bytes: &[u8], filename: &str) -> Vec<u8> {
    let options = format!(
        r#"{{"parsers":[{{"type":"pdf","mode":"auto"}}],"origin":"anydoc@{}"}}"#,
        env!("CARGO_PKG_VERSION")
    );
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 1024);
    body.extend_from_slice(
        format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"options\"\r\n\
             Content-Type: application/json\r\n\r\n{options}\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{fn}\"\r\n\
             Content-Type: application/pdf\r\n\r\n",
            b = BOUNDARY,
            options = options,
            fn = filename.replace('"', "")
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{b}--\r\n", b = BOUNDARY).as_bytes());
    body
}

/// 把 PDF 交给云端 OCR，返回 Markdown
pub fn hosted_markdown(bytes: &[u8], filename: &str) -> Result<String, String> {
    let url = std::env::var("FIRECRAWL_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let body = build_body(bytes, filename);

    crate::ui::diag(&format!(
        "云端OCR: 开始 {url} body={} 字节",
        body.len()
    ));

    let mut req = agent()
        .post(&url)
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header("Accept", "application/json");
    if let Ok(key) = std::env::var("FIRECRAWL_API_KEY") {
        let key = key.trim();
        if !key.is_empty() {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
    }

    let resp = req
        .send(&body[..])
        .map_err(|e| format!("云端 OCR 请求失败：{e}（检查网络/代理）"))?;
    let status = resp.status().as_u16();
    let text = resp
        .into_body()
        .read_to_string()
        .map_err(|e| format!("读取云端响应失败：{e}"))?;
    crate::ui::diag(&format!("云端OCR: status={status} body={} 字节", text.len()));

    // 先按 JSON 解，拿到更友好的错误
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("云端返回的不是 JSON（HTTP {status}）：{e}"))?;
    if !(200..300).contains(&status) {
        let msg = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("未知错误")
            .to_string();
        return Err(match status {
            401 => format!("云端 OCR 拒绝了请求（401）：{msg}（设置环境变量 FIRECRAWL_API_KEY 可提高额度）"),
            429 => format!("云端 OCR 限流（429）：{msg}（设置 FIRECRAWL_API_KEY 可提高额度）"),
            413 => format!("文件太大，云端拒绝（413）：{msg}"),
            _ => format!("云端 OCR 失败（HTTP {status}）：{msg}"),
        });
    }
    if v.get("success").and_then(|s| s.as_bool()) != Some(true) {
        let msg = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("云端未返回 Markdown")
            .to_string();
        return Err(format!("云端 OCR 失败：{msg}"));
    }
    let md = v
        .get("data")
        .and_then(|d| d.get("markdown"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if md.trim().is_empty() {
        return Err("云端 OCR 没有返回内容".to_string());
    }
    let mut md = md.to_string();
    if !md.ends_with('\n') {
        md.push('\n');
    }
    Ok(md)
}
