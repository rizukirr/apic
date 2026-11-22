---
name: apic
description: Read, send, check and write API contracts with the apic CLI. Use when the project has a .apic folder, when the user mentions apic or API contracts, or when asked to call, test or document an HTTP endpoint that is stored as a contract.
---

# apic

The `apic` CLI keeps API contracts as JSON files in a git repo. Each contract describes one HTTP request and its documented responses. Use the CLI to read, send, check and write them, rather than hand-written curl calls or hand edits to the JSON.

## When to use it

Use it when the project has a `.apic` folder, or when `apic list` succeeds. If `apic` is not on PATH, tell the user and stop. Do not install it unless they ask.

## The safe loop

Pass `--json` to every command. Each prints one JSON document on stdout.

1. Find contracts with `apic list --json`. Narrow the list with `--filter user`.
2. Read one with `apic read -f auth/login --json`. `-f` takes a path, a path without `.json`, or a fuzzy fragment such as `login`.
3. Preview the request with `apic send -f auth/login --env dev --dry-run --json`. Nothing is sent. Variable values show as `$name`, so secrets never reach you. Check that `target` is the server you expect and that `undefined` is empty.
4. Send it with `apic send -f auth/login --env dev --json`. The document holds the reply's `status`, `headers` and `body`.
5. Check that the server still honours the contract with `apic check -f auth/login --env dev --json`.

Preview before the first send of any contract whose method is not GET, HEAD or OPTIONS. Those requests can change data on the server.

`apic env --json` lists the environments without their values. Pick one with `--env NAME`, and override one value with `--var name=value`.

`apic check` with no `-f`, or with a folder query ending in `/` such as `-f auth/`, checks many contracts at once. It sends only GET, HEAD and OPTIONS contracts and reports the rest as `skipped`.

## Exit codes

A failure prints `{"error": {"kind": "...", "message": "..."}}`.

| Code | Kind | What to do |
|---|---|---|
| 0 | | Success. `send` exits 0 for any HTTP status, so read `status`. |
| 1 | `usage`, `not_initialized`, `error` | Read the message. `not_initialized` means there is no `.apic` folder, so ask before running `apic init`. |
| 2 | `not_found`, `ambiguous` | Run `apic list --json` and pass a more exact path. |
| 3 | `invalid` | The contract file is broken. Run `apic validate -f QUERY --json` and fix the file. |
| 4 | `network` | The server did not answer. Check `target` with `--dry-run` and the chosen env, then tell the user. |
| 5 | `mismatch` | The live response differs from the contract. Report the `differences`. Do not edit the contract to match unless the user says the server is right. |

## Writing contracts

Write contracts with flags, then validate them:

```bash
apic create -f auth/login -X POST '$base/auth/login' \
  -H 'Content-Type: application/json' \
  -d '{"email": "$email", "password": "$password"}' \
  --response '200:OK' --response-body 200 '{"token": "abc"}'
apic edit -f auth/login --response '401:Unauthorized'
apic validate -f auth/login --json
```

`apic create` refuses to overwrite a file. `apic edit` changes only the fields its flags name. Variables are `$name` or `${name}`, filled at send time from `.apic/env/NAME.toml` and `--var`.

## Importing and exporting contracts

When the user already has an OpenAPI spec or a Postman collection, import it instead of writing contracts by hand:

```bash
apic convert --openapi openapi.yaml --destination imported --json
apic convert --postman MyAPI.postman.json --destination imported --json
```

`--openapi` takes OpenAPI 3.0, 3.1 or Swagger 2.0, in JSON or YAML, and writes one contract per operation in a folder per tag. Bodies are examples taken from the spec or built from its schemas, so review them before relying on them. Warnings on stderr name what was imported lossily. An import refuses to overwrite existing contracts. Only pass `--force` when the user asked to replace them.

An imported URL can start with `{{baseUrl}}` when the spec names no server. Tell the user which env file needs `baseUrl`, then run `apic validate --json`.

To hand contracts to Postman, run `apic export --postman ../api.postman.json --json`. Write the file outside the contracts folder, or apic reads it as a broken contract. `-f folder/` exports one folder, and `--env NAME` fills the collection variables from that env's shared file only, never the local one.

## Safety rules

- Never pass `--unsafe` to `apic check` unless the user asked for it. It makes folder runs send POST, PUT, PATCH and DELETE requests.
- Never read, print, copy or commit `.apic/env/*.local.toml`. Those files hold secrets. `apic env` and `--dry-run` show what you need without the values.
- Never put a token, password or key in a contract. Use a `$variable` and tell the user which env file needs the value.
- Never run `apic remove` unless the user asked for that contract to be deleted.

## More detail

Run these when the summary above is not enough. Each prints a full reference in Markdown, matching the installed version.

- `apic docs command` covers every command and flag, and what each prints with `--json`.
- `apic docs sending` covers environments, variables, `--dry-run` and reading replies.
- `apic docs contract` covers the contract file format.
