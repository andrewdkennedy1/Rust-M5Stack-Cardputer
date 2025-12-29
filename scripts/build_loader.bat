@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%\.."

if "%IDF_PATH%"=="" (
  if exist "%USERPROFILE%\.espressif\esp-idf\v5.1.2\export.bat" (
    set "IDF_PATH=%USERPROFILE%\.espressif\esp-idf\v5.1.2"
  ) else (
    echo IDF_PATH is not set. Set it to your ESP-IDF checkout.
    exit /b 1
  )
)

call "%IDF_PATH%\export.bat" || exit /b 1

set "ESP_IDF_TOOLS_INSTALL_DIR=fromenv"
set "RUSTC_WRAPPER="

cargo build --release --bin loader
echo Tip: run scripts\build_python_runner.bat to build python_runner.bin for .py/.mpy apps.
