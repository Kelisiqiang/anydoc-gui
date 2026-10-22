// DPI 感知
//
// 用户截图显示界面在 200% 缩放下完全错位（文字重叠、控件错位）。
// 根因：进程没有声明 Per-Monitor V2 DPI 感知，Windows 做了 DPI 虚拟化，
// 导致我们按物理像素算的坐标和实际渲染尺寸对不上。
//
// 解决：
//   1. 启动时调用 SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)
//   2. 所有布局/绘制尺寸乘以 dpi/96
//   3. 窗口跨屏移动时（WM_DPICHANGED）重新计算并重建字体

use std::sync::atomic::{AtomicI32, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 当前 DPI 缩放系数（1.0 = 96dpi，1.5 = 144dpi，2.0 = 192dpi）
static SCALE: AtomicI32 = AtomicI32::new(100);

/// 设置进程级 DPI 感知。必须在创建任何窗口前调用。
pub fn enable_dpi_awareness() {
    unsafe {
        // 优先用 Per-Monitor V2（Win10 1703+），动态加载避免链接问题
        if let Ok(h) = GetModuleHandleW(PCWSTR::null()) {
            let proc = GetProcAddress(
                h,
                windows::core::PCSTR(b"SetProcessDpiAwarenessContext\0".as_ptr()),
            );
            if !proc.is_none() {
                // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
                let f: unsafe extern "system" fn(isize) -> windows::core::HRESULT =
                    std::mem::transmute(proc);
                if f(-4).is_ok() {
                    return;
                }
            }
        }
        // 退路：系统 DPI 感知（Vista+）
        let _ = SetProcessDPIAware();
    }
}

/// 取窗口所在显示器的 DPI
pub fn dpi_for_window(hwnd: HWND) -> u32 {
    unsafe {
        // GetDpiForWindow 需要 Win10 1607+，用 GetDpiForWindow 动态加载
        if let Ok(h) = GetModuleHandleW(PCWSTR::null()) {
            let proc = GetProcAddress(
                h,
                windows::core::PCSTR(b"GetDpiForWindow\0".as_ptr()),
            );
            if !proc.is_none() {
                let f: unsafe extern "system" fn(HWND) -> u32 = std::mem::transmute(proc);
                let d = f(hwnd);
                if d > 0 {
                    return d;
                }
            }
        }
        // GetDeviceCaps 兜底
        let hdc = windows::Win32::Graphics::Gdi::GetDC(None);
        let d = windows::Win32::Graphics::Gdi::GetDeviceCaps(
            hdc,
            windows::Win32::Graphics::Gdi::LOGPIXELSX,
        );
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
        if d > 0 {
            d as u32
        } else {
            96
        }
    }
}

/// 初始化缩放系数（创建窗口前调用）
///
/// 用 GetDeviceCaps 拿系统 DPI，避免依赖窗口句柄。
pub fn init_scale() -> i32 {
    unsafe {
        let hdc = windows::Win32::Graphics::Gdi::GetDC(None);
        let dpi = windows::Win32::Graphics::Gdi::GetDeviceCaps(
            hdc,
            windows::Win32::Graphics::Gdi::LOGPIXELSX,
        );
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
        let dpi = if dpi > 0 { dpi } else { 96 };
        let scale = ((dpi as f64 / 96.0) * 100.0).round() as i32;
        SCALE.store(scale, Ordering::Relaxed);
        scale
    }
}

/// 更新全局缩放系数，返回新值
pub fn update_scale(hwnd: HWND) -> i32 {
    let dpi = dpi_for_window(hwnd);
    let scale = ((dpi as f64 / 96.0) * 100.0).round() as i32;
    SCALE.store(scale, Ordering::Relaxed);
    scale
}

/// 当前缩放系数（百分比，100 = 1x）
pub fn scale() -> i32 {
    SCALE.load(Ordering::Relaxed)
}

/// 缩放一个逻辑像素值
pub fn px(v: i32) -> i32 {
    (v as f64 * scale() as f64 / 100.0).round() as i32
}

/// 缩放一个逻辑字号（负数表示绝对值）
pub fn font_px(v: i32) -> i32 {
    let a = v.abs();
    let s = (a as f64 * scale() as f64 / 100.0).round() as i32;
    if v < 0 {
        -s
    } else {
        s
    }
}
