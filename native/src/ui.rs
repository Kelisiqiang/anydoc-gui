// 主窗口：控件创建、布局、消息处理、自绘列表
//
// 布局用绝对坐标，所有尺寸经 dpi::px() 缩放。
// 绘制走双缓冲（WM_PAINT 里手动创建兼容 DC）。

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::dpi;
use crate::draw::*;
use crate::icons;
use crate::state::{AppState, Status, Task, WorkerMsg};
use crate::worker;

// ── 控件 ID ──
const ID_BTN_FILES: i32 = 101;
const ID_BTN_DIR: i32 = 102;
const ID_BTN_CLEAR: i32 = 103;
const ID_BTN_CONVERT: i32 = 104;
const ID_BTN_OUT: i32 = 105;
const ID_PROGRESS: i32 = 106;
const ID_STATIC_TITLE: i32 = 107;
const ID_STATIC_SUB: i32 = 108;
const ID_STATIC_STATUS: i32 = 109;
const ID_STATIC_PCT: i32 = 110;

// ── 逻辑布局（未缩放）──
const PAD: i32 = 22;
const DROP_Y: i32 = 86;
const DROP_H: i32 = 58;
const BTN_Y: i32 = 160;
const BTN_H: i32 = 32;
const ROW_H: i32 = 48;
const FOOT_H: i32 = 64;

pub struct Ui {
    pub state: AppState,
    pub theme: Theme,
    pub hwnd: HWND,
    pub hlist: HWND,
    pub hprogress: HWND,
    pub fonts: Fonts,
    pub icons: RefCell<Vec<Option<icons::IconEntry>>>,
    pub rx: RefCell<Option<std::sync::mpsc::Receiver<WorkerMsg>>>,
    pub dragging: RefCell<bool>,
    /// 列表滚动偏移（第一个可见行的索引）
    pub scroll: i32,
    /// 选中的行索引
    pub selected: Option<usize>,
}

pub struct Fonts {
    pub title: HFONT,
    pub sub: HFONT,
    pub name: HFONT,
    pub small: HFONT,
    pub badge: HFONT,
    pub btn: HFONT,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
}

/// 注册窗口类
pub fn register_class() {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let _ = RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: w!("AnyDocNative"),
            ..Default::default()
        });
    }
}

/// 创建主窗口
pub fn create(hinst: HINSTANCE) -> HWND {
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("AnyDocNative"),
            w!("AnyDoc - 文档转 Markdown"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            dpi::px(980),
            dpi::px(680),
            None,
            None,
            hinst,
            None,
        )
        .unwrap_or_default();

        // Win11 圆角
        let pref = 2i32; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const std::ffi::c_void,
            4,
        );

        hwnd
    }
}

// ── 消息处理 ──
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let _ = on_create(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            let steps = (-delta / WHEEL_DELTA as i16).max(-3).min(3) as i32;
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    let max_scroll = (u.state.tasks.len() as i32 - visible_rows(hwnd)).max(0);
                    u.scroll = (u.scroll + steps).max(0).min(max_scroll);
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            on_click(hwnd, lp);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            on_key(hwnd, wp.0 as u32);
            LRESULT(0)
        }
        WM_DROPFILES => {
            on_drop(hwnd, lp);
            LRESULT(0)
        }
        WM_COMMAND => {
            on_command(hwnd, (wp.0 & 0xFFFF) as i32);
            LRESULT(0)
        }
        WM_TIMER => {
            if (wp.0 as usize) == TIMER_TICK {
                tick(hwnd);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // 跨屏移动：重建字体 + 重新布局
            let _ = dpi::update_scale(hwnd);
            rebuild_fonts(hwnd);
            layout(hwnd);
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            let _ = refresh_theme(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_TICK);
            let _ = PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

const TIMER_TICK: usize = 1;

// ── 创建控件 ──
unsafe fn on_create(hwnd: HWND) -> windows::core::Result<()> {
    // 关键：必须在创建字体/计算布局之前确定 DPI
    let _ = dpi::update_scale(hwnd);

    let hinst: HINSTANCE = GetModuleHandleW(None)?.into();

    // 常用控件
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES | ICC_PROGRESS_CLASS | ICC_BAR_CLASSES,
    };
    let _ = InitCommonControlsEx(&icc);

    // 拖放需要 OLE
    let _ = OleInitialize(None);

    // 静态文本
    for (id, text) in [
        (ID_STATIC_TITLE, "文档转 Markdown"),
        (
            ID_STATIC_SUB,
            "把 Word、PowerPoint、Excel、PDF 等文档转成 LLM 能直接读的干净 Markdown",
        ),
        (ID_STATIC_STATUS, "就绪"),
        (ID_STATIC_PCT, "0%"),
    ] {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            &windows::core::HSTRING::from(text),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(id as isize as *mut std::ffi::c_void),
            hinst,
            None,
        );
    }

    // 任务列表完全自绘（不用 ListBox 控件 —— 它会盖住自绘内容）
    let hlist = HWND(std::ptr::null_mut());

    // 进度条
    let hprogress = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("msctls_progress32"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(PBS_SMOOTH),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_PROGRESS as isize as *mut std::ffi::c_void),
        hinst,
        None,
    )?;

    // 按钮
    for (id, label) in [
        (ID_BTN_FILES, "选择文件"),
        (ID_BTN_DIR, "选择文件夹"),
        (ID_BTN_CLEAR, "清空"),
        (ID_BTN_CONVERT, "开始转换"),
        (ID_BTN_OUT, "输出目录"),
    ] {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            &windows::core::HSTRING::from(label),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(id as isize as *mut std::ffi::c_void),
            hinst,
            None,
        );
    }

    // 初始化
    let ui = Rc::new(RefCell::new(Ui {
        state: AppState::new(),
        theme: Theme::light(),
        hwnd,
        hlist,
        hprogress,
        fonts: make_fonts(),
        icons: RefCell::new(Vec::new()),
        rx: RefCell::new(None),
        dragging: RefCell::new(false),
        scroll: 0,
        selected: None,
    }));
    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));

    apply_fonts(hwnd);
    layout(hwnd);

    // 强制重绘（hbrBackground 为 NULL，Windows 不会自动发 WM_PAINT）
    let _ = RedrawWindow(hwnd, None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_ERASE);

    let _ = SetTimer(hwnd, TIMER_TICK, 40, None);
    Ok(())
}

fn make_fonts() -> Fonts {
    Fonts {
        title: make_font(19, 700),
        sub: make_font(13, 400),
        name: make_font(14, 500),
        small: make_font(12, 400),
        badge: make_font(12, 600),
        btn: make_font(13, 400),
    }
}

/// 重建字体（DPI 变化时）
unsafe fn rebuild_fonts(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        {
            let mut u = ui.borrow_mut();
            let f = make_fonts();
            u.fonts.title = f.title;
            u.fonts.sub = f.sub;
            u.fonts.name = f.name;
            u.fonts.small = f.small;
            u.fonts.badge = f.badge;
            u.fonts.btn = f.btn;
        }
    });
    apply_fonts(hwnd);
}

unsafe fn apply_fonts(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let u = ui.borrow();
        let set = |id: i32, font: HFONT| {
            if let Ok(c) = GetDlgItem(hwnd, id) {
                SendMessageW(c, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            }
        };
        set(ID_STATIC_TITLE, u.fonts.title);
        set(ID_STATIC_SUB, u.fonts.sub);
        set(ID_STATIC_STATUS, u.fonts.small);
        set(ID_STATIC_PCT, u.fonts.badge);
        for id in [
            ID_BTN_FILES,
            ID_BTN_DIR,
            ID_BTN_CLEAR,
            ID_BTN_CONVERT,
            ID_BTN_OUT,
        ] {
            set(id, u.fonts.btn);
        }
    });
}

// ── 布局 ──
pub unsafe fn layout(hwnd: HWND) {
    let mut cr = RECT::default();
    let _ = GetClientRect(hwnd, &mut cr);
    let cw = cr.right - cr.left;
    let ch = cr.bottom - cr.top;

    let pad = dpi::px(PAD);
    let move_ctl = |id: i32, x: i32, y: i32, w: i32, h: i32| {
        if let Ok(c) = GetDlgItem(hwnd, id) {
            let _ = SetWindowPos(c, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    };

    // 标题区
    move_ctl(ID_STATIC_TITLE, pad, dpi::px(20), dpi::px(520), dpi::px(26));
    move_ctl(ID_STATIC_SUB, pad, dpi::px(50), dpi::px(700), dpi::px(20));

    // 按钮行
    let btn_y = dpi::px(BTN_Y);
    let bw = dpi::px(96);
    let bh = dpi::px(BTN_H);
    let mut bx = pad;
    move_ctl(ID_BTN_FILES, bx, btn_y, bw, bh);
    bx += bw + dpi::px(8);
    move_ctl(ID_BTN_DIR, bx, btn_y, dpi::px(108), bh);

    // 右侧按钮
    let mut rx = cw - pad;
    rx -= dpi::px(104);
    move_ctl(ID_BTN_CONVERT, rx, btn_y, dpi::px(104), bh);
    rx -= dpi::px(74);
    move_ctl(ID_BTN_CLEAR, rx, btn_y, dpi::px(66), bh);
    rx -= dpi::px(100);
    move_ctl(ID_BTN_OUT, rx, btn_y, dpi::px(92), bh);

    // 列表
    let list_y = btn_y + bh + dpi::px(12);
    let foot_h = dpi::px(FOOT_H);
    let list_h = (ch - list_y - foot_h).max(dpi::px(120));

    // 底部
    let foot_y = ch - foot_h;
    move_ctl(ID_PROGRESS, pad, foot_y + dpi::px(14), cw - pad * 2, dpi::px(5));
    move_ctl(
        ID_STATIC_STATUS,
        pad,
        foot_y + dpi::px(28),
        cw - pad * 2 - dpi::px(64),
        dpi::px(24),
    );
    move_ctl(
        ID_STATIC_PCT,
        cw - pad - dpi::px(60),
        foot_y + dpi::px(28),
        dpi::px(60),
        dpi::px(24),
    );
}

// ── 绘制 ──
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut cr = RECT::default();
    let _ = GetClientRect(hwnd, &mut cr);
    let cw = cr.right - cr.left;
    let ch = cr.bottom - cr.top;

    // 背景
    let (bg_color, line_color) = UI.with(|slot| {
        let slot = slot.borrow();
        match slot.as_ref() {
            Some(u) => {
                let u = u.borrow();
                (u.theme.bg, u.theme.line)
            }
            None => (rgb(0xF3, 0xF3, 0xF6), rgb(0xE4, 0xE7, 0xEC)),
        }
    });
    let bg = CreateSolidBrush(bg_color);
    FillRect(
        hdc,
        &RECT { left: 0, top: 0, right: cw, bottom: ch },
        bg,
    );
    let _ = DeleteObject(HGDIOBJ(bg.0));

    // 投放区
    {
        let drop = UI.with(|slot| {
            let slot = slot.borrow();
            match slot.as_ref() {
                Some(u) => {
                    let u = u.borrow();
                    let hot = *u.dragging.borrow();
                    let theme = u.theme.clone();
                    Some((theme, hot, u.fonts.name, u.fonts.small))
                }
                None => None,
            }
        });
        if let Some((theme, hot, f_name, f_small)) = drop {
            draw_dropzone_parts(
                hdc, &theme, hot, f_name, f_small,
                dpi::px(PAD), dpi::px(DROP_Y),
                cw - dpi::px(PAD) * 2, dpi::px(DROP_H),
            );
        }
    }

    // 任务列表（完全自绘）
    paint_list(hwnd, hdc);

    // 底部状态栏分隔线
    let foot_y = ch - dpi::px(FOOT_H);
    let sb = CreateSolidBrush(line_color);
    FillRect(hdc, &RECT { left: 0, top: foot_y, right: cw, bottom: foot_y + 1 }, sb);
    let _ = DeleteObject(HGDIOBJ(sb.0));

    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_dropzone_parts(
    hdc: HDC,
    th: &Theme,
    hot: bool,
    name_font: HFONT,
    small_font: HFONT,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let (border, bg) = if hot { (th.accent, th.run_bg) } else { (th.line, th.drop_bg) };
    fill_round_rect(hdc, &rect, dpi::px(10), bg);
    stroke_round_rect(hdc, &rect, dpi::px(10), border, 1);

    let ix = x + dpi::px(22);
    let iy = y + h / 2;
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(name_font.0));
    draw_text(hdc, "↓", ix, iy - dpi::px(14), dpi::px(24), dpi::px(28),
        if hot { th.accent } else { th.text_mute },
        DRAW_TEXT_FORMAT(DT_CENTER.0), false);
    let _ = SelectObject(hdc, old);

    let tx = ix + dpi::px(32);
    let tw = w - (tx - x) - dpi::px(16);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(name_font.0));
    let main = if hot { "松开即可添加文件".to_string() }
               else { "拖入文档到此处，或 点击「选择文件」/「选择文件夹」".to_string() };
    draw_text(hdc, &main, tx, y + dpi::px(12), tw, dpi::px(22), th.text,
        DRAW_TEXT_FORMAT(DT_LEFT.0), true);
    let _ = SelectObject(hdc, old);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(small_font.0));
    let sub = "支持 docx · pptx · xlsx · pdf · epub · odt · ods · odp · rtf · csv";
    draw_text(hdc, sub, tx, y + dpi::px(34), tw, dpi::px(18), th.text_mute,
        DRAW_TEXT_FORMAT(DT_LEFT.0), true);
    let _ = SelectObject(hdc, old);
}

// ── 自绘列表 ──
/// 计算列表区域矩形
fn list_rect(hwnd: HWND) -> RECT {
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        let cw = cr.right - cr.left;
        let ch = cr.bottom - cr.top;
        let list_y = dpi::px(BTN_Y) + dpi::px(BTN_H) + dpi::px(12);
        let foot_h = dpi::px(FOOT_H);
        RECT {
            left: dpi::px(PAD),
            top: list_y,
            right: cw - dpi::px(PAD),
            bottom: (ch - foot_h).max(list_y + dpi::px(80)),
        }
    }
}

/// 可见行数
fn visible_rows(hwnd: HWND) -> i32 {
    let r = list_rect(hwnd);
    ((r.bottom - r.top) / dpi::px(ROW_H)).max(1)
}

/// 绘制整个任务列表（完全自绘）
unsafe fn paint_list(hwnd: HWND, hdc: HDC) {
    let rect = list_rect(hwnd);
    let th: Option<Theme> = UI.with(|slot| {
        let slot = slot.borrow();
        match slot.as_ref() {
            Some(ui) => Some(ui.borrow().theme.clone()),
            None => None,
        }
    });
    if th.is_none() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/workspace/anydoc-diag.log") {
            let _ = writeln!(f, "paint_list: theme is None!");
        }
        return;
    }
    let Some(th) = th else { return };

    // 背景 + 边框
    let bg = CreateSolidBrush(th.card);
    FillRect(hdc, &rect, bg);
    let _ = DeleteObject(HGDIOBJ(bg.0));
    stroke_round_rect(hdc, &rect, dpi::px(10), th.line, 1);

    let (tasks, scroll, selected) = UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return (Vec::new(), 0, None) };
        let u = ui.borrow();
        (u.state.tasks.clone(), u.scroll, u.selected)
    });


    if tasks.is_empty() {
        // 空状态
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(UI.with(|s| { let s=s.borrow(); s.as_ref().map(|u| u.borrow().fonts.name.0).unwrap_or(std::ptr::null_mut()) }).into()));
        let msg = "把文档拖进来，或点击上方按钮选择文件";
        let sz = measure_text(hdc, msg);
        draw_text(
            hdc,
            msg,
            rect.left + (rect.right - rect.left - sz.cx) / 2,
            rect.top + (rect.bottom - rect.top) / 2 - dpi::px(10),
            sz.cx + 2,
            dpi::px(20),
            th.text_mute,
            DRAW_TEXT_FORMAT(DT_LEFT.0),
            false,
        );
        let _ = SelectObject(hdc, old);
        return;
    }

    // 注：不使用 IntersectClipRect —— 调用失败会把裁剪区置空，导致所有绘制不可见
    // 行本身都画在列表矩形内，不会溢出

    let row_h = dpi::px(ROW_H);
    let vis = visible_rows(hwnd);

    // 列位置（固定，不随内容变化）
    let icon_x = rect.left + dpi::px(16);
    let icon_sz = dpi::px(26);
    let text_left = icon_x + icon_sz + dpi::px(12);
    let badge_w = dpi::px(104);
    let size_w = dpi::px(78);
    let name_w = (rect.right - text_left - badge_w - size_w - dpi::px(30)).max(dpi::px(80));
    let size_x = rect.right - badge_w - size_w - dpi::px(12);
    let badge_x = rect.right - dpi::px(16);

    let first = scroll.max(0) as usize;
    let last = (first + vis as usize + 1).min(tasks.len());

    for (idx, task) in tasks.iter().enumerate().take(last).skip(first) {
        let top = rect.top + (idx as i32 - scroll) * row_h;
        if top + row_h < rect.top || top > rect.bottom {
            continue;
        }
        let row_rect = RECT {
            left: rect.left,
            top,
            right: rect.right,
            bottom: top + row_h,
        };

        // 行背景
        let row_bg = if Some(idx) == selected {
            th.run_bg
        } else {
            match task.status {
                Status::Error(_) => th.err_bg,
                _ => th.card,
            }
        };
        let brush = CreateSolidBrush(row_bg);
        FillRect(hdc, &row_rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        // 左侧状态色条
        let bar = CreateSolidBrush(match task.status {
            Status::Done(_) => th.ok,
            Status::Error(_) => th.err,
            Status::Running => th.accent,
            Status::Wait => th.line,
        });
        FillRect(
            hdc,
            &RECT {
                left: row_rect.left + 1,
                top: row_rect.top,
                right: row_rect.left + dpi::px(4),
                bottom: row_rect.bottom,
            },
            bar,
        );
        let _ = DeleteObject(HGDIOBJ(bar.0));

        // 图标
        let icon_y = top + (row_h - icon_sz) / 2;
        let mut icon_drawn = false;
        let icon_entry: Option<Option<icons::IconEntry>> = UI.with(|s| {
            let s = s.borrow();
            match s.as_ref() {
                Some(u) => {
                    let u = u.borrow();
                    let icons = u.icons.borrow();
                    Some(icons.get(idx).cloned())
                }
                None => None,
            }
        }).flatten();
        if let Some(Some(icons)) = icon_entry {
            if let Some(hicon) = icons.hicon {
                let _ = DrawIconEx(hdc, icon_x, icon_y, hicon, icon_sz, icon_sz, 0, None, DI_NORMAL);
                icon_drawn = true;
            }
        }
        if !icon_drawn {
            draw_icon_placeholder(hdc, icon_x, icon_y, icon_sz, th.wait_bg);
        }

        let (name_font, small_font, badge_font) = UI.with(|s| {
            let s = s.borrow();
            match s.as_ref() {
                Some(u) => {
                    let u = u.borrow();
                    (u.fonts.name, u.fonts.small, u.fonts.badge)
                }
                None => {
                    let f = make_font(14, 400);
                    (f, f, f)
                }
            }
        });

        // 文件名
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(name_font.0));
        draw_text(
            hdc, &task.name,
            text_left, top + dpi::px(7),
            name_w, dpi::px(20),
            th.text, DRAW_TEXT_FORMAT(DT_LEFT.0), true,
        );
        let _ = SelectObject(hdc, old);

        // 副标题
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(small_font.0));
        let sub = match &task.status {
            Status::Done(_) => format!("已转换 · {}", crate::state::fmt_size(task.out_size)),
            Status::Error(e) => e.clone(),
            Status::Running => "转换中…".to_string(),
            Status::Wait => format!("{} · 待转换", task.ext.to_uppercase()),
        };
        let sub_color = match task.status {
            Status::Done(_) => th.ok,
            Status::Error(_) => th.err,
            _ => th.text_mute,
        };
        draw_text(
            hdc, &sub,
            text_left, top + dpi::px(28),
            name_w, dpi::px(18),
            sub_color, DRAW_TEXT_FORMAT(DT_LEFT.0), true,
        );

        // 大小
        draw_text(
            hdc, &crate::state::fmt_size(task.size),
            size_x, top + (row_h - dpi::px(18)) / 2,
            size_w, dpi::px(18),
            th.text_mute, DRAW_TEXT_FORMAT(DT_RIGHT.0), true,
        );
        let _ = SelectObject(hdc, old);

        // 状态徽章
        let (text, fg, bgc) = status_style(&th, &task.status);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(badge_font.0));
        draw_badge(
            hdc, &text,
            badge_x, top + row_h / 2,
            fg, bgc, badge_font,
        );
        let _ = SelectObject(hdc, old);

        // 分隔线
        hline(hdc, row_rect.left, row_rect.right, row_rect.bottom - 1, th.line_soft);

    }

    // 滚动条
    if tasks.len() > vis as usize {
        let track_x = rect.right - dpi::px(8);
        let track_h = rect.bottom - rect.top - dpi::px(8);
        let thumb_h = ((vis as f64 / tasks.len() as f64) * track_h as f64).max(dpi::px(30) as f64) as i32;
        let max_scroll = (tasks.len() as i32 - vis).max(1);
        let thumb_y = rect.top + dpi::px(4)
            + ((scroll as f64 / max_scroll as f64) * (track_h - thumb_h) as f64) as i32;
        let sb = CreateSolidBrush(th.line);
        FillRect(
            hdc,
            &RECT {
                left: track_x,
                top: rect.top + dpi::px(4),
                right: track_x + dpi::px(4),
                bottom: rect.top + dpi::px(4) + track_h,
            },
            sb,
        );
        let _ = DeleteObject(HGDIOBJ(sb.0));
        let thb = CreateSolidBrush(th.text_mute);
        FillRect(
            hdc,
            &RECT {
                left: track_x,
                top: thumb_y,
                right: track_x + dpi::px(4),
                bottom: thumb_y + thumb_h,
            },
            thb,
        );
        let _ = DeleteObject(HGDIOBJ(thb.0));
    }

}

// ── 交互 ──
unsafe fn on_command(hwnd: HWND, id: i32) {
    match id {
        ID_BTN_FILES => {
            if let Some(paths) = crate::dialog::pick_files(hwnd) {
                add_paths(hwnd, &paths, "");
            }
        }
        ID_BTN_DIR => {
            if let Some(dir) = crate::dialog::pick_folder(hwnd) {
                add_folder(hwnd, &dir);
            }
        }
        ID_BTN_CLEAR => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    u.state.clear();
                    u.icons.borrow_mut().clear();
                }
            });
            refresh_list(hwnd);
            update_status(hwnd);
        }
        ID_BTN_CONVERT => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    if u.state.running || u.state.tasks.is_empty() {
                        return;
                    }
                    for t in u.state.tasks.iter_mut() {
                        t.status = Status::Wait;
                        t.out_size = 0;
                    }
                    u.state.running = true;
                    u.state.stats = Default::default();
                    u.state.stats.total = u.state.tasks.len();
                    let tasks = u.state.tasks.clone();
                    let out = u.state.out_dir.clone();
                    let rx = worker::spawn(tasks, out);
                    *u.rx.borrow_mut() = Some(rx);
                }
            });
            refresh_list(hwnd);
            update_status(hwnd);
        }
        ID_BTN_OUT => {
            if let Some(dir) = crate::dialog::pick_folder(hwnd) {
                UI.with(|slot| {
                    let slot = slot.borrow();
                    if let Some(ui) = slot.as_ref() {
                        let mut u = ui.borrow_mut();
                        u.state.out_dir = Some(dir.clone());
                        u.state.out_confirmed = true;
                        u.state.status = format!("输出到 {}", dir.display());
                    }
                });
                update_status(hwnd);
            }
        }
        _ => {}
    }
}

unsafe fn on_drop(hwnd: HWND, lp: LPARAM) {
    let hd = HDROP(lp.0 as *mut std::ffi::c_void);
    let count = DragQueryFileW(hd, 0xFFFFFFFF, None);
    let mut paths = Vec::new();
    for i in 0..count {
        let len = DragQueryFileW(hd, i, None) as usize;
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len + 1];
        if DragQueryFileW(hd, i, Some(&mut buf)) > 0 {
            let s = String::from_utf16_lossy(&buf);
            paths.push(PathBuf::from(s.trim_end_matches('\0')));
        }
    }
    let _ = DragFinish(hd);

    for p in paths {
        if p.is_dir() {
            add_folder(hwnd, &p);
        } else {
            add_paths(hwnd, &[p], "");
        }
    }
    refresh_list(hwnd);
    update_status(hwnd);
}

unsafe fn add_paths(hwnd: HWND, paths: &[PathBuf], rel: &str) {
    let mut added = 0;
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            for p in paths {
                if u.state.add_file(p.clone(), rel.to_string()) {
                    added += 1;
                }
            }
        }
    });
    if added > 0 {
        load_icons();
        refresh_list(hwnd);
    }
}

unsafe fn add_folder(hwnd: HWND, dir: &PathBuf) {
    let mut added = 0;
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            let base = dir.clone();
            for entry in walkdir::WalkDir::new(&base)
                .max_depth(16)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path().to_path_buf();
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !crate::state::is_supported(&ext) {
                    continue;
                }
                let rel = path
                    .parent()
                    .and_then(|p| p.strip_prefix(&base).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let rel = if rel.is_empty() {
                    String::new()
                } else {
                    format!("{}/", rel)
                };
                if u.state.add_file(path, rel) {
                    added += 1;
                }
            }
        }
    });
    if added > 0 {
        load_icons();
        refresh_list(hwnd);
    }
}

fn load_icons() {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let tasks: Vec<Task> = ui.borrow().state.tasks.clone();
        let mut guard = ui.borrow_mut();
        // 释放旧图标
        for ic in guard.icons.borrow().iter().flatten() {
            icons::destroy(ic);
        }
        let mut icons = guard.icons.borrow_mut();
        icons.clear();
        for t in tasks {
            icons.push(icons::load(&t.path, &t.ext));
        }
    });
}

pub unsafe fn refresh_list(hwnd: HWND) {
    // 自绘列表：只需重绘
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            // 滚动位置回正（列表变短时）
            let vis = visible_rows(hwnd);
            let max_scroll = (u.state.tasks.len() as i32 - vis).max(0);
            if u.scroll > max_scroll {
                u.scroll = max_scroll;
            }
            if u.selected.map(|s| s >= u.state.tasks.len()).unwrap_or(false) {
                u.selected = None;
            }
        }
    });
    let _ = InvalidateRect(hwnd, None, true);
}

/// 鼠标点击：选中行
unsafe fn on_click(hwnd: HWND, lp: LPARAM) {
    let x = (lp.0 & 0xFFFF) as i16 as i32;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
    let rect = list_rect(hwnd);

    if x < rect.left || x > rect.right || y < rect.top || y > rect.bottom {
        return;
    }

    let row_h = dpi::px(ROW_H);
    let idx = (y - rect.top) / row_h + scroll_offset(hwnd);
    let count = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
    });

    if idx >= 0 && (idx as usize) < count {
        UI.with(|slot| {
            let slot = slot.borrow();
            if let Some(ui) = slot.as_ref() {
                let mut u = ui.borrow_mut();
                u.selected = Some(idx as usize);
            }
        });
        let _ = InvalidateRect(hwnd, None, false);
    }
}

/// 当前滚动偏移
fn scroll_offset(hwnd: HWND) -> i32 {
    let _ = hwnd;
    UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().scroll).unwrap_or(0)
    })
}

/// 键盘：上下移动选中 / Delete 删除
unsafe fn on_key(hwnd: HWND, key: u32) {
    let count = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
    });
    if count == 0 {
        return;
    }

    match key {
        0x26 => {
            // 上箭头
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    let cur = u.selected.unwrap_or(0);
                    u.selected = Some(cur.saturating_sub(1));
                    ensure_visible(hwnd, u.scroll);
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
        }
        0x28 => {
            // 下箭头
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    let cur = u.selected.unwrap_or(0);
                    u.selected = Some((cur + 1).min(count - 1));
                    ensure_visible(hwnd, u.scroll);
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
        }
        0x2E => {
            // Delete
            let sel = UI.with(|slot| {
                let slot = slot.borrow();
                slot.as_ref().map(|u| u.borrow().selected).unwrap_or(None)
            });
            if let Some(idx) = sel {
                UI.with(|slot| {
                    let slot = slot.borrow();
                    if let Some(ui) = slot.as_ref() {
                        let mut u = ui.borrow_mut();
                        if idx < u.state.tasks.len() {
                            u.state.tasks.remove(idx);
                            u.state.recount();
                            u.selected = None;
                        }
                    }
                });
                load_icons();
                refresh_list(hwnd);
                update_status(hwnd);
            }
        }
        _ => {}
    }
}

/// 确保选中项在可视区内
fn ensure_visible(hwnd: HWND, scroll: i32) {
    let vis = visible_rows(hwnd);
    let sel = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().selected).unwrap_or(None)
    });
    let Some(sel) = sel else { return };
    let s = sel as i32;
    let new_scroll = if s < scroll {
        s
    } else if s >= scroll + vis {
        s - vis + 1
    } else {
        scroll
    };
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.borrow_mut().scroll = new_scroll;
        }
    });
}

pub unsafe fn update_status(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();
        let st = &ui.state;

        let text = if st.running {
            format!(
                "正在转换 {}/{}（{} 个进行中）",
                st.stats.settled(),
                st.stats.total,
                st.stats.running
            )
        } else {
            st.status.clone()
        };

        if let Ok(c) = GetDlgItem(hwnd, ID_STATIC_STATUS) {
            let _ = SetWindowTextW(c, &windows::core::HSTRING::from(text));
        }
        if let Ok(c) = GetDlgItem(hwnd, ID_STATIC_PCT) {
            let _ = SetWindowTextW(
                c,
                &windows::core::HSTRING::from(format!("{}%", st.stats.percent())),
            );
        }
        if let Ok(pb) = GetDlgItem(hwnd, ID_PROGRESS) {
            SendMessageW(
                pb,
                PBM_SETPOS,
                WPARAM(st.stats.percent() as usize),
                LPARAM(0),
            );
        }

        if let Ok(b) = GetDlgItem(hwnd, ID_BTN_CONVERT) {
            let label = if st.running { "转换中…" } else { "开始转换" };
            let _ = SetWindowTextW(b, &windows::core::HSTRING::from(label));
            let _ = EnableWindow(b, !st.running && !st.tasks.is_empty());
        }
        if let Ok(b) = GetDlgItem(hwnd, ID_BTN_CLEAR) {
            let _ = EnableWindow(b, !st.running);
        }
        if let Ok(b) = GetDlgItem(hwnd, ID_BTN_OUT) {
            let label = match &st.out_dir {
                Some(d) => format!("输出: {}", d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
                None => "输出目录".to_string(),
            };
            let _ = SetWindowTextW(b, &windows::core::HSTRING::from(label));
        }
    });
}

unsafe fn tick(hwnd: HWND) {
    let mut finished = Vec::new();
    let mut all_done = false;

    UI.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ui) = slot.as_ref() else { return };
        let mut rx = ui.borrow_mut().rx.borrow_mut().take();
        if let Some(ref r) = rx {
            while let Ok(msg) = r.try_recv() {
                match msg {
                    WorkerMsg::Finished { id, result } => finished.push((id, result)),
                    WorkerMsg::AllDone => all_done = true,
                }
            }
        }
        *ui.borrow_mut().rx.borrow_mut() = rx;
    });

    if finished.is_empty() && !all_done {
        return;
    }

    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            for (id, result) in finished {
                if let Some(t) = u.state.tasks.iter_mut().find(|t| t.id == id) {
                    match result {
                        Ok((size, _)) => {
                            t.status = Status::Done(size as u64);
                            t.out_size = size;
                        }
                        Err(e) => t.status = Status::Error(e),
                    }
                }
            }
            u.state.recount();
            if all_done
                || (u.state.stats.total > 0 && u.state.stats.settled() == u.state.stats.total)
            {
                u.state.running = false;
                let s = u.state.stats.clone();
                let out = u
                    .state
                    .out_dir
                    .as_ref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "源文件同目录".to_string());
                u.state.status = format!("{} 成功 · {} 失败 · 输出到 {}", s.done, s.failed, out);
            }
        }
    });

    refresh_list(hwnd);
    update_status(hwnd);
}

/// 刷新主题（跟随系统深色模式）
unsafe fn refresh_theme(hwnd: HWND) -> bool {
    let dark = is_dark_mode();
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            u.theme = if dark { Theme::dark() } else { Theme::light() };
        }
    });
    // 标题栏跟随
    let flag = i32::from(dark);
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &flag as *const _ as *const std::ffi::c_void,
        4,
    );
    let _ = InvalidateRect(hwnd, None, true);
    true
}

/// 检测系统是否为深色模式（读注册表 AppsUseLightTheme）
pub unsafe fn is_dark_mode() -> bool {
    use windows::Win32::System::Registry::*;
    let mut key = HKEY::default();
    let sub = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let res = RegOpenKeyExW(
        HKEY_CURRENT_USER,
        windows::core::PCWSTR(sub.as_ptr()),
        0,
        KEY_READ,
        &mut key,
    );
    if res.is_err() {
        return false;
    }
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let name = to_wide("AppsUseLightTheme");
    let res = RegGetValueW(
        key,
        windows::core::PCWSTR(std::ptr::null()),
        windows::core::PCWSTR(name.as_ptr()),
        RRF_RT_REG_DWORD,
        None,
        Some(&mut value as *mut _ as *mut std::ffi::c_void),
        Some(&mut size),
    );
    let _ = RegCloseKey(key);
    if res.is_err() {
        return false;
    }
    value == 0
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}


/// 预加载文件（测试 / 便捷启动用）
pub unsafe fn preload_files(hwnd: HWND, paths: &[PathBuf]) {
    add_paths(hwnd, paths, "");
}

/// 预加载目录（测试 / 便捷启动用）
pub unsafe fn preload_folder(hwnd: HWND, dir: &PathBuf) {
    add_folder(hwnd, dir);
}
