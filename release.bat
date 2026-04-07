@echo off
setlocal enabledelayedexpansion

set "ROOT=%~dp0"
set "CARGO_DIR=%ROOT%LanguageBubble"
set "PKG_DIR=%ROOT%LanguageBubble.Package"
set "OUT_DIR=%ROOT%release"
set "MANIFEST=%PKG_DIR%\Package.appxmanifest"

:: Read version from Cargo.toml [package] section (source of truth)
for /f "usebackq delims=" %%v in (`powershell -NoProfile -Command "$q=[char]34;$c=Get-Content '!CARGO_DIR!\Cargo.toml';foreach($l in $c){if($l.StartsWith('version')){$l.Split($q)[1];break}}"`) do set "CARGO_VER=%%v"

if "!CARGO_VER!"=="" (
    echo ERROR: Could not read version from Cargo.toml
    pause
    exit /b 1
)

:: MSIX requires 4-part version (major.minor.patch.revision)
set "VERSION=!CARGO_VER!.0"

:: Update Package.appxmanifest to match Cargo version
powershell -NoProfile -Command "$f='!MANIFEST!'; [xml]$x=Get-Content $f; $x.Package.Identity.Version='!VERSION!'; $x.Save($f)"

echo.
echo ============================================
echo  Language Bubble Release v!VERSION!
echo ============================================
echo.

:: Setup VS environment
set "VSBASE=C:\Program Files\Microsoft Visual Studio\18\Community"
set "MSVC_VER=14.50.35717"
set "SDK_VER=10.0.19041.0"
set "SDK_BASE=C:\Program Files (x86)\Windows Kits\10"
set "MAKEAPPX=!SDK_BASE!\bin\!SDK_VER!\x86\makeappx.exe"
set "MAKEPRI=!SDK_BASE!\bin\!SDK_VER!\x86\makepri.exe"

:: Verify tools exist
if not exist "!VSBASE!\VC\Tools\MSVC\!MSVC_VER!" (
    echo ERROR: MSVC toolchain not found at !VSBASE!\VC\Tools\MSVC\!MSVC_VER!
    echo Update VSBASE and MSVC_VER in this script to match your installation.
    pause
    exit /b 1
)
if not exist "!MAKEAPPX!" (
    echo ERROR: makeappx.exe not found at !MAKEAPPX!
    echo Update SDK_VER in this script to match your installation.
    pause
    exit /b 1
)

:: Clean output
if exist "!OUT_DIR!" rmdir /s /q "!OUT_DIR!"
mkdir "!OUT_DIR!"
mkdir "!OUT_DIR!\bundle"

:: ---- Build x64 ----
echo [1/4] Building x64...
set "LIB=!VSBASE!\VC\Tools\MSVC\!MSVC_VER!\lib\onecore\x64;!SDK_BASE!\Lib\!SDK_VER!\ucrt\x64;!SDK_BASE!\Lib\!SDK_VER!\um\x64"
pushd "!CARGO_DIR!"
cargo build --release --target x86_64-pc-windows-msvc
if !errorlevel! neq 0 (
    echo ERROR: x64 build failed
    popd
    pause
    exit /b 1
)
popd
echo     x64 build OK

:: ---- Build ARM64 ----
echo [2/4] Building ARM64...
set "LIB=!VSBASE!\VC\Tools\MSVC\!MSVC_VER!\lib\onecore\arm64;!SDK_BASE!\Lib\!SDK_VER!\ucrt\arm64;!SDK_BASE!\Lib\!SDK_VER!\um\arm64"
pushd "!CARGO_DIR!"
cargo build --release --target aarch64-pc-windows-msvc
if !errorlevel! neq 0 (
    echo ERROR: ARM64 build failed
    popd
    pause
    exit /b 1
)
popd
echo     ARM64 build OK

:: ---- Package MSIX ----
echo [3/4] Packaging MSIX...

:: x64 layout
set "X64_MSIX=!OUT_DIR!\LanguageBubble_!VERSION!_x64.msix"
set "X64_LAYOUT=!OUT_DIR!\_layout_x64"
mkdir "!X64_LAYOUT!"
mkdir "!X64_LAYOUT!\Images"
copy "!CARGO_DIR!\target\x86_64-pc-windows-msvc\release\language-bubble.exe" "!X64_LAYOUT!\" >nul
copy "!PKG_DIR!\Images\*" "!X64_LAYOUT!\Images\" >nul
copy "!MANIFEST!" "!X64_LAYOUT!\AppxManifest.xml" >nul

:: Ensure x64 architecture in manifest
powershell -NoProfile -Command "$f='!X64_LAYOUT!\AppxManifest.xml'; [xml]$x=Get-Content $f; $x.Package.Identity.SetAttribute('ProcessorArchitecture','x64'); $x.Save($f)"

:: Generate resources.pri for qualified image filenames
"!MAKEPRI!" createconfig /cf "!X64_LAYOUT!\priconfig.xml" /dq en-US /o
"!MAKEPRI!" new /pr "!X64_LAYOUT!" /cf "!X64_LAYOUT!\priconfig.xml" /mn "!X64_LAYOUT!\AppxManifest.xml" /of "!X64_LAYOUT!\resources.pri" /o
del "!X64_LAYOUT!\priconfig.xml"

"!MAKEAPPX!" pack /d "!X64_LAYOUT!" /p "!X64_MSIX!" /o
if !errorlevel! neq 0 (
    echo ERROR: x64 MSIX packaging failed
    pause
    exit /b 1
)
echo     x64 MSIX OK

:: ARM64 layout
set "ARM64_MSIX=!OUT_DIR!\LanguageBubble_!VERSION!_arm64.msix"
set "ARM64_LAYOUT=!OUT_DIR!\_layout_arm64"
mkdir "!ARM64_LAYOUT!"
mkdir "!ARM64_LAYOUT!\Images"
copy "!CARGO_DIR!\target\aarch64-pc-windows-msvc\release\language-bubble.exe" "!ARM64_LAYOUT!\" >nul
copy "!PKG_DIR!\Images\*" "!ARM64_LAYOUT!\Images\" >nul
copy "!MANIFEST!" "!ARM64_LAYOUT!\AppxManifest.xml" >nul

:: Set ARM64 architecture in manifest
powershell -NoProfile -Command "$f='!ARM64_LAYOUT!\AppxManifest.xml'; [xml]$x=Get-Content $f; $x.Package.Identity.SetAttribute('ProcessorArchitecture','arm64'); $x.Save($f)"

:: Generate resources.pri for qualified image filenames
"!MAKEPRI!" createconfig /cf "!ARM64_LAYOUT!\priconfig.xml" /dq en-US /o
"!MAKEPRI!" new /pr "!ARM64_LAYOUT!" /cf "!ARM64_LAYOUT!\priconfig.xml" /mn "!ARM64_LAYOUT!\AppxManifest.xml" /of "!ARM64_LAYOUT!\resources.pri" /o
del "!ARM64_LAYOUT!\priconfig.xml"

"!MAKEAPPX!" pack /d "!ARM64_LAYOUT!" /p "!ARM64_MSIX!" /o
if !errorlevel! neq 0 (
    echo ERROR: ARM64 MSIX packaging failed
    pause
    exit /b 1
)
echo     ARM64 MSIX OK

:: ---- Bundle ----
echo [4/4] Creating MSIX bundle...
copy "!X64_MSIX!" "!OUT_DIR!\bundle\" >nul
copy "!ARM64_MSIX!" "!OUT_DIR!\bundle\" >nul
set "BUNDLE=!OUT_DIR!\LanguageBubble_!VERSION!.msixbundle"
"!MAKEAPPX!" bundle /d "!OUT_DIR!\bundle" /p "!BUNDLE!" /o
if !errorlevel! neq 0 (
    echo ERROR: Bundle creation failed
    pause
    exit /b 1
)

:: Cleanup temp dirs
rmdir /s /q "!OUT_DIR!\_layout_x64"
rmdir /s /q "!OUT_DIR!\_layout_arm64"
rmdir /s /q "!OUT_DIR!\bundle"

echo.
echo ============================================
echo  Release complete!
echo ============================================
echo.
echo  Output:
echo    !X64_MSIX!
echo    !ARM64_MSIX!
echo    !BUNDLE!
echo.
echo  Upload the .msixbundle to Partner Center.
echo.
pause
