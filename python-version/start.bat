@echo off
cd /d "%~dp0"

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

%PY% -c "import flask, anydoc" >nul 2>nul
if errorlevel 1 (
  echo First run: installing dependencies, needs internet, about 1-2 min...
  %PY% -m pip install --disable-pip-version-check -r requirements.txt
  if errorlevel 1 (
    echo [ERROR] Dependency install failed. Check your network and retry.
    pause
    exit /b 1
  )
)

echo Starting AnyDoc converter, browser will open automatically...
echo Keep this window open. Closing it exits the app.
%PY% app.py
pause