// 主窗口：控件创建、布局、消息处理、自绘列表
//
// 布局用绝对坐标（窗口 resize 时重算）。所有绘制走双缓冲。

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Gdi::HGDIOBJ;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::draw::*;
use crate::icons;
use crate::state::{AppState, Status, Task, WorkerMsg};
use crate::worker;

// ── 窗口消息 ──
const WM_APP_WORKER: u32 = WM_APP + 1; // 后台线程结果
const WM_APP_TICK: u32 = WM_APP + 2; // 定时轮询
const TIMER_TICK: usize = 1;

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
const ID_STATIC_DROP: i32 = 111;

// 布局常量
const PAD: i32 = 20;
const LIST_ROW_H: i32 = 46;
const DROP_Y: i32 = 82;
const DROP_H: i32 = 56;
const BTN_Y: i32 = 156;
const BTN_H: i32 = 30;
const FOOT_H: i32 = 58;

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
    pub section: HFONT,
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
            940,
            640,
            None,
            None,
            hinst,
            None,
        )
        .unwrap_or_default();

        // Win11 圆角 + 深色模式支持
        let pref = 2i32; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const std::ffi::c_void, 4);
        let dark = 1i32;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const std::ffi::c_void, 4);

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
        WM_ERASEBKGND => LRESULT(1), // 自己画背景，避免闪烁
        WM_DRAWITEM => {
            draw_item(hwnd, lp);
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
        WM_SETTINGCHANGE => {
            // 系统主题变化（深色模式切换）
            let _ = refresh_theme(hwnd);
            LRESULT(0)
        }
        WM_APP_WORKER => {
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

// ── 创建控件 ──
unsafe fn on_create(hwnd: HWND) -> windows::core::Result<()> {
    let hinst: HINSTANCE = GetModuleHandleW(None)?.into();

    // 常用控件初始化
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES | ICC_PROGRESS_CLASS | ICC_BAR_CLASSES,
    };
    let _ = InitCommonControlsEx(&icc);

    // 静态文本
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("文档转 Markdown"),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_STATIC_TITLE as isize as *mut std::ffi::c_void),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("把 Word、PowerPoint、Excel、PDF 等文档转成 LLM 能直接读的干净 Markdown"),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_STATIC_SUB as isize as *mut std::ffi::c_void),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("就绪"),
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_STATIC_STATUS as isize as *mut std::ffi::c_void),
        hinst,
        None,
    );
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("0%"),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(windows::Win32::System::SystemServices::SS_RIGHT.0),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(ID_STATIC_PCT as isize as *mut std::ffi::c_void),
        hinst,
        None,
    );

    // 任务列表（自绘）
    let hlist = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("LISTBOX"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x00000011) | WINDOW_STYLE(0x00000100),
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

    // 初始化状态
    let ui = Rc::new(RefCell::new(Ui {
        state: AppState::new(),
        theme: Theme::light(),
        hwnd,
        hlist,
        hprogress,
        fonts: Fonts {
            title: make_font(20, 600),
            sub: make_font(13, 400),
            name: make_font(14, 500),
            small: make_font(12, 400),
            badge: make_font(12, 600),
            btn: make_font(13, 400),
            section: make_font(14, 600),
        },
        icons: RefCell::new(Vec::new()),
        rx: RefCell::new(None),
        dragging: RefCell::new(false),
    }));

    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));

    // 应用字体到控件
    apply_fonts(hwnd);

    // Win11 圆角需要窗口无边框？不，用 DWM 即可
    layout(hwnd);

    // 轮询定时器
    let _ = SetTimer(hwnd, TIMER_TICK, 40, None);

    Ok(())
}

unsafe fn apply_fonts(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();

        let set = |id: i32, font: HFONT| {
            if let Ok(c) = GetDlgItem(hwnd, id) {
                SendMessageW(c, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            }
        };
        set(ID_STATIC_TITLE, ui.fonts.title);
        set(ID_STATIC_SUB, ui.fonts.sub);
        set(ID_STATIC_STATUS, ui.fonts.small);
        set(ID_STATIC_PCT, ui.fonts.badge);
        set(ID_LIST, ui.fonts.name);
        for id in [
            ID_BTN_FILES,
            ID_BTN_DIR,
            ID_BTN_CLEAR,
            ID_BTN_CONVERT,
            ID_BTN_OUT,
        ] {
            set(id, ui.fonts.btn);
        }
    });
}

// ── 布局 ──
pub unsafe fn layout(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rc);
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;

    // 去掉标题栏和非客户区
    let mut cr = RECT::default();
    let _ = GetClientRect(hwnd, &mut cr);
    let cw = cr.right - cr.left;
    let ch = cr.bottom - cr.top;

    let move_ctl = |id: i32, x: i32, y: i32, ww: i32, hh: i32| {
        if let Ok(c) = GetDlgItem(hwnd, id) {
            let _ = SetWindowPos(c, None, x, y, ww, hh, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    };

    // 标题区
    move_ctl(ID_STATIC_TITLE, PAD, 18, 600, 28);
    move_ctl(ID_STATIC_SUB, PAD, 48, 700, 20);

    // 投放区（自绘区域）—— 用一个透明静态控件占位，位置稍后画
    move_ctl(ID_STATIC_DROP, PAD, DROP_Y, cw - PAD * 2, DROP_H);

    // 按钮行
    let btn_y = BTN_Y;
    let bw = 92;
    let bh = BTN_H;
    let mut bx = PAD;
    move_ctl(ID_BTN_FILES, bx, btn_y, bw, bh);
    bx += bw + 8;
    move_ctl(ID_BTN_DIR, bx, btn_y, bw, bh);

    // 右侧按钮
    let mut rx = cw - PAD;
    rx -= 96;
    move_ctl(ID_BTN_CONVERT, rx, btn_y, 96, bh);
    rx -= 74;
    move_ctl(ID_BTN_CLEAR, rx, btn_y, 66, bh);
    rx -= 96;
    move_ctl(ID_BTN_OUT, rx, btn_y, 88, bh);

    // 列表
    let list_y = btn_y + bh + 12;
    let foot_h = FOOT_H;
    let list_h = ch - list_y - foot_h - 8;
    move_ctl(ID_LIST, PAD, list_y, cw - PAD * 2, list_h.max(80));

    // 底部
    let foot_y = ch - foot_h;
    move_ctl(ID_PROGRESS, PAD, foot_y + 4, cw - PAD * 2, 4);
    move_ctl(ID_STATIC_STATUS, PAD, foot_y + 18, cw - PAD * 2 - 60, 22);
    move_ctl(ID_STATIC_PCT, cw - PAD - 56, foot_y + 18, 56, 22);

    let _ = w;
}

// ── 绘制 ──
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let cw = rc.right - rc.left;
    let ch = rc.bottom - rc.top;

    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();
        let th = &ui.theme;

        // 背景
        let bg = CreateSolidBrush(th.bg);
        FillRect(hdc, &RECT { left: 0, top: 0, right: cw, bottom: ch }, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        // 投放区（虚线圆角框 + 文字）
        draw_dropzone(hdc, &ui, PAD, DROP_Y, cw - PAD * 2, DROP_H, th);

        // 列表区边框
        let list_y = 156 + 30 + 12;
        let foot_h = FOOT_H;
        let list_h = ch - list_y - foot_h - 8;
        let lrect = RECT {
            left: PAD,
            top: list_y,
            right: cw - PAD,
            bottom: list_y + list_h.max(80),
        };
        stroke_round_rect(hdc, &lrect, 10, th.line, 1);
    });

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
        (th.line, th.card)
    };
    fill_round_rect(hdc, &rect, 10, bg);
    stroke_round_rect(hdc, &rect, 10, border, 1);

    // 文字
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
    let main = if hot {
        "松开即可添加".to_string()
    } else {
        "拖入文档到此处，或 选择文件 / 选择文件夹".to_string()
    };
    draw_text(hdc, &main, x + 46, y + 10, w - 60, 20, th.text, DRAW_TEXT_FORMAT(DT_LEFT.0), true);

    let old2: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.small.0));
    let sub = "支持 docx · pptx · xlsx · pdf · epub · odt · rtf · csv";
    draw_text(hdc, sub, x + 46, y + 31, w - 60, 18, th.text_mute, DRAW_TEXT_FORMAT(DT_LEFT.0), true);
    let _ = SelectObject(hdc, old2);
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

        // 图标（真实系统图标）
        let icon_x = rc.left + 12;
        let icon_y = rc.top + (LIST_ROW_H - 28) / 2;
        let mut icon_drawn = false;
        if let Some(icons) = ui.icons.borrow().get(idx) {
            if let Some(ic) = icons {
                if let Some(hicon) = ic.hicon {
                    let _ = DrawIconEx(
                        hdc,
                        icon_x,
                        icon_y,
                        hicon,
                        24,
                        24,
                        0,
                        None,
                        DI_NORMAL,
                    );
                    icon_drawn = true;
                }
            }
        }
        if !icon_drawn {
            draw_icon_placeholder(hdc, icon_x, icon_y, 24, th.line_soft);
        }

        // 状态条（最左侧的色条）
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
                right: rc.left + 3,
                bottom: rc.bottom,
            },
            bar,
        );
        let _ = DeleteObject(HGDIOBJ(bar.0));

        // 文件名
        let text_left = icon_x + 34;
        let badge_w = 96;
        let size_w = 74;
        let name_w = rc.right - text_left - badge_w - size_w - 20;

        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
        draw_text(
            hdc,
            &task.name,
            text_left,
            rc.top + 7,
            name_w,
            18,
            th.text,
            DT_LEFT,
            true,
        );

        // 副标题
        let old2: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.small.0));
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
            rc.top + 26,
            name_w,
            16,
            sub_color,
            DT_LEFT,
            true,
        );
        let _ = SelectObject(hdc, old2);

        // 大小
        draw_text(
            hdc,
            &crate::state::fmt_size(task.size),
            rc.right - badge_w - size_w - 8,
            rc.top + 14,
            size_w,
            18,
            th.text_mute,
            DT_RIGHT,
            true,
        );

        // 状态徽章
        let (text, fg, bg) = status_style(th, &task.status);
        let old3: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.badge.0));
        draw_badge(
            hdc,
            &text,
            rc.right - 12,
            rc.top + LIST_ROW_H / 2,
            fg,
            bg,
            ui.fonts.badge,
        );
        let _ = SelectObject(hdc, old3);

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
                    let mut ui = ui.borrow_mut();
                    ui.state.clear();
                    ui.icons.borrow_mut().clear();
                }
            });
            refresh_list(hwnd);
            update_status(hwnd);
        }
        ID_BTN_CONVERT => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut ui = ui.borrow_mut();
                    if ui.state.running {
                        return;
                    }
                    if ui.state.tasks.is_empty() {
                        return;
                    }
                    // 重置状态，开始
                    for t in ui.state.tasks.iter_mut() {
                        t.status = Status::Wait;
                        t.out_size = 0;
                    }
                    ui.state.running = true;
                    ui.state.stats = Default::default();
                    ui.state.stats.total = ui.state.tasks.len();
                    let tasks = ui.state.tasks.clone();
                    let out = ui.state.out_dir.clone();
                    let rx = worker::spawn(tasks, out);
                    *ui.rx.borrow_mut() = Some(rx);
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
                        let mut ui = ui.borrow_mut();
                        ui.state.out_dir = Some(dir.clone());
                        ui.state.out_confirmed = true;
                        ui.state.status = format!("输出目录: {}", dir.display());
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

/// 加入一组文件
unsafe fn add_paths(hwnd: HWND, paths: &[PathBuf], rel: &str) {
    let mut added = 0;
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut ui = ui.borrow_mut();
            for p in paths {
                if ui.state.add_file(p.clone(), rel.to_string()) {
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

/// 加入整个目录（递归）
unsafe fn add_folder(hwnd: HWND, dir: &PathBuf) {
    let mut added = 0;
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut ui = ui.borrow_mut();
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
                // 计算相对目录
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
                if ui.state.add_file(path, rel) {
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

/// 重新加载文件图标
fn load_icons() {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let tasks: Vec<Task> = ui.borrow().state.tasks.clone();
        let mut guard = ui.borrow_mut();
        let mut icons = guard.icons.borrow_mut();
        icons.clear();
        for t in tasks {
            icons.push(icons::load(&t.path, &t.ext));
        }
    });
}

/// 重建列表内容
pub unsafe fn refresh_list(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
        let ui = ui.borrow();

        if let Ok(list) = GetDlgItem(hwnd, ID_LIST) {
            let _ = SendMessageW(list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
            for _ in 0..ui.state.tasks.len() {
                let _ = SendMessageW(list, LB_ADDSTRING, WPARAM(0), LPARAM(0));
            }
            // 固定行高
            SendMessageW(
                list,
                LB_SETITEMHEIGHT,
                WPARAM(LIST_ROW_H as usize),
                LPARAM(0),
            );
            let _ = InvalidateRect(list, None, true);
        }
    });
}

/// 更新底部状态
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
            let _ = SendMessageW(
                pb,
                PBM_SETPOS,
                WPARAM(st.stats.percent() as usize),
                LPARAM(0),
            );
        }

        // 按钮状态
        if let Ok(b) = GetDlgItem(hwnd, ID_BTN_CONVERT) {
            let label = if st.running {
                "转换中…"
            } else if st.tasks.is_empty() {
                "开始转换"
            } else {
                "开始转换"
            };
            let _ = SetWindowTextW(b, &windows::core::HSTRING::from(label));
            let _ = EnableWindow(b, !st.running && !st.tasks.is_empty());
        }
        if let Ok(b) = GetDlgItem(hwnd, ID_BTN_CLEAR) {
            let _ = EnableWindow(b, !st.running);
        }
    });
}

/// 定时轮询后台结果
unsafe fn tick(hwnd: HWND) {
    let mut finished = Vec::new();
    let mut all_done = false;

    UI.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ui) = slot.as_ref() else { return };
        // 取出 rx，取完再放回（避免嵌套 borrow）
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
            let mut ui = ui.borrow_mut();
            for (id, result) in finished {
                if let Some(t) = ui.state.tasks.iter_mut().find(|t| t.id == id) {
                    match result {
                        Ok((size, _)) => {
                            t.status = Status::Done(size as u64);
                            t.out_size = size;
                        }
                        Err(e) => t.status = Status::Error(e),
                    }
                }
            }
            ui.state.recount();
            if all_done || (ui.state.stats.total > 0 && ui.state.stats.settled() == ui.state.stats.total) {
                ui.state.running = false;
                let s = ui.state.stats.clone();
                let out = ui
                    .state
                    .out_dir
                    .as_ref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "源文件同目录".to_string());
                ui.state.status = format!(
                    "{} 成功 · {} 失败 · 输出到 {}",
                    s.done, s.failed, out
                );
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
            let mut ui = ui.borrow_mut();
            ui.theme = if dark { Theme::dark() } else { Theme::light() };
        }
    });
    let _ = InvalidateRect(hwnd, None, true);
    true
}

/// 检测系统是否为深色模式
///
/// 读注册表 AppsUseLightTheme，0 = 深色。
/// 这是最可靠的方式（DWM 属性需要真实窗口句柄）。
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
