// 绘制基础：主题配色、圆角矩形、虚线框、徽章、复选框、图标
//
// 配色对齐设计稿（Tailwind slate / blue 色系）
// 所有尺寸为「逻辑像素」，实际绘制时按 DPI 缩放。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;

use crate::dpi;
use crate::state::Status;

/// 0x00BBGGRR
pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

/// 主题配色
#[derive(Clone)]
pub struct Theme {
    pub bg: COLORREF,
    pub card: COLORREF,
    pub border: COLORREF,
    pub header_bg: COLORREF,
    pub row_hover: COLORREF,
    pub row_active: COLORREF,
    pub text: COLORREF,
    pub text_dim: COLORREF,
    pub text_mute: COLORREF,
    pub blue: COLORREF,
    pub blue_hover: COLORREF,
    pub blue_bg: COLORREF,
    pub blue_border: COLORREF,
    pub green: COLORREF,
    pub green_bg: COLORREF,
    pub red: COLORREF,
    pub red_bg: COLORREF,
    pub amber: COLORREF,
    pub amber_bg: COLORREF,
    pub slate_100: COLORREF,
    pub slate_200: COLORREF,
    pub slate_300: COLORREF,
    pub slate_400: COLORREF,
    pub slate_500: COLORREF,
    pub slate_600: COLORREF,
    pub slate_700: COLORREF,
    pub slate_800: COLORREF,
    pub track: COLORREF,
    pub dark: bool,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg: rgb(0xFC, 0xFA, 0xF8),          // slate-50
            card: rgb(0xFF, 0xFF, 0xFF),
            border: rgb(0xE2, 0xE8, 0xF0),        // slate-200
            header_bg: rgb(0xF8, 0xFA, 0xFC),    // slate-50
            row_hover: rgb(0xF8, 0xFA, 0xFC),
            row_active: rgb(0xEF, 0xF6, 0xFF),    // blue-50
            text: rgb(0x1E, 0x29, 0x3B),         // slate-800
            text_dim: rgb(0x64, 0x74, 0x8B),     // slate-500
            text_mute: rgb(0x94, 0xA3, 0xB8),    // slate-400
            blue: rgb(0x25, 0x63, 0xEB),         // blue-600
            blue_hover: rgb(0x1D, 0x4E, 0xD8),  // blue-700
            blue_bg: rgb(0xEF, 0xF6, 0xFF),      // blue-50
            blue_border: rgb(0xDB, 0xEA, 0xFE),  // blue-100
            green: rgb(0x05, 0x96, 0x69),        // emerald-600
            green_bg: rgb(0xEC, 0xFD, 0xF5),     // emerald-50
            red: rgb(0xDC, 0x26, 0x26),          // red-600
            red_bg: rgb(0xFE, 0xF2, 0xF2),       // red-50
            amber: rgb(0xD9, 0x77, 0x06),        // amber-600
            amber_bg: rgb(0xFF, 0xFB, 0xEB),     // amber-50
            slate_100: rgb(0xF1, 0xF5, 0xF9),
            slate_200: rgb(0xE2, 0xE8, 0xF0),
            slate_300: rgb(0xCB, 0xD5, 0xE1),
            slate_400: rgb(0x94, 0xA3, 0xB8),
            slate_500: rgb(0x64, 0x74, 0x8B),
            slate_600: rgb(0x47, 0x55, 0x69),
            slate_700: rgb(0x33, 0x41, 0x55),
            slate_800: rgb(0x1E, 0x29, 0x3B),
            track: rgb(0xE2, 0xE8, 0xF0),
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: rgb(0x18, 0x18, 0x1C),
            card: rgb(0x24, 0x24, 0x2A),
            border: rgb(0x3A, 0x3A, 0x44),
            header_bg: rgb(0x22, 0x22, 0x28),
            row_hover: rgb(0x28, 0x28, 0x30),
            row_active: rgb(0x1E, 0x2A, 0x45),
            text: rgb(0xF1, 0xF5, 0xF9),
            text_dim: rgb(0xA4, 0xAE, 0xBC),
            text_mute: rgb(0x76, 0x82, 0x94),
            blue: rgb(0x60, 0xA5, 0xFA),
            blue_hover: rgb(0x3B, 0x82, 0xF6),
            blue_bg: rgb(0x1E, 0x2A, 0x45),
            blue_border: rgb(0x25, 0x3A, 0x5E),
            green: rgb(0x34, 0xD3, 0x99),
            green_bg: rgb(0x0F, 0x2E, 0x24),
            red: rgb(0xF8, 0x71, 0x71),
            red_bg: rgb(0x3B, 0x1A, 0x1A),
            amber: rgb(0xFB, 0xBF, 0x24),
            amber_bg: rgb(0x39, 0x2C, 0x10),
            slate_100: rgb(0x2C, 0x2C, 0x34),
            slate_200: rgb(0x3A, 0x3A, 0x44),
            slate_300: rgb(0x4A, 0x4A, 0x56),
            slate_400: rgb(0x76, 0x82, 0x94),
            slate_500: rgb(0xA4, 0xAE, 0xBC),
            slate_600: rgb(0xC4, 0xCB, 0xD5),
            slate_700: rgb(0xE2, 0xE8, 0xF0),
            slate_800: rgb(0xF1, 0xF5, 0xF9),
            track: rgb(0x3A, 0x3A, 0x44),
            dark: true,
        }
    }
}

/// 字体集合（按设计稿的字号层级）
pub struct Fonts {
    pub brand: HFONT,     // 12 品牌条标题
    pub mono: HFONT,      // 12 等宽（扩展名徽章 / 大小 / 路径）
    pub h1: HFONT,        // 20 大标题
    pub sub: HFONT,       // 12 副标题
    pub btn: HFONT,       // 12 按钮
    pub btn_bold: HFONT,  // 12 按钮(粗)
    pub thead: HFONT,     // 11 表头
    pub name: HFONT,      // 13 文件名
    pub meta: HFONT,      // 12 次要信息
    pub tag: HFONT,       // 11 徽章
    pub foot: HFONT,      // 12 底栏
    pub big: HFONT,       // 20 大按钮
}

/// 创建等宽字体（设计稿里扩展名徽章/大小/路径用的就是等宽）
pub fn make_mono(px: i32, weight: i32) -> HFONT {
    unsafe {
        let face: Vec<u16> = "Consolas\0".encode_utf16().collect();
        CreateFontW(
            dpi::font_px(px), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        )
    }
}

/// 创建字体（字号按 DPI 缩放）
pub fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        let face: Vec<u16> = "Microsoft YaHei UI\0".encode_utf16().collect();
        CreateFontW(
            dpi::font_px(px),
            0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        )
    }
}

/// 圆角矩形填充
pub fn fill_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    unsafe {
        // 关键：笔和刷子必须同色。RoundRect 会用「当前笔画」描边，
        // 如果只换刷子，默认黑笔就会在每个圆角矩形上留一圈黑边。
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let old_brush: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let r = dpi::px(radius).max(1);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, r, r);
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ(pen.0));
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// 圆角矩形边框
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

/// 虚线圆角矩形（拖放区专用）
///
/// GDI 没有虚线圆角 API，这里用短直线段手工拼出圆角 + 虚线效果。
pub fn dashed_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = old;
        let r = dpi::px(radius).max(1);
        let dash = dpi::px(8).max(2);
        let gap = dpi::px(6).max(2);

        // 直线段（虚线）
        let mut x = rect.left;
        while x < rect.right {
            let x2 = (x + dash).min(rect.right);
            MoveToEx(hdc, x, rect.top, None);
            LineTo(hdc, x2, rect.top);
            MoveToEx(hdc, x, rect.bottom - 1, None);
            LineTo(hdc, x2, rect.bottom - 1);
            x += dash + gap;
        }
        let mut y = rect.top;
        while y < rect.bottom {
            let y2 = (y + dash).min(rect.bottom);
            MoveToEx(hdc, rect.left, y, None);
            LineTo(hdc, rect.left, y2);
            MoveToEx(hdc, rect.right - 1, y, None);
            LineTo(hdc, rect.right - 1, y2);
            y += dash + gap;
        }

        // 四个圆角用实线弧补上（用 Ellipse 裁剪不方便，改画小圆角矩形边框）
        let _ = r;
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

/// 绘制单行文本
#[allow(clippy::too_many_arguments)]
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
        let mut rc = RECT { left: x, top: y, right: x + w, bottom: y + h };
        let flags = DT_SINGLELINE | DT_VCENTER | align
            | if clip { DT_END_ELLIPSIS } else { DRAW_TEXT_FORMAT(0) };
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(hdc, &mut buf, &mut rc, flags);
        // 宽度单独测量：DrawTextW 对不同对齐方式修改 rc 的行为不一致，
        // 直接用返回值会导致后续元素位置算错。
        measure_text(hdc, text).cx
    }
}

/// 文本测量
pub fn measure_text(hdc: HDC, text: &str) -> SIZE {
    unsafe {
        let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let _ = DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        SIZE { cx: rc.right - rc.left, cy: rc.bottom - rc.top }
    }
}

/// 药丸徽章（返回宽度）
pub fn draw_pill(
    hdc: HDC,
    text: &str,
    x: i32,
    center_y: i32,
    fg: COLORREF,
    bg: COLORREF,
    font: HFONT,
    border: Option<COLORREF>,
) -> i32 {
    unsafe {
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(font.0));
        let sz = measure_text(hdc, text);
        let pad = dpi::px(7);
        let w = sz.cx + pad * 2;
        let h = dpi::px(19);
        let rect = RECT {
            left: x,
            top: center_y - h / 2,
            right: x + w,
            bottom: center_y + h / 2,
        };
        fill_round_rect(hdc, &rect, h / 2, bg);
        if let Some(b) = border {
            stroke_round_rect(hdc, &rect, h / 2, b, 1);
        }
        draw_text(hdc, text, rect.left, rect.top, w, h, fg,
            DRAW_TEXT_FORMAT(DT_CENTER.0), false);
        let _ = SelectObject(hdc, old);
        w
    }
}

/// 状态 → (徽章文字, 前景, 背景, 边框)
pub fn status_style(theme: &Theme, status: &Status) -> (String, COLORREF, COLORREF, Option<COLORREF>) {
    match status {
        Status::Wait => ("排队等待中".to_string(), theme.text_mute, theme.slate_100, None),
        Status::Running => ("转换中".to_string(), theme.blue, theme.blue_bg, Some(theme.blue_border)),
        Status::Done(ms) => {
            let s = if *ms >= 1000 {
                format!("转换完成 ({:.1}s)", *ms as f64 / 1000.0)
            } else {
                format!("转换完成 ({}ms)", ms)
            };
            (s, theme.green, theme.green_bg, None)
        }
        Status::Error(e) => {
            let short = match e.as_str() {
                "需 OCR（anydoc 不含 OCR）" => "需 OCR".to_string(),
                "文件已加密" => "已加密".to_string(),
                "文件结构损坏" => "文件损坏".to_string(),
                "超出安全限制" => "超出限制".to_string(),
                "缺少必要内容" => "缺少内容".to_string(),
                "不支持的格式" => "不支持".to_string(),
                "文件读取失败" => "读取失败".to_string(),
                other => {
                    if other.chars().count() > 8 {
                        format!("{}…", other.chars().take(7).collect::<String>())
                    } else {
                        other.to_string()
                    }
                }
            };
            (short, theme.red, theme.red_bg, None)
        }
    }
}

/// 扩展名 → 徽章配色
pub fn ext_style(theme: &Theme, ext: &str) -> (COLORREF, COLORREF, Option<COLORREF>) {
    match ext {
        "docx" | "doc" | "docm" => (theme.blue, theme.blue_bg, Some(theme.blue_border)),
        "pdf" => (theme.red, theme.red_bg, None),
        "xlsx" | "xls" | "xlsm" | "xlsb" => (theme.green, theme.green_bg, None),
        "pptx" | "ppt" | "pptm" => (theme.amber, theme.amber_bg, None),
        _ => (theme.slate_600, theme.slate_100, None),
    }
}

/// 复选框
pub fn draw_checkbox(hdc: HDC, x: i32, center_y: i32, checked: bool, theme: &Theme) {
    unsafe {
        let s = dpi::px(15);
        let rect = RECT {
            left: x,
            top: center_y - s / 2,
            right: x + s,
            bottom: center_y + s / 2,
        };
        if checked {
            fill_round_rect(hdc, &rect, dpi::px(4), theme.blue);
            // 对勾
            let pen = CreatePen(PS_SOLID, dpi::px(2), rgb(0xFF, 0xFF, 0xFF));
            let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
            MoveToEx(hdc, x + dpi::px(3), center_y, None);
            LineTo(hdc, x + dpi::px(6), center_y + dpi::px(3));
            LineTo(hdc, x + dpi::px(12), center_y - dpi::px(3));
            let _ = SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        } else {
            fill_round_rect(hdc, &rect, dpi::px(4), theme.card);
            stroke_round_rect(hdc, &rect, dpi::px(4), theme.slate_300, 1);
        }
    }
}

/// 上传箭头图标（拖放区）
pub fn draw_upload_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(2), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let h = size / 2;
        // 竖线
        MoveToEx(hdc, cx, cy - h / 2, None);
        LineTo(hdc, cx, cy + h / 4);
        // 箭头
        MoveToEx(hdc, cx - h / 3, cy - h / 6, None);
        LineTo(hdc, cx, cy - h / 2);
        LineTo(hdc, cx + h / 3, cy - h / 6);
        // 底部托盘
        MoveToEx(hdc, cx - h / 2, cy + h / 2, None);
        LineTo(hdc, cx + h / 2, cy + h / 2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 文件夹图标（行的操作按钮）
pub fn draw_folder_icon(hdc: HDC, x: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let w = size;
        let h = (size as f64 * 0.8) as i32;
        let top = cy - h / 2;
        // 文件夹标签
        MoveToEx(hdc, x, top + dpi::px(3), None);
        LineTo(hdc, x, top);
        LineTo(hdc, x + w / 3, top);
        LineTo(hdc, x + w / 3 + dpi::px(2), top + dpi::px(3));
        LineTo(hdc, x + w, top + dpi::px(3));
        // 主体
        MoveToEx(hdc, x, top + dpi::px(3), None);
        LineTo(hdc, x, top + h);
        LineTo(hdc, x + w, top + h);
        LineTo(hdc, x + w, top + dpi::px(3));
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 闪电图标（开始转换按钮）
pub fn draw_bolt_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(2), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let s = size;
        MoveToEx(hdc, cx + s / 5, cy - s / 2, None);
        LineTo(hdc, cx - s / 3, cy + s / 6);
        LineTo(hdc, cx, cy + s / 6);
        LineTo(hdc, cx - s / 5, cy + s / 2);
        LineTo(hdc, cx + s / 3, cy - s / 6);
        LineTo(hdc, cx - s / 6, cy - s / 6);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 加号图标
pub fn draw_plus_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(2), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        MoveToEx(hdc, cx - size / 2, cy, None);
        LineTo(hdc, cx + size / 2, cy);
        MoveToEx(hdc, cx, cy - size / 2, None);
        LineTo(hdc, cx, cy + size / 2);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
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


/// 最小化图标（横线）
pub fn draw_min_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        MoveToEx(hdc, cx - size / 2, cy, None);
        LineTo(hdc, cx + size / 2, cy);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 最大化图标（方框）
pub fn draw_max_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let h = size / 2;
        MoveToEx(hdc, cx - h, cy - h, None);
        LineTo(hdc, cx + h, cy - h);
        LineTo(hdc, cx + h, cy + h);
        LineTo(hdc, cx - h, cy + h);
        LineTo(hdc, cx - h, cy - h);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 还原图标（两个错位的方框）
pub fn draw_restore_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let h = size / 2;
        let o = dpi::px(3);
        // 后层
        MoveToEx(hdc, cx - h + o, cy - h + o, None);
        LineTo(hdc, cx + h + o, cy - h + o);
        LineTo(hdc, cx + h + o, cy + h);
        // 前层
        MoveToEx(hdc, cx - h, cy - h + o, None);
        LineTo(hdc, cx - h, cy + h + o);
        LineTo(hdc, cx + h - o, cy + h + o);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 关闭图标（叉）
pub fn draw_close_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let h = size / 2;
        MoveToEx(hdc, cx - h, cy - h, None);
        LineTo(hdc, cx + h, cy + h);
        MoveToEx(hdc, cx + h, cy - h, None);
        LineTo(hdc, cx - h, cy + h);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}


/// 云朵上传图标（设计稿的 cloud-upload）
///
/// 用采样圆弧 + 折线拼出云轮廓（线性风格，和 lucide 一致），中间叠向上箭头。
pub fn draw_cloud_upload_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(2), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));

        let w = size as f64;
        let h = w * 0.60;
        let base_y = cy + (h * 0.5) as i32;
        let mid_y = cy - (h * 0.10) as i32;

        // 三段圆弧采样：(中心x, 中心y, 半径, 起始角, 结束角)
        let arcs: [(f64, f64, f64, f64, f64); 3] = [
            (cx as f64 - w * 0.30, mid_y as f64, h * 0.42, 100.0, 260.0),
            (cx as f64 - w * 0.04, mid_y as f64 - h * 0.22, h * 0.52, 190.0, 350.0),
            (cx as f64 + w * 0.26, mid_y as f64, h * 0.40, 280.0, 440.0),
        ];

        let mut pts: Vec<POINT> = Vec::new();
        for (ccx, ccy, r, a0, a1) in arcs {
            let steps = 10;
            for i in 0..=steps {
                let t = a0 + (a1 - a0) * (i as f64 / steps as f64);
                let rad = t * std::f64::consts::PI / 180.0;
                pts.push(POINT {
                    x: (ccx + r * rad.cos()) as i32,
                    y: (ccy + r * rad.sin()) as i32,
                });
            }
        }
        // 底边两个端点
        pts.push(POINT { x: cx + (w * 0.40) as i32, y: base_y });
        pts.push(POINT { x: cx - (w * 0.40) as i32, y: base_y });

        if !pts.is_empty() {
            Polyline(hdc, &pts);
        }

        // 中间向上箭头
        let ax = cx;
        let ay_top = mid_y - (h * 0.30) as i32;
        let ay_bot = base_y - dpi::px(2);
        MoveToEx(hdc, ax, ay_top, None);
        LineTo(hdc, ax, ay_bot);
        MoveToEx(hdc, ax - dpi::px(4), ay_top + dpi::px(5), None);
        LineTo(hdc, ax, ay_top);
        LineTo(hdc, ax + dpi::px(4), ay_top + dpi::px(5));

        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}
