[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$Force,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoDir = Join-Path $repoRoot "LanguageBubble"
$packageDir = Join-Path $repoRoot "LanguageBubble.Package"
$cargoTomlPath = Join-Path $cargoDir "Cargo.toml"
$cargoLockPath = Join-Path $cargoDir "Cargo.lock"
$manifestPath = Join-Path $packageDir "Package.appxmanifest"

$cargoPattern = '(?ms)(?<prefix>^\[package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*")(?<version>\d+\.\d+\.\d+)(?<suffix>")'
$lockPattern = '(?ms)(?<prefix>^\[\[package\]\]\s*\r?\n(?:(?!^\[\[package\]\]).)*?^name\s*=\s*"language-bubble"\s*\r?\n(?:(?!^\[\[package\]\]).)*?^version\s*=\s*")(?<version>\d+\.\d+\.\d+)(?<suffix>")'
$manifestPattern = '(?s)(?<prefix><Identity\b[^>]*\bVersion=")(?<version>\d+\.\d+\.\d+\.\d+)(?<suffix>")'

function Get-VersionMatch {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Text,

        [Parameter(Mandatory = $true)]
        [string]$Pattern,

        [Parameter(Mandatory = $true)]
        [string]$Description
    )

    $matches = [regex]::Matches($Text, $Pattern)
    if ($matches.Count -ne 1) {
        throw "Expected exactly one $Description version, found $($matches.Count)."
    }

    return $matches[0]
}

function Get-HostArchitecture {
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) {
        $architecture = $env:PROCESSOR_ARCHITEW6432
    }

    switch ($architecture.ToUpperInvariant()) {
        "AMD64" { return "x64" }
        "ARM64" { return "arm64" }
        "X86" { return "x86" }
        default { throw "Unsupported Windows host architecture: $architecture" }
    }
}

function Find-VsDevCmd {
    $vswhereCandidates = @(
        (Join-Path ([Environment]::GetFolderPath("ProgramFilesX86")) "Microsoft Visual Studio\Installer\vswhere.exe"),
        (Join-Path ([Environment]::GetFolderPath("ProgramFiles")) "Microsoft Visual Studio\Installer\vswhere.exe")
    ) | Select-Object -Unique

    $vswhere = $vswhereCandidates |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (-not $vswhere) {
        throw "vswhere.exe was not found. Install Visual Studio Build Tools with the C++ workload."
    }

    $installations = @(& $vswhere -latest -products * -requires `
        Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        Microsoft.VisualStudio.Component.VC.Tools.ARM64 `
        -property installationPath)
    if ($LASTEXITCODE -ne 0 -or $installations.Count -eq 0) {
        $installations = @(& $vswhere -latest -products * -property installationPath)
    }

    $installationPath = $installations |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Select-Object -First 1
    if (-not $installationPath) {
        throw "No Visual Studio installation was found. Install Visual Studio Build Tools with the C++ workload."
    }

    $vsDevCmd = Join-Path $installationPath.Trim() "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path -LiteralPath $vsDevCmd -PathType Leaf)) {
        throw "Visual Studio developer command script not found: $vsDevCmd"
    }

    return $vsDevCmd
}

function Find-WindowsSdkTools {
    param(
        [Parameter(Mandatory = $true)]
        [string]$HostArchitecture
    )

    $kitsRoot = $null
    foreach ($registryPath in @(
        "HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots"
    )) {
        try {
            $candidate = (Get-ItemProperty -LiteralPath $registryPath -Name KitsRoot10 -ErrorAction Stop).KitsRoot10
            if ($candidate) {
                $kitsRoot = $candidate
                break
            }
        }
        catch {
            continue
        }
    }

    if (-not $kitsRoot) {
        throw "Windows 10/11 SDK installation root was not found."
    }

    $toolArchitectures = @($HostArchitecture, "x64", "x86") | Select-Object -Unique
    $sdkDirectories = Get-ChildItem -LiteralPath (Join-Path $kitsRoot "bin") -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending

    foreach ($sdkDirectory in $sdkDirectories) {
        foreach ($toolArchitecture in $toolArchitectures) {
            $makeAppx = Join-Path $sdkDirectory.FullName "$toolArchitecture\makeappx.exe"
            $makePri = Join-Path $sdkDirectory.FullName "$toolArchitecture\makepri.exe"
            if ((Test-Path -LiteralPath $makeAppx -PathType Leaf) -and
                (Test-Path -LiteralPath $makePri -PathType Leaf)) {
                return [pscustomobject]@{
                    Version = $sdkDirectory.Name
                    MakeAppx = $makeAppx
                    MakePri = $makePri
                }
            }
        }
    }

    throw "makeappx.exe and makepri.exe were not found in the installed Windows SDKs."
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,

        [Parameter(Mandatory = $true)]
        [string]$Description
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Build-RustTarget {
    param(
        [Parameter(Mandatory = $true)]
        [string]$VsDevCmd,

        [Parameter(Mandatory = $true)]
        [string]$HostArchitecture,

        [Parameter(Mandatory = $true)]
        [string]$VisualCppArchitecture,

        [Parameter(Mandatory = $true)]
        [string]$RustTarget
    )

    $command = 'call "' + $VsDevCmd + '" -no_logo -arch=' + $VisualCppArchitecture +
        ' -host_arch=' + $HostArchitecture + ' >nul && cargo build --release --locked --target ' + $RustTarget

    Push-Location $cargoDir
    try {
        & $env:ComSpec /d /s /c $command
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build for $RustTarget failed with exit code $LASTEXITCODE."
        }
    }
    finally {
        Pop-Location
    }
}

function Save-StagedManifest {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Destination,

        [Parameter(Mandatory = $true)]
        [string]$Architecture,

        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    [xml]$manifest = [System.IO.File]::ReadAllText($manifestPath)
    $manifest.Package.Identity.SetAttribute("ProcessorArchitecture", $Architecture)
    $manifest.Package.Identity.SetAttribute("Version", $Version)

    $settings = New-Object System.Xml.XmlWriterSettings
    $settings.Encoding = New-Object System.Text.UTF8Encoding($false)
    $settings.Indent = $true
    $settings.OmitXmlDeclaration = $false
    $writer = [System.Xml.XmlWriter]::Create($Destination, $settings)
    try {
        $manifest.Save($writer)
    }
    finally {
        $writer.Dispose()
    }
}

function New-ArchitecturePackage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Architecture,

        [Parameter(Mandatory = $true)]
        [string]$ExecutablePath,

        [Parameter(Mandatory = $true)]
        [string]$LayoutPath,

        [Parameter(Mandatory = $true)]
        [string]$PackagePath,

        [Parameter(Mandatory = $true)]
        [string]$StoreVersion,

        [Parameter(Mandatory = $true)]
        [pscustomobject]$SdkTools
    )

    $imagesPath = Join-Path $LayoutPath "Images"
    New-Item -ItemType Directory -Path $imagesPath -Force | Out-Null
    Copy-Item -Path (Join-Path $packageDir "Images\*") -Destination $imagesPath -Recurse -Force
    Copy-Item -LiteralPath $ExecutablePath -Destination (Join-Path $LayoutPath "language-bubble.exe") -Force

    $stagedManifest = Join-Path $LayoutPath "AppxManifest.xml"
    Save-StagedManifest -Destination $stagedManifest -Architecture $Architecture -Version $StoreVersion

    $priConfig = Join-Path $LayoutPath "priconfig.xml"
    $resourcesPri = Join-Path $LayoutPath "resources.pri"
    Invoke-External -FilePath $SdkTools.MakePri -Arguments @(
        "createconfig", "/cf", $priConfig, "/dq", "en-US", "/o"
    ) -Description "MakePri configuration for $Architecture"
    Invoke-External -FilePath $SdkTools.MakePri -Arguments @(
        "new", "/pr", $LayoutPath, "/cf", $priConfig, "/mn", $stagedManifest, "/of", $resourcesPri, "/o"
    ) -Description "MakePri resources for $Architecture"
    Remove-Item -LiteralPath $priConfig -Force

    Invoke-External -FilePath $SdkTools.MakeAppx -Arguments @(
        "pack", "/d", $LayoutPath, "/p", $PackagePath, "/o"
    ) -Description "MSIX packaging for $Architecture"

    if (-not (Test-Path -LiteralPath $PackagePath -PathType Leaf) -or
        (Get-Item -LiteralPath $PackagePath).Length -eq 0) {
        throw "MSIX packaging for $Architecture did not produce a non-empty package."
    }
}

function Test-MsixBundle {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BundlePath,

        [Parameter(Mandatory = $true)]
        [string]$ValidationPath,

        [Parameter(Mandatory = $true)]
        [string]$StoreVersion,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedName,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedPublisher,

        [Parameter(Mandatory = $true)]
        [pscustomobject]$SdkTools
    )

    $unbundledPath = Join-Path $ValidationPath "bundle"
    New-Item -ItemType Directory -Path $unbundledPath -Force | Out-Null
    Invoke-External -FilePath $SdkTools.MakeAppx -Arguments @(
        "unbundle", "/p", $BundlePath, "/d", $unbundledPath, "/o"
    ) -Description "MSIX bundle validation"

    $innerPackages = @(Get-ChildItem -LiteralPath $unbundledPath -File |
        Where-Object { $_.Extension -in @(".msix", ".appx") })
    if ($innerPackages.Count -ne 2) {
        throw "Expected two packages in the bundle, found $($innerPackages.Count)."
    }

    $architectures = @()
    $index = 0
    foreach ($innerPackage in $innerPackages) {
        $index++
        $unpackedPath = Join-Path $ValidationPath "package-$index"
        New-Item -ItemType Directory -Path $unpackedPath -Force | Out-Null
        Invoke-External -FilePath $SdkTools.MakeAppx -Arguments @(
            "unpack", "/p", $innerPackage.FullName, "/d", $unpackedPath, "/o"
        ) -Description "Validation of $($innerPackage.Name)"

        $innerManifestPath = Join-Path $unpackedPath "AppxManifest.xml"
        if (-not (Test-Path -LiteralPath $innerManifestPath -PathType Leaf)) {
            throw "Package '$($innerPackage.Name)' does not contain AppxManifest.xml."
        }

        [xml]$innerManifest = [System.IO.File]::ReadAllText($innerManifestPath)
        $identity = $innerManifest.Package.Identity
        if ($identity.Name -ne $ExpectedName -or $identity.Publisher -ne $ExpectedPublisher) {
            throw "Package '$($innerPackage.Name)' has an unexpected Store identity."
        }
        if ($identity.Version -ne $StoreVersion) {
            throw "Package '$($innerPackage.Name)' has version '$($identity.Version)', expected '$StoreVersion'."
        }

        $architecture = $identity.ProcessorArchitecture.ToLowerInvariant()
        if ($architecture -notin @("x64", "arm64")) {
            throw "Package '$($innerPackage.Name)' has unexpected architecture '$architecture'."
        }
        $architectures += $architecture

        foreach ($requiredFile in @("language-bubble.exe", "resources.pri")) {
            $requiredPath = Join-Path $unpackedPath $requiredFile
            if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf) -or
                (Get-Item -LiteralPath $requiredPath).Length -eq 0) {
                throw "Package '$($innerPackage.Name)' is missing non-empty '$requiredFile'."
            }
        }
    }

    $actualArchitectures = @($architectures | Sort-Object -Unique)
    $expectedArchitectures = @("arm64", "x64")
    if (($actualArchitectures -join ",") -ne ($expectedArchitectures -join ",")) {
        throw "Bundle architectures '$($actualArchitectures -join ",")' do not match 'arm64,x64'."
    }
}

foreach ($path in @($cargoTomlPath, $cargoLockPath, $manifestPath, (Join-Path $packageDir "Images"))) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Required release input not found: $path"
    }
}

$cargoText = [System.IO.File]::ReadAllText($cargoTomlPath)
$lockText = [System.IO.File]::ReadAllText($cargoLockPath)
$manifestText = [System.IO.File]::ReadAllText($manifestPath)
$cargoVersion = (Get-VersionMatch -Text $cargoText -Pattern $cargoPattern -Description "Cargo package").Groups["version"].Value
$lockVersion = (Get-VersionMatch -Text $lockText -Pattern $lockPattern -Description "Cargo lockfile package").Groups["version"].Value
$manifestVersion = (Get-VersionMatch -Text $manifestText -Pattern $manifestPattern -Description "MSIX manifest").Groups["version"].Value
$storeVersion = "$cargoVersion.0"
[xml]$sourceManifest = $manifestText
$expectedIdentityName = $sourceManifest.Package.Identity.Name
$expectedPublisher = $sourceManifest.Package.Identity.Publisher

if ($lockVersion -ne $cargoVersion) {
    throw "Cargo.toml version '$cargoVersion' does not match Cargo.lock version '$lockVersion'."
}
if ($manifestVersion -ne $storeVersion) {
    throw "Cargo version '$cargoVersion' requires MSIX version '$storeVersion', but the manifest contains '$manifestVersion'."
}

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "release"
}
elseif (-not [System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot $OutputDirectory
}
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)

$hostArchitecture = Get-HostArchitecture
$sdkTools = Find-WindowsSdkTools -HostArchitecture $hostArchitecture
$vsDevCmd = $null
if (-not $SkipBuild) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo.exe was not found on PATH. Install Rust with rustup."
    }
    $vsDevCmd = Find-VsDevCmd
    Write-Host "Building x64 with Visual Studio tools..."
    Build-RustTarget -VsDevCmd $vsDevCmd -HostArchitecture $hostArchitecture -VisualCppArchitecture "x64" -RustTarget "x86_64-pc-windows-msvc"
    Write-Host "Building ARM64 with Visual Studio tools..."
    Build-RustTarget -VsDevCmd $vsDevCmd -HostArchitecture $hostArchitecture -VisualCppArchitecture "arm64" -RustTarget "aarch64-pc-windows-msvc"
}
else {
    Write-Warning "Skipping compilation; executable freshness cannot be verified. Use only artifacts built from the current source version."
}

$x64Executable = Join-Path $cargoDir "target\x86_64-pc-windows-msvc\release\language-bubble.exe"
$arm64Executable = Join-Path $cargoDir "target\aarch64-pc-windows-msvc\release\language-bubble.exe"
foreach ($executable in @($x64Executable, $arm64Executable)) {
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Release executable not found: $executable"
    }
}

$outputNames = [ordered]@{
    X64 = "LanguageBubble_${storeVersion}_x64.msix"
    Arm64 = "LanguageBubble_${storeVersion}_arm64.msix"
    Bundle = "LanguageBubble_${storeVersion}.msixbundle"
}
$outputPaths = @($outputNames.Values | ForEach-Object { Join-Path $OutputDirectory $_ })
$existingOutputs = @($outputPaths | Where-Object { Test-Path -LiteralPath $_ })
if ($existingOutputs.Count -gt 0 -and -not $Force) {
    throw "Release outputs already exist. Use -Force to replace only these files:`n$($existingOutputs -join "`n")"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("LanguageBubble-msix-" + [guid]::NewGuid().ToString("N"))
$tempPackages = Join-Path $tempRoot "packages"
$bundleInput = Join-Path $tempRoot "bundle"
New-Item -ItemType Directory -Path $tempPackages, $bundleInput -Force | Out-Null

try {
    $tempX64Package = Join-Path $tempPackages $outputNames.X64
    $tempArm64Package = Join-Path $tempPackages $outputNames.Arm64
    $tempBundle = Join-Path $tempPackages $outputNames.Bundle

    Write-Host "Packaging x64 MSIX with Windows SDK $($sdkTools.Version)..."
    New-ArchitecturePackage -Architecture "x64" -ExecutablePath $x64Executable `
        -LayoutPath (Join-Path $tempRoot "layout-x64") -PackagePath $tempX64Package `
        -StoreVersion $storeVersion -SdkTools $sdkTools

    Write-Host "Packaging ARM64 MSIX with Windows SDK $($sdkTools.Version)..."
    New-ArchitecturePackage -Architecture "arm64" -ExecutablePath $arm64Executable `
        -LayoutPath (Join-Path $tempRoot "layout-arm64") -PackagePath $tempArm64Package `
        -StoreVersion $storeVersion -SdkTools $sdkTools

    Copy-Item -LiteralPath $tempX64Package, $tempArm64Package -Destination $bundleInput
    Write-Host "Creating MSIX bundle..."
    Invoke-External -FilePath $sdkTools.MakeAppx -Arguments @(
        "bundle", "/d", $bundleInput, "/p", $tempBundle, "/bv", $storeVersion, "/o"
    ) -Description "MSIX bundle creation"

    if (-not (Test-Path -LiteralPath $tempBundle -PathType Leaf) -or
        (Get-Item -LiteralPath $tempBundle).Length -eq 0) {
        throw "MSIX bundle creation did not produce a non-empty bundle."
    }

    Write-Host "Validating MSIX bundle contents..."
    Test-MsixBundle -BundlePath $tempBundle -ValidationPath (Join-Path $tempRoot "validation") `
        -StoreVersion $storeVersion -ExpectedName $expectedIdentityName `
        -ExpectedPublisher $expectedPublisher -SdkTools $sdkTools

    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    if ($Force) {
        foreach ($outputPath in $outputPaths) {
            if (Test-Path -LiteralPath $outputPath) {
                Remove-Item -LiteralPath $outputPath -Force
            }
        }
    }

    Move-Item -LiteralPath $tempX64Package -Destination (Join-Path $OutputDirectory $outputNames.X64)
    Move-Item -LiteralPath $tempArm64Package -Destination (Join-Path $OutputDirectory $outputNames.Arm64)
    Move-Item -LiteralPath $tempBundle -Destination (Join-Path $OutputDirectory $outputNames.Bundle)
}
finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

Write-Host ""
Write-Host "MSIX packaging complete:"
foreach ($outputPath in $outputPaths) {
    Write-Host "  $outputPath"
}
Write-Host ""
Write-Host "Store submission is intentionally manual."
