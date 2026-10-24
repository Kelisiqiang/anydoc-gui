// 主窗口：布局、自绘、命中测试、交互
//
// 布局完全自绘（不用原生 BUTTON 控件），以匹配设计稿的圆角 + 图标 + 精确间距。
// 交互通过「命中区域表」实现：布局时记录每块可点击区域，鼠标事件时反查。

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, SetScrollInfo, ICC_BAR_CLASSES, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX,
    PBM_SETPOS, PBS_SMOOTH,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::draw::*;
use crate::dpi;
use crate::frontmatter;
use crate::icons;
use crate::state::{AppState, Status, Task, WorkerMsg};
use crate::worker;

// ── 布局常量（逻辑像素）──
const PAD: i32 = 24;
// 布局常量 —— 严格对齐设计稿的 Tailwind 数值
const BRAND_H: i32 = 40;      // h-10
const HEAD_TOP: i32 = 20;     // pt-5
const HEAD_BOT: i32 = 12;     // pb-3
const FOOT_PAD: i32 = 16;     // py-4
const ROW_H: i32 = 48;        // py-3 + 内容
const THEAD_H: i32 = 34;      // py-2.5 + 内容
const SCROLLBAR_W: i32 = 16;   // 系统滚动条
const HEAD_H: i32 = 68;
const FOOT_H: i32 = 64;

// ── 命中区 ID ──
const HIT_ADD_FILES: u32 = 1;
const HIT_ADD_DIR: u32 = 2;
const HIT_CLEAR: u32 = 3;
const HIT_OUTDIR: u32 = 4;
const HIT_FRONT: u32 = 5;
const HIT_CONVERT: u32 = 6;
const HIT_ROW_DEL: u32 = 100; // + 行号
const HIT_ROW_OPEN: u32 = 200; // + 行号
const HIT_MIN: u32 = 300;
const HIT_MAX: u32 = 301;
const HIT_CLOSE: u32 = 302;
const HIT_SCROLL_TRACK: u32 = 400;

/// 可点击区域
#[derive(Clone, Copy)]
pub struct Hit {
    pub rect: RECT,
    pub id: u32,
}

pub struct Ui {
    pub state: AppState,
    pub theme: Theme,
    pub hwnd: HWND,
    pub hprogress: HWND,
    pub hscroll: HWND,
    pub fonts: Fonts,
    pub icons: RefCell<Vec<Option<icons::IconEntry>>>,
    pub rx: RefCell<Option<std::sync::mpsc::Receiver<WorkerMsg>>>,
    pub dragging: Cell<bool>,
    pub scroll: Cell<i32>,
    pub selected: Cell<Option<usize>>,
    pub dragging_scroll: Cell<bool>,
    pub scroll_anchor: Cell<i32>,
    pub hot: Cell<Option<u32>>,
    pub hits: RefCell<Vec<Hit>>,
    /// 应用图标（从 exe 资源加载）
    pub app_icon: Option<HICON>,
    /// 注入 YAML Frontmatter
    pub inject_frontmatter: Cell<bool>,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
}

/// 注册窗口类
pub fn register_class() {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let res_id = windows::core::PCWSTR(1 as *const u16);

        // LoadIconW 只能取系统图标，自定义图标必须用 LoadImageW
        let icon_big = LoadImageW(hinst, res_id, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE).ok();
        let icon_small = LoadImageW(
            hinst,
            res_id,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            IMAGE_FLAGS(0),
        )
        .ok();

        let _ = RegisterClassExW(&WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            hIcon: icon_big.map(|h| HICON(h.0)).unwrap_or_default(),
            hIconSm: icon_small.map(|h| HICON(h.0)).unwrap_or_default(),
            lpszClassName: w!("AnyDocNative"),
            ..Default::default()
        });
    }
}

/// 创建主窗口
pub fn create(hinst: HINSTANCE) -> HWND {
    unsafe {
        // TEMP DIAG
        let _ = &hinst;
        let hwnd = CreateWindowExW(
            // WS_EX_APPWINDOW：WS_POPUP 窗口必须带这个才会出现在任务栏 / Alt+Tab
            WS_EX_APPWINDOW,
            w!("AnyDocNative"),
            w!("AnyDoc - 文档转 Markdown"),
            // 无边框自绘标题栏（还原设计稿）
            WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            dpi::px(896),
            dpi::px(640),
            None,
            None,
            hinst,
            None,
        )
        .unwrap_or_default();

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
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DROPFILES => {
            on_drop(hwnd, lp);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            let steps = (-delta / WHEEL_DELTA as i16).max(-3).min(3) as i32;
            let cur = get_scroll();
            let next = (cur + steps * 3).max(0).min(max_scroll(hwnd));
            set_scroll(hwnd, next);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            on_click(hwnd, lp);
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
            let l = compute_layout(hwnd);
            if y >= l.brand.top && y < l.brand.bottom {
                let maximized = is_maximized(hwnd);
                let _ = ShowWindow(hwnd, if maximized { SW_RESTORE } else { SW_MAXIMIZE });
                layout(hwnd);
                let _ = InvalidateRect(hwnd, None, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    ui.borrow_mut().dragging_scroll.set(false);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lp);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    ui.borrow_mut().dragging_scroll.set(false);
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            on_key(hwnd, wp.0 as u32);
            LRESULT(0)
        }
        WM_TIMER => {
            if (wp.0 as usize) == TIMER_TICK {
                tick(hwnd);
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            // WS_POPUP 最大化会铺满整个屏幕（含任务栏），
            // 这里把最大尺寸限制在「工作区」范围内。
            let mmi = &mut *(lp.0 as *mut MINMAXINFO);
            let mut work = RECT::default();
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut RECT as *mut std::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            mmi.ptMaxPosition.x = 0;
            mmi.ptMaxPosition.y = 0;
            mmi.ptMaxSize.x = work.right - work.left;
            mmi.ptMaxSize.y = work.bottom - work.top;
            mmi.ptMaxTrackSize.x = work.right - work.left;
            mmi.ptMaxTrackSize.y = work.bottom - work.top;
            LRESULT(0)
        }
        WM_DPICHANGED => {
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

    // 加载应用图标（exe 资源 ID 1），用于左上角显示
    let app_icon = LoadImageW(
        hinst,
        windows::core::PCWSTR(1 as *const u16),
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE,
    )
    .ok()
    .map(|h| HICON(h.0));

    // 关键：UI 状态最先初始化，后续任何失败都不影响
    let ui = Rc::new(RefCell::new(Ui {
        state: AppState::new(),
        theme: Theme::light(),
        hwnd,
        hprogress: HWND(std::ptr::null_mut()),
        hscroll: HWND(std::ptr::null_mut()),
        fonts: make_fonts(),
        icons: RefCell::new(Vec::new()),
        rx: RefCell::new(None),
        dragging: Cell::new(false),
        scroll: Cell::new(0),
        selected: Cell::new(None),
        dragging_scroll: Cell::new(false),
        scroll_anchor: Cell::new(0),
        hot: Cell::new(None),
        hits: RefCell::new(Vec::new()),
        app_icon,
        inject_frontmatter: Cell::new(true),
    }));
    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));

    // 通用控件初始化
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS | ICC_BAR_CLASSES,
    };
    let _ = InitCommonControlsEx(&icc);

    // 底部进度条
    let hprogress = match CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("msctls_progress32"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(PBS_SMOOTH),
        0,
        0,
        0,
        0,
        hwnd,
        HMENU(106 as isize as *mut std::ffi::c_void),
        hinst,
        None,
    ) {
        Ok(h) => h,
        Err(_) => HWND(std::ptr::null_mut()),
    };
    {
        let mut u = ui.borrow_mut();
        u.hprogress = hprogress;
    }

    let _ = SetTimer(hwnd, TIMER_TICK, 40, None);
    Ok(())
}

fn make_fonts() -> Fonts {
    Fonts {
        brand: make_font(13, 600),
        mono: make_mono(13, 500),
        h1: make_font(21, 700),
        sub: make_font(13, 400),
        btn: make_font(13, 500),
        btn_bold: make_font(13, 600),
        thead: make_font(12, 700),
        name: make_font(14, 600),
        meta: make_font(13, 500),
        tag: make_font(11, 600),
        foot: make_font(13, 500),
        big: make_font(14, 700),
    }
}

unsafe fn rebuild_fonts(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let f = make_fonts();
            let mut u = ui.borrow_mut();
            u.fonts = f;
        }
    });
    let _ = hwnd;
}

// ── 布局计算 ──
struct Layout {
    cw: i32,
    ch: i32,
    /// 品牌条
    brand: RECT,
    /// 头部
    head: RECT,
    /// 工作区
    work: RECT,
    /// 底部
    foot: RECT,
    /// 表格卡片
    card: RECT,
}

fn compute_layout(hwnd: HWND) -> Layout {
    unsafe {
        let mut cr = RECT::default();
        let _ = GetClientRect(hwnd, &mut cr);
        let cw = cr.right - cr.left;
        let ch = cr.bottom - cr.top;
        let p = dpi::px(PAD);

        // 标题栏
        let brand = RECT { left: 0, top: 0, right: cw, bottom: dpi::px(BRAND_H) };

        // 头部：pt-5 起，内容高 28(标题)+20(副标题)，pb-3 收
        let head_top = dpi::px(BRAND_H) + dpi::px(HEAD_TOP);
        let head = RECT {
            left: p,
            top: head_top,
            right: cw - p,
            bottom: head_top + dpi::px(48) + dpi::px(HEAD_BOT),
        };

        // 底部
        let foot = RECT {
            left: 0,
            top: ch - dpi::px(FOOT_H),
            right: cw,
            bottom: ch,
        };

        // 工作区（列表 / 空状态共用）
        let work = RECT {
            left: p,
            top: head.bottom,
            right: cw - p,
            bottom: foot.top - dpi::px(8),
        };

        Layout { cw, ch, brand, head, work, foot, card: work }
    }
}

/// 列表区域（工作区）
fn list_rect(hwnd: HWND) -> RECT {
    compute_layout(hwnd).work
}

/// 重建命中区
fn rebuild_hits(hwnd: HWND, l: &Layout) {
    let mut hits: Vec<Hit> = Vec::new();
    let p = dpi::px(PAD);

    // 标题栏右侧窗口按钮（自绘：最小化 / 最大化 / 关闭）
    let wb = dpi::px(34);
    let mut wx = l.brand.right;
    wx -= wb;
    hits.push(Hit {
        rect: RECT { left: wx, top: l.brand.top, right: wx + wb, bottom: l.brand.bottom },
        id: HIT_CLOSE,
    });
    wx -= wb;
    hits.push(Hit {
        rect: RECT { left: wx, top: l.brand.top, right: wx + wb, bottom: l.brand.bottom },
        id: HIT_MAX,
    });
    wx -= wb;
    hits.push(Hit {
        rect: RECT { left: wx, top: l.brand.top, right: wx + wb, bottom: l.brand.bottom },
        id: HIT_MIN,
    });

    // 头部右侧三个按钮（从右往左排）
    let btn_y = l.head.top + dpi::px(16);
    let bh = dpi::px(32);
    let mut x = l.head.right;
    // 清空
    x -= dpi::px(56);
    hits.push(Hit {
        rect: RECT { left: x, top: btn_y, right: x + dpi::px(56), bottom: btn_y + bh },
        id: HIT_CLEAR,
    });
    x -= dpi::px(104) + dpi::px(8);
    hits.push(Hit {
        rect: RECT { left: x, top: btn_y, right: x + dpi::px(104), bottom: btn_y + bh },
        id: HIT_ADD_DIR,
    });
    x -= dpi::px(96) + dpi::px(8);
    hits.push(Hit {
        rect: RECT { left: x, top: btn_y, right: x + dpi::px(96), bottom: btn_y + bh },
        id: HIT_ADD_FILES,
    });

    // 底部：输出目录 / 复选框 / 开始按钮
    let fy = l.foot.top + dpi::px(20);
    // 输出目录
    let od_w = dpi::px(230);
    hits.push(Hit {
        rect: RECT { left: p + dpi::px(58), top: fy - dpi::px(12), right: p + dpi::px(58) + od_w, bottom: fy + dpi::px(16) },
        id: HIT_OUTDIR,
    });
    // 复选框
    let cb_x = p + dpi::px(58) + od_w + dpi::px(20);
    hits.push(Hit {
        rect: RECT { left: cb_x, top: fy - dpi::px(10), right: cb_x + dpi::px(200), bottom: fy + dpi::px(14) },
        id: HIT_FRONT,
    });
    // 开始转换按钮
    let bw = dpi::px(132);
    hits.push(Hit {
        rect: RECT { left: l.foot.right - p - bw, top: fy - dpi::px(14), right: l.foot.right - p, bottom: fy + dpi::px(18) },
        id: HIT_CONVERT,
    });

    // 列表行（如果非空）
    let n = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
    });
    if n > 0 {
        let rows_top = l.card.top + dpi::px(THEAD_H);
        let vis = visible_rows(hwnd);
        let scroll = get_scroll();
        // 取出任务状态，决定每行操作按钮是「文件夹」还是「✕」
        let statuses: Vec<(usize, bool)> = UI.with(|slot| {
            let slot = slot.borrow();
            slot.as_ref()
                .map(|u| {
                    u.borrow()
                        .state
                        .tasks
                        .iter()
                        .map(|t| (t.id, matches!(t.status, crate::state::Status::Done(_))))
                        .collect()
                })
                .unwrap_or_default()
        });

        for i in 0..n {
            let row_idx = scroll + i as i32;
            if row_idx < 0 || row_idx >= n as i32 {
                continue;
            }
            if i as i32 >= vis {
                break;
            }
            let top = rows_top + i as i32 * dpi::px(ROW_H);
            let is_done = statuses
                .get(row_idx as usize)
                .map(|(_, d)| *d)
                .unwrap_or(false);

            // 操作按钮：完成 → 文件夹；其他 → ✕
            let (rect, id) = if is_done {
                (
                    RECT {
                        left: l.card.right - dpi::px(64),
                        top: top + dpi::px(10),
                        right: l.card.right - dpi::px(38),
                        bottom: top + dpi::px(42),
                    },
                    HIT_ROW_OPEN + row_idx as u32,
                )
            } else {
                (
                    RECT {
                        left: l.card.right - dpi::px(54),
                        top: top + dpi::px(10),
                        right: l.card.right - dpi::px(34),
                        bottom: top + dpi::px(42),
                    },
                    HIT_ROW_DEL + row_idx as u32,
                )
            };
            hits.push(Hit { rect, id });
        }

        // 滚动条命中区（整条轨道）
        if n > vis as usize {
            let track_x = l.card.right - dpi::px(14);
            hits.push(Hit {
                rect: RECT {
                    left: track_x,
                    top: l.card.top + dpi::px(THEAD_H),
                    right: l.card.right,
                    bottom: l.card.bottom,
                },
                id: HIT_SCROLL_TRACK,
            });
        }
    }

    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            *ui.borrow_mut().hits.borrow_mut() = hits;
        }
    });
}

pub unsafe fn layout(hwnd: HWND) {
    let l = compute_layout(hwnd);
    rebuild_hits(hwnd, &l);
    // 底部进度条（真实控件，压在自绘底栏上）
    let p = dpi::px(PAD);
    if let Ok(pb) = GetDlgItem(hwnd, 106) {
        let _ = SetWindowPos(
            pb,
            None,
            p,
            l.foot.top - dpi::px(3),
            l.cw - p * 2,
            dpi::px(4),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

// ── 绘制 ──
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let l = compute_layout(hwnd);
    let cw = l.cw;
    let ch = l.ch;

    // 双缓冲
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
        FillRect(mem_dc, &RECT { left: 0, top: 0, right: cw, bottom: ch }, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        draw_brand(mem_dc, &ui, hwnd, &l);
        draw_head(mem_dc, &ui, &l);
        if ui.state.tasks.is_empty() {
            draw_empty(mem_dc, &ui, &l);
        } else {
            draw_table(mem_dc, &ui, hwnd, &l);
        }
        draw_foot(mem_dc, &ui, &l);
    });

    let _ = BitBlt(hdc, 0, 0, cw, ch, mem_dc, 0, 0, SRCCOPY);
    let _ = SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(HGDIOBJ(bmp.0));
    let _ = DeleteDC(mem_dc);
    let _ = EndPaint(hwnd, &ps);
}

/// ① 品牌条
unsafe fn draw_brand(hdc: HDC, ui: &Ui, hwnd: HWND, l: &Layout) {
    let th = &ui.theme;
    let p = dpi::px(PAD);

    // 白底 + 底边框
    let card = CreateSolidBrush(th.card);
    FillRect(hdc, &l.brand, card);
    let _ = DeleteObject(HGDIOBJ(card.0));

    // Logo：直接用 exe 资源里的应用图标
    let logo_sz = dpi::px(22);
    let logo = RECT {
        left: p,
        top: (l.brand.bottom - logo_sz) / 2,
        right: p + logo_sz,
        bottom: (l.brand.bottom + logo_sz) / 2,
    };
    if let Some(icon) = ui.app_icon {
        let _ = DrawIconEx(
            hdc,
            logo.left,
            logo.top,
            icon,
            logo_sz,
            logo_sz,
            0,
            None,
            DI_NORMAL,
        );
    } else {
        // 兜底：没有资源图标时画一个蓝色圆角方块
        fill_round_rect(hdc, &logo, dpi::px(6), th.blue);
    }

    // 标题
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.brand.0));
    let tx = logo.right + dpi::px(10);
    let title = "AnyDoc - 文档转 Markdown";
    let tw = draw_text(hdc, title, tx, l.brand.top, dpi::px(240), l.brand.bottom - l.brand.top,
        th.slate_700, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);

    // LLM Ready 徽章
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.tag.0));
    draw_pill(hdc, "LLM Ready", tx + tw + dpi::px(10), l.brand.bottom / 2,
        th.blue, th.blue_bg, ui.fonts.tag, Some(th.blue_border));
    let _ = SelectObject(hdc, old);

    // 窗口按钮（自绘）
    let hits = ui.hits.borrow();
    let maximized = is_maximized(hwnd);
    for id in [HIT_MIN, HIT_MAX, HIT_CLOSE] {
        let Some(h) = hits.iter().find(|h| h.id == id) else { continue };
        let is_hot = ui.hot.get() == Some(id);
        // hover 反馈：关闭按钮红色，其余浅灰
        if id == HIT_CLOSE {
            if is_hot {
                fill_round_rect(hdc, &h.rect, 0, th.red);
            }
        } else if is_hot {
            fill_round_rect(hdc, &h.rect, 0, th.slate_100);
        }
        let color = if id == HIT_CLOSE && is_hot {
            rgb(0xFF, 0xFF, 0xFF)
        } else {
            th.slate_500
        };
        let cx = (h.rect.left + h.rect.right) / 2;
        let cy = (h.rect.top + h.rect.bottom) / 2;
        match id {
            HIT_MIN => draw_min_icon(hdc, cx, cy, dpi::px(11), color),
            HIT_MAX => {
                if maximized {
                    draw_restore_icon(hdc, cx, cy, dpi::px(11), color);
                } else {
                    draw_max_icon(hdc, cx, cy, dpi::px(11), color);
                }
            }
            _ => draw_close_icon(hdc, cx, cy, dpi::px(11), color),
        }
    }
}

/// ② 头部（大标题 + 副标题 + 操作按钮）
unsafe fn draw_head(hdc: HDC, ui: &Ui, l: &Layout) {
    let th = &ui.theme;
    let p = dpi::px(PAD);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.h1.0));
    draw_text(hdc, "文档转 Markdown", l.head.left, l.head.top, dpi::px(400), dpi::px(28),
        th.slate_800, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.sub.0));
    // 副标题宽度不能侵占右侧按钮区
    let sub_w = (l.head.right - l.head.left - dpi::px(460)).max(dpi::px(200));
    draw_text(hdc, "将 Word、PowerPoint、Excel、PDF 等文档转换为 LLM 可直接读取的纯净 Markdown",
        l.head.left, l.head.top + dpi::px(28), sub_w, dpi::px(18),
        th.slate_500, DRAW_TEXT_FORMAT(DT_LEFT.0), true);
    let _ = SelectObject(hdc, old);

    // 三个按钮
    let hits = ui.hits.borrow();
    for (id, label, icon) in [
        (HIT_ADD_FILES, "添加文件", 0u8),
        (HIT_ADD_DIR, "添加文件夹", 1u8),
        (HIT_CLEAR, "清空", 2u8),
    ] {
        let Some(h) = hits.iter().find(|h| h.id == id) else { continue };
        let is_hot = ui.hot.get() == Some(id);
        let is_clear = id == HIT_CLEAR;
        let (fg, bg) = if is_clear {
            // 设计稿：清空 = 纯文字，无边框无底色
            (if is_hot { th.red } else { th.slate_400 }, th.card)
        } else {
            (th.slate_700, if is_hot { th.slate_100 } else { th.card })
        };
        if !is_clear {
            fill_round_rect(hdc, &h.rect, dpi::px(8), bg);
            stroke_round_rect(hdc, &h.rect, dpi::px(8), th.border, 1);
        }

        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.btn.0));
        let ty = (h.rect.top + h.rect.bottom) / 2;
        let mut tx = h.rect.left + dpi::px(12);
        match icon {
            0 => {
                draw_plus_icon(hdc, tx + dpi::px(7), ty, dpi::px(14), th.blue);
                tx += dpi::px(20);
            }
            1 => {
                draw_folder_icon(hdc, tx, ty - dpi::px(5), dpi::px(14), th.amber);
                tx += dpi::px(20);
            }
            _ => {
                tx += dpi::px(2);
            }
        }
        draw_text(hdc, label, tx, h.rect.top, h.rect.right - tx, h.rect.bottom - h.rect.top,
            fg, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
        let _ = SelectObject(hdc, old);
    }
}

/// ③ 空状态（沉浸式拖拽区）
unsafe fn draw_empty(hdc: HDC, ui: &Ui, l: &Layout) {
    let th = &ui.theme;
    let card = CreateSolidBrush(th.card);
    FillRect(hdc, &l.card, card);
    let _ = DeleteObject(HGDIOBJ(card.0));

    let hot = ui.dragging.get();
    dashed_round_rect(hdc, &l.card, dpi::px(16), if hot { th.blue } else { th.slate_300 });

    // 整块内容垂直居中（设计稿 flex-col items-center justify-center）
    // 高度 = 图标块 56 + mb-4 16 + 主文案 20 + mb-1 4 + 副文案 16 + mb-5 20 + 标签 20
    let box_sz = dpi::px(56);
    let h_main = dpi::px(20);
    let h_sub = dpi::px(16);
    let h_tag = dpi::px(20);
    let block_h = box_sz + dpi::px(16) + h_main + dpi::px(4) + h_sub + dpi::px(20) + h_tag;
    let cx = (l.card.left + l.card.right) / 2;
    let mut y = l.card.top + ((l.card.bottom - l.card.top) - block_h) / 2;

    // 图标块
    let box_rect = RECT {
        left: cx - box_sz / 2,
        top: y,
        right: cx + box_sz / 2,
        bottom: y + box_sz,
    };
    fill_round_rect(hdc, &box_rect, dpi::px(16), th.blue_bg);
    draw_cloud_upload_icon(hdc, cx, y + box_sz / 2, dpi::px(32), th.blue);
    y += box_sz + dpi::px(16);

    // 主文案 text-sm(14) font-semibold slate-700
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
    let t1 = "将文档或文件夹拖入此处";
    let sz1 = measure_text(hdc, t1);
    draw_text(hdc, t1, cx - sz1.cx / 2, y, sz1.cx + 4, h_main,
        th.slate_700, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);
    y += h_main + dpi::px(4);

    // 副文案 text-xs(12) slate-400
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.meta.0));
    let t2 = "或者通过上方按钮浏览本地文件，支持批量队列处理";
    let sz2 = measure_text(hdc, t2);
    draw_text(hdc, t2, cx - sz2.cx / 2, y, sz2.cx + 4, h_sub,
        th.text_mute, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);
    y += h_sub + dpi::px(20);

    // 格式胶囊标签
    let tags = ["DOCX", "PDF", "PPTX", "XLSX", "CSV", "EPUB", "RTF", "ODT/ODS/ODP"];
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.mono.0));
    let mut widths = Vec::new();
    for t in tags {
        let sz = measure_text(hdc, t);
        widths.push(sz.cx + dpi::px(16));
    }
    let gap = dpi::px(6);
    let total: i32 = widths.iter().sum::<i32>() + gap * (tags.len() as i32 - 1);
    let mut tx = cx - total / 2;
    for (i, t) in tags.iter().enumerate() {
        let r = RECT { left: tx, top: y, right: tx + widths[i], bottom: y + dpi::px(20) };
        fill_round_rect(hdc, &r, dpi::px(4), th.slate_100);
        draw_text(hdc, t, r.left, r.top, r.right - r.left, r.bottom - r.top,
            th.text_dim, DRAW_TEXT_FORMAT(DT_CENTER.0), false);
        tx += widths[i] + gap;
    }
    let _ = SelectObject(hdc, old);
}

/// ④ 表格（列表态）
unsafe fn draw_table(hdc: HDC, ui: &Ui, hwnd: HWND, l: &Layout) {
    let th = &ui.theme;
    let p = dpi::px(PAD);

    // 卡片
    let card = CreateSolidBrush(th.card);
    FillRect(hdc, &l.card, card);
    let _ = DeleteObject(HGDIOBJ(card.0));
    stroke_round_rect(hdc, &l.card, dpi::px(12), th.border, 1);

    // 列位置
    let name_x = l.card.left + dpi::px(16);
    let name_w = (l.card.right - l.card.left) * 46 / 100;
    let size_r = l.card.left + (l.card.right - l.card.left) * 62 / 100;
    let status_x = l.card.left + (l.card.right - l.card.left) * 64 / 100;
    let status_w = (l.card.right - l.card.left) * 26 / 100;

    // 表头
    let hr = RECT { left: l.card.left, top: l.card.top, right: l.card.right, bottom: l.card.top + dpi::px(THEAD_H) };
    let hb = CreateSolidBrush(th.header_bg);
    FillRect(hdc, &hr, hb);
    let _ = DeleteObject(HGDIOBJ(hb.0));
    hline(hdc, hr.left, hr.right, hr.bottom - 1, th.border);

    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.thead.0));
    // 设计稿表头用了 tracking-wider（字间距）
    SetTextCharacterExtra(hdc, dpi::px(1));
    let hy = hr.top;
    draw_text(hdc, "文档名称", name_x, hy, name_w, hr.bottom - hy, th.text_dim,
        DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    draw_text(hdc, "大小", size_r - dpi::px(60), hy, dpi::px(60), hr.bottom - hy, th.text_dim,
        DRAW_TEXT_FORMAT(DT_RIGHT.0), false);
    draw_text(hdc, "转换状态", status_x, hy, status_w, hr.bottom - hy, th.text_dim,
        DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    draw_text(hdc, "操作", l.card.right - dpi::px(76), hy, dpi::px(44), hr.bottom - hy, th.text_dim,
        DRAW_TEXT_FORMAT(DT_RIGHT.0), false);
    SetTextCharacterExtra(hdc, 0);
    let _ = SelectObject(hdc, old);

    // 行
    let tasks = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.clone()).unwrap_or_default()
    });
    let scroll = get_scroll();
    let vis = visible_rows(hwnd);
    let rows_top = l.card.top + dpi::px(THEAD_H);
    let row_h = dpi::px(ROW_H);
    let sel = ui.selected.get();

    for (i, task) in tasks.iter().enumerate().skip(scroll.max(0) as usize).take(vis as usize) {
        let top = rows_top + (i as i32 - scroll) * row_h;
        if top + row_h > l.card.bottom {
            break;
        }
        let rr = RECT { left: l.card.left, top, right: l.card.right, bottom: top + row_h };

        // 行背景
        let bg = if Some(i) == sel {
            th.row_active
        } else if task.status == Status::Running {
            th.row_active
        } else {
            th.card
        };
        let b = CreateSolidBrush(bg);
        FillRect(hdc, &rr, b);
        let _ = DeleteObject(HGDIOBJ(b.0));

        let cy = top + row_h / 2;

        // 扩展名徽章
        let (fg, bgc, bd) = ext_style(th, &task.ext);
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.mono.0));
        let tag = task.ext.to_uppercase();
        let tw = draw_pill(hdc, &tag, name_x, cy, fg, bgc, ui.fonts.mono, bd);
        let _ = SelectObject(hdc, old);

        // 文件名
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.name.0));
        draw_text(hdc, &task.name, name_x + tw + dpi::px(10), top + dpi::px(11),
            name_w - tw - dpi::px(20), dpi::px(19), th.slate_700,
            DRAW_TEXT_FORMAT(DT_LEFT.0), true);
        let _ = SelectObject(hdc, old);

        // 大小
        let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.mono.0));
        draw_text(hdc, &crate::state::fmt_size(task.size),
            size_r - dpi::px(60), cy - dpi::px(10), dpi::px(60), dpi::px(20),
            th.text_dim, DRAW_TEXT_FORMAT(DT_RIGHT.0), false);
        let _ = SelectObject(hdc, old);

        // 状态
        match &task.status {
            Status::Running => {
                // 进行中：阶段文字 + 该文件独立的真实进度条
                let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.meta.0));
                let phase = if task.phase.is_empty() { "正在转换".to_string() } else { task.phase.clone() };
                draw_text(hdc, &phase, status_x, cy - dpi::px(17),
                    status_w - dpi::px(70), dpi::px(17), th.blue,
                    DRAW_TEXT_FORMAT(DT_LEFT.0), true);
                // 百分比（右对齐）；不确定阶段不显示数字
                if task.phase != "解析转换" {
                    draw_text(hdc, &format!("{}%", task.progress),
                        status_x + status_w - dpi::px(70), cy - dpi::px(17),
                        dpi::px(70), dpi::px(17), th.blue,
                        DRAW_TEXT_FORMAT(DT_RIGHT.0), false);
                }
                let _ = SelectObject(hdc, old);

                // 进度条
                let bar = RECT {
                    left: status_x,
                    top: cy + dpi::px(3),
                    right: status_x + status_w - dpi::px(70),
                    bottom: cy + dpi::px(9),
                };
                let bar_w = bar.right - bar.left;
                let tb = CreateSolidBrush(th.slate_100);
                FillRect(hdc, &bar, tb);
                let _ = DeleteObject(HGDIOBJ(tb.0));

                // 解析阶段无法细分进度（官方内核是一次性调用），
                // 用不确定态动画表示「正在进行」，而不是卡在一个数字上。
                let indeterminate = task.phase == "解析转换";
                let pb = CreateSolidBrush(th.blue);
                if indeterminate {
                    // 三角波往返
                    let t = (GetTickCount64() % 1600) as f64 / 1600.0;
                    let wave = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
                    let seg = bar_w / 4;
                    let x = bar.left + ((bar_w - seg) as f64 * wave) as i32;
                    FillRect(
                        hdc,
                        &RECT { left: x, top: bar.top, right: x + seg, bottom: bar.bottom },
                        pb,
                    );
                } else {
                    // 真实百分比（读取 / 写入阶段）
                    let pw = bar_w * task.progress.min(100) as i32 / 100;
                    if pw > 0 {
                        FillRect(
                            hdc,
                            &RECT { left: bar.left, top: bar.top, right: bar.left + pw, bottom: bar.bottom },
                            pb,
                        );
                    }
                }
                let _ = DeleteObject(HGDIOBJ(pb.0));
            }
            Status::Done(_) | Status::Error(_) | Status::Wait => {
                let (text, fg, bgc, _) = status_style(th, &task.status);
                let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.meta.0));
                // 成功带 ✓
                let prefix = if matches!(task.status, Status::Done(_)) { "✓ " } else { "" };
                draw_text(hdc, &format!("{prefix}{text}"), status_x, cy - dpi::px(10),
                    status_w - dpi::px(60), dpi::px(20),
                    if matches!(task.status, Status::Wait) { th.text_dim } else { fg },
                    DRAW_TEXT_FORMAT(DT_LEFT.0), true);
                let _ = SelectObject(hdc, old);
            }
        }

        // 操作列：按状态切换
        //   转换完成 → 文件夹图标（打开所在文件夹）
        //   转换中 / 排队中 / 失败 → ✕（移除该条）
        let is_done = matches!(task.status, Status::Done(_));
        let hot_open = ui.hot.get() == Some(HIT_ROW_OPEN + i as u32);
        let hot_del = ui.hot.get() == Some(HIT_ROW_DEL + i as u32);

        if is_done {
            draw_folder_icon(hdc, l.card.right - dpi::px(56), cy - dpi::px(8), dpi::px(16),
                if hot_open { th.blue } else { th.amber });
        } else {
            let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.btn.0));
            draw_text(hdc, "✕", l.card.right - dpi::px(54), cy - dpi::px(10), dpi::px(18), dpi::px(20),
                if hot_del { th.red } else { th.slate_300 },
                DRAW_TEXT_FORMAT(DT_CENTER.0), false);
            let _ = SelectObject(hdc, old);
        }

        // 分隔线
        hline(hdc, rr.left + dpi::px(12), rr.right - dpi::px(12), rr.bottom - 1, th.border);
    }

    // ── 滚动条（自绘，Win11 现代样式：细、无箭头、圆角滑块）──
    if tasks.len() > vis as usize {
        let track_x = l.card.right - dpi::px(10);
        let track_w = dpi::px(5);
        let track_y = rows_top;
        let track_h = l.card.bottom - rows_top - dpi::px(4);

        // 滑块尺寸与位置
        let ratio = vis as f64 / tasks.len() as f64;
        let thumb_h = ((ratio * track_h as f64) as i32).max(dpi::px(28));
        let max_scroll = (tasks.len() as i32 - vis).max(1);
        let pos = (scroll as f64 / max_scroll as f64).clamp(0.0, 1.0);
        let thumb_y = track_y + ((pos * (track_h - thumb_h) as f64) as i32);

        // 滑块（拖动时高亮）
        let dragging = ui.dragging_scroll.get();
        let hovered = ui.hot.get() == Some(HIT_SCROLL_TRACK);
        let color = if dragging || hovered { th.slate_400 } else { th.slate_300 };
        let rect = RECT {
            left: track_x,
            top: thumb_y,
            right: track_x + track_w,
            bottom: thumb_y + thumb_h,
        };
        fill_round_rect(hdc, &rect, track_w / 2, color);
    }

}

/// ⑤ 底部栏
unsafe fn draw_foot(hdc: HDC, ui: &Ui, l: &Layout) {
    let th = &ui.theme;
    let p = dpi::px(PAD);

    // 白底 + 上边框
    let card = CreateSolidBrush(th.card);
    FillRect(hdc, &l.foot, card);
    let _ = DeleteObject(HGDIOBJ(card.0));
    hline(hdc, l.foot.left, l.foot.right, l.foot.top, th.border);

    let fy = l.foot.top + dpi::px(20);

    // 输出目录
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.foot.0));
    draw_text(hdc, "输出目录:", p, fy - dpi::px(10), dpi::px(56), dpi::px(20),
        th.text_mute, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);

    let od_x = p + dpi::px(58);
    let od_w = dpi::px(230);
    let hot_out = ui.hot.get() == Some(HIT_OUTDIR);
    let od_rect = RECT { left: od_x, top: fy - dpi::px(13), right: od_x + od_w, bottom: fy + dpi::px(15) };
    fill_round_rect(hdc, &od_rect, dpi::px(6), if hot_out { th.slate_200 } else { th.slate_100 });

    let out_label = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref()
            .and_then(|u| u.borrow().state.out_dir.as_ref().map(|d| d.display().to_string()))
            .unwrap_or_else(|| "与源文件同目录".to_string())
    });
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.mono.0));
    draw_folder_icon(hdc, od_x + dpi::px(10), fy - dpi::px(6), dpi::px(14), th.slate_400);
    draw_text(hdc, &out_label, od_x + dpi::px(28), fy - dpi::px(13), od_w - dpi::px(82), dpi::px(28),
        th.slate_700, DRAW_TEXT_FORMAT(DT_LEFT.0), true);
    // 「更改」
    draw_text(hdc, "更改", od_rect.right - dpi::px(38), fy - dpi::px(13), dpi::px(32), dpi::px(28),
        th.blue, DRAW_TEXT_FORMAT(DT_RIGHT.0), false);
    let _ = SelectObject(hdc, old);

    // 复选框
    let cb_x = od_x + od_w + dpi::px(20);
    let checked = ui.inject_frontmatter.get();
    draw_checkbox(hdc, cb_x, fy, checked, th);
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.foot.0));
    draw_text(hdc, "注入 YAML Frontmatter", cb_x + dpi::px(22), fy - dpi::px(10), dpi::px(180), dpi::px(20),
        th.slate_600, DRAW_TEXT_FORMAT(DT_LEFT.0), false);
    let _ = SelectObject(hdc, old);

    // 状态文字
    let st = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.status.clone()).unwrap_or_default()
    });
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.foot.0));
    let stw = draw_text(hdc, &st, l.foot.right - p - dpi::px(150) - dpi::px(140), fy - dpi::px(10),
        dpi::px(140), dpi::px(20), th.text_mute, DRAW_TEXT_FORMAT(DT_RIGHT.0), true);
    let _ = SelectObject(hdc, old);

    // 开始转换按钮
    let bw = dpi::px(132);
    let bh = dpi::px(34);
    let br = RECT {
        left: l.foot.right - p - bw,
        top: fy - dpi::px(16),
        right: l.foot.right - p,
        bottom: fy - dpi::px(16) + bh,
    };
    let running = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.running).unwrap_or(false)
    });
    let (bg, fg) = if running { (th.slate_300, th.card) } else { (th.blue, rgb(0xFF, 0xFF, 0xFF)) };
    fill_round_rect(hdc, &br, dpi::px(8), bg);
    let old: HGDIOBJ = SelectObject(hdc, HGDIOBJ(ui.fonts.big.0));
    draw_text(hdc, if running { "转换中…" } else { "开始转换" },
        br.left, br.top, bw, bh, fg,
        DRAW_TEXT_FORMAT(DT_CENTER.0), false);
    let _ = SelectObject(hdc, old);
}

// ── 交互 ──
unsafe fn on_click(hwnd: HWND, lp: LPARAM) {
    let x = (lp.0 & 0xFFFF) as i16 as i32;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;

    let l = compute_layout(hwnd);

    // 滚动条拖动
    if x >= l.card.right - dpi::px(SCROLLBAR_W) && x <= l.card.right {
        UI.with(|slot| {
            let slot = slot.borrow();
            if let Some(ui) = slot.as_ref() {
                let mut u = ui.borrow_mut();
                u.dragging_scroll.set(true);
                u.scroll_anchor.set(y - u.scroll.get());
                let _ = SetCapture(hwnd);
            }
        });
        return;
    }

    // 命中检测
    let hit = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref()
            .and_then(|ui| {
                ui.borrow()
                    .hits
                    .borrow()
                    .iter()
                    .find(|h| {
                        x >= h.rect.left && x < h.rect.right
                            && y >= h.rect.top && y < h.rect.bottom
                    })
                    .map(|h| h.id)
            })
    });
    let Some(id) = hit else {
        // 点在标题栏空白处 → 交给系统处理拖动
        if y >= l.brand.top && y < l.brand.bottom {
            let _ = ReleaseCapture();
            let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
        }
        return;
    };

    // 命中滚动条 → 按下即跳到对应位置，并开始拖动
    if id == HIT_SCROLL_TRACK {
        let rect = list_rect(hwnd);
        let count = UI.with(|slot| {
            let slot = slot.borrow();
            slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
        }) as i32;
        let vis = visible_rows(hwnd);
        if count > vis {
            let track_y = rect.top + dpi::px(THEAD_H);
            let track_h = rect.bottom - track_y - dpi::px(4);
            let ratio_h = vis as f64 / count as f64;
            let thumb_h = ((ratio_h * track_h as f64) as i32).max(dpi::px(28));
            let travel = (track_h - thumb_h).max(1);
            let max_scroll = count - vis;

            // 指针在轨道上的位置 → 目标滚动值
            let pos = ((y - track_y - thumb_h / 2) as f64 / travel as f64).clamp(0.0, 1.0);
            let new_scroll = (pos * max_scroll as f64).round() as i32;

            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    u.dragging_scroll.set(true);
                    u.scroll_anchor.set(y - u.scroll.get());
                }
            });
            let _ = SetCapture(hwnd);
            set_scroll(hwnd, new_scroll);
        }
        return;
    }

    match id {
        HIT_MIN => {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
        HIT_MAX => {
            let maximized = is_maximized(hwnd);
            let _ = ShowWindow(hwnd, if maximized { SW_RESTORE } else { SW_MAXIMIZE });
            layout(hwnd);
            let _ = InvalidateRect(hwnd, None, true);
        }
        HIT_CLOSE => {
            let _ = DestroyWindow(hwnd);
        }
        HIT_ADD_FILES => {
            if let Some(paths) = crate::dialog::pick_files(hwnd) {
                add_paths(hwnd, &paths, "");
            }
        }
        HIT_ADD_DIR => {
            if let Some(dir) = crate::dialog::pick_folder(hwnd) {
                add_folder(hwnd, &dir);
            }
        }
        HIT_CLEAR => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    u.state.clear();
                    u.icons.borrow_mut().clear();
                    u.scroll.set(0);
                    u.selected.set(None);
                }
            });
            refresh_list(hwnd);
            update_status(hwnd);
        }
        HIT_OUTDIR => {
            if let Some(dir) = crate::dialog::pick_folder(hwnd) {
                UI.with(|slot| {
                    let slot = slot.borrow();
                    if let Some(ui) = slot.as_ref() {
                        let mut u = ui.borrow_mut();
                        u.state.out_dir = Some(dir.clone());
                        u.state.out_confirmed = true;
                    }
                });
                update_status(hwnd);
            }
        }
        HIT_FRONT => {
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    u.inject_frontmatter.set(!u.inject_frontmatter.get());
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
        }
        HIT_CONVERT => {
            start_convert(hwnd);
        }
        id if id >= HIT_ROW_DEL && id < HIT_ROW_DEL + 1000 => {
            let idx = (id - HIT_ROW_DEL) as usize;
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    if idx < u.state.tasks.len() {
                        // 通知后台线程停止（如果正在转换）
                        u.state.tasks[idx].cancel();
                        u.state.tasks.remove(idx);
                        u.state.recount();
                        u.selected.set(None);
                    }
                }
            });
            load_icons();
            refresh_list(hwnd);
            update_status(hwnd);
        }
        id if id >= HIT_ROW_OPEN && id < HIT_ROW_OPEN + 1000 => {
            let idx = (id - HIT_ROW_OPEN) as usize;
            // 打开「输出文件所在文件夹」——优先用生成的 .md 路径
            let path = UI.with(|slot| {
                let slot = slot.borrow();
                slot.as_ref().and_then(|u| {
                    let u = u.borrow();
                    u.state.tasks.get(idx).and_then(|t| {
                        t.out_path
                            .clone()
                            .or_else(|| Some(t.path.clone()))
                    })
                })
            });
            if let Some(p) = path {
                reveal_in_folder(&p);
            }
        }
        _ => {}
    }
}

unsafe fn on_mouse_move(hwnd: HWND, lp: LPARAM) {
    let x = (lp.0 & 0xFFFF) as i16 as i32;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;

    let dragging = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().dragging_scroll.get()).unwrap_or(false)
    });

    if dragging {
        let l = compute_layout(hwnd);
        let track_y = l.card.top + dpi::px(THEAD_H);
        let track_h = l.card.bottom - track_y - dpi::px(4);
        let vis = visible_rows(hwnd);
        let count = UI.with(|slot| {
            let slot = slot.borrow();
            slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
        }) as i32;
        if count > vis && track_h > 0 {
            let ratio = vis as f64 / count as f64;
            let thumb_h = (((ratio * track_h as f64) as i32).max(dpi::px(28))).min(track_h);
            let max_scroll = count - vis;
            let travel = (track_h - thumb_h).max(1);
            let anchor = UI.with(|slot| {
                let slot = slot.borrow();
                slot.as_ref().map(|u| u.borrow().scroll_anchor.get()).unwrap_or(0)
            });
            let ratio_pos = (y - anchor) as f64 / travel as f64;
            let new_scroll = ((ratio_pos * max_scroll as f64).round() as i32).clamp(0, max_scroll);
            set_scroll(hwnd, new_scroll);
        }
        return;
    }

    // hover 检测
    let hot = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().and_then(|ui| {
            ui.borrow()
                .hits
                .borrow()
                .iter()
                .find(|h| x >= h.rect.left && x < h.rect.right && y >= h.rect.top && y < h.rect.bottom)
                .map(|h| h.id)
        })
    });
    let changed = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|ui| {
            let mut u = ui.borrow_mut();
            if u.hot.get() != hot {
                u.hot.set(hot);
                true
            } else {
                false
            }
        })
    });
    if changed == Some(true) {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

unsafe fn on_key(hwnd: HWND, key: u32) {
    let count = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
    });
    if count == 0 {
        return;
    }
    match key {
        0x26 => { // 上
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    let cur = u.selected.get().unwrap_or(0);
                    u.selected.set(Some(cur.saturating_sub(1)));
                    ensure_visible(hwnd, u.scroll.get());
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
        }
        0x28 => { // 下
            UI.with(|slot| {
                let slot = slot.borrow();
                if let Some(ui) = slot.as_ref() {
                    let mut u = ui.borrow_mut();
                    let cur = u.selected.get().unwrap_or(0);
                    u.selected.set(Some((cur + 1).min(count - 1)));
                    ensure_visible(hwnd, u.scroll.get());
                }
            });
            let _ = InvalidateRect(hwnd, None, false);
        }
        0x2E => { // Delete
            let sel = UI.with(|slot| {
                let slot = slot.borrow();
                slot.as_ref().and_then(|u| u.borrow().selected.get())
            });
            if let Some(idx) = sel {
                UI.with(|slot| {
                    let slot = slot.borrow();
                    if let Some(ui) = slot.as_ref() {
                        let mut u = ui.borrow_mut();
                        if idx < u.state.tasks.len() {
                            u.state.tasks.remove(idx);
                            u.state.recount();
                            u.selected.set(None);
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

// ── 转换 ──
unsafe fn start_convert(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return };
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
        let inject = u.inject_frontmatter.get();
        let rx = worker::spawn(tasks, out, inject);
        *u.rx.borrow_mut() = Some(rx);
    });
    refresh_list(hwnd);
    update_status(hwnd);
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
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if !crate::state::is_supported(&ext) {
                    continue;
                }
                let rel = path
                    .parent()
                    .and_then(|p| p.strip_prefix(&base).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let rel = if rel.is_empty() { String::new() } else { format!("{}/", rel) };
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
        for ic in guard.icons.borrow().iter().flatten() {
            icons::destroy(ic);
        }
        let mut list = guard.icons.borrow_mut();
        list.clear();
        for t in tasks {
            list.push(icons::load(&t.path, &t.ext));
        }
    });
}

pub unsafe fn refresh_list(hwnd: HWND) {
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            let vis = visible_rows(hwnd);
            let max_scroll = (u.state.tasks.len() as i32 - vis).max(0);
            if u.scroll.get() > max_scroll {
                u.scroll.set(max_scroll);
            }
            if u.selected.get().map(|s| s >= u.state.tasks.len()).unwrap_or(false) {
                u.selected.set(None);
            }
        }
    });
    layout(hwnd);
    let _ = InvalidateRect(hwnd, None, true);
}

pub unsafe fn update_status(hwnd: HWND) {
    let text = UI.with(|slot| {
        let slot = slot.borrow();
        let Some(ui) = slot.as_ref() else { return String::new() };
        let u = ui.borrow();
        if u.state.running {
            let s = &u.state.stats;
            format!(
                "正在转换 ({}/{}) · 总进度 {}%",
                s.settled(),
                s.total,
                s.percent()
            )
        } else {
            u.state.status.clone()
        }
    });
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.borrow_mut().state.status = text;
        }
    });
    if let Ok(pb) = GetDlgItem(hwnd, 106) {
        let pct = UI.with(|slot| {
            let slot = slot.borrow();
            slot.as_ref().map(|u| u.borrow().state.stats.percent()).unwrap_or(0)
        });
        SendMessageW(pb, PBM_SETPOS, WPARAM(pct as usize), LPARAM(0));
    }
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe fn tick(hwnd: HWND) {
    let mut finished: Vec<(usize, u64, Result<(u64, String), String>)> = Vec::new();
    let mut progress: Vec<(usize, u8, String)> = Vec::new();
    let mut started: Vec<usize> = Vec::new();
    let mut all_done = false;

    UI.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(ui) = slot.as_ref() else { return };
        let mut rx = ui.borrow_mut().rx.borrow_mut().take();
        if let Some(ref r) = rx {
            while let Ok(msg) = r.try_recv() {
                match msg {
                    WorkerMsg::Finished { id, elapsed_ms, result } => finished.push((id, elapsed_ms, result)),
                    WorkerMsg::Progress { id, percent, phase } => progress.push((id, percent, phase)),
                    WorkerMsg::Started { id } => started.push(id),
                    WorkerMsg::AllDone => all_done = true,
                }
            }
        }
        *ui.borrow_mut().rx.borrow_mut() = rx;
    });

    if finished.is_empty() && progress.is_empty() && !all_done {
        return;
    }

    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            let mut u = ui.borrow_mut();
            for id in started {
                if let Some(t) = u.state.tasks.iter_mut().find(|t| t.id == id) {
                    if matches!(t.status, Status::Wait) {
                        t.status = Status::Running;
                        t.progress = 0;
                        t.phase = "准备中".to_string();
                    }
                }
            }
            for (id, percent, phase) in progress {
                if let Some(t) = u.state.tasks.iter_mut().find(|t| t.id == id) {
                    if matches!(t.status, Status::Running) {
                        t.progress = percent;
                        t.phase = phase;
                    }
                }
            }
            for (id, elapsed_ms, result) in finished {
                let root = u.state.out_dir.clone();
                if let Some(t) = u.state.tasks.iter_mut().find(|t| t.id == id) {
                    match result {
                        Ok((size, _)) => {
                            t.status = Status::Done(elapsed_ms);
                            t.out_size = size;
                            t.progress = 100;
                            // 记录输出路径，用于「打开所在文件夹」
                            let root = root.unwrap_or_else(|| {
                                t.path
                                    .parent()
                                    .map(|p| p.to_path_buf())
                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                            });
                            t.out_path = Some(t.output_path(&root));
                        }
                        Err(e) => {
                            t.status = Status::Error(e);
                            t.progress = 100;
                        }
                    }
                }
            }
            u.state.recount();
            if all_done || (u.state.stats.total > 0 && u.state.stats.settled() == u.state.stats.total) {
                u.state.running = false;
                let s = u.state.stats.clone();
                u.state.status = format!("{} 成功 · {} 失败", s.done, s.failed);
            }
        }
    });

    refresh_list(hwnd);
    update_status(hwnd);
}

// ── 滚动辅助 ──
fn get_scroll() -> i32 {
    UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().scroll.get()).unwrap_or(0)
    })
}

fn set_scroll(hwnd: HWND, pos: i32) {
    let max = max_scroll(hwnd);
    let pos = pos.max(0).min(max);
    UI.with(|slot| {
        let slot = slot.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.borrow_mut().scroll.set(pos);
        }
    });
    unsafe {
        let _ = InvalidateRect(hwnd, None, false);
    }
}


fn max_scroll(hwnd: HWND) -> i32 {
    let count = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().map(|u| u.borrow().state.tasks.len()).unwrap_or(0)
    }) as i32;
    (count - visible_rows(hwnd)).max(0)
}

fn visible_rows(hwnd: HWND) -> i32 {
    let l = compute_layout(hwnd);
    let rows_h = l.card.bottom - (l.card.top + dpi::px(THEAD_H));
    (rows_h / dpi::px(ROW_H)).max(1)
}

fn ensure_visible(hwnd: HWND, scroll: i32) {
    let vis = visible_rows(hwnd);
    let sel = UI.with(|slot| {
        let slot = slot.borrow();
        slot.as_ref().and_then(|u| u.borrow().selected.get())
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
            ui.borrow_mut().scroll.set(new_scroll);
        }
    });
}

/// 在文件管理器中定位文件
unsafe fn reveal_in_folder(path: &std::path::Path) {
    let wide = to_wide(&path.display().to_string());
    let verb = to_wide("explore");
    let file = to_wide(&path.display().to_string());
    ShellExecuteW(
        HWND(std::ptr::null_mut()),
        PCWSTR(verb.as_ptr()),
        PCWSTR(file.as_ptr()),
        PCWSTR(std::ptr::null()),
        PCWSTR(std::ptr::null()),
        SW_SHOWNORMAL,
    );
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
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

/// 检测系统是否为深色模式
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


/// 预加载文件（--gui 参数用）
pub unsafe fn preload_files(hwnd: HWND, paths: &[PathBuf]) {
    add_paths(hwnd, paths, "");
}

/// 预加载目录（--gui 参数用）
pub unsafe fn preload_folder(hwnd: HWND, dir: &PathBuf) {
    add_folder(hwnd, dir);
}



/// 窗口是否最大化
unsafe fn is_maximized(hwnd: HWND) -> bool {
    let mut rc = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rc);
    // 最大化时窗口 rect 等于工作区
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut work as *mut RECT as *mut std::ffi::c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    (rc.right - rc.left) >= (work.right - work.left)
        && (rc.bottom - rc.top) >= (work.bottom - work.top)
}


