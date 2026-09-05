# R7 — Release tooling and architecture validation

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [bump-version.ps1](../../scripts/bump-version.ps1), [package-msix.ps1](../../scripts/package-msix.ps1), [CI](../../.github/workflows/ci.yml), and [release guide](../RELEASE.md).

Problem: both PowerShell scripts duplicate version patterns and `Get-VersionMatch`. Default CI does not explicitly validate ARM64. Packaging/version safety already exists and must be preserved.

Ownership: `scripts/`, workflow files, build/release documentation; no changes to the live package manifest or Cargo versions. Tooling extraction can proceed independently; final checks depend on integrated runtime packets. Risk: medium.

## Checkpoints

1. Extract only shared version-reading/validation logic into a side-effect-free script or module loaded via `$PSScriptRoot`. Keep bump mutation/ShouldProcess separate from packaging. Preserve rejection of missing/duplicate matches, mismatched Cargo/lock/MSIX versions, and current three-part/four-part mapping.
2. Add fixture-based PowerShell checks in an isolated temporary repository for clean/dirty trees, mismatches, malformed and duplicate version fields, BOM/newline preservation, and `-WhatIf` non-mutation. Check native exit codes explicitly. Do not invoke a real bump in the user's dirty checkout.
3. Retain packaging output collision protection, `-SkipBuild`, `-Force`, target architecture generation, temporary-layout cleanup, and no tracked-manifest edits. Test destructive/output cases only against fixtures or a fresh, explicitly resolved output directory.
4. Evaluate explicit x64/ARM64 compile gates against the actual available Windows runner/SDK toolchain. Add a supported build matrix only after validating linker setup. Keep native ARM64 execution distinct from cross-building; document a manual lane if an appropriate runner is unavailable.
5. After R1–R6, run common gates, both release builds, and packaging into a fresh output directory per [release guide](../RELEASE.md). If existing user manifest edits conflict with version checks, report the mismatch and use a clean fixture/checkout for validation; do not overwrite user intent.

## Acceptance

- Fixture checks prove unchanged version parsing and dry-run behavior; packaging does not mutate tracked source.
- No release tag, version bump, push, upload, or Store submission performed by this packet.
- Report actual results for both architecture builds and packaged smoke checks. Do not claim native ARM64 coverage from an x64 job.
- Updated documentation matches script entry points; common helper has no execution side effects when imported.

Reviewer focus: filesystem target resolution, cleanup scope, encoding preservation, version agreement, explicit exit-code handling, and CI toolchain availability. Rollback: revert helper extraction and both script imports together, then any workflow changes independently.
