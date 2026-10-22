// 绘制辅助：圆角矩形、状态徽章、文本
//
// 全部走 GDI + BeginBufferedPaint（双缓冲），避免闪烁。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::HGDIOBJ;
use windows::Win32::Graphics::Gdi::*;

use crate::state::Status;

/// 主题配色（浅色）
pub struct Theme {
    pub bg: COLORREF,
    pub card: COLORREF,
    pub line: COLORREF,
    pub line_soft: COLORREF,
    pub text: COLORREF,
    pub text_dim: COLORREF,
    pub text_mute: COLORREF,
    pub accent: COLORREF,
    pub ok: COLORREF,
    pub err: COLORREF,
    pub wait: COLORREF,
    pub ok_bg: COLORREF,
    pub err_bg: COLORREF,
    pub wait_bg: COLORREF,
    pub run_bg: COLORREF,
    pub dark: bool,
}

/// 0x00BBGGRR
pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg: rgb(0xF3, 0xF3, 0xF6),
            card: rgb(0xFF, 0xFF, 0xFF),
            line: rgb(0xE4, 0xE7, 0xEC),
            line_soft: rgb(0xEF, 0xF1, 0xF4),
            text: rgb(0x10, 0x18, 0x28),
            text_dim: rgb(0x66, 0x70, 0x85),
            text_mute: rgb(0x98, 0xA2, 0xB3),
            accent: rgb(0x4F, 0x46, 0xE5),
            ok: rgb(0x12, 0xB7, 0x6A),
            err: rgb(0xF0, 0x44, 0x38),
            wait: rgb(0x98, 0xA2, 0xB3),
            ok_bg: rgb(0xE7, 0xF8, 0xF0),
            err_bg: rgb(0xFE, 0xF0, 0xEF),
            wait_bg: rgb(0xF2, 0xF4, 0xF7),
            run_bg: rgb(0xEE, 0xF2, 0xFF),
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: rgb(0x20, 0x20, 0x24),
            card: rgb(0x2A, 0x2A, 0x30),
            line: rgb(0x3A, 0x3A, 0x42),
            line_soft: rgb(0x33, 0x33, 0x3A),
            text: rgb(0xF2, 0xF4, 0xF7),
            text_dim: rgb(0xA0, 0xAA, 0xBA),
            text_mute: rgb(0x76, 0x81, 0x94),
            accent: rgb(0x7C, 0x8C, 0xFA),
            ok: rgb(0x32, 0xD5, 0x8A),
            err: rgb(0xF9, 0x71, 0x71),
            wait: rgb(0x76, 0x81, 0x94),
            ok_bg: rgb(0x14, 0x3A, 0x2C),
            err_bg: rgb(0x45, 0x1D, 0x1D),
            wait_bg: rgb(0x33, 0x33, 0x3A),
            run_bg: rgb(0x2A, 0x2C, 0x54),
            dark: true,
        }
    }
}

/// 创建字体
pub fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR::null(),
        )
    }
}

/// 绘制圆角矩形（filled）
pub fn fill_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// 画一条圆角矩形边框
pub fn stroke_round_rect(
    hdc: HDC,
    rect: &RECT,
    radius: i32,
    color: COLORREF,
    width: i32,
) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let null = GetStockObject(NULL_BRUSH);
        let old2: HGDIOBJ = SelectObject(hdc, null);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        let _ = SelectObject(hdc, old2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 画圆点
pub fn fill_circle(hdc: HDC, cx: i32, cy: i32, radius: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        let pen = CreatePen(PS_SOLID, 1, color);
        let old2: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = Ellipse(
            hdc,
            cx - radius,
            cy - radius,
            cx + radius,
            cy + radius,
        );
        let _ = SelectObject(hdc, old2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// 绘制单行文本，返回绘制宽度
pub fn draw_text(
    hdc: HDC,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: COLORREF,
    align: DRAW_TEXT_FORMAT,
    clip: bool,
) -> i32 {
    unsafe {
        SetTextColor(hdc, color);
        let mut rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let flags = DT_SINGLELINE | DT_VCENTER | align
            | if clip { DT_END_ELLIPSIS } else { DRAW_TEXT_FORMAT(0) };
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(hdc, &mut buf, &mut rc, flags);
        rc.right - rc.left
    }
}

/// 文本测量
pub fn measure_text(hdc: HDC, text: &str) -> SIZE {
    unsafe {
        let mut sz = SIZE { cx: 0, cy: 0 };
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let _ = DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        sz.cx = rc.right - rc.left;
        sz.cy = rc.bottom - rc.top;
        sz
    }
}

/// 绘制状态徽章
///
/// 返回徽章宽度。text 是已经格式化好的内容。
pub fn draw_badge(
    hdc: HDC,
    text: &str,
    right_x: i32,
    center_y: i32,
    fg: COLORREF,
    bg: COLORREF,
    font: HFONT,
) {
    unsafe {
        let old_font: HGDIOBJ = SelectObject(hdc, HGDIOBJ(font.0));
        let sz = measure_text(hdc, text);
        let pad = 10;
        let w = sz.cx + pad * 2;
        let h = 22;
        let left = right_x - w;
        let rect = RECT {
            left,
            top: center_y - h / 2,
            right: right_x,
            bottom: center_y + h / 2,
        };
        fill_round_rect(hdc, &rect, h / 2, bg);
        draw_text(
            hdc,
            text,
            rect.left,
            rect.top,
            w,
            h,
            fg,
            DT_CENTER,
            false,
        );
        let _ = SelectObject(hdc, old_font);
    }
}

/// 状态 → (文字, 前景色, 背景色)
pub fn status_style(theme: &Theme, status: &Status) -> (String, COLORREF, COLORREF) {
    match status {
        Status::Wait => ("等待".to_string(), theme.wait, theme.wait_bg),
        Status::Running => ("转换中".to_string(), theme.accent, theme.run_bg),
        Status::Done(ms) => (format!("✓ {}ms", ms), theme.ok, theme.ok_bg),
        Status::Error(e) => {
            let short = if e.len() > 12 {
                format!("✗ {}…", &e[..safe_cut(e, 10)])
            } else {
                format!("✗ {}", e)
            };
            (short, theme.err, theme.err_bg)
        }
    }
}

/// 按字符（不是字节）安全截断，避免切坏 UTF-8
fn safe_cut(s: &str, max_chars: usize) -> usize {
    s.char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 绘制一个文件图标占位（真实图标由 icons.rs 提供）
pub fn draw_icon_placeholder(hdc: HDC, x: i32, y: i32, size: i32, bg: COLORREF) {
    let rect = RECT {
        left: x,
        top: y,
        right: x + size,
        bottom: y + size,
    };
    fill_round_rect(hdc, &rect, 6, bg);
}

/// 画一条水平细线
pub fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        MoveToEx(hdc, x1, y, None);
        LineTo(hdc, x2, y);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 释放 GDI 对象
pub fn cleanup(obj: HGDIOBJ) {
    unsafe {
        let _ = DeleteObject(obj);
    }
}

/// 转换 POINT
pub fn pt(x: i32, y: i32) -> POINT {
    POINT { x, y }
}
