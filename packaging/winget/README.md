# winget packaging

Manifests for publishing apic to the [Windows Package Manager](https://learn.microsoft.com/windows/package-manager/) (winget):

- `rizukirr.apic` — the CLI / TUI (`apic`)
- `rizukirr.apic-gui` — the desktop GUI (`apic-gui`)

Both are portable apps shipped inside the `x86_64-pc-windows-msvc` `.zip`
archives attached to each GitHub Release.

## End users

```powershell
winget install rizukirr.apic        # CLI / TUI
winget install rizukirr.apic-gui    # desktop GUI
```

## Maintainer: how publishing works

Submission to `microsoft/winget-pkgs` is **manual and local by design**. No
token is stored in this repository or in GitHub Actions — `release.yml` is
intentionally left untouched. The token lives only in your machine's
credential vault.

### One-time setup

```powershell
winget install Microsoft.WingetCreate
wingetcreate token --store
```

`wingetcreate token --store` prompts for a **Classic** Personal Access Token
with the `public_repo` scope (optionally `delete_repo` to auto-clean failed
forks) and stores it in the OS credential vault. Fine-grained tokens are not
supported. Never pass `--token` on the command line — it can be logged.

### First-time submission (new package)

The seed manifests in this directory are the source of truth for the initial
submission. After a release exists with the Windows `.zip` assets:

```powershell
wingetcreate submit packaging/winget/rizukirr.apic
wingetcreate submit packaging/winget/rizukirr.apic-gui
```

Each opens a PR to `microsoft/winget-pkgs`. Microsoft moderation reviews and
merges them; once merged, `winget install rizukirr.apic` works.

### Updating for a new release

After a new tag's release finishes building, run the helper from the repo root:

```powershell
pwsh packaging/winget/submit.ps1 -Version 0.3.2     # explicit version
pwsh packaging/winget/submit.ps1                     # version read from Cargo.toml
pwsh packaging/winget/submit.ps1 -DryRun             # print commands, submit nothing
```

It runs `wingetcreate update … --submit` for both packages, which pulls the
current manifest from winget-pkgs, bumps the version, URL, and SHA-256, and
opens the PRs — all from your machine.

After publishing a new version, bump the `PackageVersion`, `InstallerUrl`, and
`InstallerSha256` in the seed manifests here too, so the in-repo copies stay in
sync with what's live.

### Verifying a manifest locally

```powershell
winget validate --manifest packaging/winget/rizukirr.apic
winget install --manifest packaging/winget/rizukirr.apic   # smoke-test install
```
