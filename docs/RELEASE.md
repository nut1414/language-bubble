# Release Guide

Language Bubble releases use one version in three related formats:

- Cargo: `MAJOR.MINOR.PATCH`, for example `0.5.0`.
- Git: a lightweight `vMAJOR.MINOR.PATCH` tag, for example `v0.5.0`.
- MSIX: `MAJOR.MINOR.PATCH.0`, for example `0.5.0.0`.

`LanguageBubble/Cargo.toml` is the source of truth. The bump and packaging
scripts keep the lockfile and MSIX manifest synchronized with it.

## TL;DR

For a minor release, start from a clean `main` branch and replace `0.5.0`
below with the version printed by the bump script:

```powershell
git switch main
git pull --ff-only
git status --short

.\scripts\bump-version.ps1 minor

Push-Location LanguageBubble
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
Pop-Location

.\scripts\package-msix.ps1

git diff --check
git add LanguageBubble/Cargo.toml `
    LanguageBubble/Cargo.lock `
    LanguageBubble.Package/Package.appxmanifest
git commit -m "bump version to 0.5.0"
git tag v0.5.0
git push origin main v0.5.0
```

Upload `release/LanguageBubble_0.5.0.0.msixbundle` manually in Partner
Center. Use `patch` or `major` instead of `minor` when appropriate. The
sections below explain prerequisites, options, version rules, and recovery.

## One-time setup

Install:

1. Rust through `rustup`.
2. The x64 and ARM64 Rust targets.
3. Visual Studio or Visual Studio Build Tools with the C++ workload.
4. A Windows 10 or Windows 11 SDK containing `makepri.exe` and
   `makeappx.exe`.

```powershell
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-pc-windows-msvc
```

The packaging script discovers Visual Studio and the newest compatible
Windows SDK automatically. No certificate is required to create the Store
bundle; Store submission remains manual.

## Prepare a release

Start from a clean, current `main` branch:

```powershell
git switch main
git pull --ff-only
git status --short
```

The status output must be empty. Choose exactly one version increment:

```powershell
# 0.4.0 -> 0.4.1
.\scripts\bump-version.ps1 patch

# 0.4.0 -> 0.5.0
.\scripts\bump-version.ps1 minor

# 0.4.0 -> 1.0.0
.\scripts\bump-version.ps1 major
```

Preview a bump without changing files by adding `-WhatIf`:

```powershell
.\scripts\bump-version.ps1 minor -WhatIf
```

The bump script changes only these version locations:

- `LanguageBubble/Cargo.toml`
- `LanguageBubble/Cargo.lock`
- `LanguageBubble.Package/Package.appxmanifest`

It does not commit, tag, push, or contact Partner Center. Inspect the changes:

```powershell
git diff -- LanguageBubble/Cargo.toml `
    LanguageBubble/Cargo.lock `
    LanguageBubble.Package/Package.appxmanifest
```

## Validate and package

Run the same quality checks as CI:

```powershell
Push-Location LanguageBubble
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
Pop-Location
```

Build both architectures and create the MSIX packages and bundle:

```powershell
.\scripts\package-msix.ps1
```

`release.bat` is a compatibility wrapper for the same command:

```powershell
.\release.bat
```

For version `0.5.0`, the outputs are:

```text
release/LanguageBubble_0.5.0.0_x64.msix
release/LanguageBubble_0.5.0.0_arm64.msix
release/LanguageBubble_0.5.0.0.msixbundle
```

Useful packaging options:

```powershell
# Advanced: repackage release executables already built from this exact version.
.\scripts\package-msix.ps1 -SkipBuild

# Replace only the three expected outputs for the current version.
.\scripts\package-msix.ps1 -Force

# Write the artifacts somewhere else.
.\scripts\package-msix.ps1 -OutputDirectory C:\release-staging
```

The script never edits the tracked manifest. Architecture-specific manifests
and temporary layouts are generated outside the repository and removed when
packaging finishes.

## Commit, tag, and push

Only tag after validation and packaging succeed. Replace `0.5.0` below with
the version reported by the bump script:

```powershell
git add LanguageBubble/Cargo.toml `
    LanguageBubble/Cargo.lock `
    LanguageBubble.Package/Package.appxmanifest
git commit -m "bump version"
git tag v0.5.0
git push origin main v0.5.0
```

The repository uses lightweight tags, so do not substitute
`git push --follow-tags`; that option only automatically includes annotated
tags. Pushing the tag triggers the GitHub release workflow.

The local `.msixbundle` is the package intended for later manual Store review.
Neither local script performs any network upload or Partner Center action.

The scripts deliberately preserve the repository's existing `0.x.y` version
style. Current Microsoft Store validation may require a nonzero first MSIX
version component. If Partner Center rejects a `0.x.y.0` bundle, do not change
only the manifest or tag: decide and document a new synchronized version before
rebuilding.

## Recover from a release mistake

If the tag has not been pushed, delete it, correct the release, and recreate
it:

```powershell
git tag -d v0.5.0
```

Never move or reuse a published release tag. If a pushed release is wrong,
fix the issue and publish a new patch version instead.
