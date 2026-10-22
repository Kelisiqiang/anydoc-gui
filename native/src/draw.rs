// 绘制辅助：圆角矩形、状态徽章、文本
//
// 所有尺寸都是「逻辑像素」，实际绘制时按 DPI 缩放。
// 统一走 GDI，绘制前用 BeginBufferedPaint 避免闪烁（后续可优化）。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;

use crate::dpi;
use crate::state::Status;

/// 主题配色
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
    pub drop_bg: COLORREF,
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
            line: rgb(0xD5, 0xD9, 0xE0),
            line_soft: rgb(0xEC, 0xEF, 0xF3),
            text: rgb(0x10, 0x18, 0x28),
            text_dim: rgb(0x5C, 0x67, 0x7A),
            text_mute: rgb(0x8A, 0x94, 0xA6),
            accent: rgb(0x4F, 0x46, 0xE5),
            ok: rgb(0x12, 0xB7, 0x6A),
            err: rgb(0xF0, 0x44, 0x38),
            wait: rgb(0x8A, 0x94, 0xA6),
            ok_bg: rgb(0xE6, 0xF7, 0xEF),
            err_bg: rgb(0xFD, 0xEE, 0xED),
            wait_bg: rgb(0xF1, 0xF3, 0xF6),
            run_bg: rgb(0xEC, 0xF0, 0xFF),
            drop_bg: rgb(0xFA, 0xFB, 0xFC),
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: rgb(0x1E, 0x1E, 0x22),
            card: rgb(0x28, 0x28, 0x2E),
            line: rgb(0x3C, 0x3C, 0x46),
            line_soft: rgb(0x33, 0x33, 0x3B),
            text: rgb(0xF2, 0xF4, 0xF7),
            text_dim: rgb(0xA6, 0xAF, 0xBD),
            text_mute: rgb(0x78, 0x82, 0x94),
            accent: rgb(0x8B, 0x99, 0xFA),
            ok: rgb(0x32, 0xD5, 0x8A),
            err: rgb(0xF9, 0x71, 0x71),
            wait: rgb(0x78, 0x82, 0x94),
            ok_bg: rgb(0x14, 0x3A, 0x2C),
            err_bg: rgb(0x45, 0x1D, 0x1D),
            wait_bg: rgb(0x33, 0x33, 0x3B),
            run_bg: rgb(0x2A, 0x2C, 0x54),
            drop_bg: rgb(0x24, 0x24, 0x2A),
            dark: true,
        }
    }
}

/// 创建字体（字号按 DPI 缩放，字体名优先用微软雅黑 UI）
pub fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        // 微软雅黑 UI 是 Win7+ 自带的中文 UI 字体
        let face: Vec<u16> = "Microsoft YaHei UI\0".encode_utf16().collect();
        CreateFontW(
            dpi::font_px(px),
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
            PCWSTR(face.as_ptr()),
        )
    }
}

/// 绘制圆角矩形（填充）
pub fn fill_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        let r = dpi::px(radius).max(1);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, r, r);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// 画圆角矩形边框
pub fn stroke_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF, width: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width.max(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let null = GetStockObject(NULL_BRUSH);
        let old2: HGDIOBJ = SelectObject(hdc, null);
        let r = dpi::px(radius).max(1);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, r, r);
        let _ = SelectObject(hdc, old2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 实心圆点
pub fn fill_circle(hdc: HDC, cx: i32, cy: i32, radius: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        let pen = CreatePen(PS_SOLID, 1, color);
        let old2: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let r = dpi::px(radius).max(1);
        let _ = Ellipse(hdc, cx - r, cy - r, cx + r, cy + r);
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
        SetBkMode(hdc, TRANSPARENT);
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
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let _ = DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        SIZE {
            cx: rc.right - rc.left,
            cy: rc.bottom - rc.top,
        }
    }
}

/// 绘制状态徽章（圆角药丸）
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
        let pad = dpi::px(9);
        let w = sz.cx + pad * 2;
        let h = dpi::px(22);
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
            DRAW_TEXT_FORMAT(DT_CENTER.0),
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
            let short = if e.chars().count() > 10 {
                format!("✗ {}…", e.chars().take(8).collect::<String>())
            } else {
                format!("✗ {}", e)
            };
            (short, theme.err, theme.err_bg)
        }
    }
}

/// 按字符安全截断
fn safe_cut(s: &str, max_chars: usize) -> usize {
    s.char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 图标占位块
pub fn draw_icon_placeholder(hdc: HDC, x: i32, y: i32, size: i32, bg: COLORREF) {
    let rect = RECT {
        left: x,
        top: y,
        right: x + size,
        bottom: y + size,
    };
    fill_round_rect(hdc, &rect, dpi::px(6), bg);
}

/// 水平细线
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

/// 垂直细线
pub fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        MoveToEx(hdc, x, y1, None);
        LineTo(hdc, x, y2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 清理 GDI 对象
pub fn cleanup(obj: HGDIOBJ) {
    unsafe {
        let _ = DeleteObject(obj);
    }
}

/// POINT 构造
pub fn pt(x: i32, y: i32) -> POINT {
    POINT { x, y }
}

/// 保持 safe_cut 不被误删
#[allow(dead_code)]
pub fn cut(s: &str, n: usize) -> usize {
    safe_cut(s, n)
}
