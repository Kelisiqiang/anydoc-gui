# -*- coding: utf-8 -*-
"""
AnyDoc 本地转换器 —— 桌面窗口入口
用 pywebview 提供原生应用窗口（Windows 基于系统自带 WebView2），
加载本地 Flask 服务，界面即为软件窗口本身，不借助浏览器。
关闭窗口即退出程序。
"""

import ctypes
import os
import sys
import threading
import time
import traceback
from pathlib import Path

import webview

import app as server

APP_TITLE = "AnyDoc 本地转换器"

if getattr(sys, "frozen", False):
    RES_DIR = Path(getattr(sys, "_MEIPASS", Path(sys.executable).resolve().parent))
else:
    RES_DIR = Path(__file__).resolve().parent


def apply_window_icon():
    """把 anydoc.ico 设成窗口左上角（标题栏/任务栏）图标。

    大小图标必须分别加载对应尺寸的帧，否则系统拿 16x16 的小图放大
    到标题栏/任务栏显示，就会模糊。
    """
    try:
        ico = RES_DIR / "anydoc.ico"
        if not ico.exists():
            return
        user32 = ctypes.windll.user32
        IMAGE_ICON = 1
        LR_LOADFROMFILE = 0x10
        WM_SETICON = 0x0080
        ICON_SMALL = 0
        ICON_BIG = 1
        # 大图标（标题栏/任务栏）用小尺寸会放大变糊，统一按真实尺寸加载
        hicon_small = user32.LoadImageW(None, str(ico), IMAGE_ICON, 16, 16, LR_LOADFROMFILE)
        hicon_big = user32.LoadImageW(None, str(ico), IMAGE_ICON, 48, 48, LR_LOADFROMFILE)
        if not hicon_small or not hicon_big:
            return

        found = []

        @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
        def enum_cb(hwnd, _lparam):
            buf = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, buf, 256)
            if buf.value.startswith(APP_TITLE):
                found.append(hwnd)
                return False
            return True

        user32.EnumWindows(enum_cb, 0)
        for hwnd in found:
            user32.SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon_small)
            user32.SendMessageW(hwnd, WM_SETICON, ICON_BIG, hicon_big)
    except Exception:
        pass


def main():
    try:
        # Flask 服务在后台线程运行
        threading.Thread(
            target=server.start_server,
            kwargs={"open_browser": False},
            daemon=True,
        ).start()

        # 等待服务就绪
        port = None
        for _ in range(300):
            if server.SERVER_PORT["value"]:
                port = server.SERVER_PORT["value"]
                break
            time.sleep(0.05)
        if not port:
            raise RuntimeError("本地服务启动失败")

        window = webview.create_window(
            APP_TITLE,
            f"http://127.0.0.1:{port}",
            width=1060,
            height=880,
            min_size=(900, 640),
            resizable=True,
            background_color="#f5f6f8",
        )

        def on_loaded():
            apply_window_icon()

        window.events.loaded += on_loaded
        # 阻塞直到窗口关闭
        webview.start(debug=False)
    except Exception:
        try:
            with open(server.APP_DIR / "AnyDocConverter-error.log", "w", encoding="utf-8") as f:
                traceback.print_exc(file=f)
        except Exception:
            pass
    finally:
        try:
            os._exit(0)  # 窗口关闭即结束进程（含后台 Flask 线程）
        except Exception:
            pass


if __name__ == "__main__":
    main()
