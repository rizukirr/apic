# Sending requests

`apic send` fires the request a contract describes and prints the real response. Values that change between servers or people, such as the base URL or an access token, live in environments and fill the contract's variables at send time.

The desktop GUI can send too, see the Sending section of [GUI.md](GUI.md#sending).

## Your first request

```bash
mkdir -p .apic/env
cat > .apic/env/dev.toml <<'EOF'
baseUrl = "https://api.dev.example.com"
EOF
cat > .apic/env/dev.local.toml <<'EOF'
access_token = "paste-your-token-here"
EOF

apic send -f auth/login --env dev
```

With a contract whose URL is `$baseUrl/auth/login` and a header `Authorization: Bearer $access_token`, this sends `POST https://api.dev.example.com/auth/login` with your token, prints the response body on stdout, and prints a line such as `HTTP 200 OK · 143 ms` on stderr.

To try it without a server of your own, this repository has a `demo` environment and contracts in `example/demo/` that call the public mock APIs dummyjson.com and httpbin.org. From the repository root, run `apic send -f demo/login --env demo`, then put the returned `accessToken` in `.apic/env/demo.local.toml` as `access_token = "..."` to make `demo/me` work. `demo/get_product` needs `--var id=1`, and `demo/echo_upload` sends a multipart file.

## Variables

Any string in a contract's URL, query, headers or body can hold variables.

| You write | It becomes |
|-----------|------------|
| `$name` | the value of `name` |
| `${name}` | the value of `name`, for when text follows directly, as in `${version}beta` |
| `{{name}}` | the value of `name`, the form Postman uses |
| `{name}` in the URL | the value of `name`, for path params such as `/users/{id}` |
| `$$` | a literal `$` |

A name starts with a letter or `_` and continues with letters, digits and `_`. Only names that have a value are replaced. Everything else stays exactly as written, so `$set`, `$5` or a lone `$` in a JSON example is never touched. When the URL or a header uses a name with no value, `apic send` prints a warning and sends the text as written.

In a JSON body, variables are filled inside string values. Keys, numbers and booleans are sent as they are.

## Environments

An environment is a name, such as `dev` or `staging`, backed by up to two TOML files in `.apic/env/`:

| File | Purpose | In git |
|------|---------|--------|
| `<name>.toml` | values the team shares, such as base URLs and client IDs | committed |
| `<name>.local.toml` | your own values, such as tokens and passwords | ignored |

Both files are flat lists of `name = "value"` lines, and every value is a string. When the same name is in both files, the local one wins. An environment exists as soon as either file exists, so a local-only environment is fine.

apic keeps the line `env/*.local.toml` in `.apic/.gitignore`. `apic init` writes it, and every `apic send` adds it again if it went missing, so a local file stays out of commits. Check with `git status` before your first commit anyway if your repository has unusual ignore rules.

```toml
# .apic/env/staging.toml
baseUrl = "https://api.staging.example.com"
app_id = "apic-staging"
```

```toml
# .apic/env/staging.local.toml
access_token = "eyJhbGciOi..."
```

`apic env` lists the environments and marks those with a local file. It never prints values.

```
$ apic env
dev (+local)
staging
```

Without `--env`, no environment is used.

## One-off values

`--var NAME=VALUE` sets a variable for one send and wins over the environment. It is repeatable. It is the usual way to fill a path param:

```bash
apic send -f users/get --env dev --var id=42
apic send -f users/get --env dev --var id=42 --var access_token="$TOKEN"
```

When the URL still has a `{name}` with no value, `apic send` stops before sending and tells you which `--var` to pass.

## What gets sent

| Part | Sent as |
|------|---------|
| method | the contract's `method` |
| URL | `url` with variables filled, then every `query` entry appended as `name=value`, URL-encoded |
| headers | every `headers` entry, values filled |
| JSON body | `request`, filled and compact, with `Content-Type: application/json` unless the contract sets its own `Content-Type` |
| multipart body | `multipart`, as `multipart/form-data` with its own boundary |

In a multipart body, a text part sends its `value`. A file part, one with a `filename`, sends the contents of the file whose path is in `value`, relative to the project root:

```json
{ "name": "avatar", "value": "fixtures/avatar.png", "filename": "avatar.png", "contentType": "image/png" }
```

A file part with no path, a missing file, or a path that leads outside the project stops the send.

The URL must start with `http://` or `https://` once variables are filled. A URL still starting with `$baseUrl` means the variable has no value.

## Output and exit codes

The response body goes to stdout. On a terminal, a JSON body is pretty-printed and coloured, and `NO_COLOR` turns the colour off. When piped, the body is written exactly as received, so `apic send -f login --env dev | jq .data` works. The status summary and the warnings go to stderr.

`-i` prints the status line and the response headers on stdout before the body.

| Exit code | Meaning |
|-----------|---------|
| 0 | the server replied, with any status, 4xx and 5xx included |
| 1 | the send was refused, the connection failed, or it timed out |

Up to 5 redirects are followed. A redirect drops the `Authorization` header. The timeout is 30 seconds, changed with `--timeout <secs>`.

## Moving from `{{name}}` to `$name`

`{{name}}` keeps working. To switch a project to the `$name` form, run:

```bash
apic vars --convert           # lists each file and how many it would change
apic vars --convert --write   # rewrites them
```

It covers every contract and the project templates. A variable followed directly by a name character becomes `${name}`, so `{{id}}x` turns into `${id}x`.
