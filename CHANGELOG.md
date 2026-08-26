# Changelog

All notable changes to apic are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-08-25

A feature release on top of 0.4.1. The CLI, the TUI, and the contract format are unchanged, so existing projects need no migration. Everything new lives in the desktop GUI, which gained a git panel, and that panel is the headline: status, staging, commit, branch switching, branch creation and deletion, and merge conflict resolution, all built on top of a restructured `apic-gui` codebase.

### Added

- **Git panel in the desktop GUI.** A new Git sidebar tab shows the working tree's status, nested by folder, with a dirty indicator on the tab and per-file symbol buttons for staging. A worker thread keeps status current, the central panel renders a diff for the selected file (including the content of untracked files, not just a field list), and stage, unstage, discard, and commit actions are wired to a git service layer built on the porcelain v2 status format.
- **Branch support.** List, switch, create, and delete branches from a branch row in the git panel. Switching guards against unsaved edits and reconciles open contracts against the new branch's state, and the branch list stops retrying indefinitely when a fetch comes back empty.
- **Merge conflict resolution.** Conflicted files get their own sidebar section. A conflict parser renders each conflict block as a colorized diff with a live preview of the resolved result, whole-file take-ours, take-theirs and resolve actions sit in the file's header row, and resolving a conflict writes the merged content and stages it.
- **Semantic diff for contracts.** Git status for a contract file is summarized field by field (what actually changed in the JSON) rather than shown as a raw text diff.

### Changed

- **`apic-gui` restructured into `app`, `features`, and `ui` layers.** `main.rs` was split into app modules, `ContractsState` and `ShellState` were extracted from `App`, the sidebar and central panel frames moved into a shell, `ui/widgets.rs` became `ui/components` plus `ui/focus`, and `ui/theme.rs` became separate token modules for colors, spacing, and typography. Item visibility was narrowed throughout to what callers actually need.
- **A suite of git wiring tests was added**, including an `App` test fixture and a bounded settle helper, and it pinned (then fixed) several defects in git status and staging that unit tests on individual functions had not caught.
- The top bar now reads as a menu bar (New, Open, Import) rather than a row of filled buttons, and the sidebar's tab row and header are visually unified with it.

### Fixed

- Scope paths are resolved through symlinks before comparison, closing a case where a symlinked directory made the git panel treat in-scope files as out of scope.
- Unstaging an untracked file works correctly, instead of leaving it staged.

### Documentation

- README documents the new git panel.

### Packaging

- The AUR `apic-bin` PKGBUILD and `.SRCINFO`, the Copr `apic.spec`, the winget manifests for `rizukirr.apic` and `rizukirr.apic-gui`, and the Flatpak manifest and metainfo were synced to the 0.5.0 artifacts and checksums. The winget GUI ProductCode was refreshed to the value baked into the 0.5.0 MSI, since `apic-gui/wix/main.wxs` declares `Product Id='*'` and so mints a new one on every build.

## [0.4.1] - 2026-07-30

A maintenance release on top of 0.4.0. No contract format changes and no new
commands, so 0.4.0 projects work as they are. The bulk of the work is the
eframe/egui 0.35 upgrade behind the desktop GUI, a dependency refresh across the
workspace, and a higher minimum supported Rust version.

### Changed

- **Desktop GUI now builds on eframe/egui 0.35** (up from 0.33.3), including the
  Windows wgpu renderer feature. The upgrade required following three breaking
  changes in egui:
  - `eframe::App` no longer exposes `update(&mut self, ctx, frame)`. The GUI
    implements `ui(&mut self, ui, frame)` instead, so the root of the app is a
    `Ui` with no margin or background rather than a `Context`. Panels attach to
    that `Ui`, while the file dialogs and modals keep working off the `Context`
    reached through `ui.ctx()`.
  - `TopBottomPanel` and `SidePanel` are replaced by the unified `egui::Panel`
    (`Panel::top`, `Panel::bottom`, `Panel::left`), and the sidebar's
    `default_width`/`min_width` become `default_size`/`min_size`. Nested panels
    use `show` rather than `show_inside`.
  - `TextEdit::frame(bool)` is replaced by `TextEdit::frame(egui::Frame)`, so
    every frameless inline input now passes `egui::Frame::NONE`.
- **The neon theme is installed for every egui theme variant.** egui 0.35 dropped
  the single global `Context::set_style` in favour of per-theme styles, so
  `apply_theme` uses `all_styles_mut`. The dark monospace palette holds no matter
  what the host OS reports as its light/dark preference.
- **Minimum supported Rust version raised to 1.97**, declared uniformly across
  `apic-cli`, `apic-core`, and `apic-gui`. The previous floor across all three
  crates was 1.88. One version for the whole workspace means any toolchain that
  builds one crate builds all of them.
- **Dependencies refreshed and pinned to the patch level.** `clap` 4.6.1 to
  4.6.4, `serde` 1.0.228 to 1.0.229, `serde_json` 1.0.150 to 1.0.151, plus
  explicit patch floors for `libc` (0.2.189), `ratatui` (0.30.2), and
  `ratatui-textarea` (0.9.2) that were previously loose minor requirements.
- CI and release workflows use `actions/checkout@v5` instead of `v4`, and the
  runner images were refreshed.

### Fixed

- `egui_extras`' frame cache now returns an owned value in the JSON syntax
  highlighter, matching the 0.35 API and keeping the cached layout job valid for
  the rest of the frame.
- The GUI frame-timing test drives the app through `Context::run_ui` instead of
  the removed `Context::run`, so it exercises the same root `Ui` path that
  eframe hands the real app.

### Documentation

- `apic_core::fuzzy::fuzzy_find`'s example is a real, compiled doctest rather
  than an ` ```ignore ` block, so the documented usage is verified on every test
  run.
- Rewrote the Flatpak packaging guidance in `packaging/flatpak/README.md` to
  match the current update flow.

### Packaging

- The AUR `apic-bin` PKGBUILD and `.SRCINFO`, and the Copr `apic.spec`, were
  synced to the 0.4.0 artifacts and checksums.

## [0.4.0] - 2026-07-06

A large release: a simplified, breaking contract format plus full redesigns of
both the TUI and the desktop GUI editors around it.

### Changed

- **Contract format (breaking).** The request URL is now a single free-form
  string (`"url": "https://api.example.com/v1/users/{id}"`) with inline `{name}`
  path tokens, replacing the old structured URL object. Query parameters are
  `{ name, value, description, required }`, both query parameters and headers
  gain a `required` flag, and each response can carry its own `headers`. A
  request body is the raw JSON payload written directly under `request`, and a
  response body is written under the response's `schema` key. The old
  `type`/`schema` field-level model and the `{ "example": ... }` body wrapper are
  gone.
- User-facing messages use commas instead of semicolons and em-dashes.
- **TUI editing, redesigned.** The endpoint header is one inline-editable
  ` METHOD url` line; the method is chosen from a picker popup instead of cycling.
  `QUERY` and `HEADERS` are `NAME/VALUE(/DESCRIPTION)` tables. `RESPONSE` is a
  `code - title` tab strip: `a` opens a `Status / Short Description` form and then
  the JSON editor, `e` edits a tab's status/title (or opens the editor on its
  body), and `d` removes a response. The JSON editor saves with `Ctrl-S`, cancels
  with `Esc`, and pretty-prints with `Ctrl-P`; saving an empty body removes its
  response.
- **Desktop GUI, redesigned.** A single tabbed editor (Overview, Headers, Query,
  Request, Response) with a line-numbered JSON editor, metadata tables carrying a
  Required/Optional chip, editable response-code tabs, frameless inline inputs,
  and a calmer green theme.

### Added

- The HTTP method is selected from a **dropdown** in the GUI (and a picker popup
  in the TUI) listing all methods, rather than cycling one click at a time.
- A `required` flag on request headers and query parameters, surfaced across the
  CLI/TUI/GUI, the Postman converter, and the project template.
- Response-level `headers`, editable in both the TUI and the GUI.
- `apic convert --postman` gains a `--force` flag to overwrite contracts that
  already exist. The default still refuses (erroring on an existing file), and
  the error now points to `--force`.
- Refreshed bundled `example/` project (`authentication/` and `profile/` sets).

### Removed

- The structured body schema, `type`, field `schema`, typed fields, `properties`,
  `file`/`accept` parts, and `object[]` array bodies, along with the structured
  URL object. Request and response bodies are now raw JSON payloads (the request
  written directly under `request`, a response under `schema`).
- `apic read --example` (bodies are already example-only) and the TUI schema
  generate/infer keys, which no longer have a schema to operate on.

### Fixed

- GUI: the sidebar method badge now refreshes when a contract is saved.
- GUI: warn when installing a per-user launcher entry would duplicate an existing
  system-wide `apic` entry.
- TUI: numerous editing-correctness fixes (method focus on the url line, modal
  key handling after close, selected response-tab text color, empty-row cleanup).
- Windows: statically link the MSVC CRT so binaries run without VCRedist, and
  install `apic-gui.exe` at the MSI root so winget resolves the executable.

## [0.3.6] - 2026-06-30

### Added
- Windows: `apic-gui` now ships as an MSI installer (via winget) that adds an
  **apic** entry to the Start menu and an uninstaller in Settings → Apps.

### Fixed
- Windows: `apic-gui` uses the wgpu (DirectX) renderer, so it launches in
  environments without an OpenGL driver (VMs, RDP, fresh installs) where the
  previous OpenGL backend failed to open a window.

## [0.3.5] - 2026-06-24

### Added
- `apic-gui` renders JSON request/response examples with syntax highlighting, in
  both the read-only view and the editable body editor.
- Generate a body's schema from its example JSON (the inverse of generating an
  example from the schema): a "generate schema from example" button in
  `apic-gui`, and the `G` key in the TUI, with each field's type inferred from
  its value.
- TUI `e` key to edit a body's example directly, so an example can be written
  even when the schema is empty (then `G` infers the schema from it).

### Changed
- The TUI keeps a body's example visible when its schema is empty, so the
  example stays reachable and editable.

## [0.3.4] - 2026-06-22

### Added
- `apic-gui` new-file and new-template dialogs submit on Enter (not only the
  Create button) and auto-focus the name input the moment the dialog opens.
- Newly created contracts and templates open immediately in the central editor,
  with no extra click.
- A `+ field` button at the bottom of each nested object in the schema editor.

### Changed
- Contract creation rules in `apic-gui`: a trailing slash creates a folder,
  otherwise a contract file is created with `.json` appended when omitted (so a
  bare `logout` becomes `logout.json`).
- Template names already ending in `.json` no longer get a duplicate extension.
- Extracted the `apic-gui` layout, widgets, and theme into a `ui` module so the
  presentation lives apart from app state.

### Fixed
- Restored the response tabs and the add-response button in the schema editor,
  including in preview mode.

## [0.3.3] - 2026-06-21

### Added
- `apic-gui` can collapse its sidebar: a top-bar button toggles the sidebar, the
  collapsed sidebar (and its search) is skipped from rendering to give the
  editor more room, and the open/closed state is tracked via a `sidebar_open`
  flag and `ToggleSidebar` action.
- `apic-gui` schema editor now lets you edit field descriptions in request and
  response schemas, and request/response templates are editable.

### Changed
- New responses default to HTTP status code `200`, with a clearer error message
  for invalid codes.
- Invalid GUI inputs are flagged via an explicit error state, and the response
  code label is centered.
- Reworded the CLI `about` / `long_about` help text for clarity.
- Removed the search icon from the GUI search field.

### Fixed
- Editing a contract and then cancelling now restores the original contract
  data instead of keeping the partial edits.

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

[Unreleased]: https://github.com/rizukirr/apic/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/rizukirr/apic/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/rizukirr/apic/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/rizukirr/apic/compare/v0.3.6...v0.4.0
[0.2.4]: https://github.com/rizukirr/apic/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/rizukirr/apic/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/rizukirr/apic/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/rizukirr/apic/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/rizukirr/apic/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/rizukirr/apic/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.2...v0.1.0
[0.1.0-beta.2]: https://github.com/rizukirr/apic/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/rizukirr/apic/releases/tag/v0.1.0-beta.1
