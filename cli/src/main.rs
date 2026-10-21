// AnyDoc CLI - 把文档秒变 Markdown
// 解析内核: firecrawl/anydoc (MIT)
//
// 用法:
//   anydoc file.docx                  # 单文件
//   anydoc ./docs/                    # 转换目录
//   anydoc ./docs/ -o ./output/ -r    # 批量递归
//   anydoc --help

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, ValueHint};
use indicatif::{ProgressBar, ProgressStyle};

use anydoc_core::{convert_directory, convert_file, is_supported_extension};

#[derive(Parser, Debug)]
#[command(name = "anydoc")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(
    about = "AnyDoc - 把 Word/PPT/Excel/PDF 等文档秒变 Markdown",
    long_about = "AnyDoc v0.2.0\n\
                  解析内核由 firecrawl/anydoc (MIT) 提供\n\
                  支持 14 种格式: Word/PowerPoint/Excel/OpenDocument/RTF/EPUB/CSV/PDF\n\n\
                  格式识别基于文件字节签名, 改过扩展名的文件也能正确解析。"
)]
struct Cli {
    /// 输入文件或目录
    #[arg(value_hint = ValueHint::FilePath)]
    input: PathBuf,

    /// 输出目录（默认与输入同目录）
    #[arg(short, long, value_hint = ValueHint::DirPath)]
    output: Option<PathBuf>,

    /// 递归处理子目录
    #[arg(short, long)]
    recursive: bool,

    /// 详细输出（显示每个文件耗时）
    #[arg(short, long)]
    verbose: bool,

    /// 不显示进度条
    #[arg(long)]
    no_progress: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !cli.input.exists() {
        eprintln!("❌ 路径不存在: {}", cli.input.display());
        return ExitCode::from(1);
    }

    let start = std::time::Instant::now();
    let mut success = 0usize;
    let mut failed = 0usize;

    let result: Result<()> = (|| -> Result<()> {
        if cli.input.is_file() {
            // ---------- 单文件模式 ----------
            match convert_file(&cli.input, cli.output.as_deref()) {
                Ok(r) => {
                    println!(
                        "✅ {} -> {}  ({} bytes, {}ms)",
                        r.input.display(),
                        r.output.display(),
                        r.size_bytes,
                        r.duration_ms
                    );
                    success += 1;
                }
                Err(e) => {
                    eprintln!("{}", e);
                    failed += 1;
                }
            }
        } else if cli.input.is_dir() {
            // ---------- 目录模式 ----------
            // 先预扫描，让用户知道有多少文件
            let walker = if cli.recursive {
                walkdir::WalkDir::new(&cli.input)
            } else {
                walkdir::WalkDir::new(&cli.input).max_depth(1)
            };
            let files: Vec<PathBuf> = walker
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .filter(|p| is_supported_extension(p))
                .collect();

            if files.is_empty() {
                println!("⚠️  目录下没有支持的文档文件");
                println!("    支持的扩展名: doc docx docm ppt pptx pptm xls xlsx xlsm odt ods odp rtf epub csv pdf");
                return Ok(());
            }

            println!("📁 找到 {} 个文件", files.len());

            let pb = if !cli.no_progress {
                let pb = ProgressBar::new(files.len() as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
                        .unwrap()
                        .progress_chars("█▉▊▋▌▍▎▏  "),
                );
                Some(pb)
            } else {
                None
            };

            for path in &files {
                if let Some(ref p) = pb {
                    p.set_message(format!(
                        "{:?}",
                        path.file_name().unwrap_or_default()
                    ));
                }

                match convert_file(path, cli.output.as_deref()) {
                    Ok(r) => {
                        if cli.verbose {
                            println!(
                                "✅ {} -> {} ({} bytes, {}ms)",
                                r.input.display(),
                                r.output.display(),
                                r.size_bytes,
                                r.duration_ms
                            );
                        }
                        success += 1;
                    }
                    Err(e) => {
                        eprintln!("❌ {}", e);
                        failed += 1;
                    }
                }

                if let Some(ref p) = pb {
                    p.inc(1);
                }
            }

            if let Some(p) = pb {
                p.finish_with_message("完成");
            }

            let _ = convert_directory(&cli.input, cli.output.as_deref(), cli.recursive);
        }

        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("❌ {}", e);
        return ExitCode::from(1);
    }

    let elapsed = start.elapsed();
    println!(
        "\n📊 总计: 成功 {} 失败 {} 耗时 {:.3}s",
        success,
        failed,
        elapsed.as_secs_f64()
    );

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}