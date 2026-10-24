// AnyDoc 原生版入口
//
// 单实例运行：已有实例时激活它，而不是开第二个窗口。

mod dialog;
mod dpi;
mod frontmatter;
mod draw;
mod icons;
mod state;
#[cfg(test)]
mod tests;
mod ui;
mod worker;

use windows::core::w;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::*;

fn main() {
    ui::diag("MAIN_ENTER");
    // 崩溃也要留证据：panic 时写日志 + 弹窗，界面黑掉时能一眼定位
    {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                let msg = format!("panic: {info}");
                ui::diag(&msg);
                use windows::Win32::UI::WindowsAndMessaging::{
                    MessageBoxW, MB_ICONERROR, MB_OK,
                };
                use windows::core::PCWSTR;
                let text: Vec<u16> = "AnyDoc 遇到问题，请把 anydoc-diag.log 发给开发者"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let title: Vec<u16> = "AnyDoc"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                unsafe {
                    let _ = MessageBoxW(
                        None,
                        PCWSTR(text.as_ptr()),
                        PCWSTR(title.as_ptr()),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }));
        });
    }

    // 参数解析
    //   anydoc                 → GUI 模式
    //   anydoc --gui [文件...]  → GUI 模式 + 预加载文件（测试/便捷用）
    //   anydoc <路径> [-o 目录] → 命令行模式
    let args: Vec<String> = std::env::args().collect();
    let gui_flag = args.iter().any(|a| a == "--gui");
    if args.len() > 1 && !gui_flag {
        std::process::exit(cli_mode(&args));
    }
    // --gui 后面跟的文件用于预加载
    let preload: Vec<PathBuf> = if gui_flag {
        let mut v = Vec::new();
        let mut seen = false;
        for a in &args[1..] {
            if a == "--gui" {
                seen = true;
                continue;
            }
            if seen {
                let p = PathBuf::from(a);
                if p.exists() {
                    v.push(p);
                }
            }
        }
        v
    } else {
        Vec::new()
    };

    // DPI 感知必须在创建窗口前
    dpi::enable_dpi_awareness();
    // 初始化缩放系数（窗口尺寸要用它算）
    let _ = dpi::init_scale();

    unsafe {
        // 单实例检查
        let mutex_name = w!("Local\\AnyDocNativeSingleton");
        let mutex = CreateMutexW(None, true, mutex_name);
        let already_running = mutex.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS;
        if already_running {
            // 已有实例，激活它
            if let Ok(w) = FindWindowW(w!("AnyDocNative"), None) {
                let _ = ShowWindow(w, SW_RESTORE);
                let _ = SetForegroundWindow(w);
            }
            return;
        }

        // 拖放需要 OLE 初始化
        let _ = windows::Win32::System::Ole::OleInitialize(None);

        ui::register_class();

        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let hwnd = ui::create(hinst);
        let _ = ShowWindow(hwnd, SW_SHOW);

        // 预加载文件
        if !preload.is_empty() {
            for p in preload {
                if p.is_dir() {
                    ui::preload_folder(hwnd, &p);
                } else {
                    ui::preload_files(hwnd, &[p]);
                }
            }
        }

        // 消息循环
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}


/// 命令行模式（无 GUI）：用于脚本 / 自动化 / 测试
fn cli_mode(args: &[String]) -> i32 {
    use crate::state::is_supported;

    let input = PathBuf::from(&args[1]);
    let mut out_dir: Option<PathBuf> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "-o" || args[i] == "--output" {
            if i + 1 < args.len() {
                out_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    if !input.exists() {
        eprintln!("❌ 路径不存在: {}", input.display());
        return 1;
    }

    let start = std::time::Instant::now();
    let mut files = Vec::new();
    if input.is_dir() {
        for entry in walkdir::WalkDir::new(&input)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let p = entry.path().to_path_buf();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if is_supported(&ext) {
                files.push(p);
            }
        }
    } else {
        files.push(input.clone());
    }

    if files.is_empty() {
        eprintln!("❌ 没有找到可转换的文件");
        return 1;
    }

    let mut ok = 0;
    let mut fail = 0;
    for f in &files {
        let t0 = std::time::Instant::now();
        match anydoc::to_markdown(f) {
            Ok(md) => {
                let target_dir = match &out_dir {
                    Some(d) => d.clone(),
                    None => f.parent().unwrap_or(Path::new(".")).to_path_buf(),
                };
                let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
                let out = target_dir.join(format!("{}.md", stem));
                if let Some(p) = out.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                match std::fs::write(&out, md.as_bytes()) {
                    Ok(_) => {
                        println!("✅ {} -> {} ({} bytes, {}ms)",
                            f.display(), out.display(), md.len(), t0.elapsed().as_millis());
                        ok += 1;
                    }
                    Err(e) => {
                        eprintln!("❌ 写入失败 {}: {}", out.display(), e);
                        fail += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ {} — {}", f.display(), e.code());
                fail += 1;
            }
        }
    }

    println!("\n📊 成功 {} 失败 {} 耗时 {:.2}s", ok, fail, start.elapsed().as_secs_f64());
    if fail > 0 { 1 } else { 0 }
}
