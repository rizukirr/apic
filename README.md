# apic

**API contracts, the Git way.** `apic` is a high-performance CLI/TUI designed for total freedom in contract collaboration. By storing your API contracts as plain JSON files in your repository, your architecture becomes natively diffable, reviewable, and versioned. No more silos—just your code and your contracts in perfect sync.

https://github.com/user-attachments/assets/89b5fb4b-7942-49a1-9bee-41308d234236

## Why?

Mainstream API tooling gates collaboration behind paywalls—forcing you to pay per team member just to share a workspace. Costs scale aggressively as teams grow. 

`apic` flips the script: because contracts live as plain JSON files in your repository, **your existing Git workflow *is* the collaboration layer**. No seats, no separate accounts—if a developer can clone the repo, they can collaborate on the contract.

This approach brings immediate advantages:

*   **Zero-Cost Collaboration** — Sharing is a simple `git push`. Everyone with repository access already has full collaboration capabilities.
*   **Atomic Versioning** — Contracts change in the exact same commit as the implementation, preserving full history and `git blame`.
*   **Native Code Review** — Contract modifications show up as clean diffs in Pull Requests, reviewed by the same team, on the same platform.
*   **Terminal-First Readability** — No raw JSON eye-strain. `apic read` renders your contracts into clean, colorized tables directly in your shell.

## Install

**crates.io** (recommended) — installs the `apic` command:

```bash
cargo install apic-cli
```

**From source** (requires a Rust toolchain, 1.88+):

```bash
git clone https://github.com/rizukirr/apic
cd apic
cargo install --path .
```

To run without installing, use `cargo run -- <args>` from the project directory.

**Prebuilt binaries** — grab the archive for your platform from the
[latest release](https://github.com/rizukirr/apic/releases), verify the
`.sha256` checksum, extract, and put `apic` on your `PATH`. Builds are provided
for Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), and Windows (x86_64).

## Quick start

```bash
# 1. Initialize a project in the current directory (creates .apic/config.toml)
apic init

# 2. Point apic at the folder that holds your contract files
apic config --set-dir api-contract

# 3. Scaffold a new contract from a template (opens the interactive editor)
apic create -f auth/login.json

# 4. List and read contracts
apic list
apic read -f login
```

### Editing contracts

`apic create <file>` and `apic open <file>` open an interactive terminal editor
(TUI) by default. It shows the contract exactly as `apic read` renders it — the
same header, sections, tables, and inline JSON examples — and lets you edit in
place:

- **Navigate:** `↑/↓` (or `j/k`) select a row; `Enter` steps into the row's cells;
  `←/→` (or `h/l`) move between cells; `Esc` steps back out.
- **Edit a cell:** `Enter` or `i` edits a text cell; `Enter` cycles the method,
  toggles a `required` flag, or toggles a body `type` between `object` and
  `object[]`. While typing, `Enter` commits and `Esc` cancels.
- **Expand:** `Enter` on the `METHOD url` line reveals the protocol, host, and
  path; `Enter` on a `REQUEST`/`RESPONSE` title reveals its code, description, and
  type. `Esc` collapses either.
- **Add / delete:** `a` adds a row to the current section — a nested field when
  you're on an `object` field, or a new response on the `+ add response` line;
  `d` deletes the selected row after a confirmation.
- **Examples:** `Enter` on an example edits its JSON in a pop-up; `g` generates a
  sample example from the body's schema (an array for `object[]` bodies).
- **Save / quit:** `Ctrl-S` saves; `Esc`/`q` exits; `?` shows the full key map.

Prefer your own editor? Pass `--editor` to open the file in `$VISUAL`/`$EDITOR`
(or a specific one, e.g. `apic open login --editor "code --wait"`).

## Commands

### `apic init [--set-dir <dir>]`
Initializes an `.apic` project in the current directory by creating
`.apic/config.toml`. The optional `--set-dir` records which directory contract
files are scanned from (defaults to the current directory).

### `apic config [--set-dir <dir>]`
Updates project configuration.

- `--set-dir <dir>` — change the working directory that contracts are scanned
  from (must exist).

### `apic create -f <filename> [-e <editor>]`
Creates a new contract seeded from the project template (`.apic/template.json`,
falling back to the built-in default) and opens it in the interactive TUI; the
file is written only when you save. A relative path is resolved against the
configured working directory. `apic` refuses to overwrite an existing file.

Pass `-e`/`--editor` to scaffold the file to disk and open it in your external
editor instead of the TUI. Editor resolution order: `--editor` flag → `$VISUAL`
→ `$EDITOR` → `vi`. The flag picks the editor for a single invocation (e.g.
`apic create -f auth/login.json -e nano`) and the value may include arguments.
GUI editors need their wait flag (`code --wait`, `subl -w`) so `apic` waits for
the file to be saved.

### `apic list [--filter <query>] [--absolute <true|false>]`
Lists discovered `.json` contract files under the working directory.

- `--filter <query>` — show only contracts whose path fuzzy-matches the query,
  best match first (e.g. `apic list --filter user`).
- `--absolute <true|false>` — print absolute paths or paths relative to the
  working directory (`false`, the default).

### `apic read -f <query> [-s <status>]`
Renders a contract as formatted tables. `-s <status>` filters the response
section to a single HTTP status code.

`<query>` is resolved flexibly — an exact match wins, then fuzzy:

1. a path relative to the working directory — `user/user.json`
2. the same without the `.json` extension — `user/user`, `auth/login`
3. a fuzzy fragment — `user`, `logn`

```bash
apic read -f user/user.json   # exact path
apic read -f auth/login       # extension optional
apic read -f login            # fuzzy
apic read -f login -s 401     # show only the 401 response
apic read -f login --example  # show raw JSON example payloads
```

By default each schema table is followed by its example payload (when the
contract provides one), labeled `Example:`, so structure and a concrete
payload read together. With `--example` (or `-e`) the schema tables are
skipped entirely and only the raw JSON payloads print — the compact
copy-paste view:

```text
 REQUEST
 {
   "username": "rizukirr",
   "password": "123qweA@"
 }

 RESPONSE 200 — Successful login
 {
   "status": 200,
   "message": "Login successful",
   "data": { "access_token": "..." }
 }
```

### `apic open (-f <query> | --template) [-e <editor>]`
Resolves `<query>` exactly like `read` (path, extensionless, or fuzzy) and
opens the matching contract in the interactive TUI. Pass `-e`/`--editor` to open
it in your external editor instead — the same editor resolution as `apic create`.

Pass `--template` instead of `-f` to edit the project template
(`.apic/template.json`) that `apic create` scaffolds from; it is seeded from
the built-in default first if it does not exist yet. `--template` and `-f` are
mutually exclusive, and exactly one is required.

```bash
apic open -f user/user.json
apic open -f user
apic open -f user -e nano       # open with a one-off editor
apic open --template            # edit the project template
```

Output is colorized when stdout is a terminal and plain when piped, so it stays
clean in scripts. Contract strings are sanitized before display, so a file from
an untrusted source cannot inject terminal escape sequences.

### `apic remove -f <query>`
Resolves `<query>` exactly like `read`/`open` (path, extensionless, or
fuzzy, prompting to pick when ambiguous) and deletes the matching contract
file. On an interactive terminal it asks `Remove <path>? [y/N]` first and only
deletes on `y`/`yes`; when stdin/stdout is not a terminal (scripts) it removes
without prompting.

```bash
apic remove -f user/user.json
apic remove -f login            # fuzzy, with confirmation
```

### `apic validate [-f <query>] [--template]`
Checks that contracts parse and conform to the schema. With no `-f`, every
contract under the working directory is checked. A query ending in `/` (e.g.
`auth/`) validates every contract under that folder, recursively; otherwise the
query resolves to a single contract like `read` (path, extensionless, or fuzzy).
Prints `ok`/`FAIL` per file with the parse error (line and column) for failures,
and **exits non-zero if any contract is invalid** — so it drops straight into a
CI step or pre-commit hook.

```bash
apic validate               # check every contract
apic validate -f login      # check one
apic validate -f auth/      # check every contract under auth/, recursively
apic validate --template    # check the project template (.apic/template.json)
```

```text
ok   auth/login.json
FAIL user/user.json: EOF while parsing an object at line 12 column 1

2 passed, 1 failed
```

> The long flag is `--find` on the commands that resolve an existing contract
> (`read`, `open`, `remove`, `validate`); the short `-f` is unchanged. `create`
> keeps `--filename` because it names a new file rather than finding one.

### `apic convert --postman <file> [--destination <dir>]`
Imports a Postman collection as apic contracts — one JSON file per request,
mirroring the collection's folder nesting. Accepts Postman Collection exports of
v1.0.0, v2.0.0, and v2.1.0 (auto-detected).

- `--postman <file>` — the Postman collection JSON to import.
- `--destination <dir>` — where to write the contracts, relative to the working
  directory (created if missing). **Optional** — defaults to the working
  directory itself. The path is confined to the working directory (`..`/absolute
  escapes are rejected) and existing files are never overwritten.

Each Postman folder becomes a directory and each request becomes
`folder/request_name.json`. Only the fields apic models are imported (method,
URL, headers, request/response bodies); Postman-specific data (auth blocks,
scripts, events, variables) is ignored. A request whose HTTP method apic does
not model (anything other than `GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`/
`OPTIONS`) is imported as `GET` with a warning, so nothing is downgraded
silently.

```bash
apic init                                   # an apic project is required
apic convert --postman MyAPI.postman.json   # writes into the working directory
apic convert --postman MyAPI.postman.json --destination imported
```

```text
warning: request "Preflight" uses method TRACE, unsupported by apic — imported as GET
Converted 12 contract(s) into imported (1 warning)
```

## Security

`apic` treats contract files and paths as untrusted, so it is safe to run
against contracts from any source:

- **Terminal-escape safe** — all file-derived strings (contract fields, file
  names) are stripped of control characters before printing.
- **Path-confined** — `apic create` refuses paths that escape the working
  directory via `..` or an absolute path elsewhere.
- **Bounded** — contract files larger than 5 MiB are rejected before reading,
  and pathologically nested JSON is rejected rather than overflowing the stack.

## Contract format

A contract is a single JSON object describing one endpoint. See
[`src/templates/contract.json`](src/templates/contract.json) for the full
template that `apic create` writes.

`apic init` writes a starter template to `.apic/template.json`. Edit it to set
a project-wide convention — for example a standing `device-id` header — and
every `apic create` reuses it. The file is never overwritten once it exists; if
it is missing or malformed, `apic create` falls back to the built-in default.

```json
{
    "name": "update-user",
    "description": "Update a user",
    "method": "PUT",
    "url": {
        "protocol": "https",
        "host": "api.example.com",
        "path": ["users", "{id}"],
        "query": [
            {
                "name": "notify",
                "value": "true",
                "description": "Send a notification email",
                "required": false
            }
        ],
        "variable": [
            {
                "name": "id",
                "type": "int",
                "description": "User ID"
            }
        ]
    },
    "headers": [
        { "name": "Content-Type", "value": "application/json" },
        { "name": "Authorization", "value": "Bearer {token}" }
    ],
    "request": {
        "schema": [
            {
                "name": "name",
                "type": "string",
                "default": null,
                "description": "Display name",
                "required": true
            }
        ],
        "example": {
            "name": "Rizki Rakasiwi"
        }
    },
    "responses": [
        {
            "code": 200,
            "description": "User updated",
            "schema": [
                {
                    "name": "status",
                    "type": "int",
                    "default": "200",
                    "description": "Status code",
                    "required": true,
                    "properties": null
                },
                {
                    "name": "message",
                    "type": "string",
                    "default": null,
                    "description": "Human-readable message",
                    "required": true,
                    "properties": null
                }
            ],
            "example": {
                "status": 200,
                "message": "OK"
            }
        }
    ]
}
```

Both `schema` (field-level detail, rendered as tables) and `example` (a raw
JSON payload) are optional in the request and in each response — early-stage
contracts often start with just an example, formal ones with just a schema.
The default view shows the example beneath its schema table (or alone when
there is no schema), and `read --example` shows only the payloads.

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Endpoint name. |
| `description` | no | Short description of the endpoint. |
| `method` | yes | HTTP method: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, or `OPTIONS`. |
| `url` | yes | Request URL, broken into parts (see below). |
| `headers` | yes | Array of headers (`name`, `value`). |
| `request` | no | Request body: `{ "type": <body shape>, "schema": [fields], "example": <raw JSON> }` — all parts optional; `type` defaults to `"object"` (see [Array bodies](#array-bodies)). |
| `responses` | yes | Array of responses (`code`, `description`, optional `type`, optional `schema`, optional `example`). |

The `url` object has:

| Field | Required | Description |
|-------|----------|-------------|
| `protocol` | yes | URL scheme, e.g. `http` or `https`. |
| `host` | yes | Host, e.g. `api.example.com`. |
| `path` | no | Path segments as an array, e.g. `["auth", "login"]`. |
| `query` | no | Array of query parameters (`name`, `value`, `description`, `required`). |
| `variable` | no | Array of path variables (`name`, optional `type` — defaults to `string`, `description`). |

A **field** (in the request `schema` and response `schema`) has:

| Field | Description |
|-------|-------------|
| `name` | Field name. |
| `type` | Data type (`string`, `int`, `file`, `object`, …). Append `[]` for a list — `string[]`, `object[]` (see [Array bodies](#array-bodies)). |
| `default` | Default value as a string, or `null`. |
| `description` | Field description. |
| `required` | Whether the field is required. |
| `accept` | Allowed MIME types for `file` fields, e.g. `"image/png, image/jpeg"`; omit for ordinary fields. |
| `properties` | Nested fields for `object` (or `object[]`) types, or `null`. |

Request and response fields share the same shape, so request bodies can nest
objects via `properties` just like responses.

### Multipart / file uploads

For `multipart/form-data` endpoints, declare the encoding in the
`Content-Type` header as usual and use `"type": "file"` for file parts. The
optional `accept` field documents which MIME types the part allows, and
`apic read` shows it in an extra ACCEPT column:

```json
{
    "name": "upload-avatar",
    "method": "POST",
    "url": {
        "protocol": "https",
        "host": "api.example.com",
        "path": ["user", "avatar"]
    },
    "headers": [
        { "name": "Content-Type", "value": "multipart/form-data" }
    ],
    "request": {
        "schema": [
            {
                "name": "avatar",
                "type": "file",
                "default": null,
                "description": "Avatar image, max 2MB",
                "required": true,
                "accept": "image/png, image/jpeg"
            },
            {
                "name": "caption",
                "type": "string",
                "default": null,
                "description": "Optional caption",
                "required": false
            }
        ]
    },
    "responses": []
}
```

```text
REQUEST
 NAME     TYPE    REQ  ACCEPT                 DESCRIPTION
 avatar   file    ✓    image/png, image/jpeg  Avatar image, max 2MB
 caption  string                              Optional caption
```

### Array bodies

A request or response body can be a JSON **array** instead of a single object —
useful for bulk requests and list endpoints. Set the body-level `"type"` to an
array form, and `apic` reads the `schema` fields as a description of **each
element**:

- `"object"` — a single object (the default when `type` is omitted).
- `"object[]"` — an array of objects; `schema` describes each element's fields.
- A field's own `"type"` may carry the same `[]` suffix: `"string[]"` is a list
  of scalars (e.g. `["a", "b"]`), `"object[]"` a list of objects whose fields go
  in `properties`.

`apic read` marks an array body with a `· <type>` suffix on the section title
and shows the raw `string[]`/`object[]` in the TYPE column. See
[`example/items/bulk-create.json`](example/items/bulk-create.json) (an array
request **and** an array response) and
[`example/items/list.json`](example/items/list.json) (an array response).

```json
{
    "name": "bulk-create-items",
    "method": "POST",
    "url": { "protocol": "https", "host": "api.example.com", "path": ["items", "bulk"] },
    "headers": [{ "name": "Content-Type", "value": "application/json" }],
    "request": {
        "type": "object[]",
        "schema": [
            { "name": "name", "type": "string",   "default": null, "description": "Item name",        "required": true },
            { "name": "tags", "type": "string[]", "default": null, "description": "Free-form labels", "required": false }
        ],
        "example": [
            { "name": "Widget", "tags": ["new", "featured"] },
            { "name": "Gadget", "tags": [] }
        ]
    },
    "responses": [
        {
            "code": 201,
            "type": "object[]",
            "description": "Items created",
            "schema": [
                { "name": "id",   "type": "string", "default": null, "description": "Generated id", "required": true, "properties": null },
                { "name": "name", "type": "string", "default": null, "description": "Item name",     "required": true, "properties": null }
            ]
        }
    ]
}
```

```text
 REQUEST · object[]
 NAME  TYPE      REQ  DESCRIPTION
 name  string    ✓    Item name
 tags  string[]       Free-form labels

 RESPONSE 201 — Items created · object[]
 NAME  TYPE    REQ  DESCRIPTION
 id    string  ✓    Generated id
 name  string  ✓    Item name
```

## Configuration

`apic init` writes `.apic/config.toml`:

```toml
name = "apic"
version = "0.1.0"

[root]
working_dir = "api-contract"
```

`working_dir` is stored relative to the project root, so `.apic/config.toml`
is safe to commit and share — it resolves correctly on any clone. `apic`
locates the project by walking up from the current directory to find the
`.apic` directory, so commands work from anywhere inside the project tree.

## License

Licensed under the [MIT License](LICENSE).
