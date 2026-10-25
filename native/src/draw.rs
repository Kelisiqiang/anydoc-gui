// 绘制基础：主题配色、圆角矩形、虚线框、徽章、复选框、图标
//
// 配色对齐设计稿（Tailwind slate / blue 色系）
// 所有尺寸为「逻辑像素」，实际绘制时按 DPI 缩放。

use std::sync::OnceLock;

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
    pub blue500: COLORREF,
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
            blue500: rgb(0x3B, 0x82, 0xF6),      // blue-500（进度条填充）
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
            blue500: rgb(0x60, 0xA5, 0xFA),
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
    pub brand: HFONT,     // 12 品牌条标题 (text-xs)
    pub mono: HFONT,      // 12 等宽（扩展名徽章 / 大小 / 路径）
    pub h1: HFONT,        // 20 大标题 (text-xl)
    pub sub: HFONT,       // 12 副标题 (text-xs)
    pub btn: HFONT,       // 12 按钮 (text-xs)
    pub btn_bold: HFONT,  // 12 按钮(粗)
    pub thead: HFONT,     // 11 表头 (text-[11px])
    pub small: HFONT,     // 11 小字（表头 / 「更改」）
    pub name: HFONT,      // 12 文件名 (text-xs)
    pub meta: HFONT,      // 12 次要信息 (text-xs)
    pub tag: HFONT,       // 10 徽章 (text-[10px])
    pub mono_tag: HFONT,  // 10 扩展名徽章 (mono text-[10px])
    pub mono_small: HFONT,// 11 空态格式标签 (mono text-[11px])
    pub empty_title: HFONT, // 14 空态主文案 (text-sm font-semibold)
    pub foot: HFONT,      // 12 底栏 (text-xs)
    pub big: HFONT,       // 14 大按钮
}

/// 创建等宽字体（设计稿里扩展名徽章/大小/路径用的就是等宽）
pub fn make_mono(px: i32, weight: i32) -> HFONT {
    unsafe {
        let face = mono_family().to_string();
        let face: Vec<u16> = face.encode_utf16().chain(std::iter::once(0)).collect();
        // 高度必须是负数：负数 = 字面高度（em），正数 = 行高（会按行高反推字号，
        // 中文雅黑的行高比字面高 1.3 倍左右，传正数会让字比设计稿小一圈）
        CreateFontW(
            -dpi::font_px(px), 0, 0, 0, weight, 0, 0, 0,
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
        let face = ui_family().to_string();
        let face: Vec<u16> = face.encode_utf16().chain(std::iter::once(0)).collect();
        // 负高度 = 字面高度，这样 12px 就是真的 12px
        CreateFontW(
            -dpi::font_px(px),
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

// ── 字体回退 ──
// 首选 Microsoft YaHei UI（Windows 自带）；若系统没有（如精简版 / 服务端），
// 依次回退到其它中文字体，避免中文变成方块。

fn family_exists(name: &str) -> bool {
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return false;
        }
        let face: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let hf = CreateFontW(
            12, 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        );
        let old = SelectObject(hdc, HGDIOBJ(hf.0));
        let mut buf = [0u16; 64];
        let n = GetTextFaceW(hdc, Some(&mut buf));
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(hf.0));
        let _ = ReleaseDC(None, hdc);
        let actual = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        actual.eq_ignore_ascii_case(name)
    }
}

/// 字体是否真的能画出中文（有些环境里字体名存在但没有汉字字形）
fn family_renders_cjk(name: &str) -> bool {
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return false;
        }
        let face: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let hf = CreateFontW(
            12, 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        );
        let old = SelectObject(hdc, HGDIOBJ(hf.0));
        let s: Vec<u16> = "文".encode_utf16().collect();
        let mut gi: [u16; 1] = [0xFFFF; 1];
        // GGI_MARK_NONEXISTING_GLYPHS = 1：没有的字形返回 0xFFFF
        let r = GetGlyphIndicesW(hdc, PCWSTR(s.as_ptr()), 1, gi.as_mut_ptr(), 1);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(hf.0));
        let _ = ReleaseDC(None, hdc);
        r != 0 && gi[0] != 0xFFFF && gi[0] != 0
    }
}

fn pick_family(candidates: &[&str]) -> String {
    // 先按名字找，再确认它真的有汉字字形
    for c in candidates {
        if family_exists(c) && family_renders_cjk(c) {
            return c.to_string();
        }
    }
    for c in candidates {
        if family_exists(c) {
            return c.to_string();
        }
    }
    candidates[0].to_string()
}

/// 界面字体族（只解析一次）
pub fn ui_family() -> &'static str {
    static F: OnceLock<&'static str> = OnceLock::new();
    F.get_or_init(|| {
        let s = pick_family(&[
            "Microsoft YaHei UI",
            "Microsoft YaHei",
            "Noto Sans CJK SC",
            "WenQuanYi Zen Hei",
            "Segoe UI",
        ]);
        Box::leak(s.into_boxed_str())
    })
}

/// 等宽字体族（只解析一次）
pub fn mono_family() -> &'static str {
    static F: OnceLock<&'static str> = OnceLock::new();
    F.get_or_init(|| {
        let s = pick_family(&[
            "Consolas",
            "Noto Sans Mono CJK SC",
            "WenQuanYi Zen Hei",
            "DejaVu Sans Mono",
            "Courier New",
        ]);
        Box::leak(s.into_boxed_str())
    })
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
/// 用 PS_DASH 虚线笔 + RoundRect，圆角由 GDI 正确处理，
/// 不会出现手绘直线段在转角处错乱的问题。
pub fn dashed_round_rect(hdc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_DASH, 1, color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let old2: HGDIOBJ = SelectObject(hdc, GetStockObject(NULL_BRUSH));
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
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        // 字体行高可能超过给定的高度（微软雅黑的行高本来就比字号大），
        // 直接画会被裁掉上下缘。这里按需把矩形上下撑开，保证字形完整。
        let mut need = RECT::default();
        let _ = DrawTextW(
            hdc,
            &mut buf,
            &mut need,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        let need_h = need.bottom - need.top;
        let (mut ry, mut rh) = (y, h);
        if need_h > h {
            let extra = (need_h - h) / 2;
            ry -= extra;
            rh += extra * 2;
        }
        let mut rc = RECT { left: x, top: ry, right: x + w, bottom: ry + rh };
        let flags = DT_SINGLELINE | DT_VCENTER | align
            | if clip { DT_END_ELLIPSIS } else { DRAW_TEXT_FORMAT(0) };
        let _ = DrawTextW(hdc, &mut buf, &mut rc, flags);
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

/// 徽章（返回宽度）
///
/// 设计稿里徽章统一是 `rounded`（4px 圆角）+ `px-1.5 py-0.5` 内边距，
/// 不是胶囊形，所以圆角单独作为参数传入。
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
        let pad = dpi::px(6);
        let w = sz.cx + pad * 2;
        let h = dpi::px(17);
        let rect = RECT {
            left: x,
            top: center_y - h / 2,
            right: x + w,
            bottom: center_y + h / 2,
        };
        let r = dpi::px(4);
        fill_round_rect(hdc, &rect, r, bg);
        if let Some(b) = border {
            stroke_round_rect(hdc, &rect, r, b, 1);
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
                "需 OCR（本地不含 OCR）" | "需 OCR（已跳过）" | "需 OCR（该格式不支持云端 OCR）" => "需 OCR".to_string(),
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

/// 文件夹图标（对齐 lucide folder 轮廓）
///
/// 参数是图标外接框的左上角 (x, top) 与边长 size；
/// 圆角用折线近似，14px 下与设计稿几乎一致。
pub fn draw_folder_icon(hdc: HDC, x: i32, top: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1).max(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let f = size as f64 / 24.0;
        let pts: [(i32, i32); 11] = [
            (3, 7), (3, 17), (5, 19), (19, 19), (21, 17),
            (21, 9), (19, 7), (13, 7), (11, 5), (5, 5), (3, 7),
        ];
        for (i, p) in pts.iter().enumerate() {
            let px_ = x + (p.0 as f64 * f).round() as i32;
            let py_ = top + (p.1 as f64 * f).round() as i32;
            if i == 0 {
                MoveToEx(hdc, px_, py_, None);
            } else {
                LineTo(hdc, px_, py_);
            }
        }
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 垃圾桶图标（对齐 lucide trash-2）
pub fn draw_trash_icon(hdc: HDC, x: i32, top: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1).max(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let f = size as f64 / 24.0;
        let P = |a: i32, b: i32| (x + (a as f64 * f).round() as i32, top + (b as f64 * f).round() as i32);
        // 盖子
        let (l1, l2) = P(4, 7);
        let (r1, r2) = P(20, 7);
        MoveToEx(hdc, l1, l2, None);
        LineTo(hdc, r1, r2);
        // 提手
        let (h1, h2) = P(9, 7);
        let (h3, h4) = P(9, 4);
        let (h5, h6) = P(15, 4);
        let (h7, h8) = P(15, 7);
        MoveToEx(hdc, h1, h2, None);
        LineTo(hdc, h3, h4);
        LineTo(hdc, h5, h6);
        LineTo(hdc, h7, h8);
        // 桶身
        let (b1, b2) = P(6, 7);
        let (b3, b4) = P(7, 19);
        let (b5, b6) = P(17, 19);
        let (b7, b8) = P(18, 7);
        MoveToEx(hdc, b1, b2, None);
        LineTo(hdc, b3, b4);
        LineTo(hdc, b5, b6);
        LineTo(hdc, b7, b8);
        // 内部竖线
        let (v1, v2) = P(10, 11);
        let (v3, v4) = P(10, 17);
        MoveToEx(hdc, v1, v2, None);
        LineTo(hdc, v3, v4);
        let (w1, w2) = P(14, 11);
        let (w3, w4) = P(14, 17);
        MoveToEx(hdc, w1, w2, None);
        LineTo(hdc, w3, w4);
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 对勾图标（对齐 lucide check）
pub fn draw_check_icon(hdc: HDC, x: i32, top: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(2).max(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let f = size as f64 / 24.0;
        let pts: [(i32, i32); 3] = [(5, 13), (9, 17), (19, 7)];
        for (i, p) in pts.iter().enumerate() {
            let px_ = x + (p.0 as f64 * f).round() as i32;
            let py_ = top + (p.1 as f64 * f).round() as i32;
            if i == 0 {
                MoveToEx(hdc, px_, py_, None);
            } else {
                LineTo(hdc, px_, py_);
            }
        }
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// 叉号图标（行的删除按钮）
///
/// 不用「✕」字符——某些字体没有这个字形，会变成方块。
pub fn draw_x_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let pen = CreatePen(PS_SOLID, dpi::px(1).max(1), color);
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

/// 闪电图标（实心，对齐 lucide zap）
///
/// 设计稿的「开始转换」按钮左侧是实心闪电，这里用多边形填充。
pub fn draw_zap_icon(hdc: HDC, x: i32, top: i32, size: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let old_b: HGDIOBJ = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_p: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let f = size as f64 / 24.0;
        let pts: [(i32, i32); 6] = [(13, 2), (3, 14), (12, 14), (11, 22), (21, 10), (12, 10)];
        let mut poly: Vec<POINT> = Vec::with_capacity(pts.len());
        for p in pts.iter() {
            poly.push(POINT {
                x: x + (p.0 as f64 * f).round() as i32,
                y: top + (p.1 as f64 * f).round() as i32,
            });
        }
        let _ = Polygon(hdc, &poly);
        let _ = SelectObject(hdc, old_p);
        let _ = SelectObject(hdc, old_b);
        let _ = DeleteObject(HGDIOBJ(pen.0));
        let _ = DeleteObject(HGDIOBJ(brush.0));
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
        let o = (size / 3).max(2);
        // 后层方框（左上）：上边 + 左边 + 右边（到前层为止）
        MoveToEx(hdc, cx - h, cy - h + o, None);
        LineTo(hdc, cx - h, cy - h);
        LineTo(hdc, cx + h - o, cy - h);
        LineTo(hdc, cx + h - o, cy + h - o);
        // 前层方框（右下）：左边 + 下边 + 右边
        MoveToEx(hdc, cx - h + o, cy - h + o, None);
        LineTo(hdc, cx - h + o, cy + h);
        LineTo(hdc, cx + h, cy + h);
        LineTo(hdc, cx + h, cy - h + o);
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


/// 云朵上传图标（对齐 lucide cloud-upload）
///
/// 24×24 网格：云轮廓 + 向上的箭头（箭杆穿过云底，和 lucide 一致）。
/// 之前那版是「半圆 + 平底」，缩到 24px 就糊成一坨，这里改成完整云形。
/// SVG 椭圆弧 → 折线点列
///
/// 设计稿里的图标是 SVG（含圆弧），GDI 没有直接画 SVG 弧的接口，
/// 这里按 SVG 的「端点参数化 → 中心参数化」把弧拆成折线，还原设计稿的形状。
/// 本项目用到的弧都是 rotation=0 的，所以不考虑旋转。
fn arc_points(
    x1: f64,
    y1: f64,
    rx: f64,
    ry: f64,
    large: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        return out;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-9 || ry < 1e-9 {
        return out;
    }
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    // 半径太小就按规范放大
    let lambda = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * dy * dy - ry * ry * dx * dx;
    let den = rx * rx * dy * dy + ry * ry * dx * dx;
    let mut coef = if den <= 1e-12 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * dy / ry;
    let cyp = -coef * ry * dx / rx;
    let cx = cxp + (x1 + x2) / 2.0;
    let cy = cyp + (y1 + y2) / 2.0;
    let ux = (dx - cxp) / rx;
    let uy = (dy - cyp) / ry;
    let vx = (-dx - cxp) / rx;
    let vy = (-dy - cyp) / ry;
    // 注意：Rust 的 f64::atan2(self, other) 是 atan2(y=self, x=other)，
    // 所以这里要写 uy.atan2(ux)，别写成 ux.atan2(uy)（那样弧会整个画歪）
    let theta1 = uy.atan2(ux);
    let mut dtheta = vy.atan2(vx) - theta1;
    const TAU: f64 = std::f64::consts::PI * 2.0;
    if !sweep && dtheta > 0.0 {
        dtheta -= TAU;
    }
    if sweep && dtheta < 0.0 {
        dtheta += TAU;
    }
    let steps = 24;
    for i in 0..=steps {
        let t = theta1 + dtheta * (i as f64) / (steps as f64);
        out.push((cx + rx * t.cos(), cy + ry * t.sin()));
    }
    out
}

/// 云朵上传图标（tabler cloud-upload 新版）
///
/// 路径：
///   M7 18 a4.6 4.4 0 0 1 0 -9 a5 4.5 0 0 1 11 2 h1 a3.5 3.5 0 0 1 0 7 h-1
///   M9 15 l3 -3 l3 3
///   M12 12 l0 9
/// 云是一段近闭合的饱满轮廓（底部有平底线），上传箭头整个在云里、
/// 杆子从云下穿出 —— 比旧版（云是三段大小不一的鼓包、箭头杆直伸到底）更稳。
pub fn draw_cloud_upload_icon(hdc: HDC, cx: i32, cy: i32, size: i32, color: COLORREF) {
    unsafe {
        let f = size as f64 / 24.0;
        let pen = CreatePen(PS_SOLID, ((size as f64 * 2.0 / 24.0).round() as i32).max(1), color);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(pen.0));
        let p = |x: f64, y: f64| -> POINT {
            POINT {
                x: cx + ((x - 12.0) * f).round() as i32,
                y: cy + ((y - 12.0) * f).round() as i32,
            }
        };

        // ── 云轮廓：左弧 → 顶弧 → 一小段横线 → 右弧 → 一小段横线 ──
        let mut cloud: Vec<(f64, f64)> = Vec::new();
        // M7 18 a4.6 4.4 0 0 1 0 -9   → (7, 9)
        cloud.extend(arc_points(7.0, 18.0, 4.6, 4.4, false, true, 7.0, 9.0));
        // a5 4.5 0 0 1 11 2            → (18, 11)
        cloud.extend(arc_points(7.0, 9.0, 5.0, 4.5, false, true, 18.0, 11.0));
        // h1                          → (19, 11)
        cloud.push((19.0, 11.0));
        // a3.5 3.5 0 0 1 0 7           → (19, 18)
        cloud.extend(arc_points(19.0, 11.0, 3.5, 3.5, false, true, 19.0, 18.0));
        // h-1                         → (18, 18)
        cloud.push((18.0, 18.0));

        for (i, (x, y)) in cloud.iter().enumerate() {
            let pt = p(*x, *y);
            if i == 0 {
                MoveToEx(hdc, pt.x, pt.y, None);
            } else {
                LineTo(hdc, pt.x, pt.y);
            }
        }

        // ── 上传箭头：M9 15 l3 -3 l3 3 ／ M12 12 l0 9 ──
        let head = [p(9.0, 15.0), p(12.0, 12.0), p(15.0, 15.0)];
        MoveToEx(hdc, head[0].x, head[0].y, None);
        LineTo(hdc, head[1].x, head[1].y);
        LineTo(hdc, head[2].x, head[2].y);
        let shaft_top = p(12.0, 12.0);
        let shaft_bot = p(12.0, 21.0);
        MoveToEx(hdc, shaft_top.x, shaft_top.y, None);
        LineTo(hdc, shaft_bot.x, shaft_bot.y);

        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}
