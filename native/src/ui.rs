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
const ID_LIST: i32 = 100;
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
            980,
            680,
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
        WM_DRAWITEM => {
            draw_item(hwnd, lp);
            LRESULT(1)
        }
        WM_MEASUREITEM => {
            // 固定行高
            let mi = &mut *(lp.0 as *mut MEASUREITEMSTRUCT);
            mi.itemHeight = dpi::px(ROW_H) as u32;
            LRESULT(1)
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

    // 任务列表（自绘，报告模式）
    let hlist = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("LISTBOX"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(LBS_OWNERDRAWFIXED as u32) | WINDOW_STYLE(LBS_NOINTEGRALHEIGHT as u32),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_LIST as isize as *mut std::ffi::c_void),
        hinst,
        None,
    )?;

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
    }));
    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));

    apply_fonts(hwnd);
    layout(hwnd);

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
        set(ID_LIST, u.fonts.name);
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
    move_ctl(ID_LIST, pad, list_y, cw - pad * 2, list_h);

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

    // 双缓冲：内存 DC + 位图
    let mem_dc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, cw, ch);
    let old_bmp = SelectObject(mem_dc, HGDIOBJ(bmp.0));

    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();
        let th = &ui.theme;

        // 背景
        let bg = CreateSolidBrush(th.bg);
        FillRect(
            mem_dc,
            &RECT {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            },
            bg,
        );
        let _ = DeleteObject(HGDIOBJ(bg.0));

        // 投放区
        draw_dropzone(mem_dc, &ui, dpi::px(PAD), dpi::px(DROP_Y), cw - dpi::px(PAD) * 2, dpi::px(DROP_H), th);

        // 列表边框
        let list_y = dpi::px(BTN_Y) + dpi::px(BTN_H) + dpi::px(12);
        let foot_h = dpi::px(FOOT_H);
        let list_h = (ch - list_y - foot_h).max(dpi::px(120));
        let lrect = RECT {
            left: dpi::px(PAD),
            top: list_y,
            right: cw - dpi::px(PAD),
            bottom: list_y + list_h,
        };
        stroke_round_rect(mem_dc, &lrect, dpi::px(10), th.line, 1);

        // 底部状态栏（浅色分隔）
        let foot_y = ch - foot_h;
        let sep = RECT {
            left: 0,
            top: foot_y,
            right: cw,
            bottom: foot_y + 1,
        };
        let sb = CreateSolidBrush(th.line);
        FillRect(mem_dc, &sep, sb);
        let _ = DeleteObject(HGDIOBJ(sb.0));
    });

    // 贴回屏幕
    let _ = BitBlt(hdc, 0, 0, cw, ch, mem_dc, 0, 0, SRCCOPY);
    let _ = SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(HGDIOBJ(bmp.0));
    let _ = DeleteDC(mem_dc);

    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_dropzone(
    hdc: HDC,
    ui: &Ui,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    th: &Theme,
) {
    let hot = *ui.dragging.borrow();
    let rect = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    let (border, bg) = if hot {
        (th.accent, th.run_bg)
    } else {
        (th.line, th.drop_bg)
    };
    fill_round_rect(hdc, &rect, dpi::px(10), bg);
    stroke_round_rect(hdc, &rect, dpi::px(10), border, 1);

    // 图标（向下箭头）
    let ix = x + dpi::px(22);
    let iy = y + h / 2;
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
    let arrow = "↓";
    let sz = measure_text(hdc, arrow);
    let _ = SelectObject(hdc, old);
    let _ = sz;
    draw_text(
        hdc,
        arrow,
        ix,
        iy - dpi::px(14),
        dpi::px(24),
        dpi::px(28),
        if hot { th.accent } else { th.text_mute },
        DRAW_TEXT_FORMAT(DT_CENTER.0),
        false,
    );

    let tx = ix + dpi::px(32);
    let tw = w - (tx - x) - dpi::px(16);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
    let main = if hot {
        "松开即可添加文件".to_string()
    } else {
        "拖入文档到此处，或 点击「选择文件」/「选择文件夹」".to_string()
    };
    draw_text(
        hdc,
        &main,
        tx,
        y + dpi::px(12),
        tw,
        dpi::px(22),
        th.text,
        DRAW_TEXT_FORMAT(DT_LEFT.0),
        true,
    );
    let _ = SelectObject(hdc, old);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.small.0));
    let sub = "支持 docx · pptx · xlsx · pdf · epub · odt · ods · odp · rtf · csv";
    draw_text(
        hdc,
        sub,
        tx,
        y + dpi::px(34),
        tw,
        dpi::px(18),
        th.text_mute,
        DRAW_TEXT_FORMAT(DT_LEFT.0),
        true,
    );
    let _ = SelectObject(hdc, old);
}

// ── 自绘列表 ──
unsafe fn draw_item(hwnd: HWND, lp: LPARAM) {
    let item = &*(lp.0 as *const DRAWITEMSTRUCT);
    let Ok(hlist) = GetDlgItem(hwnd, ID_LIST) else {
        return;
    };
    if item.hwndItem != hlist {
        return;
    }

    let hdc = item.hDC;
    let rc = item.rcItem;
    let idx = item.itemID as usize;

    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();
        let th = &ui.theme;

        let Some(task) = ui.state.tasks.get(idx) else {
            return;
        };

        // 背景
        let bg = match task.status {
            Status::Error(_) => th.err_bg,
            _ => th.card,
        };
        let brush = CreateSolidBrush(bg);
        FillRect(hdc, &rc, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        // 选中态
        if (item.itemState.0 & 0x0000000F) == 0x00000002 {
            let sel = CreateSolidBrush(th.run_bg);
            FillRect(hdc, &rc, sel);
            let _ = DeleteObject(HGDIOBJ(sel.0));
        }

        let row_h = rc.bottom - rc.top;

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
                left: rc.left,
                top: rc.top,
                right: rc.left + dpi::px(3),
                bottom: rc.bottom,
            },
            bar,
        );
        let _ = DeleteObject(HGDIOBJ(bar.0));

        // 图标
        let icon_x = rc.left + dpi::px(14);
        let icon_y = rc.top + (row_h - dpi::px(26)) / 2;
        let icon_sz = dpi::px(26);
        let mut icon_drawn = false;
        if let Some(icons) = ui.icons.borrow().get(idx) {
            if let Some(ic) = icons {
                if let Some(hicon) = ic.hicon {
                    let _ = DrawIconEx(
                        hdc,
                        icon_x,
                        icon_y,
                        hicon,
                        icon_sz,
                        icon_sz,
                        0,
                        None,
                        DI_NORMAL,
                    );
                    icon_drawn = true;
                }
            }
        }
        if !icon_drawn {
            draw_icon_placeholder(hdc, icon_x, icon_y, icon_sz, th.wait_bg);
        }

        // 文本区
        let text_left = icon_x + icon_sz + dpi::px(12);
        let badge_w = dpi::px(104);
        let size_w = dpi::px(76);
        let name_w = (rc.right - text_left - badge_w - size_w - dpi::px(24)).max(dpi::px(60));

        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
        draw_text(
            hdc,
            &task.name,
            text_left,
            rc.top + dpi::px(7),
            name_w,
            dpi::px(20),
            th.text,
            DRAW_TEXT_FORMAT(DT_LEFT.0),
            true,
        );
        let _ = SelectObject(hdc, old);

        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.small.0));
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
            hdc,
            &sub,
            text_left,
            rc.top + dpi::px(27),
            name_w,
            dpi::px(18),
            sub_color,
            DRAW_TEXT_FORMAT(DT_LEFT.0),
            true,
        );
        let _ = SelectObject(hdc, old);

        // 大小
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.small.0));
        draw_text(
            hdc,
            &crate::state::fmt_size(task.size),
            rc.right - badge_w - size_w - dpi::px(8),
            rc.top + (row_h - dpi::px(18)) / 2,
            size_w,
            dpi::px(18),
            th.text_mute,
            DRAW_TEXT_FORMAT(DT_RIGHT.0),
            true,
        );
        let _ = SelectObject(hdc, old);

        // 状态徽章
        let (text, fg, bgc) = status_style(th, &task.status);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.badge.0));
        draw_badge(
            hdc,
            &text,
            rc.right - dpi::px(14),
            rc.top + row_h / 2,
            fg,
            bgc,
            ui.fonts.badge,
        );
        let _ = SelectObject(hdc, old);

        // 底部分隔线
        hline(hdc, rc.left, rc.right, rc.bottom - 1, th.line_soft);
    });
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
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();

        if let Ok(list) = GetDlgItem(hwnd, ID_LIST) {
            SendMessageW(list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
            for _ in 0..ui.state.tasks.len() {
                SendMessageW(list, LB_ADDSTRING, WPARAM(0), LPARAM(0));
            }
            // 固定行高：wParam = 高度，lParam = 0xFFFF (应用到所有项)
            SendMessageW(
                list,
                LB_SETITEMHEIGHT,
                WPARAM(dpi::px(ROW_H) as usize),
                LPARAM(0xFFFF),
            );
            let _ = InvalidateRect(list, None, true);
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
