# apic

**A Free, Full Open Source** Git-able api contract tools. `Apic` stores your API contracts as plain JSON in your repo, so they're diffable, reviewable, and versioned like any other code. No paywalled seats or separate workspaces, if a teammate can clone the repo, they can collaborate on the contract.

CLI and desktop GUI, both over one shared core (`apic-core`), so every interface edits the same files and never drifts.

<img width="1898" height="1022" alt="2026-11-04-230314_screenshot" src="https://github.com/user-attachments/assets/c0f0a8e8-9237-41e5-a65f-0c9ef764462f" />

## Why?

Mainstream API tooling gates collaboration behind paywalls, charging per team member just to share a workspace. Because `apic` contracts are plain JSON in your repo, **your existing Git workflow *is* the collaboration layer**:

*   **Zero-Cost Collaboration**, Sharing is a simple `git push`. Everyone with repository access already has full collaboration capabilities.
*   **Atomic Versioning**, Contracts change in the exact same commit as the implementation, preserving full history and `git blame`.
*   **Native Code Review**, Contract modifications show up as clean diffs in Pull Requests, reviewed by the same team, on the same platform.
*   **Terminal-First Readability**, No raw JSON eye-strain. `apic read` renders your contracts into clean, colorized tables directly in your shell.

## Install

`apic` ships as two binaries over one core: `apic` (the CLI) and `apic-gui` (the desktop app). Install either or both.

- **Arch / CachyOS (AUR):** `yay -S apic-bin`, which ships both.
- **Fedora (COPR):** `sudo dnf copr enable rizukirr/apic && sudo dnf install apic-cli apic-gui`
- **Flatpak:** `flatpak install io.github.rizukirr.apic` (GUI)
- **Windows (winget):** `winget install rizukirr.apic` (CLI), `winget install rizukirr.apic-gui` (GUI)
- **crates.io:** `cargo install apic-cli` and `cargo install apic-gui` (Rust 1.92+). On Linux, run `apic-gui --desktop-entry` once to add the GUI to your launcher.
- **Prebuilt binaries:** archives for Linux, macOS and Windows on the [releases page](https://github.com/rizukirr/apic/releases), each with a `.sha256` checksum.

Building the GUI on Linux needs the X11, Wayland and GL development libraries (on Debian and Ubuntu: `libxkbcommon-dev`, `libwayland-dev`, `libxcb1-dev`, `libgl1-mesa-dev`). If the GUI's Open and New dialogs never appear, install `xdg-desktop-portal` and a backend such as `xdg-desktop-portal-kde` or `-gtk`.

## Quick start

```bash
apic init                                   # create .apic/config.toml here
apic create -f auth/login -X POST https://api.example.com/auth/login \
  -H 'Content-Type: application/json' -d '{"user":"rizki"}' --response '200:OK'
apic edit -f login -H 'X-Trace: 1'          # change only what you pass
apic read -f login                          # print it as colored tables
apic open -f login                          # open it in your editor
apic send -f login --env dev                # send it with values from .apic/env/dev.toml
```

New contracts start from the project template in `.apic/template/`, so the team's base URL, standing headers and response envelope come for free. Everything else is in [COMMAND.md](COMMAND.md), and environments and variables for `apic send` are in [SENDING.md](SENDING.md).

## Desktop GUI

`apic-gui` opens the same projects: browse and edit contracts in tables, edit JSON and multipart bodies, send requests with an environment and see the live response, import Postman collections, and stage, commit, switch branches and resolve conflicts with git, all on the same files the CLI uses. See [GUI.md](GUI.md).

## Contracts

A contract is one JSON file per endpoint: name, method, URL, query params, headers, a request body and a list of responses. Bodies are the raw JSON payload, or a list of multipart parts. The full format, the templates and the project config are described in [CONTRACT.md](CONTRACT.md), and [`example/`](example) holds complete projects.

## Commands

| Command | What it does |
|---------|--------------|
| `apic init` | Set up an apic project in the current folder. |
| `apic config` | Change project settings in `.apic/config.toml`. |
| `apic create` | Write a new contract or template, with curl-style flags. |
| `apic edit` | Change fields of an existing contract with the same flags. |
| `apic list` | List the contracts in the project. |
| `apic read` | Print a contract as formatted, colored tables. |
| `apic open` | Open a contract or template in your editor. |
| `apic remove` | Delete a contract or template. |
| `apic validate` | Check contracts against the schema, for CI or a pre-commit hook. |
| `apic convert` | Import a Postman collection as contracts. |
| `apic send` | Send a contract's request and print the response. |
| `apic env` | List the environments in `.apic/env`. |
| `apic vars` | Convert `{{name}}` variables to `$name`. |

Every command, flag and example is documented in [COMMAND.md](COMMAND.md).

## Security

`apic` treats contract files and paths as untrusted, so it is safe to run
against contracts from any source:

- **Terminal-escape safe**, all file-derived strings (contract fields, file
  names) are stripped of control characters before printing.
- **Path-confined**, `apic create` refuses paths that escape the working
  directory via `..` or an absolute path elsewhere.
- **Secrets stay local**, `.apic/env/*.local.toml` is gitignored through
  `.apic/.gitignore`, `apic env` never prints values, and env parse errors
  name the line, never its text.
- **Bounded**, contract files larger than 5 MiB are rejected before reading,
  and pathologically nested JSON is rejected rather than overflowing the stack.

## License

Copyright (c) 2026 Rizki Rakasiwi. Licensed under the [GNU General Public License, version 3 only](LICENSE) (GPL-3.0). Releases up to and including 0.5.1 were published under the MIT License and stay available under it.
