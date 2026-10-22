// 文件/文件夹选择对话框
//
// 用 IFileDialog（COM）而不是老旧的 SHBrowseForFolder —— 前者是 Win11 现代风格，
// 后者是 Win98 样式。

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;

use crate::state::is_supported;

/// 选择一个或多个文件
pub fn pick_files(hwnd: HWND) -> Option<Vec<PathBuf>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog = CoCreateInstance(
            &FileOpenDialog,
            None::<&windows::core::IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
        .ok()?;

        let mut opts = dialog.GetOptions().ok()?;
        opts |= FOS_ALLOWMULTISELECT | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST;
        let _ = dialog.SetOptions(opts);

        // 文件类型过滤器
        let filters: Vec<(&str, &str)> = vec![
            ("支持格式", "*.docx;*.doc;*.docm;*.pptx;*.ppt;*.pptm;*.xlsx;*.xls;*.xlsm;*.xlsb;*.pdf;*.epub;*.odt;*.ods;*.odp;*.rtf;*.csv"),
            ("Word", "*.docx;*.doc;*.docm"),
            ("PowerPoint", "*.pptx;*.ppt;*.pptm"),
            ("Excel", "*.xlsx;*.xls;*.xlsm;*.xlsb"),
            ("PDF", "*.pdf"),
            ("其他", "*.epub;*.odt;*.ods;*.odp;*.rtf;*.csv"),
            ("所有文件", "*.*"),
        ];
        for (name, pattern) in filters {
            let _ = dialog.SetFileTypes(
                &[COMDLG_FILTERSPEC {
                    pszName: windows::core::PCWSTR(to_wide(name).as_ptr()),
                    pszSpec: windows::core::PCWSTR(to_wide(pattern).as_ptr()),
                }]
                .as_slice(),
            );
        }

        if dialog.Show(hwnd).is_err() {
            return None;
        }

        let items = dialog.GetResults().ok()?;
        let count = items.GetCount().ok().unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(item) = items.GetItemAt(i) else { continue };
            if let Ok(path) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                let ps = path.to_string().ok()?;
                let p = PathBuf::from(ps);
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if is_supported(&ext) {
                    out.push(p);
                }
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

/// 选择文件夹
pub fn pick_folder(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog = CoCreateInstance(
            &FileOpenDialog,
            None::<&windows::core::IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
        .ok()?;

        let mut opts = dialog.GetOptions().ok()?;
        opts |= FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM;
        let _ = dialog.SetOptions(opts);

        if dialog.Show(hwnd).is_err() {
            return None;
        }

        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = path.to_string().ok()?;
        Some(PathBuf::from(s))
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
