@echo off
cd /d "%~dp0"
echo Installing/updating AnyDoc converter dependencies...
set "PY="
where py >nul 2>nul && set "PY=py -3"
if not defined PY (
  where python >nul 2>nul && set "PY=python"
)
if not defined PY (
  echo [ERROR] Python 3.10+ not found. Install it first: https://www.python.org/downloads/
  pause
  exit /b 1
)
%PY% -m pip install --upgrade pip
%PY% -m pip install -r requirements.txt
echo.
echo Done. Double-click start.vbs to use the converter.
pause