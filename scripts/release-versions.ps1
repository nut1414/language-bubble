# Shared definitions only: importing this file performs no I/O or mutation.

function Get-ReleaseVersionPatterns {
    [pscustomobject]@{
        cargoPattern = '(?ms)(?<prefix>^\[package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*")(?<version>\d+\.\d+\.\d+)(?<suffix>")'
        lockPattern = '(?ms)(?<prefix>^\[\[package\]\]\s*\r?\n(?:(?!^\[\[package\]\]).)*?^name\s*=\s*"language-bubble"\s*\r?\n(?:(?!^\[\[package\]\]).)*?^version\s*=\s*")(?<version>\d+\.\d+\.\d+)(?<suffix>")'
        manifestPattern = '(?s)(?<prefix><Identity\b[^>]*\bVersion=")(?<version>\d+\.\d+\.\d+\.\d+)(?<suffix>")'
    }
}

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

function Get-ReleaseVersions {
    param(
        [Parameter(Mandatory = $true)][string]$CargoText,
        [Parameter(Mandatory = $true)][string]$LockText,
        [Parameter(Mandatory = $true)][string]$ManifestText
    )
    $patterns = Get-ReleaseVersionPatterns
    $cargoMatch = Get-VersionMatch -Text $CargoText -Pattern $patterns.cargoPattern -Description "Cargo package"
    $lockMatch = Get-VersionMatch -Text $LockText -Pattern $patterns.lockPattern -Description "Cargo lockfile package"
    $manifestMatch = Get-VersionMatch -Text $ManifestText -Pattern $patterns.manifestPattern -Description "MSIX manifest"
    $cargoVersion = $cargoMatch.Groups["version"].Value
    $lockVersion = $lockMatch.Groups["version"].Value
    $manifestVersion = $manifestMatch.Groups["version"].Value
    $storeVersion = "$cargoVersion.0"
    if ($lockVersion -ne $cargoVersion) {
        throw "Cargo.toml version '$cargoVersion' does not match Cargo.lock version '$lockVersion'."
    }
    if ($manifestVersion -ne $storeVersion) {
        throw "Cargo version '$cargoVersion' requires MSIX version '$storeVersion', but the manifest contains '$manifestVersion'."
    }
    [pscustomobject]@{
        Cargo = $cargoVersion
        Lock = $lockVersion
        Manifest = $manifestVersion
        Store = $storeVersion
    }
}
