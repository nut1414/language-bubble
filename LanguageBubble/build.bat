@echo off
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
set LIB=C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.50.35717\lib\onecore\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.19041.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.19041.0\um\x64
cargo build --release
echo.
echo Output: target\release\language-bubble.exe
pause
