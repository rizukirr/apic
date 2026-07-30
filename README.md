# apic

**A Free, Full Open Source** Git-able api contract tools. `Apic` stores your API contracts as plain JSON in your repo, so they're diffable, reviewable, and versioned like any other code. No paywalled seats or separate workspaces, if a teammate can clone the repo, they can collaborate on the contract.

CLI, TUI, and desktop GUI, all over one shared core (`apic-core`), so every interface edits the same files and never drifts.

<img width="1227" height="831" alt="2026-07-06-223454_hyprshot" src="https://github.com/user-attachments/assets/858475f7-5c68-4be8-9a1c-9753b999c32d" />

> ## ROADMAP
> - Distribution
>   - ~Aur~
>   - ~Copr~
>   - Launchpad
>   - ~Flatpak~
>   - ~Winget~
>   - Homebrew
> - Git GUI

## Why?

Mainstream API tooling gates collaboration behind paywalls, charging per team member just to share a workspace. Because `apic` contracts are plain JSON in your repo, **your existing Git workflow *is* the collaboration layer**:

*   **Zero-Cost Collaboration**, Sharing is a simple `git push`. Everyone with repository access already has full collaboration capabilities.
*   **Atomic Versioning**, Contracts change in the exact same commit as the implementation, preserving full history and `git blame`.
*   **Native Code Review**, Contract modifications show up as clean diffs in Pull Requests, reviewed by the same team, on the same platform.
*   **Terminal-First Readability**, No raw JSON eye-strain. `apic read` renders your contracts into clean, colorized tables directly in your shell.

## Install

`apic` ships as two binaries that share one core: `apic` (the CLI/TUI) and
`apic-gui` (the desktop app). Install either or both.

### Package managers

- **Arch / CachyOS (AUR):** `yay -S apic-bin` (or `paru -S apic-bin`) — ships both binaries.
- **Fedora (COPR):** `sudo dnf copr enable rizukirr/apic && sudo dnf install apic-cli apic-gui`.
- **Flatpak:** `flatpak install io.github.rizukirr.apic` (GUI)
- **Windows (winget):** `winget install rizukirr.apic` (CLI) · `winget install rizukirr.apic-gui` (GUI). The GUI installs as an MSI that adds an **apic** entry to the Start menu and an uninstaller in Settings → Apps.

### CLI / TUI (`apic`)

**crates.io** (recommended):

```bash
cargo install apic-cli
```

**From source** (requires a Rust toolchain, 1.97+):

```bash
git clone https://github.com/rizukirr/apic
cd apic
cargo install --path .
```

To run without installing, use `cargo run -- <args>` from the project directory.

### Desktop GUI (`apic-gui`)

**crates.io**:

```bash
cargo install apic-gui
```

`cargo install` only puts the binary on your `PATH`. On Linux, run
`apic-gui --desktop-entry` once to add it to your application launcher.

On Linux, building the GUI needs the system X11/Wayland/GL development libraries
(on Debian/Ubuntu: `libxkbcommon-dev`, `libwayland-dev`, `libxcb1-dev`,
`libgl1-mesa-dev`, and friends). macOS and Windows build with no extra packages.

At runtime, the Linux GUI opens folders/files through your desktop's portal. If
the **Open**/**New** dialogs never appear, install a portal and a backend, e.g.
`xdg-desktop-portal` plus `xdg-desktop-portal-kde` (KDE) or `-gtk` (GNOME/other).

### Prebuilt binaries

Grab the archive for your platform from the
[latest release](https://github.com/rizukirr/apic/releases), verify the
`.sha256` checksum, extract, and put the binary on your `PATH`. Each release
provides `apic-<target>` (CLI) and `apic-gui-<target>` (GUI) archives. CLI
builds cover Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), and Windows
(x86_64); GUI builds cover the same platforms (arm64 Linux is best-effort).

## Quick start

```bash
# 1. Initialize a project in the current directory (creates .apic/config.toml)
apic init

# 2. Point apic at the folder that holds your contract files (Optional)
apic config --set-dir api-contract

# 3. Edit the default template, or author a new named one
apic open --template              # edit .apic/template/convention.json
apic create --template mobile     # author .apic/template/mobile.json

# 4. Scaffold a new contract (opens the interactive editor)
# uses the only template, prompts to pick when several exist, or
# --use-template chooses one; falls back to the built-in template
apic create -f auth/login.json
apic create -f auth/login.json --use-template mobile

# 5. List and read contracts
apic list
apic read -f login
```

### Editing contracts

`apic create <file>` and `apic open <file>` open an interactive terminal editor
(TUI) by default. It shows the contract exactly as `apic read` renders it, the
same header, sections, tables, and inline JSON examples, and lets you edit in
place:

- **Navigate:** `↑/↓` (or `j/k`) select a row; `Enter` steps into the row's cells;
  `←/→` (or `h/l`) move between cells; `Esc` steps back out.
- **Method + URL:** the header shows one ` METHOD url` line. `Enter` on it focuses
  the method; `Enter` there opens a **method picker** (`j/k` to choose, `Enter`
  to select, `Esc` to cancel). `l` moves to the URL, edited inline with `Enter`/`i`.
- **Query / headers:** `QUERY` and `HEADERS` are `NAME/VALUE(/DESCRIPTION)` tables.
  `a` adds a row and drops you into it; `d` deletes the selected row after a
  confirmation. A row you add and then cancel while still empty is discarded.
- **Request body:** `a` in `REQUEST` opens the JSON editor (creating the body);
  an empty body reads as `(none)`.
- **Responses:** `RESPONSE` is a `code - title` tab strip. `a` opens a small
  `Status / Short Description` form, then the JSON editor for the new response.
  `e` on a tab re-opens that form to edit its status/title; `e` (or `Enter`) on
  the JSON opens the editor. `d` removes the whole response.
- **JSON editor:** type the example payload; `Ctrl-P` pretty-prints it, `Ctrl-S`
  saves, `Esc` cancels. Saving an empty body removes its response.
- **Save / quit:** `Ctrl-S` saves the contract; `Esc`/`q` exits; `?` shows the
  full key map.

Prefer your own editor? Pass `--editor` to open the file in `$VISUAL`/`$EDITOR`
(or a specific one, e.g. `apic open login --editor "code --wait"`).

## Desktop GUI

`apic-gui` works on the same `.apic` projects as the CLI, on the exact same JSON
files, both are thin layers over the shared `apic-core` crate. Run it, then
**Open** a project folder (or **New** to create one); it reopens the last
project on the next launch.

```bash
apic-gui
```

What it does:

- **Open / New**, **Open** picks a project folder (**New** creates one); the
  last project is remembered between launches.
- **Browse**, a sidebar lists every contract (with its HTTP method badge) and
  every template; a search box filters them.
- **Read / Edit**, selecting a contract renders it, and **Edit** lets you change
  fields in place through the same model as the TUI, saving to the file. The
  method is a dropdown, the request/response bodies are tabs with a
  line-numbered JSON editor, and metadata (headers, query, response headers) are
  editable tables.
- **Repair**, invalid contracts are flagged; fix the raw JSON and the GUI
  switches back to the structured view automatically.
- **Import**, bring in a Postman collection from the **Import** menu.
- **Manage**, scaffold new contracts and templates, or delete them with a
  confirmation.

## Templates

A **template** is a starter contract that `apic create -f <file>` copies from
when you scaffold a new endpoint. `apic init` seeds one at
`.apic/template/convention.json` from the built-in default; a project can hold
several named templates (`apic create --template <name>` authors more), and
`--use-template <name>` chooses which one seeds a new contract (`apic create`
prompts when several exist). A template is never overwritten once it exists.

The project template is **overlaid onto the built-in default**, so it only needs
to carry the fields you want to standardize, it can be a small partial contract.
It lives in `.apic/template/`, committed to the repo like everything else, so the
whole team scaffolds from the same starting point.

### Put your general API design in the template

The template is the single thing every new contract inherits from, so it is where
a team's shared API conventions belong. Encode the *general* design once and every
endpoint starts consistent, with none of the boilerplate re-typed:

- **Base URL and versioning**, e.g. `https://api.example.com/v1/...` (or a
  `{{baseUrl}}/{{base_path}}/...` placeholder), so every endpoint targets the same
  host and version.
- **Standing headers**, the ones every request carries, `Content-Type`,
  `Authorization: Bearer {token}`, a `DeviceID`/`AppID`, each with its `required`
  flag set the way your API expects.
- **A house response envelope**, if your API always wraps responses (e.g.
  `{ "statusCode", "message", "error", "value" }`), put that shape in the
  template's example responses so new endpoints follow it by default.
- **Common query parameters**, pagination (`page`, `limit`) or filter keys that
  most list endpoints share.

Because scaffolding always starts from these defaults, the template turns your API
design guidelines into something executable: consistency becomes the path of least
resistance, drift shows up as a diff in review, and adding an endpoint is "edit
what's different" instead of "remember every convention". When the convention
itself changes, you edit one file and every *new* contract picks it up.

## Commands

### `apic init [--set-dir <dir>]`
Initializes an `.apic` project in the current directory by creating
`.apic/config.toml`. The optional `--set-dir` records which directory contract
files are scanned from (defaults to the current directory).

### `apic config [--set-dir <dir>]`
Updates project configuration.

- `--set-dir <dir>`, change the working directory that contracts are scanned
  from (must exist).

### `apic create (-f <filename> | --template <name>) [--use-template <name>] [-e <editor>]`
Creates a new **contract** (`-f`) or authors a new **template** (`--template`).

With `-f <filename>`, a contract is seeded from a project template and opened in
the interactive TUI; the file is written only when you save. A relative path is
resolved against the configured working directory, and `apic` refuses to
overwrite an existing file. When `.apic/template/` holds a single template it is
used; when it holds several you are prompted to pick one (an inline picker), and
`--use-template <name>` selects one directly (fuzzy-matched) to skip the prompt.
With no usable template, the built-in default is used.

With `--template <name>`, a new template is authored at
`.apic/template/<name>.json` (a flat name), seeded from the built-in default or,
with `--use-template <name>`, from an existing template. It opens in the TUI and
refuses to overwrite an existing template. `--template` and `-f` are mutually
exclusive; `--use-template` composes with either.

```bash
apic create -f auth/login.json                       # contract from the project template
apic create -f auth/login.json --use-template mobile # contract from the `mobile` template
apic create --template mobile                        # author a new template
apic create --template mobile --use-template convention  # seed it from `convention`
```

Pass `-e`/`--editor` to scaffold the file to disk and open it in your external
editor instead of the TUI. Editor resolution order: `--editor` flag → `$VISUAL`
→ `$EDITOR` → `vi`. The flag picks the editor for a single invocation (e.g.
`apic create -f auth/login.json -e nano`) and the value may include arguments.
GUI editors need their wait flag (`code --wait`, `subl -w`) so `apic` waits for
the file to be saved.

### `apic list [--filter <query>] [--absolute <true|false>]`
Lists discovered `.json` contract files under the working directory.

- `--filter <query>`, show only contracts whose path fuzzy-matches the query,
  best match first (e.g. `apic list --filter user`).
- `--absolute <true|false>`, print absolute paths or paths relative to the
  working directory (`false`, the default).

### `apic read -f <query> [-s <status>]`
Renders a contract as formatted tables. `-s <status>` filters the response
section to a single HTTP status code.

`<query>` is resolved flexibly, an exact match wins, then fuzzy:

1. a path relative to the working directory, `user/user.json`
2. the same without the `.json` extension, `user/user`, `auth/login`
3. a fuzzy fragment, `user`, `logn`

```bash
apic read -f user/user.json   # exact path
apic read -f auth/login       # extension optional
apic read -f login            # fuzzy
apic read -f login -s 401     # show only the 401 response
apic read --template          # render the project template
```

It prints the endpoint header (name, description, and the ` METHOD url` line),
the `QUERY` and `HEADERS` tables, and the request/response bodies as their inline
JSON example payloads:

```text
 REQUEST
 {
   "agentCode": "79009901",
   "password": "newPassword123!"
 }

 RESPONSE 200 — OK
 {
   "statusCode": 200,
   "message": "Login berhasil",
   "value": { "accessToken": "...", "tokenType": "Bearer" }
 }
```

### `apic open (-f <query> | --template) [-e <editor>]`
Resolves `<query>` exactly like `read` (path, extensionless, or fuzzy) and
opens the matching contract in the interactive TUI. Pass `-e`/`--editor` to open
it in your external editor instead, the same editor resolution as `apic create`.

Pass `--template` instead of `-f` to edit the project template in
`.apic/template/` that `apic create` scaffolds from (the sole template, or
`convention.json` by default); it is seeded from the built-in default first if
none exists yet. `--template` and `-f` are mutually exclusive, and exactly one
is required. To author a *new* named template, use `apic create --template <name>`.

```bash
apic open -f user/user.json
apic open -f user
apic open -f user -e nano       # open with a one-off editor
apic open --template            # edit the project template
```

Output is colorized when stdout is a terminal and plain when piped, so it stays
clean in scripts. Contract strings are sanitized before display, so a file from
an untrusted source cannot inject terminal escape sequences.

### `apic remove (-f <query> | --template <name>)`
Resolves `<query>` exactly like `read`/`open` (path, extensionless, or
fuzzy, prompting to pick when ambiguous) and deletes the matching contract
file. On an interactive terminal it asks `Remove <path>? [y/N]` first and only
deletes on `y`/`yes`; when stdin/stdout is not a terminal (scripts) it removes
without prompting.

Pass `--template <name>` instead of `-f` to remove a project template from
`.apic/template/` (fuzzy-matched the same way, with the same confirmation). No
template is protected — removing `convention.json` or the last template is
allowed; `apic create` reseeds `convention.json` from the built-in default next
time. `--template` and `-f` are mutually exclusive.

```bash
apic remove -f user/user.json
apic remove -f login            # fuzzy, with confirmation
apic remove --template mobile   # remove a project template
```

### `apic validate [-f <query>] [--template]`
Checks that contracts parse and match the required contract format. With no `-f`, every
contract under the working directory is checked. A query ending in `/` (e.g.
`auth/`) validates every contract under that folder, recursively; otherwise the
query resolves to a single contract like `read` (path, extensionless, or fuzzy).
Prints `ok`/`FAIL` per file with the parse error (line and column) for failures,
and **exits non-zero if any contract is invalid**, so it drops straight into a
CI step or pre-commit hook.

```bash
apic validate               # check every contract
apic validate -f login      # check one
apic validate -f auth/      # check every contract under auth/, recursively
apic validate --template    # check the project template (in .apic/template/)
```

```text
ok   auth/login.json
FAIL user/user.json: EOF while parsing an object at line 12 column 1

2 passed, 1 failed
```

### `apic convert --postman <file> [--destination <dir>]`
Imports a Postman collection as apic contracts, one JSON file per request,
mirroring the collection's folder nesting. Accepts Postman Collection exports of
v1.0.0, v2.0.0, and v2.1.0 (auto-detected).

- `--postman <file>`, the Postman collection JSON to import.
- `--destination <dir>`, where to write the contracts, relative to the working
  directory (created if missing). **Optional**, defaults to the working
  directory itself. The path is confined to the working directory (`..`/absolute
  escapes are rejected). An existing contract is left untouched (the import
  errors) unless `--force` is passed.
- `--force`, overwrite contracts that already exist instead of erroring.

Each Postman folder becomes a directory and each request becomes
`folder/request_name.json`. Only the fields apic models are imported (method,
URL, headers, request/response bodies), Postman-specific data (auth blocks,
scripts, events, variables) is ignored. A request whose HTTP method apic does
not model (anything other than `GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`/
`OPTIONS`) is imported as `GET` with a warning, so nothing is downgraded
silently.

```bash
apic init                                   # an apic project is required
apic convert --postman MyAPI.postman.json   # writes into the working directory
apic convert --postman MyAPI.postman.json --destination imported
apic convert --postman MyAPI.postman.json --force   # overwrite existing contracts
```

```text
warning: request "Preflight" uses method TRACE, unsupported by apic — imported as GET
Converted 12 contract(s) into imported (1 warning)
```

## Security

`apic` treats contract files and paths as untrusted, so it is safe to run
against contracts from any source:

- **Terminal-escape safe**, all file-derived strings (contract fields, file
  names) are stripped of control characters before printing.
- **Path-confined**, `apic create` refuses paths that escape the working
  directory via `..` or an absolute path elsewhere.
- **Bounded**, contract files larger than 5 MiB are rejected before reading,
  and pathologically nested JSON is rejected rather than overflowing the stack.

## Contract format

A contract is a single JSON object describing one endpoint. See
[`apic-core/src/templates/contract.json`](apic-core/src/templates/contract.json)
for the built-in template `apic create` seeds from (and [Templates](#templates)
for how to make it your own), or [`example/`](example) for complete projects.

```json
{
    "name": "update-user",
    "description": "Update a user",
    "method": "PUT",
    "url": "https://api.example.com/users/{id}",
    "query": [
        {
            "name": "notify",
            "value": "true",
            "description": "Send a notification email",
            "required": false
        }
    ],
    "headers": [
        { "name": "Content-Type", "value": "application/json", "required": true },
        { "name": "Authorization", "value": "Bearer {token}", "required": true }
    ],
    "request": {
        "name": "Rizki Rakasiwi"
    },
    "responses": [
        {
            "code": 200,
            "description": "User updated",
            "headers": [
                { "name": "X-Request-Id", "value": "abc-123", "required": false }
            ],
            "schema": {
                "status": 200,
                "message": "OK"
            }
        }
    ]
}
```

A **body is a raw JSON payload**, the exact thing you would send or receive. The
request body is written **directly** under `request`; a response body is written
under the response's **`schema`** key. There is no separate field-level schema:
the payload is the contract's source of truth for a body's shape, and `apic read`
prints it verbatim. Omit `request` (or a response's `schema`) when there is no
body.

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Endpoint name. |
| `description` | no | Short description of the endpoint. |
| `method` | yes | HTTP method: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, or `OPTIONS`. |
| `url` | yes | Request URL as a single free-form string, e.g. `https://api.example.com/users/{id}`. Path parameters are written inline as `{name}` tokens. |
| `query` | no | Array of query parameters; see below. |
| `headers` | yes | Array of request headers; see below. |
| `request` | no | Request body: the raw JSON payload, written directly. Omit it when the endpoint has no body. |
| `responses` | yes | Array of responses; see below. |

Each **query** parameter:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Parameter key, e.g. `page`. |
| `value` | no | Example value, e.g. `2`. |
| `description` | no | What the parameter is for. |
| `required` | no | Whether the parameter is required (default `false`). |

Each **header** (request or response):

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Header name, e.g. `Authorization`. |
| `value` | yes | Example value, e.g. `Bearer {token}`. |
| `required` | no | Whether the header is required (default `false`). |

Each **response**:

| Field | Required | Description |
|-------|----------|-------------|
| `code` | yes | HTTP status code, e.g. `200`. |
| `description` | yes | Short description, e.g. `OK`. |
| `headers` | no | Array of response headers (same shape as request headers). |
| `schema` | no | The response body: the raw JSON payload for this response. |

Path parameters are not a separate section: write them inline in the `url`
string as `{name}` (e.g. `{id}` in `.../users/{id}`).

> The request body has no wrapper key, it is the JSON value itself. A response's
> body lives under `schema` (not `example`). Contracts written for the older
> `{ "example": ... }` body shape do not carry over and must be updated.

## Configuration

`apic init` writes `.apic/config.toml`:

```toml
name = "apic"
version = "0.1.0"

[root]
working_dir = "api-contract"
```

`working_dir` is stored relative to the project root, so `.apic/config.toml`
is safe to commit and share, it resolves correctly on any clone. `apic`
locates the project by walking up from the current directory to find the
`.apic` directory, so commands work from anywhere inside the project tree.

## License

Licensed under the [MIT License](LICENSE).
