// AnyDoc core library - 包装 firecrawl/anydoc
//
// 设计说明：
// 解析内核完全使用官方 anydoc crate (MIT License, https://crates.io/crates/anydoc)
// 我们只负责：文件遍历、批量调度、输出路径管理、进度反馈。

use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 支持的扩展名（anydoc 支持 14 种格式，这里列出对应扩展名用于预筛选）
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "doc", "docx", "docm", // Word
    "ppt", "pptx", "pptm", // PowerPoint
    "xls", "xlsx", "xlsm", // Excel
    "odt", "ods", "odp",   // OpenDocument
    "rtf",                 // RTF
    "epub",                // EPUB
    "csv",                 // CSV
    "pdf",                 // PDF
];

/// 判断扩展名是否在支持列表中（注意：仅用于快速过滤，
/// 真正的格式识别由 anydoc 的字节签名完成）
pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|e| {
            let e = e.to_lowercase();
            SUPPORTED_EXTENSIONS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// 转换结果
#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub input: PathBuf,
    pub output: PathBuf,
    pub size_bytes: usize,
    pub duration_ms: u128,
}

/// 单文件转换
///
/// 使用 anydoc::to_markdown —— 它内部先读字节签名识别格式，
/// 扩展名只作为 CSV 等无签名格式的兜底。因此改过扩展名的文件也能正确识别。
pub fn convert_file(input: &Path, output_dir: Option<&Path>) -> Result<ConvertResult> {
    if !input.exists() {
        return Err(anyhow!("文件不存在: {}", input.display()));
    }

    let start = Instant::now();

    let markdown = anydoc::to_markdown(input).map_err(|e| {
        anyhow!("转换失败 ({}): {}", input.display(), e)
    })?;

    // 输出路径
    let out_dir = match output_dir {
        Some(d) => {
            if !d.exists() {
                fs::create_dir_all(d)
                    .map_err(|e| anyhow!("无法创建输出目录 {}: {}", d.display(), e))?;
            }
            d.to_path_buf()
        }
        None => input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("无效文件名: {}", input.display()))?;

    let output_path = out_dir.join(format!("{}.md", stem));

    fs::write(&output_path, &markdown)
        .map_err(|e| anyhow!("写入失败 {}: {}", output_path.display(), e))?;

    Ok(ConvertResult {
        input: input.to_path_buf(),
        output: output_path,
        size_bytes: markdown.len(),
        duration_ms: start.elapsed().as_millis(),
    })
}

/// 批量转换目录
pub fn convert_directory(
    dir: &Path,
    output_dir: Option<&Path>,
    recursive: bool,
) -> Result<Vec<ConvertResult>> {
    let walker = if recursive {
        walkdir::WalkDir::new(dir)
    } else {
        walkdir::WalkDir::new(dir).max_depth(1)
    };

    let files: Vec<PathBuf> = walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| {
            // 跳过已生成的 .md
            p.extension().and_then(|s| s.to_str()).map(|e| e.eq_ignore_ascii_case("md")).unwrap_or(false) == false
                && is_supported_extension(p)
        })
        .collect();

    let mut results = Vec::with_capacity(files.len());
    for path in files {
        match convert_file(&path, output_dir) {
            Ok(r) => results.push(r),
            Err(e) => eprintln!("❌ {}", e),
        }
    }

    Ok(results)
}