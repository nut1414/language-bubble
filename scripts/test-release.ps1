[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot "release-versions.ps1")

$enginePath = (Get-Process -Id $PID).Path
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("LanguageBubble-release-tests-" + [guid]::NewGuid().ToString("N"))
$script:checks = 0

function Assert-True([bool]$Condition, [string]$Description) {
    if (-not $Condition) { throw "FAILED: $Description" }
    $script:checks++
}

function Assert-Rejected([scriptblock]$Action, [string]$Expected) {
    try { & $Action | Out-Null }
    catch {
        Assert-True ($_.Exception.Message -like "*$Expected*") "Expected '$Expected', got '$($_.Exception.Message)'"
        return
    }
    throw "FAILED: Expected rejection containing '$Expected'"
}

function New-Fixture([string]$Name, [bool]$Bom, [string]$Newline) {
    $path = Join-Path $fixtureRoot $Name
    New-Item -ItemType Directory -Path (Join-Path $path "scripts"), (Join-Path $path "LanguageBubble"), (Join-Path $path "LanguageBubble.Package") -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "bump-version.ps1"), (Join-Path $PSScriptRoot "release-versions.ps1") -Destination (Join-Path $path "scripts")
    & git -C $path init --quiet
    if ($LASTEXITCODE -ne 0) { throw "Fixture git init failed: $LASTEXITCODE" }
    # Ignore fixture inputs so clean-tree tests need no staging, commits, or user
    # identity. A deliberately unignored sentinel tests dirty-tree rejection.
    [System.IO.File]::WriteAllText((Join-Path $path ".git/info/exclude"), "*`n!dirty.txt`n")
    $encoding = [System.Text.UTF8Encoding]::new($Bom)
    $contents = [ordered]@{
        "LanguageBubble/Cargo.toml" = '[package]' + $Newline + 'name = "language-bubble"' + $Newline + 'version = "0.5.0"' + $Newline
        "LanguageBubble/Cargo.lock" = '[[package]]' + $Newline + 'name = "language-bubble"' + $Newline + 'version = "0.5.0"' + $Newline
        "LanguageBubble.Package/Package.appxmanifest" = '<Package><Identity Name="fixture" Version="0.5.0.0" /></Package>' + $Newline
    }
    foreach ($entry in $contents.GetEnumerator()) {
        [System.IO.File]::WriteAllText((Join-Path $path $entry.Key), $entry.Value, $encoding)
    }
    [pscustomobject]@{ Path = $path; Contents = $contents; Encoding = $encoding }
}

function Invoke-FixtureBump($Fixture, [string[]]$Options, [bool]$Success, [string]$Expected = "") {
    $output = & $enginePath -NoProfile -File (Join-Path $Fixture.Path "scripts/bump-version.ps1") @Options 2>&1
    $code = $LASTEXITCODE
    Assert-True (($code -eq 0) -eq $Success) "Bump exit $code; output: $($output -join ' ')"
    if ($Expected) { Assert-True (($output -join "`n") -like "*$Expected*") "Expected output '$Expected'" }
}

try {
    $cargo = "[package]`nname = `"language-bubble`"`nversion = `"0.5.0`"`n"
    $lock = "[[package]]`nname = `"language-bubble`"`nversion = `"0.5.0`"`n"
    $manifest = '<Package><Identity Version="0.5.0.0" /></Package>'
    $versions = Get-ReleaseVersions $cargo $lock $manifest
    Assert-True ($versions.Cargo -eq "0.5.0" -and $versions.Store -eq "0.5.0.0") "Version mapping"
    Assert-Rejected { Get-ReleaseVersions $cargo ($lock.Replace('0.5.0', '0.5.1')) $manifest } "does not match Cargo.lock"
    Assert-Rejected { Get-ReleaseVersions $cargo $lock ($manifest.Replace('0.5.0.0', '0.5.1.0')) } "requires MSIX version"
    $patterns = Get-ReleaseVersionPatterns
    foreach ($case in @(@($cargo, $patterns.cargoPattern), @($lock, $patterns.lockPattern), @($manifest, $patterns.manifestPattern))) {
        Assert-Rejected { Get-VersionMatch -Text ($case[0] + $case[0]) -Pattern $case[1] -Description "fixture" } "found 2"
        Assert-Rejected { Get-VersionMatch -Text ($case[0].Replace('0.5.0', 'invalid')) -Pattern $case[1] -Description "fixture" } "found 0"
        Assert-Rejected { Get-VersionMatch -Text "missing" -Pattern $case[1] -Description "fixture" } "found 0"
    }

    foreach ($bom in @($false, $true)) {
        foreach ($newline in @("`n", "`r`n")) {
            $fixture = New-Fixture "encoding-$bom-$($newline.Length)" $bom $newline
            $before = @($fixture.Contents.Keys | ForEach-Object { (Get-FileHash -LiteralPath (Join-Path $fixture.Path $_)).Hash })
            Invoke-FixtureBump $fixture @("patch", "-WhatIf") $true "Version bump preview"
            $after = @($fixture.Contents.Keys | ForEach-Object { (Get-FileHash -LiteralPath (Join-Path $fixture.Path $_)).Hash })
            Assert-True (($before -join ',') -eq ($after -join ',')) "WhatIf preserves every byte"
            Invoke-FixtureBump $fixture @("patch") $true "Version bumped successfully"
            foreach ($entry in $fixture.Contents.GetEnumerator()) {
                $bytes = [System.IO.File]::ReadAllBytes((Join-Path $fixture.Path $entry.Key))
                $expectedBytes = $fixture.Encoding.GetPreamble() + $fixture.Encoding.GetBytes($entry.Value.Replace('0.5.0', '0.5.1'))
                Assert-True ([Convert]::ToBase64String($bytes) -eq [Convert]::ToBase64String($expectedBytes)) "Patch preserves encoding and newlines: $($entry.Key)"
            }
        }
    }

    foreach ($case in @(@("minor", "0.6.0"), @("major", "1.0.0"))) {
        $fixture = New-Fixture $case[0] $false "`n"
        Invoke-FixtureBump $fixture @($case[0]) $true
        Assert-True ((Get-Content -LiteralPath (Join-Path $fixture.Path "LanguageBubble/Cargo.toml") -Raw).Contains($case[1])) "$($case[0]) increments correctly"
    }
    $fixture = New-Fixture "dirty" $false "`n"
    [System.IO.File]::WriteAllText((Join-Path $fixture.Path "dirty.txt"), "untracked")
    Invoke-FixtureBump $fixture @("patch", "-WhatIf") $false "working tree must be clean"
    $fixture = New-Fixture "mismatch" $false "`n"
    [System.IO.File]::WriteAllText((Join-Path $fixture.Path "LanguageBubble/Cargo.lock"), $lock.Replace('0.5.0', '0.5.1'))
    $before = (Get-FileHash -LiteralPath (Join-Path $fixture.Path "LanguageBubble/Cargo.toml")).Hash
    Invoke-FixtureBump $fixture @("patch") $false "does not match Cargo.lock"
    Assert-True ($before -eq (Get-FileHash -LiteralPath (Join-Path $fixture.Path "LanguageBubble/Cargo.toml")).Hash) "Mismatch does not mutate Cargo"
    Write-Host "Passed $script:checks release checks. All mutations were confined to temporary fixtures."
}
finally {
    if (Test-Path -LiteralPath $fixtureRoot) {
        $resolvedFixture = (Resolve-Path -LiteralPath $fixtureRoot).Path
        $tempParent = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\')
        if ($resolvedFixture -ne [System.IO.Path]::GetFullPath($fixtureRoot) -or
            (Split-Path -Parent $resolvedFixture).TrimEnd('\') -ne $tempParent -or
            (Split-Path -Leaf $resolvedFixture) -notmatch '^LanguageBubble-release-tests-[0-9a-f]{32}$') {
            throw "Refusing cleanup outside the generated fixture directory: $resolvedFixture"
        }
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
