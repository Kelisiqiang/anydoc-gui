// 系统文件图标提取
//
// 用 SHGetFileInfo 取 Windows 自带的文件类型图标（Word 蓝标、Excel 绿标），
// 比 emoji 更专业，也不用打包字体。

use std::path::Path;

use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

pub struct IconEntry {
    pub hicon: Option<HICON>,
}

/// 取文件图标
pub fn load(path: &Path, _ext: &str) -> Option<IconEntry> {
    unsafe {
        let wide = to_wide(path.as_os_str());
        let mut shfi = SHFILEINFOW {
            ..Default::default()
        };

        let flags = SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES;
        let ok = SHGetFileInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        if ok == 0 {
            return None;
        }
        if shfi.hIcon.0.is_null() {
            return None;
        }
        Some(IconEntry {
            hicon: Some(shfi.hIcon),
        })
    }
}

/// 释放图标
pub fn destroy(entry: &IconEntry) {
    unsafe {
        if let Some(h) = entry.hicon {
            let _ = DestroyIcon(h);
        }
    }
}

/// 字符串转宽字符
fn to_wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}
