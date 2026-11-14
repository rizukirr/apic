# apic commands

Every `apic` subcommand, with its flags and examples. For installing apic, the
quick start, templates and the contract format, see the [README](README.md).

Run `apic <command> --help` for the same reference in your terminal.

## `apic init [--set-dir <dir>]`
Initializes an `.apic` project in the current directory by creating
`.apic/config.toml`. The optional `--set-dir` records which directory contract
files are scanned from (defaults to the current directory).

## `apic config [--set-dir <dir>]`
Updates project configuration.

- `--set-dir <dir>`, change the working directory that contracts are scanned
  from (must exist).

## `apic create (-f <filename> | --template <name>) [<url>] [--use-template <name>] [flags]`
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

## `apic edit -f <query> [<url>] [flags]`
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

## `apic list [--filter <query>] [--absolute <true|false>]`
Lists discovered `.json` contract files under the working directory.

- `--filter <query>`, show only contracts whose path fuzzy-matches the query,
  best match first (e.g. `apic list --filter user`).
- `--absolute <true|false>`, print absolute paths or paths relative to the
  working directory (`false`, the default).

## `apic read -f <query> [-s <status>]`
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

## `apic send -f <query> [--env <name>] [--var <name=value>]... [-i] [--timeout <secs>]`
Resolves `<query>` like `read`, fills the variables and sends the contract's request. The response body goes to stdout, pretty-printed and coloured on a terminal and raw when piped. The status summary and warnings about undefined variables go to stderr. Up to 5 redirects are followed. The exit code is 0 for any HTTP reply, including 4xx and 5xx, 1 for a refusal, and 4 for a connection failure or timeout. Variables, environments and what gets sent are covered in [SENDING.md](SENDING.md).

- `--env <name>` takes values from `.apic/env/<name>.toml`, overlaid by `<name>.local.toml`.
- `--var <name=value>` sets a variable and overrides the environment. Repeatable.
- `-i` prints the status line and response headers before the body.
- `--timeout <secs>` gives up after that many seconds, 30 by default.

```bash
apic send -f auth/login --env dev
apic send -f login --env dev --var tok=abc -i | head
```

## `apic env`
Lists the environments in `.apic/env`. A name followed by `(+local)` has a local file next to it. Values are never printed.

```bash
apic env
```

## `apic vars --convert [--write]`
Finds `{{name}}` variables in every contract and template and shows the file and the count for each. Without `--write` nothing changes. With `--write` each `{{name}}` becomes `$name`.

```bash
apic vars --convert           # dry run
apic vars --convert --write   # apply
```

## `apic open (-f <query> | --template) [-e <editor>]`
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

## `apic remove (-f <query> | --template <name>) [-y]`
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

## `apic validate [-f <query>] [--template]`
Checks that contracts parse and match the required contract format. With no `-f`, every
contract under the working directory is checked. A query ending in `/` (e.g.
`auth/`) validates every contract under that folder, recursively; otherwise the
query resolves to a single contract like `read` (path, extensionless, or fuzzy).
Prints `ok`/`FAIL` per file with the parse error (line and column) for failures,
and **exits 3 if any contract is invalid**, so it drops straight into a
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

## `apic check [-f <query>] [--env <name>] [--var <name=value>]... [--status <code>] [--unsafe] [--timeout <secs>]`
Sends each contract's request and compares the live response with the documented responses. Targets resolve like `validate`: no `-f` checks every contract, a query ending in `/` checks every contract under that folder, and any other query checks one contract. `--env`, `--var` and `--timeout` work as in `send`.

The live status must be one the contract documents, and equal `--status` when given. The reply passes when it matches any documented response with that status:

- every key in the example body exists in the live body with the same JSON type. Extra keys are fine, and integers and floats count as one type.
- a `null` on either side matches anything.
- live array elements are compared against the example's first element, and an empty array matches.
- every header marked `required` is present, in any case.

Only types are compared, never values. A response with no example body, or a multipart one, accepts any body.

A folder or whole-project run sends only `GET`, `HEAD` and `OPTIONS` contracts and reports the others as skipped, unless `--unsafe` is given. A single contract is always sent.

The run **exits 5 if any contract mismatched**, otherwise 4 if a connection failed, 3 if a contract did not parse, 1 for any other error, and 0 when every contract passed or was skipped.

```bash
apic check --env staging                  # every GET, HEAD and OPTIONS contract
apic check -f auth/ --unsafe              # every contract under auth/, POST included
apic check -f get_product --status 200    # one contract, and it must answer 200
```

```text
pass      contracts/demo/get_product.json   200 · 118 ms
mismatch  contracts/demo/me.json   200 · 95 ms
          $.value.id: expected number, got string
skipped   contracts/demo/login.json   POST, pass --unsafe to send it

1 passed, 1 mismatched, 1 skipped, 0 failed
```


## `apic convert --postman <file> [--destination <dir>]`
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

## Machine-readable output

Every command takes `--json`, before or after the subcommand. It then prints exactly one JSON document on stdout: the command's result, or an error object. Nothing is coloured, and the output is the same on a terminal and in a pipe. Warnings stay as text on stderr.

```bash
apic list --json
{"contracts":["auth/login.json","user/get_user.json"]}

apic read -f nothing --json
{"error":{"kind":"not_found","message":"No contract found matching 'nothing'"}}
```

The exit code is the same with or without `--json`:

| Code | Error kinds | When |
|------|-------------|------|
| 0 | | success, including `send` getting any HTTP status |
| 1 | `usage`, `not_initialized`, `error` | bad flags, outside a project, a file that cannot be read or written |
| 2 | `not_found`, `ambiguous` | no contract or template matches the query, or several do |
| 3 | `invalid` | a contract does not parse or fails the schema, or `validate` found failures |
| 4 | `network` | the connection failed or timed out |
| 5 | `mismatch` | `check` found a live response that differs from its contract |

What each command prints with `--json`:

| Command | Result |
|---------|--------|
| `list` | `{"contracts": [...]}`, best match first with `--filter` |
| `read` | the contract as stored, with `--status` keeping only that response |
| `send` | `{"status", "reason", "elapsed_ms", "headers": [{"name", "value"}], "body", "body_encoding"}` |
| `validate` | `{"results": [{"path", "ok", "error"}], "passed", "failed"}` |
| `check` | `{"results": [{"path", "method", "result", "code", "elapsed_ms", "differences": [{"path", "message"}], "reason", "error"}], "passed", "mismatched", "skipped", "failed"}` |
| `env` | `{"environments": [{"name", "has_local"}]}`, never the values |
| `vars --convert` | `{"files": [{"path", "count"}], "total", "written"}` |
| `create`, `edit`, `remove` | `{"action": "created" / "updated" / "removed", "path"}` |
| `convert` | `{"written", "destination"}` |
| `open` | `{"path"}` once the editor closes |
| `init` | `{"status": "initialized" / "template_seeded"}` |
| `config` | `{"working_dir"}` |

`send` puts a JSON body in `body` as a JSON value and sets `body_encoding` to `json`. Other text bodies are a string with `text`, and binary bodies are base64 with `base64`.

`validate` with failures prints its full results document and exits 3. `remove --json` deletes only with `--yes`, and without it fails with a `usage` error. `check` always prints its full results document, and `result` is `pass`, `mismatch`, `skipped` or `error`.

Contract paths are relative to the working directory, the form `-f` accepts. Template paths are relative to the project root.
