[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("major", "minor", "patch")]
    [string]$Bump
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoTomlPath = Join-Path $repoRoot "LanguageBubble\Cargo.toml"
$cargoLockPath = Join-Path $repoRoot "LanguageBubble\Cargo.lock"
$manifestPath = Join-Path $repoRoot "LanguageBubble.Package\Package.appxmanifest"

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

function Set-MatchedVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Text,

        [Parameter(Mandatory = $true)]
        [string]$Pattern,

        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    $replacement = '${prefix}' + $Version + '${suffix}'
    return ([regex]::new($Pattern)).Replace($Text, $replacement, 1)
}

function Read-Utf8File {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $hasBom = $bytes.Length -ge 3 -and
        $bytes[0] -eq 0xEF -and
        $bytes[1] -eq 0xBB -and
        $bytes[2] -eq 0xBF
    $offset = if ($hasBom) { 3 } else { 0 }
    $encoding = New-Object System.Text.UTF8Encoding($hasBom, $true)
    $text = $encoding.GetString($bytes, $offset, $bytes.Length - $offset)

    return [pscustomobject]@{
        Text = $text
        Encoding = $encoding
    }
}

function Write-Utf8File {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Text,

        [Parameter(Mandatory = $true)]
        [System.Text.Encoding]$Encoding
    )

    [System.IO.File]::WriteAllText($Path, $Text, $Encoding)
}

foreach ($path in @($cargoTomlPath, $cargoLockPath, $manifestPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required release file not found: $path"
    }
}

$gitStatus = @(& git -C $repoRoot status --porcelain --untracked-files=all)
if ($LASTEXITCODE -ne 0) {
    throw "Could not inspect the Git working tree."
}
if ($gitStatus.Count -gt 0) {
    throw "The working tree must be clean before bumping the version.`n$($gitStatus -join "`n")"
}

$cargoFile = Read-Utf8File -Path $cargoTomlPath
$lockFile = Read-Utf8File -Path $cargoLockPath
$manifestFile = Read-Utf8File -Path $manifestPath
$cargoText = $cargoFile.Text
$lockText = $lockFile.Text
$manifestText = $manifestFile.Text

$cargoMatch = Get-VersionMatch -Text $cargoText -Pattern $cargoPattern -Description "Cargo package"
$lockMatch = Get-VersionMatch -Text $lockText -Pattern $lockPattern -Description "Cargo lockfile package"
$manifestMatch = Get-VersionMatch -Text $manifestText -Pattern $manifestPattern -Description "MSIX manifest"

$currentVersion = $cargoMatch.Groups["version"].Value
$lockVersion = $lockMatch.Groups["version"].Value
$manifestVersion = $manifestMatch.Groups["version"].Value
$expectedManifestVersion = "$currentVersion.0"

if ($lockVersion -ne $currentVersion) {
    throw "Cargo.toml version '$currentVersion' does not match Cargo.lock version '$lockVersion'."
}
if ($manifestVersion -ne $expectedManifestVersion) {
    throw "Cargo version '$currentVersion' requires MSIX version '$expectedManifestVersion', but the manifest contains '$manifestVersion'."
}

$parts = $currentVersion.Split('.') | ForEach-Object { [int]$_ }
switch ($Bump) {
    "major" {
        $parts[0]++
        $parts[1] = 0
        $parts[2] = 0
    }
    "minor" {
        $parts[1]++
        $parts[2] = 0
    }
    "patch" {
        $parts[2]++
    }
}

foreach ($part in $parts) {
    if ($part -gt 65535) {
        throw "Version component '$part' exceeds the MSIX limit of 65535."
    }
}

$newVersion = $parts -join "."
$newManifestVersion = "$newVersion.0"
$newCargoText = Set-MatchedVersion -Text $cargoText -Pattern $cargoPattern -Version $newVersion
$newLockText = Set-MatchedVersion -Text $lockText -Pattern $lockPattern -Version $newVersion
$newManifestText = Set-MatchedVersion -Text $manifestText -Pattern $manifestPattern -Version $newManifestVersion

if ($PSCmdlet.ShouldProcess($repoRoot, "Bump version from $currentVersion to $newVersion")) {
    try {
        Write-Utf8File -Path $cargoTomlPath -Text $newCargoText -Encoding $cargoFile.Encoding
        Write-Utf8File -Path $cargoLockPath -Text $newLockText -Encoding $lockFile.Encoding
        Write-Utf8File -Path $manifestPath -Text $newManifestText -Encoding $manifestFile.Encoding
    }
    catch {
        Write-Utf8File -Path $cargoTomlPath -Text $cargoText -Encoding $cargoFile.Encoding
        Write-Utf8File -Path $cargoLockPath -Text $lockText -Encoding $lockFile.Encoding
        Write-Utf8File -Path $manifestPath -Text $manifestText -Encoding $manifestFile.Encoding
        throw
    }

    Write-Host "Version bumped successfully:"
}
else {
    Write-Host "Version bump preview:"
}

Write-Host "  Cargo: $currentVersion -> $newVersion"
Write-Host "  MSIX:  $manifestVersion -> $newManifestVersion"
Write-Host ""
Write-Host "Review and release with:"
Write-Host "  git diff -- LanguageBubble/Cargo.toml LanguageBubble/Cargo.lock LanguageBubble.Package/Package.appxmanifest"
Write-Host "  git add LanguageBubble/Cargo.toml LanguageBubble/Cargo.lock LanguageBubble.Package/Package.appxmanifest"
Write-Host '  git commit -m "bump version"'
Write-Host "  git tag v$newVersion"
Write-Host "  git push origin main v$newVersion"
