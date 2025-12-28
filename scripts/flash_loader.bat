@echo off
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%\.."

set "BUILD=Y"
set /p BUILD=Build loader first? [Y/n] 
if /I "%BUILD%"=="N" goto skip_build
call scripts\build_loader.bat || exit /b 1

:skip_build
if not defined ESPFLASH (
  if exist "temp_espflash\espflash.exe" (
    set "ESPFLASH=%CD%\temp_espflash\espflash.exe"
  ) else (
    set "ESPFLASH=espflash"
  )
)

if /I "%ESPFLASH%"=="espflash" (
  where /q espflash
  if errorlevel 1 (
    echo espflash not found on PATH. Install with: cargo install espflash
    exit /b 1
  )
) else (
  if not exist "%ESPFLASH%" (
    echo espflash not found at "%ESPFLASH%".
    exit /b 1
  )
)

set "PORTS="
for /f "usebackq delims=" %%P in (`powershell -NoProfile -Command "Get-CimInstance Win32_SerialPort | Select-Object -ExpandProperty DeviceID"`) do (
  if not "%%P"=="" (
    set "PORTS=!PORTS! %%P"
  )
)

if not "%PORTS%"=="" (
  echo Available serial ports:
  for %%P in (!PORTS!) do echo   %%P
)

set "PORT="
set /p PORT=Enter serial port (e.g. COM3): 
if "%PORT%"=="" (
  echo No serial port selected.
  exit /b 1
)

set "BAUD="
set /p BAUD=Baud rate [921600]: 
if "%BAUD%"=="" set "BAUD=921600"

set "ELF=target\xtensa-esp32s3-espidf\release\loader"
if not exist "%ELF%" (
  echo ELF not found at %ELF%. Build first.
  exit /b 1
)

echo Flashing %ELF% to %PORT% at %BAUD% baud...
%ESPFLASH% flash --chip esp32s3 --port %PORT% --baud %BAUD% --monitor "%ELF%"
