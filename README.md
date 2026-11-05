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

`apic` ships as two binaries that share one core: `apic` (the CLI) and
`apic-gui` (the desktop app). Install either or both.

### Package managers

- **Arch / CachyOS (AUR):** `yay -S apic-bin` (or `paru -S apic-bin`) — ships both binaries.
- **Fedora (COPR):** `sudo dnf copr enable rizukirr/apic && sudo dnf install apic-cli apic-gui`.
- **Flatpak:** `flatpak install io.github.rizukirr.apic` (GUI)
- **Windows (winget):** `winget install rizukirr.apic` (CLI) · `winget install rizukirr.apic-gui` (GUI). The GUI installs as an MSI that adds an **apic** entry to the Start menu and an uninstaller in Settings → Apps.

### CLI (`apic`)

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

# 4. Scaffold a new contract with curl-style flags
# uses the only template, convention.json when several exist, or
# --use-template chooses one; falls back to the built-in template
apic create -f auth/login -X POST https://api.example.com/auth/login \
  -H 'Content-Type: application/json' -d '{"user":"rizki"}' --response '200:OK'
apic create -f auth/login --use-template mobile

# 5. Change it later, only the flags you pass are touched
apic edit -f login -H 'X-Trace: 1' --response '401:Unauthorized'

# 6. List and read contracts
apic list
apic read -f login
```

### Editing contracts

`apic create` and `apic edit` take curl-style flags, so a contract can be written
or changed in one command: `-X` for the method, a bare URL argument, `-H` for
headers, `-d` for a JSON body, `-F` for multipart parts, and `--response`,
`--response-body` and `--response-header` for responses. `create` starts from the
project template, and `edit` changes only the fields you pass. Both validate the
result and write nothing when any flag fails.

For anything the flags do not cover, such as descriptions on a query param or
the `required` flag on a header, `apic open -f <query>` opens the file in your own
editor (`-e`, then `$VISUAL`, then `$EDITOR`, then `vi`).

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
  fields in place through the same edit model as the CLI, saving to the file. The
  method is a dropdown, the request/response bodies are tabs with a
  line-numbered JSON editor, and metadata (headers, query, response headers) are
  editable tables.
- **Repair**, invalid contracts are flagged; fix the raw JSON and the GUI
  switches back to the structured view automatically.
- **Import**, bring in a Postman collection from the **Import** menu.
- **Manage**, scaffold new contracts and templates, or delete them with a
  confirmation.
- **Git**, a second sidebar tab for staging, committing, branching and
  resolving conflicts without leaving the app. See below.

### Git

The **Git** tab appears beside **Explorer** when the project sits inside a git
repository. It shells out to your own `git`, so hooks, credential helpers and
config all behave exactly as they do in a terminal. The tab label turns red when
the working tree is dirty and gains a `[conflict]` suffix when a file is
conflicted, so you can see it without switching tabs.

- **Changed files**, listed under `CONFLICTS`, `STAGED` and `UNSTAGED`, nested by
  folder like the Explorer tree. Scoped to your contract working directory plus
  `.apic/`, with anything changed elsewhere in the repository counted on a line
  you can expand rather than hidden.
- **Stage, unstage, discard**, `+`, `-` and `x` on any file or folder. Discard is
  offered for tracked files only, since reverting a tracked file is recoverable
  and deleting an untracked one is not, and it always asks first, naming what it
  will revert.
- **Commit**, a message box and a commit button at the foot of the tab.
- **Diffs**, selecting a file shows it whole with changed lines coloured. A
  toggle switches to a field-level summary that names each changed field and its
  old and new value, like `name user-login -> login-v2`, which is the better view
  for a schema change buried in a large contract. Contracts that no longer parse,
  and every non-contract file, fall back to the line view.
- **Branches**, a dropdown lists every local branch. Picking one checks it out,
  `+` creates one, `x` deletes one behind a confirmation. Deleting is never
  forced, so git's refusal to drop unmerged work is surfaced rather than
  overridden.
- **Switching is safe**, a checkout is refused while a contract has unsaved
  edits, naming the file. After a successful switch the open contract is
  reloaded from the new branch, or closed if it does not exist there, so the
  editor never shows content the working tree no longer has.
- **Conflicts**, a conflicted file opens a resolver: each conflict block in file
  order with both sides shown, and `Accept ours`, `Accept theirs` or `Accept
  both` per block. A live preview on the right shows the resulting file as you
  choose. Nothing is written until **Resolve**, which stays disabled until every
  block is decided, then writes the file and stages it.

Not included: merge, pull, fetch, rebase, stash and remote branches. Those stay
in your terminal for now.

## Templates

A **template** is a starter contract that `apic create -f <file>` copies from
when you scaffold a new endpoint. `apic init` seeds one at
`.apic/template/convention.json` from the built-in default; a project can hold
several named templates (`apic create --template <name>` authors more), and
`--use-template <name>` chooses which one seeds a new contract (with several and
no `--use-template`, `apic create` uses `convention.json`). A template is never overwritten once it exists.

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

### `apic create (-f <filename> | --template <name>) [<url>] [--use-template <name>] [flags]`
Creates a new **contract** (`-f`) or authors a new **template** (`--template`).

With `-f <filename>`, a contract is seeded from a project template, the
authoring flags (listed under [`apic edit`](#apic-edit--f-query-url-flags)) are
applied on top, and the result is validated and written. A relative path is
resolved against the configured working directory and confined to it, `.json`
is appended when the name lacks it, and `apic` refuses to overwrite an existing
file. The contract name defaults to the file stem unless `--name` is given. When
`.apic/template/` holds a single template it is used; when it holds several,
`convention.json` is used unless `--use-template <name>` selects one
(fuzzy-matched). With no usable template, the built-in default is used. On
success it prints `created <path>`.

With `--template <name>`, a new template is authored at
`.apic/template/<name>.json` (a flat name), seeded from the built-in default or,
with `--use-template <name>`, from an existing template, with the same flags
applied. It refuses to overwrite an existing template. `--template` and `-f` are
mutually exclusive; `--use-template` composes with either.

```bash
apic create -f auth/login                            # contract from the project template
apic create -f auth/login --use-template mobile      # contract from the `mobile` template
apic create -f auth/login -X POST 'https://api.example.com/auth/login?remember=true' \
  -H 'Content-Type: application/json' -d @body.json --response '200:OK'
apic create --template mobile                        # author a new template
apic create --template mobile --use-template convention  # seed it from `convention`
```

### `apic edit -f <query> [<url>] [flags]`
Resolves `<query>` exactly like `read` (path, extensionless, or fuzzy) and
changes only the fields the flags name, leaving the rest of the contract as it
is. The result is validated before it is written, and on success it prints
`updated <path>`. An ambiguous query fails and lists the candidates.

The same flags work on `apic create`:

- `<url>`, the request URL. A `?a=1&b=2` query string is split off into query
  params, each added or updated by name:
  `apic edit -f login 'https://api.example.com/auth/login?remember=true'`
- `-X`/`--request <method>`, the HTTP method, in any case:
  `apic edit -f login -X POST`
- `-H`/`--header 'Name: value'`, adds a request header or replaces the one with
  that name (names compare without case). `'Name:'` with no value removes it.
  Repeatable: `apic edit -f login -H 'Authorization: Bearer {token}' -H 'DeviceID:'`
- `-d`/`--data <body>`, the JSON request body, as literal text, `@file`, or
  `@-` for stdin: `apic edit -f login -d @body.json`
- `-F`/`--form <part>`, a multipart part, `name=value` for a text part or
  `name=@file` (optionally `'name=@file;type=mime'`) for a file part, which
  records the file's name, not its contents. Added or replaced by name,
  repeatable: `apic edit -f upload -F title=Hello -F 'avatar=@me.png;type=image/png'`
- `--response <code:desc>`, adds a response or updates the description of the
  first one with that code. Repeatable: `apic edit -f login --response '401:Unauthorized'`
- `--response-body <code> <body>`, the JSON body of response `<code>`, in the
  `-d` forms: `apic edit -f login --response-body 200 @ok.json`
- `--response-header <code> <header>`, a header on response `<code>`, with the
  `-H` rules: `apic edit -f login --response-header 200 'X-Request-Id: abc-123'`
- `--name <name>`, the contract name: `apic edit -f login --name login-v2`
- `--description <text>`, the contract description:
  `apic edit -f login --description 'Log a user in'`

A request body is either JSON or multipart, so `-d` on a contract with multipart
parts, and `-F` on one with a JSON body, fail and point you at `apic open`.
`--response-body` and `--response-header` for a code with no response fail and
point you at `--response`, which you can pass in the same command to add it
first. Nothing is written when any flag fails or the result is not a valid
contract.

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
opens the matching contract in your external editor. An ambiguous query fails
and lists the candidates. Editor resolution order: `--editor` flag → `$VISUAL`
→ `$EDITOR` → `vi`. The flag picks the editor for a single invocation and the
value may include arguments. GUI editors need their wait flag (`code --wait`,
`subl -w`) so `apic` waits for the file to be saved.

Pass `--template` instead of `-f` to edit the project template in
`.apic/template/` that `apic create` scaffolds from (the sole template, or
`convention.json` by default); it is seeded from the built-in default first if
none exists yet. `--template` and `-f` are mutually exclusive, and exactly one
is required. To author a *new* named template, use `apic create --template <name>`.

```bash
apic open -f user/user.json
apic open -f user
apic open -f user -e nano       # open with a one-off editor
apic open -f user -e "code --wait"
apic open --template            # edit the project template
```

Output is colorized when stdout is a terminal and plain when piped, so it stays
clean in scripts. Contract strings are sanitized before display, so a file from
an untrusted source cannot inject terminal escape sequences.

### `apic remove (-f <query> | --template <name>) [-y]`
Resolves `<query>` exactly like `read`/`open` (path, extensionless, or
fuzzy, failing with the candidates when ambiguous) and deletes the matching
contract file. On an interactive terminal it asks `Remove <path>? [y/N]` first
and only deletes on `y`/`yes`; `-y`/`--yes` skips the question, and when
stdin/stdout is not a terminal (scripts) it removes without prompting.

Pass `--template <name>` instead of `-f` to remove a project template from
`.apic/template/` (fuzzy-matched the same way, with the same confirmation). No
template is protected — removing `convention.json` or the last template is
allowed; `apic create` reseeds `convention.json` from the built-in default next
time. `--template` and `-f` are mutually exclusive.

```bash
apic remove -f user/user.json
apic remove -f login            # fuzzy, with confirmation
apic remove -f login -y         # no confirmation
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
| `multipart` | no | A multipart request body, as an array of parts; see [Multipart bodies](#multipart-bodies). Use it instead of `request`, not alongside it. |
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
| `multipart` | no | A multipart response body, as an array of parts. Use it instead of `schema`, not alongside it. |

Path parameters are not a separate section: write them inline in the `url`
string as `{name}` (e.g. `{id}` in `.../users/{id}`).

> The request body has no wrapper key, it is the JSON value itself. A response's
> body lives under `schema` (not `example`). Contracts written for the older
> `{ "example": ... }` body shape do not carry over and must be updated.

### Multipart bodies

A request or a response can carry a multipart body (form data and file
uploads) instead of a JSON one. Each part is one form field:

```json
"multipart": [
    { "name": "title", "value": "Hello", "required": true },
    {
        "name": "avatar",
        "filename": "avatar.png",
        "contentType": "image/png",
        "description": "Profile picture",
        "required": false
    }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Form field name. |
| `value` | no | Example value for a text part. |
| `filename` | no | Present on a file part, e.g. `avatar.png`. A part with a `filename` is a file part; one without is a text part. |
| `contentType` | no | The part's own media type, e.g. `image/png`. |
| `description` | no | What the part is for. |
| `required` | no | Whether the part is required (default `false`). |

A request sets `request` or `multipart`, and a response sets `schema` or
`multipart`, never both: an HTTP message carries one body, and `apic validate`
rejects a contract that sets both. In the desktop GUI, the body-type dropdown
switches a body between JSON and Multipart while editing.

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
