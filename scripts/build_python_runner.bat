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

echo Building python_runner (release)...
cargo build --release --bin python_runner || exit /b 1

set "ELF_PATH=target\xtensa-esp32s3-espidf\release\python_runner"
if not exist "%ELF_PATH%" (
  echo ELF not found at %ELF_PATH%. Build first.
  exit /b 1
)

set "OUT_DIR=dist"
if not exist "%OUT_DIR%" (
  mkdir "%OUT_DIR%"
)
set "OUT_PATH=%OUT_DIR%\python_runner.bin"

where espflash >nul 2>&1
if %errorlevel%==0 (
  espflash save-image --chip esp32s3 --output "%OUT_PATH%" "%ELF_PATH%"
) else (
  set "ESPTOOL=%IDF_PATH%\components\esptool_py\esptool\esptool.py"
  if not exist "%ESPTOOL%" (
    echo esptool.py not found at %ESPTOOL%
    exit /b 1
  )
  python "%ESPTOOL%" --chip esp32s3 elf2image -o "%OUT_PATH%" "%ELF_PATH%"
)

echo python_runner.bin saved to %OUT_PATH%
echo Upload it to /sdcard/cardputer/python_runner.bin on the SD card.
