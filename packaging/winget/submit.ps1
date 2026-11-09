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
#     pwsh packaging/winget/submit.ps1 -Force          # submit even if already published
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$DryRun,
    [switch]$Force
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

# winget-pkgs stores manifests under manifests/<first letter of publisher, lowercased>/<Publisher>/<Package>/<Version>.
# A version already merged there cannot be resubmitted -- wingetcreate rejects it with
# "Submitting a manifest without any updated changes is not allowed." Check up front so an
# already-published package is skipped instead of aborting the run before later packages.
function Test-WingetVersionPublished {
    param([string]$Id, [string]$PkgVersion)

    $parts = $Id.Split('.')
    $shard = $Id.Substring(0, 1).ToLowerInvariant()
    $path  = "manifests/$shard/" + ($parts -join '/') + "/$PkgVersion"
    $uri   = "https://api.github.com/repos/microsoft/winget-pkgs/contents/$path"

    try {
        Invoke-RestMethod -Uri $uri -Headers @{ 'User-Agent' = 'apic-winget-submit' } -ErrorAction Stop | Out-Null
        return $true
    } catch {
        $status = $_.Exception.Response.StatusCode.value__
        if ($status -eq 404) { return $false }
        Write-Warning "Could not check winget-pkgs for $Id $PkgVersion (HTTP $status); attempting submission anyway."
        return $false
    }
}

$base = "https://github.com/rizukirr/apic/releases/download/v$Version"
# The installer artifact must match the type already published for the package, or wingetcreate
# cannot map the new URL onto the existing installer node:
#   apic     -> zip (portable, nested apic.exe)
#   apic-gui -> MSI (wix); the zip is NOT interchangeable here.
$packages = @(
    @{ Id = 'rizukirr.apic';     Url = "$base/apic-v$Version-x86_64-pc-windows-msvc.zip" },
    @{ Id = 'rizukirr.apic-gui'; Url = "$base/apic-gui-v$Version-x86_64.msi" }
)

# Your winget-pkgs fork goes stale between releases. wingetcreate cannot open a PR from a fork
# that is behind upstream and reports only "The forked repository could not be synced with the
# upstream commits", so fast-forward it here. Only a clean sync is attempted: if the fork has
# commits upstream lacks, we warn and leave it alone rather than discarding your work.
function Sync-WingetForkIfStale {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        Write-Warning "gh not found; skipping winget-pkgs fork sync. If submission fails with a fork-sync error, sync https://github.com/<you>/winget-pkgs manually."
        return
    }

    $owner = & gh api user --jq .login 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $owner) {
        Write-Warning "Could not determine your GitHub login via gh; skipping fork sync check."
        return
    }

    $cmp = & gh api "repos/microsoft/winget-pkgs/compare/master...${owner}:master" --jq '[.ahead_by, .behind_by] | @tsv' 2>$null
    # A missing fork is fine -- wingetcreate creates one on first submission.
    if ($LASTEXITCODE -ne 0 -or -not $cmp) { return }

    $ahead, $behind = $cmp -split "	"
    if ([int]$behind -eq 0) { return }

    if ([int]$ahead -gt 0) {
        Write-Warning "$owner/winget-pkgs is $behind behind AND $ahead ahead of upstream. Not auto-syncing; resolve it manually so no commits are lost."
        return
    }

    Write-Host "Fast-forwarding $owner/winget-pkgs ($behind commits behind upstream)..."
    & gh repo sync "$owner/winget-pkgs" --source microsoft/winget-pkgs | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Warning "Fork sync failed; submission may fail. Sync https://github.com/$owner/winget-pkgs manually." }
}

# Catch a missing or misnamed release asset here, where the error names the file, rather than
# letting wingetcreate fail deeper with a less obvious message.
function Test-UrlExists {
    param([string]$Url)
    try {
        Invoke-WebRequest -Uri $Url -Method Head -UseBasicParsing -ErrorAction Stop | Out-Null
        return $true
    } catch {
        return $false
    }
}

# `wingetcreate update` copies every locale field from the last published version, so a
# license change or new release notes never reach the PR on their own (1.0.0 went out as MIT
# with no ReleaseNotes, and apic-gui 1.0.0 without a PrivacyUrl). Rewrite these fields in the
# generated locale manifest before submitting.
$PrivacyUrl = 'https://rizukirr.github.io/apic-web/privacy/'
$cargoToml = Join-Path $PSScriptRoot '..\..\Cargo.toml'
$licenseLine = Get-Content $cargoToml | Where-Object { $_ -match '^\s*license\s*=' } | Select-Object -First 1
if ($licenseLine -match '"([^"]+)"') { $License = $Matches[1] } else { throw "Could not read license from $cargoToml" }

function Get-ReleaseNotes {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        Write-Warning "gh not found; submitting without ReleaseNotes."
        return $null
    }
    $body = (& gh release view "v$Version" -R rizukirr/apic --json body -q .body 2>$null) -join "`n"
    if ($LASTEXITCODE -ne 0 -or -not $body.Trim()) {
        Write-Warning "Could not read release notes for v$Version; submitting without ReleaseNotes."
        return $null
    }
    $body = $body.Trim()
    # The winget schema caps ReleaseNotes at 10000 characters.
    if ($body.Length -gt 10000) {
        Write-Warning "Release notes exceed 10000 characters; submitting without ReleaseNotes."
        return $null
    }
    return $body
}

function Update-LocaleManifest {
    param([string]$Dir, [string]$Id, [string]$Notes)

    $file = Get-ChildItem $Dir -Recurse -Filter "$Id.locale.en-US.yaml" | Select-Object -First 1
    if (-not $file) { throw "No generated locale manifest for $Id under $Dir" }

    $lines = [System.IO.File]::ReadAllLines($file.FullName) |
        Where-Object { $_ -notmatch '^(License|PrivacyUrl):' }
    # Drop any carried-over ReleaseNotes block: the key line plus its indented continuation.
    $out = New-Object System.Collections.Generic.List[string]
    $inNotes = $false
    foreach ($l in $lines) {
        if ($l -match '^ReleaseNotes:') { $inNotes = $true; continue }
        if ($inNotes -and ($l -match '^\s' -or $l -eq '')) { continue }
        $inNotes = $false
        if ($l -match '^LicenseUrl:') { $out.Add("License: $License") }
        if ($Notes -and $l -match '^ReleaseNotesUrl:') {
            $out.Add('ReleaseNotes: |-')
            foreach ($n in ($Notes -split "`r?`n")) { $out.Add($(if ($n) { "  $n" } else { '' })) }
        }
        $out.Add($l)
        if ($l -match '^PublisherSupportUrl:') { $out.Add("PrivacyUrl: $PrivacyUrl") }
    }
    if (-not ($out -match '^License:')) { throw "Generated locale manifest for $Id has no LicenseUrl to anchor License on" }
    if (-not ($out -match '^PrivacyUrl:')) { throw "Generated locale manifest for $Id has no PublisherSupportUrl to anchor PrivacyUrl on" }
    [System.IO.File]::WriteAllLines($file.FullName, $out, (New-Object System.Text.UTF8Encoding $false))
}

$releaseNotes = $null
$notesFetched = $false

# Keep generated manifests out of the repo root; they are wingetcreate scratch output.
$outDir = Join-Path ([System.IO.Path]::GetTempPath()) "apic-winget-$Version"

$skipped = @()
$failed  = @()

$forkChecked = $false

foreach ($p in $packages) {
    if (-not $Force -and (Test-WingetVersionPublished -Id $p.Id -PkgVersion $Version)) {
        Write-Host "Skipping $($p.Id) $Version - already published in winget-pkgs (use -Force to resubmit)."
        $skipped += $p.Id
        continue
    }

    if (-not (Test-UrlExists -Url $p.Url)) {
        Write-Warning "Release asset not found for $($p.Id): $($p.Url)"
        $failed += $p.Id
        continue
    }

    # Only sync the fork once, and only when there is something to submit.
    if (-not $DryRun -and -not $forkChecked) {
        Sync-WingetForkIfStale
        $forkChecked = $true
    }

    # A separate, emptied directory per package so a stale run cannot leak into the submission.
    $pkgDir = Join-Path $outDir $p.Id
    $params = @('update', $p.Id, '--version', $Version, '--urls', $p.Url, '--out', $pkgDir)
    Write-Host "wingetcreate $($params -join ' ')"
    Write-Host "  then set License: $License, PrivacyUrl and ReleaseNotes, and run: wingetcreate submit $pkgDir"
    if (-not $DryRun) {
        if (-not $notesFetched) { $releaseNotes = Get-ReleaseNotes; $notesFetched = $true }
        if (Test-Path $pkgDir) { Remove-Item -Recurse -Force $pkgDir }
        # Do not abort the loop: one package failing must not block the others.
        & wingetcreate @params
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "wingetcreate update failed for $($p.Id) (exit $LASTEXITCODE)"
            $failed += $p.Id
            continue
        }
        try {
            Update-LocaleManifest -Dir $pkgDir -Id $p.Id -Notes $releaseNotes
        } catch {
            Write-Warning "Could not patch manifest for $($p.Id): $_"
            $failed += $p.Id
            continue
        }
        $manifestDir = (Get-ChildItem $pkgDir -Recurse -Filter "$($p.Id).yaml" | Select-Object -First 1).DirectoryName
        & wingetcreate submit $manifestDir
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "wingetcreate submit failed for $($p.Id) (exit $LASTEXITCODE)"
            $failed += $p.Id
        }
    }
}

if ($DryRun) {
    Write-Host ''
    Write-Host 'Dry run only - nothing submitted. Re-run without -DryRun to open the PRs.'
}

if ($skipped) { Write-Host "Skipped (already published): $($skipped -join ', ')" }
if ($failed)  { throw "winget submission failed for: $($failed -join ', ')" }
