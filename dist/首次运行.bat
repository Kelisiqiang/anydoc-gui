@echo off
chcp 65001 >nul 2>&1
title AnyDoc 启动器

rem ============================================================
rem  解除 Windows 的「来自网络」标记（Zone.Identifier）
rem  这是 SmartScreen 弹窗的直接原因
rem ============================================================

echo.
echo   AnyDoc - 文档转 Markdown
echo   ----------------------------------------
echo.

rem 解除当前目录下所有文件的网络标记
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "Unblock-File -Path '%~dp0*' -ErrorAction SilentlyContinue" 2>nul

rem 同时解除 exe 自身
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "Unblock-File -Path '%~dp0AnyDoc.exe' -ErrorAction SilentlyContinue" 2>nul

echo   [1/2] 已解除网络标记
echo.

rem 检查 exe 是否存在
if not exist "%~dp0AnyDoc.exe" (
  echo   [错误] 找不到 AnyDoc.exe
  echo   请确认解压完整后再运行本文件。
  echo.
  pause
  exit /b 1
)

rem 如果仍被拦截，尝试加入 Defender 排除项（需要管理员权限）
echo   [2/2] 检查 Defender 拦截...
net session >nul 2>&1
if %errorlevel% equ 0 (
  echo         检测到管理员权限，尝试加入 Defender 排除项...
  powershell -NoProfile -ExecutionPolicy Bypass -Command ^
    "Add-MpPreference -ExclusionPath '%~dp0' -ErrorAction SilentlyContinue" 2>nul
  echo         已加入排除项
) else (
  echo         未以管理员运行，已跳过 Defender 排除项
  echo         如仍被拦截，请右键本文件选择「以管理员身份运行」
)

echo.
echo   正在启动 AnyDoc...
echo.

start "" "%~dp0AnyDoc.exe"

rem 若 3 秒后仍有拦截提示，说明需要信誉积累或代码签名
exit /b 0
