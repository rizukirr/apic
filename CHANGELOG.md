# Changelog

All notable changes to apic are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.2] - 2026-06-20

### Changed
- `apic-gui` adopts the flatpak app id (`FLATPAK_ID`) as its window app id when
  running inside a Flatpak, so the compositor associates the window with the
  installed desktop entry (correct icon and name). Behavior is unchanged outside
  Flatpak.

### Packaging
- Distribution packaging added: AUR (`apic-bin`), Fedora COPR (`apic-cli` /
  `apic-gui`), winget (`rizukirr.apic` / `rizukirr.apic-gui`), and a Flathub
  Flatpak manifest (`io.github.rizukirr.apic`).

## [0.3.1] - 2026-06-19

### Added
- `apic-gui --desktop-entry` registers the GUI in the Linux application launcher
  (writes a `.desktop` file and icon into the per-user XDG data dir, pointing at
  the running binary). Useful after `cargo install apic-gui`, which otherwise
  only puts the binary on `PATH`.

## [0.3.0] - 2026-06-18

### Added
- `apic-gui`, a styled desktop GUI front-end for browsing and editing contracts,
  built on the shared core. Published to crates.io (`cargo install apic-gui`)
  and shipped as prebuilt binaries on tagged releases.

### Changed
- Refactored the project into a workspace: the contract model and logic now live
  in the `apic-core` crate, shared by both the CLI/TUI (`apic`) and the GUI so
  the two cannot drift. `apic-core` is published to crates.io.

## [0.2.4] - 2026-06-16

### Added
- `apic validate` now checks each contract for conformance against
  `.apic/template.json`, in addition to schema validation. The template is
  treated as a partial: only the sections it declares are enforced. Checks cover
  headers (names, case-insensitive), `url.protocol`/`url.host` (exact values),
  `url.path`/`query`/`variable` (declared segments and names present), and
  `request`/`responses` schema field names (recursing into nested `properties`;
  responses matched by code). `.apic/` is excluded from the validate scan so a
  partial template is not itself validated as a contract (#23).

### Security
- Path confinement is now symlink-aware. `confine_to_dir` rejects a path whose
  component is a symlink, closing a bypass where a symlinked directory or file
  inside the working directory could redirect `apic create`, `convert
  --destination`, or `remove` to write or resolve outside the configured root
  (#22, #24).
- Absolute paths in command output now collapse the user's home directory to
  `~`, so the `Created` line and error messages no longer disclose the username
  or full filesystem layout. Paths outside home are left intact (#25).

### Changed
- Tightened item visibility across the crate (`pub` narrowed to `pub(crate)`,
  single-module helpers made private) and enabled the `unreachable_pub` lint as
  a guardrail. No behavior change (#26).

## [0.2.3] - 2026-06-15

### Changed
- The default schema view now renders `(none)` whenever a request or response
  has no schema, in both `apic read` and the TUI viewer. Previously it fell back
  to printing the example payload; example payloads remain available via
  `apic read -e` (#20).
- `apic open --template` now seeds the editor the same way `apic create` does —
  the project template's own values layered over a blanked built-in structure —
  instead of merging `.apic/template.json` onto the full built-in default. The
  template's schema is preserved while the built-in's placeholder headers,
  schema fields, and examples are no longer pulled in; only the built-in's
  scalar `name`/`description`/`url` defaults fill in when the template omits
  them (#20).

## [0.2.2] - 2026-06-13

### Added
- `apic convert --postman <file> [--destination <dir>]` — import a Postman
  collection (v1.0.0 / v2.0.0 / v2.1.0, auto-detected) as per-endpoint
  contracts, mirroring the collection's folder nesting at any depth.
  `--destination` is optional and defaults to the working directory; paths are
  confined to it and existing files are never overwritten.
- Recursive folder validation: a `validate` query ending in `/` (e.g.
  `apic validate -f auth/`) validates every contract under that folder.
- `HEAD` and `OPTIONS` are now first-class HTTP methods — in contracts, the
  `read`/`validate` rendering, and the TUI method cycler. `apic convert` maps
  them natively; a method apic still does not model (`TRACE`, `CONNECT`, custom
  verbs) is imported as `GET` with a warning so nothing is downgraded silently.

### Changed
- The long flag `--filename` is renamed to `--find` on `read`, `open`,
  `remove`, and `validate` (the short `-f` is unchanged; `create` keeps
  `--filename` since it names a new file).
- `validate` prints contract paths with forward slashes on every OS.

## [0.2.1] - 2026-06-13

### Fixed
- `apic open --template` no longer fails to launch the TUI. The partial
  `.apic/template.json` is now merged onto the built-in default before it is
  parsed, so a template missing required fields (e.g. `name`) opens correctly
  (#15).

## [0.2.0] - 2026-06-13

### Added
- Interactive authoring TUI — the default surface for `apic create` and
  `apic open`. Edit contracts in place: inline text cells, enum cycling, and
  boolean toggles; an inline JSON example editor (generate one from the schema
  with `g`); nested schema editing; response editing; selectable section titles;
  `Tab`/`Shift-Tab` cell navigation; and accurate unsaved-changes detection. The
  external editor remains available behind `--editor`.

### Changed
- `apic create` seeds builtin scalar defaults and empty arrays, so a fresh
  contract is valid and ready to edit.
- Upgraded to ratatui 0.30 (via ratatui-textarea), consolidating on
  crossterm 0.29.

## [0.1.1] - 2026-06-09

### Added
- `apic validate --template` — validates the project's `.apic/template.json`
  (as merged onto the built-in default), printing `ok`/`FAIL` and exiting
  non-zero on failure. Mutually exclusive with `--filename`.

### Changed
- `apic create` now aborts with an error (writing nothing) when
  `.apic/template.json` exists but is invalid, instead of silently falling back
  to the built-in template. The zero-config path (no project, a missing template
  file, or a freshly seeded template) is unchanged.

## [0.1.0] - 2026-06-09

First crates.io release. Adds interactive resolution, tree output, project
templates, an editor flag, and a `remove` command on top of the betas.

### Added
- `list` renders contracts as a box-drawing tree on terminals, with fuzzy-match
  highlighting; piped/non-TTY output stays flat for scripts.
- Interactive picker that prompts you to choose when a contract name is ambiguous,
  wired into `read`, `open`, and `validate -f`.
- `--editor <cmd>` flag to choose the editor per invocation, replacing the editor
  setting in config.
- `.apic/template.json` is seeded on `init` and used by `apic create`; supports
  partial template merge so you only override the fields you set.
- `open --template` to open the project template directly.
- `remove` command to delete a contract.
- `init` now recovers a partially-initialized project instead of erroring.

### Changed
- Contract `url` restructured into a `url` object (base + path + query + variables).
- `init` template seeding is best-effort and no longer blocks initialization.
- Contract paths are displayed and stored with forward slashes on every OS, so
  the committed `working_dir` and contract references stay portable across
  Windows, macOS, and Linux.

## [0.1.0-beta.2] - 2026-06-04

### Added
- `read --example` and example payloads rendered beneath the schema tables.

### Changed
- Dropped the `author` field from crate metadata.

## [0.1.0-beta.1] - 2026-06-04

Initial beta release.

### Added
- Core commands: `init`, `config`, `create`, `list`, `read`, `validate`, `open`.
- Contracts stored as plain per-endpoint JSON files, designed to be diffed and
  reviewed in git.
- Security hardening: path confinement, file-size cap, output sanitization.
- SIGPIPE handling so piped output (e.g. `apic read | head`) exits cleanly.
- CI (fmt, clippy, build, test) and unit + end-to-end test suites.
- MIT license.

[Unreleased]: https://github.com/rizukirr/apic/compare/v0.2.4...HEAD
[0.2.4]: https://github.com/rizukirr/apic/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/rizukirr/apic/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/rizukirr/apic/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rizukirr/apic/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rizukirr/apic/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/rizukirr/apic/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.2...v0.1.0
[0.1.0-beta.2]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/rizukirr/apic/releases/tag/v0.1.0-beta.1
