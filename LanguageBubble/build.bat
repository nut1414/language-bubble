@echo off
set PATH=%USERPROFILE%\.cargo\bin;%PATH%

cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc

echo.
echo Output: target\release\language-bubble.exe
pause
