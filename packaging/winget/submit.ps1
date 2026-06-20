#!/usr/bin/env pwsh
# Manual, local winget submission helper for apic + apic-gui.
#
# Submission is intentionally NOT automated in CI: no token is ever stored in
# the repo or in GitHub Actions secrets. One-time setup on your own machine:
#
#     winget install Microsoft.WingetCreate
#     wingetcreate token --store     # Classic PAT, public_repo scope
#
# wingetcreate then reads the token from the OS credential vault. This script
# never accepts, prints, or passes a token.
#
# Usage:
#     pwsh packaging/winget/submit.ps1                 # version from Cargo.toml
#     pwsh packaging/winget/submit.ps1 -Version 0.3.2  # explicit version
#     pwsh packaging/winget/submit.ps1 -DryRun         # print commands only
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

if (-not (Get-Command wingetcreate -ErrorAction SilentlyContinue)) {
    throw "wingetcreate not found on PATH. Install it with: winget install Microsoft.WingetCreate"
}

if (-not $Version) {
    $cargoPath = Join-Path $PSScriptRoot '..\..\Cargo.toml'
    $line = Get-Content $cargoPath | Where-Object { $_ -match '^\s*version\s*=' } | Select-Object -First 1
    if ($line -match '"([^"]+)"') { $Version = $Matches[1] }
    if (-not $Version) { throw "Could not read version from Cargo.toml; pass -Version x.y.z" }
}

$base = "https://github.com/rizukirr/apic/releases/download/v$Version"
$packages = @(
    @{ Id = 'rizukirr.apic';     Url = "$base/apic-v$Version-x86_64-pc-windows-msvc.zip" },
    @{ Id = 'rizukirr.apic-gui'; Url = "$base/apic-gui-v$Version-x86_64-pc-windows-msvc.zip" }
)

foreach ($p in $packages) {
    $params = @('update', $p.Id, '--version', $Version, '--urls', $p.Url)
    if (-not $DryRun) { $params += '--submit' }
    Write-Host "wingetcreate $($params -join ' ')"
    if (-not $DryRun) {
        & wingetcreate @params
        if ($LASTEXITCODE -ne 0) { throw "wingetcreate failed for $($p.Id)" }
    }
}

if ($DryRun) {
    Write-Host ''
    Write-Host 'Dry run only — nothing submitted. Re-run without -DryRun to open the PRs.'
}
