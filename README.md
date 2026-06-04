# apic

A small CLI for **git-able API contracts**. Each endpoint is a plain JSON file
that lives in your repository, so contracts are diffable, reviewable in pull
requests, and versioned alongside the code they describe. `apic` discovers,
renders, and scaffolds those files.

## Why

Mainstream API tools like Postman and apidoc gate collaboration behind a
paywall — you pay per team member to share workspaces, and seats add up fast as
a team grows. `apic` takes a different approach: contracts are plain JSON files
in your repository, so **your existing git workflow _is_ the collaboration
layer**. No seats, no separate accounts — if someone can clone the repo, they
can read, edit, and review contracts.

That means contracts are:

- **Free to collaborate on** — sharing is `git push`/`git pull`, not a billing
  tier. Everyone with repo access already has full access.
- **Version-controlled** — contracts change in the same commit as the code, with
  full history and blame.
- **Reviewable** — a contract change is a readable diff in a pull request,
  reviewed by the same people on the same platform as the code.
- **Readable** — `apic read` renders a contract as a clean, colorized table in
  the terminal instead of raw JSON.

## Install

Build from source (requires a Rust toolchain, edition 2024):

```bash
git clone <repo-url> apic
cd apic
cargo install --path .
```

This puts the `apic` binary on your `PATH`. To run without installing, use
`cargo run -- <args>` from the project directory.

## Quick start

```bash
# 1. Initialize a project in the current directory (creates .apic/config.toml)
apic init

# 2. Point apic at the folder that holds your contract files
apic config --set-dir api-contract

# 3. Scaffold a new contract from a template (opens it in your editor)
apic create -f auth/login.json

# 4. List and read contracts
apic list
apic read -f login
```

## Commands

### `apic init [--set-dir <dir>]`
Initializes an `.apic` project in the current directory by creating
`.apic/config.toml`. The optional `--set-dir` records which directory contract
files are scanned from (defaults to the current directory).

### `apic config [--set-dir <dir>] [--set-editor <editor>]`
Updates project configuration.

- `--set-dir <dir>` — change the working directory that contracts are scanned
  from (must exist).
- `--set-editor <editor>` — set the editor used by `apic create`, e.g.
  `apic config --set-editor nvim`. The value may include arguments, such as
  `--set-editor "code --wait"`.

### `apic create -f <filename>`
Creates a new contract from the built-in template and opens it in your editor.
A relative path is resolved against the configured working directory. `apic`
refuses to overwrite an existing file.

Editor resolution order: `config.toml` editor → `$VISUAL` → `$EDITOR` → `vi`.
GUI editors need their wait flag (`code --wait`, `subl -w`) so `apic` waits for
the file to be saved.

### `apic list [--depth <n>] [--absolute <true|false>]`
Lists discovered `.json` contract files under the working directory.

- `--depth <n>` — truncate reported paths to `n` components below the root
  (`0`, the default, shows full paths).
- `--absolute <true|false>` — print absolute paths (default `true`) or paths
  relative to the working directory.

### `apic read -f <filename> [-s <status>]`
Fuzzy-finds the contract whose path best matches `<filename>` and renders it as
formatted tables. `-s <status>` filters the response section to a single HTTP
status code.

```bash
apic read -f login          # render the whole contract
apic read -f login -s 401   # show only the 401 response
```

Output is colorized when stdout is a terminal and plain when piped, so it stays
clean in scripts. Contract strings are sanitized before display, so a file from
an untrusted source cannot inject terminal escape sequences.

## Contract format

A contract is a single JSON object describing one endpoint. See
[`src/templates/contract.json`](src/templates/contract.json) for the full
template that `apic create` writes.

```json
{
    "name": "login",
    "description": "Login to the API",
    "method": "POST",
    "path": "/auth/login",
    "headers": [
        { "name": "Content-Type", "value": "application/json" }
    ],
    "request": [
        {
            "name": "username",
            "type": "string",
            "default": null,
            "description": "Username",
            "required": true
        }
    ],
    "responses": [
        {
            "code": 200,
            "description": "Successful login",
            "schema": [
                {
                    "name": "access_token",
                    "type": "string",
                    "default": null,
                    "description": "Token",
                    "required": true,
                    "properties": null
                }
            ]
        }
    ]
}
```

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Endpoint name. |
| `description` | no | Short description of the endpoint. |
| `method` | yes | HTTP method (`GET`, `POST`, …). |
| `path` | yes | Request path, e.g. `/auth/login`. |
| `query` | no | Array of query parameters (`name`, `value`, `description`, `required`). |
| `params` | no | Array of path parameters (`name`, `value`, `description`, `required`). |
| `headers` | yes | Array of headers (`name`, `value`). |
| `request` | no | Array of request-body fields (see field schema below). |
| `responses` | yes | Array of responses (`code`, `description`, `schema`). |

A **field** (in `request` and response `schema`) has:

| Field | Description |
|-------|-------------|
| `name` | Field name. |
| `type` | Data type (`string`, `int`, `object`, …). |
| `default` | Default value as a string, or `null`. |
| `description` | Field description. |
| `required` | Whether the field is required. |
| `properties` | Nested fields (for `object` types), or `null`. Response schema only. |

## Configuration

`apic init` writes `.apic/config.toml`:

```toml
name = "apic"
version = "0.1.0"
author = "rizukirr"
editor = "nvim"

[root]
working_dir = "/path/to/api-contract"
```

`apic` locates the project by walking up from the current directory to find the
`.apic` directory, so commands work from anywhere inside the project tree.

## Roadmap

- `apic tui` — an interactive terminal UI for browsing contracts (scaffolded,
  not yet implemented).

## License

See the repository for license details.
