@echo off
chcp 65001 >nul 2>&1
title 解除 AnyDoc 的 Windows 安全警告

rem ============================================================
rem  为当前用户把 AnyDoc 目录加入 Microsoft Defender 排除项
rem  需要「以管理员身份运行」
rem ============================================================

net session >nul 2>&1
if %errorlevel% neq 0 (
  echo.
  echo   需要管理员权限！
  echo   请右键本文件，选择「以管理员身份运行」。
  echo.
  pause
  exit /b 1
)

echo.
echo   正在把 AnyDoc 加入 Microsoft Defender 排除项...
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "Add-MpPreference -ExclusionPath '%~dp0' -ErrorAction SilentlyContinue; Add-MpPreference -ExclusionProcess '%~dp0AnyDoc.exe' -ErrorAction SilentlyContinue"

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "Unblock-File -Path '%~dp0*' -ErrorAction SilentlyContinue"

echo.
echo   完成。现在可以正常启动 AnyDoc 了。
echo.
pause
