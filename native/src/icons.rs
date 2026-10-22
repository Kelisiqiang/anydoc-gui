// 系统文件图标提取
//
// 用 SHGetFileInfo 取 Windows 自带的文件类型图标（Word 蓝标、Excel 绿标），
// 比 emoji 更专业，也不用打包字体。
//
// 注意：必须传有效的 dwFileAttributes（比如 FILE_ATTRIBUTE_NORMAL），
// 否则配合 SHGFI_USEFILEATTRIBUTES 时系统认不出类型，全返回通用图标。

use std::path::Path;

use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES};
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

        // SHGFI_USEFILEATTRIBUTES: 用文件属性（扩展名）决定图标，不读文件内容
        // 这样对不存在的路径也能拿到正确图标
        // LARGEICON(32px) 在 26px 显示位更清晰
        let flags = SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES;
        let ok = SHGetFileInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        if ok == 0 || shfi.hIcon.0.is_null() {
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
