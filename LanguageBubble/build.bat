@echo off
set PATH=%USERPROFILE%\.cargo\bin;%PATH%

cargo build --release --locked --target x86_64-pc-windows-msvc
if errorlevel 1 exit /b %errorlevel%

cargo build --release --locked --target aarch64-pc-windows-msvc
if errorlevel 1 exit /b %errorlevel%

echo.
echo Outputs:
echo   target\x86_64-pc-windows-msvc\release\language-bubble.exe
echo   target\aarch64-pc-windows-msvc\release\language-bubble.exe
pause
