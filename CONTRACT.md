# Contract format

How an apic contract file is laid out, what each field means, and how templates and the project config work. For the commands that read and write these files, see [COMMAND.md](COMMAND.md).

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

## Fields

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

## Multipart bodies

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
